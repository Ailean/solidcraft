//! Per-document, transient render ownership. Captured scenes remain immutable when CAD edits
//! continue; callers get the captured revision and must never present it as a newer scene.

use crate::render_interactive::{Controller, InteractiveRender, with_controller};
use solidcraft_render::{
    ExecutorDescriptor,
    job::{CpuHost, ErrorCode, HostKind, Integrator, JobError, JobHandle, JobId, JobStatus, Phase, RenderFrame},
};
use std::collections::VecDeque;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

/// The native CPU worker has two explicit execution methods. This pure metadata
/// lookup neither creates a host nor enables raytracing in cooperative/feature-off builds.
/// A stopped native host keeps its descriptor, marked unavailable by the host.
pub(crate) fn cpu_raytrace_descriptor(host: Option<&CpuHost>) -> Option<ExecutorDescriptor> {
    let host = host.filter(|host| host.kind() == HostKind::NativeWorker)?;
    host.descriptor_for(Integrator::Raytrace).ok()
}

fn owner() -> u64 {
    let mut n = NEXT_OWNER.load(Ordering::Relaxed);
    loop {
        let Some(next) = n.checked_add(1) else { return 0 };
        match NEXT_OWNER.compare_exchange_weak(n, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return n,
            Err(current) => n = current,
        }
    }
}

pub(crate) struct Execution {
    pub executor: solidcraft_render::ExecutorId,
    pub options: solidcraft_render::job::JobOptions,
    pub environment: Option<solidcraft_render::lighting::environment_input::EnvironmentInput>,
}

pub(crate) struct Jobs {
    owner: u64,
    serial: u64,
    generation: u64,
    pub(crate) host: Option<CpuHost>,
    pub(crate) descriptor: Option<ExecutorDescriptor>,
    pub(crate) additional: Vec<(CpuHost, ExecutorDescriptor)>,
    live: Vec<JobHandle>,
    history: VecDeque<JobStatus>,
    last_good: Option<JobHandle>,
    /// One explicitly displayed completed result, independent of a newer completion.
    /// The existing frame Arc keeps its original host reservation accounted for.
    pinned: Option<JobHandle>,
    /// Preview lifetime only; it never joins final live/history/export ownership.
    interactive: Weak<Controller>,
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            owner: owner(),
            serial: 0,
            generation: 0,
            host: None,
            descriptor: None,
            additional: Vec::new(),
            live: Vec::new(),
            history: VecDeque::new(),
            last_good: None,
            pinned: None,
            interactive: Weak::new(),
        }
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        let _ = self.revoke_interactive();
        for job in &self.live {
            let _ = job.cancel();
        }
    }
}

impl Jobs {
    pub(crate) fn is_interactive_current(&self, controller: &Weak<Controller>) -> bool {
        Weak::ptr_eq(&self.interactive, controller)
    }

    fn revoke_interactive(&mut self) -> Result<(), JobError> {
        let previous = std::mem::take(&mut self.interactive);
        match previous.upgrade() {
            Some(controller) => with_controller(&controller, |state| state.disable()),
            None => Ok(()),
        }
    }

    fn suspend_interactive(&self) -> Result<(), JobError> {
        match self.interactive.upgrade() {
            Some(controller) => with_controller(&controller, |state| state.set_final_active(true, std::time::Duration::ZERO)),
            None => Ok(()),
        }
    }

    fn final_active(&self) -> Result<bool, JobError> {
        for job in &self.live {
            if !job.status()?.progress.phase.terminal() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn create_interactive(&mut self, document_uid: u64) -> Result<InteractiveRender, JobError> {
        let host = self.host.as_ref().ok_or_else(|| JobError::new(ErrorCode::HostUnavailable, "no CPU image host is installed"))?;
        if !host.is_available() || host.kind() != HostKind::NativeWorker {
            return Err(JobError::new(ErrorCode::HostUnavailable, "Interactive CPU requires an available native CPU host"));
        }
        host.validate_integrator(Integrator::Raytrace)?;
        let final_active = self.final_active()?;
        if document_uid == 0 || self.owner == 0 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "render session identity exhausted"));
        }
        // Use the same allocator as final owners; never reuse the document uid or
        // scratch owner as a preview namespace. Zero reports allocator exhaustion.
        let preview_session = owner();
        if preview_session == 0 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "Interactive owner identity exhausted"));
        }
        let next = InteractiveRender::new(document_uid, preview_session, final_active)?;
        self.revoke_interactive()?;
        self.interactive = next.weak();
        Ok(next)
    }

    pub(crate) fn host_for(&self, executor: &solidcraft_render::ExecutorId) -> Option<&CpuHost> {
        if executor.as_str() == "cpu-raytrace" {
            return self
                .host
                .as_ref()
                .filter(|host| host.descriptor_for(solidcraft_render::job::Integrator::Raytrace).is_ok_and(|d| &d.id == executor));
        }
        if self.descriptor.as_ref().is_some_and(|d| &d.id == executor) {
            return self.host.as_ref();
        }
        self.additional.iter().find(|(_, d)| &d.id == executor).map(|(host, _)| host)
    }

    pub(crate) fn scratch(&self) -> Self {
        let mut jobs = Self::default();
        jobs.host = self.host.clone();
        jobs.descriptor = self.descriptor.clone();
        jobs.additional = self.additional.clone();
        jobs
    }

    pub(crate) fn collect(&mut self) -> Result<(), JobError> {
        let mut i = 0;
        while i < self.live.len() {
            let status = self.live.get(i).ok_or_else(|| JobError::new(ErrorCode::Internal, "missing live job"))?.status()?;
            if !status.progress.phase.terminal() {
                i += 1;
                continue;
            }
            let handle = self.live.remove(i);
            if status.progress.phase == Phase::Completed && self.last_good.as_ref().is_none_or(|old| old.id().serial < handle.id().serial) {
                self.last_good = Some(handle);
            }
            self.history.push_back(status);
            while self.history.len() > 4 {
                self.history.pop_front();
            }
        }
        Ok(())
    }

    pub(crate) fn start_with_environment(
        &mut self,
        revision: u64,
        scene: solidcraft_render::ProductScene,
        request: solidcraft_render::job::ImageRequest,
        replace: bool,
        execution: Execution,
    ) -> Result<JobStatus, JobError> {
        let Execution { executor, options, environment } = execution;
        self.collect()?;
        let host = self.host_for(&executor).ok_or_else(|| JobError::new(ErrorCode::HostUnavailable, "resolved image host is not installed"))?.clone();
        if replace {
            for job in self.live.drain(..) {
                let mut status = job.cancel()?;
                // Replaced handles are deliberately abandoned: keep a cancellation tombstone,
                // not a permanently "cancelling" snapshot. The token already prevents any
                // publication; the host can finish releasing its buffers asynchronously.
                if !status.progress.phase.terminal() {
                    status.progress.phase = Phase::Cancelled;
                }
                self.history.push_back(status);
            }
            while self.history.len() > 4 {
                self.history.pop_front();
            }
        }
        if self.live.len() >= 2 {
            return Err(JobError::new(ErrorCode::QueueFull, "this document already has an active and pending render"));
        }
        if self.owner == 0 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "render session identity exhausted"));
        }
        let serial = self.serial.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "render job identity exhausted"))?;
        let generation = self.generation.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "render generation exhausted"))?;
        self.suspend_interactive()?;
        let handle = host.submit_with_environment_and_options(
            JobId { session: self.owner, serial, generation },
            revision,
            scene,
            request,
            options,
            environment,
        )?;
        let status = handle.status()?;
        self.serial = serial;
        self.generation = generation;
        self.live.push(handle);
        Ok(status)
    }

    pub(crate) fn regrade(
        &mut self,
        source_id: JobId,
        presentation: solidcraft_render::job::presentation::Presentation,
    ) -> Result<JobStatus, JobError> {
        presentation.validate()?;
        self.collect()?;
        let source = self.handle(source_id)?;
        let frame = source.frame()?.ok_or_else(|| JobError::new(ErrorCode::NotCompleted, "regrade requires a completed frame"))?;
        let master = frame.master_source()?;
        // Prefer the source pool so its retained source and derivative work share
        // one admission authority. Regrading itself never invokes its executor.
        let host = self
            .host_for(&master.info.executor)
            .or(self.host.as_ref())
            .ok_or_else(|| JobError::new(ErrorCode::HostUnavailable, "no image host is installed for master presentation"))?
            .clone();
        if self.live.len() >= 2 {
            return Err(JobError::new(ErrorCode::QueueFull, "this document already has an active and pending render"));
        }
        let serial = self.serial.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "render job identity exhausted"))?;
        let generation = self.generation.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "render generation exhausted"))?;
        if self.owner == 0 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "render session identity exhausted"));
        }
        self.suspend_interactive()?;
        let handle = host.submit_regrade(JobId { session: self.owner, serial, generation }, &source, presentation)?;
        let status = handle.status()?;
        self.serial = serial;
        self.generation = generation;
        self.live.push(handle);
        Ok(status)
    }

    fn check_owner(&self, id: JobId) -> Result<(), JobError> {
        if id.session != self.owner {
            return Err(JobError::new(ErrorCode::WrongOwner, "render job belongs to another document session"));
        }
        Ok(())
    }

    pub(crate) fn handle(&self, id: JobId) -> Result<JobHandle, JobError> {
        self.check_owner(id)?;
        self.live
            .iter()
            .chain(self.last_good.iter())
            .chain(self.pinned.iter())
            .find(|j| j.id() == id)
            .cloned()
            .ok_or_else(|| JobError::new(ErrorCode::UnknownJob, "render job is no longer retained"))
    }

    pub(crate) fn status(&mut self, id: JobId) -> Result<JobStatus, JobError> {
        self.check_owner(id)?;
        self.collect()?;
        if let Ok(handle) = self.handle(id) {
            return handle.status();
        }
        self.history.iter().find(|s| s.job == id).cloned().ok_or_else(|| JobError::new(ErrorCode::UnknownJob, "render job is no longer retained"))
    }

    pub(crate) fn pin(&mut self, id: Option<JobId>) -> Result<Option<solidcraft_render::job::FrameInfo>, JobError> {
        let Some(id) = id else {
            self.pinned = None;
            return Ok(None);
        };
        let handle = self.handle(id)?;
        let frame = handle.frame()?.ok_or_else(|| JobError::new(ErrorCode::NotCompleted, "only a completed result can be pinned"))?;
        let info = frame.info.clone();
        self.pinned = Some(handle);
        Ok(Some(info))
    }

    pub(crate) fn frame(&mut self, id: JobId) -> Result<Arc<RenderFrame>, JobError> {
        self.collect()?;
        self.handle(id)?.frame()?.ok_or_else(|| JobError::new(ErrorCode::NotCompleted, "render has no completed image"))
    }
}

impl crate::Session {
    /// Explicit activation support. The caller owns one app-wide capture worker.
    /// This registers a lifetime only; it never captures or starts an image.
    pub fn interactive_render(&mut self) -> crate::Result<InteractiveRender> {
        self.render_jobs.create_interactive(self.uid).map_err(job_error)
    }

    /// Bounded final-job metadata for current/background document scheduling.
    /// Retained completed images do not block Interactive; unknown status fails.
    pub fn render_final_active(&self) -> crate::Result<bool> {
        self.render_jobs.final_active().map_err(job_error)
    }

    /// A host is installed by the application, never by document data or a command.
    pub fn set_render_job_host(&mut self, host: Option<CpuHost>) -> crate::Result<()> {
        if self.render_jobs.host.as_ref().zip(host.as_ref()).is_some_and(|(a, b)| a.same_host(b)) {
            return Ok(());
        }
        let descriptor = host.as_ref().map(CpuHost::descriptor).transpose().map_err(job_error)?;
        if descriptor.as_ref().is_some_and(|d| d.id.as_str() == "cpu-raytrace") {
            return Err(crate::EngineError::Other("cpu-raytrace is reserved for the native CPU raytrace integrator".into()));
        }
        let previous_raytrace = cpu_raytrace_descriptor(self.render_jobs.host.as_ref());
        let mut backend = self.render_backend();
        backend.executors.retain(|e| {
            self.render_jobs.descriptor.as_ref().is_none_or(|old| old.id != e.id) && previous_raytrace.as_ref().is_none_or(|old| old.id != e.id)
        });
        if let Some(d) = &descriptor {
            backend.executors.push(d.clone());
        }
        if let Some(raytrace) = cpu_raytrace_descriptor(host.as_ref()) {
            backend.executors.push(raytrace);
        }
        solidcraft_render::resolve_request(
            &[backend],
            &solidcraft_render::RenderRequest {
                backend: None,
                mode: solidcraft_render::RenderMode::Preview,
                operation: solidcraft_render::RenderOperation::Image,
                width: 1,
                height: 1,
                format: Some(solidcraft_render::RenderFormat::Png),
                features: Vec::new(),
                policy: Default::default(),
            },
        )
        .map_err(|e| crate::EngineError::Other(e.to_string()))?;
        let mut jobs = Jobs::default();
        jobs.additional = self.render_jobs.additional.clone();
        jobs.host = host;
        jobs.descriptor = descriptor;
        self.render_jobs = jobs;
        Ok(())
    }

    /// Add an executor without replacing CPU jobs or their ownership. Applications share
    /// this host across document sessions; descriptor IDs must be unique in the registry.
    pub fn register_render_job_host(&mut self, host: solidcraft_render::job::ImageHost) -> crate::Result<()> {
        let descriptor = host.descriptor().map_err(job_error)?;
        if descriptor.id.as_str() == "cpu-raytrace" {
            return Err(crate::EngineError::Other("cpu-raytrace is reserved for the native CPU raytrace integrator".into()));
        }
        if self.render_jobs.additional.iter().any(|(old, _)| old.same_host(&host)) {
            return Ok(());
        }
        if descriptor.profiles.iter().any(|p| p.operation != solidcraft_render::RenderOperation::Image) {
            return Err(crate::EngineError::Other("image host must only register image profiles".into()));
        }
        let mut registry = self.render_backend();
        registry.executors.push(descriptor.clone());
        solidcraft_render::resolve_request(
            &[registry],
            &solidcraft_render::RenderRequest {
                backend: None,
                mode: solidcraft_render::RenderMode::Preview,
                operation: solidcraft_render::RenderOperation::Image,
                width: 1,
                height: 1,
                format: Some(solidcraft_render::RenderFormat::Png),
                features: Vec::new(),
                policy: Default::default(),
            },
        )
        .map_err(|e| crate::EngineError::Other(e.to_string()))?;
        self.render_jobs.additional.push((host, descriptor));
        Ok(())
    }

    pub fn render_job_host(&self) -> Option<&CpuHost> {
        self.render_jobs.host.as_ref()
    }

    /// Explicit native CLI waiting is separate from non-blocking status commands.
    pub fn render_job_handle(&self, id: JobId) -> crate::Result<JobHandle> {
        self.render_jobs.handle(id).map_err(job_error)
    }
}

pub(crate) fn job_error(error: JobError) -> crate::EngineError {
    crate::EngineError::Other(error.to_string())
}

#[cfg(test)]
mod displayed_result_tests {
    use super::*;
    use serde_json::json;
    fn complete(s: &mut crate::Session, color: [u8; 3]) -> JobId {
        let reply = s.execute("render.start", &json!({"settings":{"width":8,"height":8,"background":color}})).unwrap();
        let id: JobId = serde_json::from_value(reply["job"].clone()).unwrap();
        let host = s.render_job_host().unwrap().clone();
        for _ in 0..10000 {
            host.pump(16384).unwrap();
            if s.render_job_handle(id).unwrap().status().unwrap().progress.phase.terminal() {
                return id;
            }
        }
        panic!("bounded result fixture did not finish")
    }
    #[test]
    fn explicit_display_pin_preserves_the_old_exact_frame_without_unbounded_history() {
        let mut s = crate::Session::default();
        let host = CpuHost::cooperative();
        s.set_render_job_host(Some(host.clone())).unwrap();
        let before = s.doc.to_json();
        let undo = s.undo.len();
        let old = complete(&mut s, [10, 20, 30]);
        let bytes = s.render_job_handle(old).unwrap().frame().unwrap().unwrap().png().to_vec();
        s.execute("render.result.pin", &json!({"job":old})).unwrap();
        for color in [[40, 50, 60], [70, 80, 90], [100, 110, 120], [130, 140, 150], [160, 170, 180]] {
            let new = complete(&mut s, color);
            s.execute("render.status", &json!({"job":new})).unwrap();
            assert_eq!(s.render_job_handle(old).unwrap().frame().unwrap().unwrap().png(), bytes);
        }
        let retained = host.admitted_bytes();
        let foreign = JobId { session: old.session + 1, ..old };
        assert!(s.execute("render.result.pin", &json!({"job":foreign})).is_err());
        assert_eq!(host.admitted_bytes(), retained);
        assert!(s.execute("render.result.pin", &json!({})).is_err());
        s.execute("render.result.pin", &json!({"job":null})).unwrap();
        assert!(s.render_job_handle(old).is_err());
        assert!(host.admitted_bytes() < retained);
        assert_eq!(s.doc.to_json(), before);
        assert_eq!(s.undo.len(), undo);
        let reply = s.execute("render.start", &json!({"settings":{"width":8,"height":8}})).unwrap();
        assert!(s.execute("render.result.pin", &json!({"job":reply["job"]})).is_err(), "a pending result is never exportable");
    }
}

// Original context ownership and real capture integration cases.
#[cfg(all(test, feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod interactive_context_tests {
    use super::*;
    use crate::render_interactive::{InteractiveRender, Snapshot, adopt_created};
    use serde_json::{Value, json};
    use solidcraft_render::job::interactive::{State, VisualRevision};
    use std::time::Duration;

    fn native() -> (crate::Session, CpuHost) {
        let host = CpuHost::for_platform().unwrap();
        let mut session = crate::Session::default();
        session.set_render_job_host(Some(host.clone())).unwrap();
        (session, host)
    }

    fn wire(background: [u8; 3]) -> Value {
        json!({"integrator":"raytrace","settings":{"width":16,"height":16,"background":background},"raytrace":{"lights":[],"shadows":false}})
    }

    fn capture(context: &InteractiveRender, session: &crate::Session, request: &Value) -> Snapshot {
        context.enable(VisualRevision { scene_generation: session.revision, visual_revision: 1 }, Duration::ZERO).unwrap();
        let ticket = context.capture(Duration::from_millis(200)).unwrap().unwrap();
        context.snapshot(session, ticket, request).unwrap()
    }

    fn terminal(job: &JobHandle) -> Phase {
        // Completion watchdog follows existing native job tests; no speed assertion.
        job.wait_timeout(Duration::from_secs(60)).unwrap().progress.phase
    }

    fn created(session: &crate::Session, ticket: solidcraft_render::job::interactive::CaptureTicket, request: &Value) -> JobHandle {
        let p = crate::cmd::prepare_interactive(session, request).unwrap();
        session
            .render_job_host()
            .unwrap()
            .submit_with_environment_and_options(ticket.job_id(), ticket.revision().scene_generation, p.scene, p.request, p.options, p.environment)
            .unwrap()
    }

    #[test]
    fn registration_is_explicit_and_preview_frames_never_join_final_registry() {
        let (mut session, _) = native();
        let document = session.doc.clone();
        let history = (session.revision, session.undo.len(), session.redo.len());
        let context = session.interactive_render().unwrap();
        assert_eq!(context.state().unwrap(), State::Off);
        assert!(context.is_current(&session));
        assert!(context.capture(Duration::from_secs(1)).unwrap().is_none());
        let snapshot = capture(&context, &session, &wire([31, 67, 101]));
        let ticket = snapshot.ticket();
        assert_ne!(ticket.job_id().session, session.render_jobs.owner);
        assert_eq!(snapshot.document_uid(), session.uid);
        assert!(snapshot.run().unwrap());
        let preview = context.current().unwrap().unwrap();
        assert_eq!(terminal(&preview), Phase::Completed);
        assert!(preview.frame().unwrap().is_some());
        assert!(session.render_job_handle(preview.id()).is_err());
        assert!(session.execute("render.status", &json!({"job":preview.id()})).is_err());
        assert!(session.render_jobs.live.is_empty());
        assert!(session.render_jobs.last_good.is_none());
        assert!(session.render_jobs.pinned.is_none());
        assert_eq!(history, (session.revision, session.undo.len(), session.redo.len()));
        assert!(Arc::ptr_eq(&document, &session.doc));
    }

    #[test]
    fn direct_final_start_preempts_only_interactive_and_finishes() {
        let (mut session, _) = native();
        let context = session.interactive_render().unwrap();
        assert!(capture(&context, &session, &wire([11, 29, 47])).run().unwrap());
        let old = context.current().unwrap().unwrap();
        let reply = session.execute("render.start", &wire([83, 107, 139])).unwrap();
        let final_id: JobId = serde_json::from_value(reply["job"].clone()).unwrap();
        let final_job = session.render_job_handle(final_id).unwrap();
        assert_ne!(final_id.session, old.id().session);
        assert_eq!(context.state().unwrap(), State::Paused);
        assert!(context.current().unwrap().is_none());
        assert!(context.capture(Duration::from_secs(100)).unwrap().is_none());
        assert_eq!(terminal(&final_job), Phase::Completed);
        assert!(matches!(terminal(&old), Phase::Cancelled | Phase::Completed));
        assert!(!session.render_final_active().unwrap());
        let digest = final_job.frame().unwrap().unwrap().info.png_sha256.clone();
        context.disable().unwrap();
        drop(context);
        assert_eq!(session.render_job_handle(final_id).unwrap().frame().unwrap().unwrap().info.png_sha256, digest);
    }

    #[test]
    fn replacement_context_revokes_old_capture_without_meshing() {
        let (mut session, host) = native();
        session.execute("solid.box", &json!({"length":10,"width":20,"height":5,"body_name":"Source"})).unwrap();
        let world = session.world_state();
        assert!(world.body("Source").unwrap().cached_mesh().is_none());
        let old = session.interactive_render().unwrap();
        let snapshot = capture(&old, &session, &wire([1, 2, 3]));
        let old_owner = snapshot.ticket().job_id().session;
        let fresh = session.interactive_render().unwrap();
        assert!(!old.is_current(&session));
        assert_eq!(old.state().unwrap(), State::Off);
        assert_eq!(fresh.state().unwrap(), State::Off);
        assert!(!snapshot.run().unwrap());
        assert!(world.body("Source").unwrap().cached_mesh().is_none());
        assert_eq!(host.admitted_bytes(), 0);
        let next = capture(&fresh, &session, &wire([4, 5, 6]));
        assert_ne!(next.ticket().job_id().session, old_owner);
    }

    #[test]
    fn host_a_b_a_cannot_revive_old_registration_in_the_same_document() {
        let (mut session, host_a) = native();
        let uid = session.uid;
        let old = session.interactive_render().unwrap();
        let snapshot = capture(&old, &session, &wire([1, 2, 3]));
        let host_b = CpuHost::for_platform().unwrap();
        session.set_render_job_host(Some(host_b)).unwrap();
        assert_eq!(session.uid, uid);
        assert!(!old.is_current(&session));
        assert_eq!(old.state().unwrap(), State::Off);
        assert!(!snapshot.run().unwrap());
        session.set_render_job_host(Some(host_a.clone())).unwrap();
        assert!(!old.is_current(&session));
        old.enable(VisualRevision { scene_generation: session.revision, visual_revision: 2 }, Duration::ZERO).unwrap();
        let stale = old.capture(Duration::from_millis(200)).unwrap().unwrap();
        assert!(old.snapshot(&session, stale, &wire([7, 8, 9])).is_err());
        assert_eq!(host_a.admitted_bytes(), 0);
        assert!(session.interactive_render().unwrap().is_current(&session));
    }

    #[test]
    fn new_design_and_equal_revision_foreign_session_refuse_old_tickets() {
        let (mut session, host) = native();
        let context = session.interactive_render().unwrap();
        let snapshot = capture(&context, &session, &wire([1, 2, 3]));
        let ticket = snapshot.ticket();
        let uid = session.uid;
        let mut foreign = crate::Session::new((*session.doc).clone());
        foreign.revision = session.revision;
        foreign.set_render_job_host(Some(host.clone())).unwrap();
        assert_ne!(foreign.uid, uid);
        assert!(!context.is_current(&foreign));
        assert!(context.snapshot(&foreign, ticket, &wire([4, 5, 6])).is_err());
        session.execute("file.new", &json!({})).unwrap();
        assert_ne!(session.uid, uid);
        assert!(!context.is_current(&session));
        assert_eq!(context.state().unwrap(), State::Off);
        assert!(!snapshot.run().unwrap());
        assert_eq!(host.admitted_bytes(), 0);
    }

    #[test]
    fn scratch_drop_does_not_revoke_source_and_exhaustion_is_local() {
        let (mut session, _) = native();
        let context = session.interactive_render().unwrap();
        drop(session.scratch());
        assert!(context.is_current(&session));
        assert_eq!(context.state().unwrap(), State::Off);
        assert!(InteractiveRender::new(session.uid, 0, false).is_err());
        let original_owner = session.render_jobs.owner;
        session.render_jobs.owner = 0;
        assert!(session.interactive_render().is_err());
        assert!(context.is_current(&session));
        session.render_jobs.owner = original_owner;
    }

    #[test]
    fn failed_adoption_with_poisoned_controller_cancels_unpumped_cooperative_record() {
        let (mut session, _) = native();
        let context = session.interactive_render().unwrap();
        let snapshot = capture(&context, &session, &wire([1, 2, 3]));
        let ticket = snapshot.ticket();
        let p = crate::cmd::prepare_interactive(&session, &wire([1, 2, 3])).unwrap();
        let mut request = p.request;
        request.integrator = Integrator::Raster;
        request.raytrace = None;
        // A real cooperative queue cannot complete without an explicit pump.
        // Adoption checks owner, then the poisoned lifecycle before integrator.
        let cooperative = CpuHost::cooperative();
        let job = cooperative.submit(ticket.job_id(), ticket.revision().scene_generation, p.scene, request).unwrap();
        assert_eq!(job.status().unwrap().progress.phase, Phase::Queued);
        let controller = session.render_jobs.interactive.upgrade().unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = controller.lock().unwrap();
            panic!("original poison fixture");
        }));
        assert!(adopt_created(&controller, ticket, job.clone()).is_err());
        assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(context.current().is_err(), "a poisoned lifecycle is not resumed");
        // No pump, sleeps, synthetic executor or large scene is needed. Removing
        // Unadopted::drop cancellation leaves Queued and fails this assertion.
        // Only controller poison is tested; record.data poison is best-effort.
    }

    #[test]
    fn rejected_fresh_equal_id_record_preserves_retained_completed_frame() {
        let (mut session, _) = native();
        let context = session.interactive_render().unwrap();
        let snapshot = capture(&context, &session, &wire([17, 31, 59]));
        let ticket = snapshot.ticket();
        assert!(snapshot.run().unwrap());
        let accepted = context.current().unwrap().unwrap();
        assert_eq!(terminal(&accepted), Phase::Completed);
        let frame = accepted.frame().unwrap().unwrap();
        let digest = frame.info.png_sha256.clone();
        let duplicate = created(&session, ticket, &wire([83, 107, 139]));
        assert_eq!(duplicate.id(), accepted.id());
        let controller = session.render_jobs.interactive.upgrade().unwrap();
        assert!(!adopt_created(&controller, ticket, duplicate.clone()).unwrap());
        let phase = duplicate.status().unwrap().progress.phase;
        assert!(phase.terminal() || phase == Phase::Cancelling);
        assert!(matches!(terminal(&duplicate), Phase::Cancelled | Phase::Completed));
        let retained = context.current().unwrap().unwrap().frame().unwrap().unwrap();
        assert!(Arc::ptr_eq(&retained, &frame));
        assert_eq!(retained.info.png_sha256, digest);
        // This native fixture proves accepted-record retention only. Completed
        // duplicate work does not prove a cancellation request; core queued
        // distinct-record acceptance covers that separate invariant.
        assert_eq!(accepted.status().unwrap().request.background, [17, 31, 59]);
    }

    #[test]
    fn dropping_context_before_worker_start_keeps_the_source_mesh_cold() {
        let (mut session, host) = native();
        session.execute("solid.box", &json!({"length":10,"width":20,"height":5,"body_name":"Source"})).unwrap();
        let world = session.world_state();
        assert!(world.body("Source").unwrap().cached_mesh().is_none());
        let context = session.interactive_render().unwrap();
        let snapshot = capture(&context, &session, &wire([1, 2, 3]));
        drop(context);
        assert!(!snapshot.run().unwrap());
        assert!(world.body("Source").unwrap().cached_mesh().is_none());
        assert_eq!(host.admitted_bytes(), 0);
    }
}
