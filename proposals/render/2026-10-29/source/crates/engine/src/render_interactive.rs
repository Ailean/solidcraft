//! Transient CPU Interactive capture, separate from final render ownership/export.
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde_json::Value;
use solidcraft_render::job::{
    self, ErrorCode, JobError, JobHandle,
    interactive::{CaptureTicket, Interactive, State, VisualRevision},
};

use crate::{Result, Session, render_jobs::job_error};

pub(crate) type Controller = Mutex<Interactive>;

/// A poisoned lifecycle is never resumed. Cleanup only its owned work, then fail.
pub(crate) fn with_controller<T>(controller: &Controller, f: impl FnOnce(&mut Interactive) -> job::Result<T>) -> job::Result<T> {
    match controller.lock() {
        Ok(mut state) => f(&mut state),
        Err(poisoned) => {
            let _ = poisoned.into_inner().disable();
            Err(JobError::new(ErrorCode::Internal, "Interactive lifecycle lock poisoned"))
        }
    }
}

/// One transient controller for the current document/service registration.
/// Only Session::interactive_render allocates its dedicated preview namespace.
pub struct InteractiveRender {
    document_uid: u64,
    preview_session: u64,
    controller: Arc<Controller>,
}

impl InteractiveRender {
    pub(crate) fn new(document_uid: u64, preview_session: u64, final_active: bool) -> job::Result<Self> {
        if document_uid == 0 || preview_session == 0 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "Interactive identity exhausted"));
        }
        let mut state = Interactive::new(preview_session);
        state.set_final_active(final_active, Duration::ZERO)?;
        Ok(Self { document_uid, preview_session, controller: Arc::new(Mutex::new(state)) })
    }

    pub(crate) fn weak(&self) -> Weak<Controller> {
        Arc::downgrade(&self.controller)
    }

    fn with<T>(&self, f: impl FnOnce(&mut Interactive) -> job::Result<T>) -> Result<T> {
        with_controller(&self.controller, f).map_err(job_error)
    }

    /// Cheap ownership metadata only: no geometry, files, host discovery or work.
    pub fn is_current(&self, session: &Session) -> bool {
        self.document_uid == session.uid && session.render_jobs.is_interactive_current(&self.weak())
    }

    pub fn document_uid(&self) -> u64 {
        self.document_uid
    }

    pub fn enable(&self, revision: VisualRevision, now: Duration) -> Result<()> {
        self.with(|state| state.enable(revision, now))
    }

    pub fn edit(&self, revision: VisualRevision, now: Duration) -> Result<()> {
        self.with(|state| state.edit(revision, now))
    }

    pub fn disable(&self) -> Result<()> {
        self.with(Interactive::disable)
    }

    pub fn pause(&self) -> Result<()> {
        self.with(Interactive::pause)
    }

    pub fn resume(&self, now: Duration) -> Result<()> {
        self.with(|state| state.resume(now))
    }

    pub fn set_final_active(&self, active: bool, now: Duration) -> Result<()> {
        self.with(|state| state.set_final_active(active, now))
    }

    pub fn capture(&self, now: Duration) -> Result<Option<CaptureTicket>> {
        self.with(|state| state.capture(now))
    }

    pub fn state(&self) -> Result<State> {
        self.with(|state| state.state())
    }

    pub fn current(&self) -> Result<Option<JobHandle>> {
        self.with(|state| Ok(state.current().cloned()))
    }

    /// Capture failures live in the controller; worker failures live in its job.
    pub fn error(&self) -> Result<Option<String>> {
        self.with(|state| {
            if let Some(error) = state.error() {
                return Ok(Some(error.to_string()));
            }
            let status = state.current().map(JobHandle::status).transpose()?;
            Ok(status.and_then(|status| status.error).map(|error| error.to_string()))
        })
    }

    pub fn capture_failed(&self, ticket: CaptureTicket, message: String) -> Result<()> {
        self.with(|state| {
            state.capture_failed(ticket, JobError::new(ErrorCode::InvalidScene, message));
            Ok(())
        })
    }

    fn reject_snapshot(&self, ticket: CaptureTicket, error: JobError) -> Result<Snapshot> {
        self.with(|state| {
            state.capture_failed(ticket, error.clone());
            Ok(())
        })?;
        Err(job_error(error))
    }

    /// Freeze one complete UI intent and document. The caller supplies the same
    /// immutable numerical intent that produced this visual revision/ticket.
    /// A name, CAD revision or reused Value alone is not visual provenance.
    pub fn snapshot(&self, session: &Session, ticket: CaptureTicket, params: &Value) -> Result<Snapshot> {
        if !self.is_current(session) || ticket.job_id().session != self.preview_session || ticket.revision().scene_generation != session.revision {
            return self.reject_snapshot(
                ticket,
                JobError::new(ErrorCode::WrongOwner, "Interactive capture belongs to another document or service revision"),
            );
        }
        if session.model.is_stale() {
            return self.reject_snapshot(ticket, JobError::new(ErrorCode::InvalidScene, "finish the modelling gesture before Interactive capture"));
        }
        if !self.with(|state| Ok(state.accepts_capture(ticket)))? {
            return self.reject_snapshot(ticket, JobError::new(ErrorCode::WrongOwner, "Interactive capture ticket is no longer current"));
        }
        Ok(Snapshot {
            session: session.scratch(),
            params: params.clone(),
            ticket,
            controller: self.weak(),
            scope: solidcraft_io::vfs::current_scope(),
        })
    }
}

impl Drop for InteractiveRender {
    fn drop(&mut self) {
        let _ = self.disable();
    }
}

/// Immutable capture envelope; no engine worker or queue is created here.
/// The caller maintains one app-wide capture worker, including across tab swaps.
pub struct Snapshot {
    session: Session,
    params: Value,
    ticket: CaptureTicket,
    controller: Weak<Controller>,
    scope: Option<solidcraft_io::vfs::PathScope>,
}

/// Internal output of the ordinary authoritative CPU preparation boundary.
pub(crate) struct Prepared {
    pub(crate) scene: solidcraft_render::ProductScene,
    pub(crate) request: job::ImageRequest,
    pub(crate) options: job::JobOptions,
    pub(crate) environment: Option<solidcraft_render::lighting::environment_input::EnvironmentInput>,
}

/// This guard only receives a record just created by this capture, never a
/// caller's retained/manual/foreign handle. Dropping a JobHandle alone is not a
/// cancellation request: native queue/running work keeps its own clone.
struct Unadopted {
    job: JobHandle,
    adopted: bool,
}

impl Drop for Unadopted {
    fn drop(&mut self) {
        if !self.adopted {
            let _ = self.job.cancel();
        }
    }
}

/// Adopt a newly submitted local record. Even an equal-ID duplicate is cancelled
/// when rejected: it is a different new record, not a copy of the accepted job.
pub(crate) fn adopt_created(controller: &Controller, ticket: CaptureTicket, job: JobHandle) -> job::Result<bool> {
    if job.id().session != ticket.job_id().session {
        return Err(JobError::new(ErrorCode::WrongOwner, "created Interactive record has another identity"));
    }
    let mut pending = Unadopted { job, adopted: false };
    let accepted = with_controller(controller, |state| state.attach(ticket, pending.job.clone()))?;
    pending.adopted = accepted;
    Ok(accepted)
}

impl Snapshot {
    /// Correlate the caller's receiver and discard old errors after tab changes.
    pub fn ticket(&self) -> CaptureTicket {
        self.ticket
    }

    pub fn document_uid(&self) -> u64 {
        self.session.uid
    }

    /// Invoke beside the window, never while holding a lifecycle lock. Both
    /// preparation and admission may be long; edits/finals can invalidate either.
    pub fn run(mut self) -> Result<bool> {
        let Some(controller) = self.controller.upgrade() else { return Ok(false) };
        if !with_controller(&controller, |state| Ok(state.accepts_capture(self.ticket))).map_err(job_error)? {
            return Ok(false);
        }
        let result = (|| {
            let prepared = solidcraft_io::vfs::scoped(self.scope.take(), || crate::cmd::prepare_interactive(&self.session, &self.params))?;
            if !with_controller(&controller, |state| Ok(state.accepts_capture(self.ticket))).map_err(job_error)? {
                return Ok(false);
            }
            let host = self
                .session
                .render_job_host()
                .ok_or_else(|| job_error(JobError::new(ErrorCode::HostUnavailable, "CPU image host is unavailable")))?;
            let job = host
                .submit_with_environment_and_options(
                    self.ticket.job_id(),
                    self.ticket.revision().scene_generation,
                    prepared.scene,
                    prepared.request,
                    prepared.options,
                    prepared.environment,
                )
                .map_err(job_error)?;
            adopt_created(&controller, self.ticket, job).map_err(job_error)
        })();
        if let Err(error) = &result {
            let _ = with_controller(&controller, |state| {
                state.capture_failed(self.ticket, JobError::new(ErrorCode::InvalidScene, error.to_string()));
                Ok(())
            });
        }
        result
    }
}

/// Opaque visual-source stamp: no placed world, mesh, strong document or B-rep
/// ownership. Construct only after the previous stamp stops matching; retain it
/// and call is_current on each frame instead of cloning session metadata again.
pub struct SceneIdentity {
    uid: u64,
    revision: u64,
    document: Weak<solidcraft_doc::Document>,
    model: Weak<solidcraft_doc::ModelState>,
    pending_moves: std::collections::BTreeMap<u64, solidcraft_doc::Mat>,
    active_sketch: Option<u64>,
    active_component: u64,
    active_occurrence_path: Option<Vec<u64>>,
    visibility: solidcraft_doc::Visibility,
}

impl SceneIdentity {
    /// Exact metadata comparison. model.state only clones the existing evaluated
    /// Arc; it neither evaluates nor places bodies nor obtains a display mesh.
    /// Weak allocation identities cannot alias a later allocation while retained.
    pub fn is_current(&self, session: &Session) -> bool {
        self.uid == session.uid
            && self.revision == session.revision
            && Weak::ptr_eq(&self.document, &Arc::downgrade(&session.doc))
            && Weak::ptr_eq(&self.model, &Arc::downgrade(&session.model.state()))
            && self.pending_moves == session.pending_moves
            && self.active_sketch == session.active_sketch
            && self.active_component == session.active_component
            && self.active_occurrence_path == session.active_occurrence_path
            && self.visibility == session.visibility
    }
}

impl Session {
    /// Source identity without computing cold assembly placements or meshes.
    /// Stale modelling gestures must still be rejected separately before capture.
    /// View camera/draft/output edits belong to the caller's visual intent stamp.
    pub fn interactive_scene_identity(&self) -> SceneIdentity {
        SceneIdentity {
            uid: self.uid,
            revision: self.revision,
            document: Arc::downgrade(&self.doc),
            model: Arc::downgrade(&self.model.state()),
            pending_moves: self.pending_moves.clone(),
            active_sketch: self.active_sketch,
            active_component: self.active_component,
            active_occurrence_path: self.active_occurrence_path.clone(),
            visibility: self.visibility.clone(),
        }
    }
}

impl Session {
    /// Existing prepared world only, for Interactive reflection-binding proof.
    /// None is not a capture-availability decision: SceneIdentity remains valid
    /// for cold assemblies and the capture worker prepares their real world.
    pub fn interactive_cached_world_state(&self) -> Option<Arc<solidcraft_doc::ModelState>> {
        self.cached_world_state()
    }
}

// Exact identity checks must not prepare cold geometry on the window thread.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod scene_identity_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cold_assembly_stamp_neither_places_nor_meshes() {
        let mut session = Session::default();
        session.execute("solid.box", &json!({"length":10,"width":20,"height":5,"body_name":"Source"})).unwrap();
        // A root body plus an empty placed component is a valid cold assembly.
        let doc = session.doc_mut();
        doc.components.push(solidcraft_doc::Component { id: 100, name: "Empty component".into(), ..Default::default() });
        doc.occurrences.push(solidcraft_doc::Occurrence {
            id: 101,
            component: 100,
            parent: 0,
            name: "Empty placement".into(),
            transform: solidcraft_doc::IDENTITY,
            grounded: false,
        });
        assert!(session.world_cache.lock().unwrap().is_none());
        let state = session.model.state();
        assert!(state.body("Source").unwrap().cached_mesh().is_none());
        let stamp = session.interactive_scene_identity();
        for _ in 0..10 {
            assert!(stamp.is_current(&session));
        }
        assert!(session.world_cache.lock().unwrap().is_none());
        assert!(state.body("Source").unwrap().cached_mesh().is_none());
    }

    #[test]
    fn exact_context_and_visibility_changes_invalidate_without_revision() {
        let mut session = Session::default();
        let revision = session.revision;
        let stamp = session.interactive_scene_identity();
        session.pending_moves.insert(7, solidcraft_doc::IDENTITY);
        assert!(!stamp.is_current(&session));
        session.pending_moves.clear();
        assert!(stamp.is_current(&session));
        session.active_component = 7;
        assert!(!stamp.is_current(&session));
        session.active_component = 0;
        session.active_occurrence_path = Some(vec![7, 8]);
        assert!(!stamp.is_current(&session));
        session.active_occurrence_path = None;
        session.active_sketch = Some(9);
        assert!(!stamp.is_current(&session));
        session.active_sketch = None;
        session.visibility.hidden_occurrences.push(vec![7, 8]);
        assert!(!stamp.is_current(&session));
        session.visibility.hidden_occurrences.clear();
        session.visibility.hidden_folders.push("c0/bodies".into());
        assert!(!stamp.is_current(&session));
        assert_eq!(session.revision, revision);
    }

    #[test]
    fn stamp_has_no_strong_document_or_model_retention() {
        let session = Session::default();
        let model = session.model.state();
        let doc_count = Arc::strong_count(&session.doc);
        let model_count = Arc::strong_count(&model);
        let doc = Arc::downgrade(&session.doc);
        let weak_model = Arc::downgrade(&model);
        let stamp = session.interactive_scene_identity();
        assert_eq!(Arc::strong_count(&session.doc), doc_count);
        assert_eq!(Arc::strong_count(&model), model_count);
        assert!(stamp.is_current(&session));
        drop(model);
        drop(session);
        assert!(doc.upgrade().is_none());
        assert!(weak_model.upgrade().is_none());
        // Retained Weak allocation identities cannot be recycled for a new object.
        let mut replacement = Session::default();
        replacement.uid = stamp.uid;
        replacement.revision = stamp.revision;
        assert!(!stamp.is_current(&replacement));
    }

    #[test]
    fn same_document_and_revision_do_not_hide_a_replaced_evaluated_model() {
        let mut session = Session::default();
        let stamp = session.interactive_scene_identity();
        session.model = Session::default().model;
        assert!(!stamp.is_current(&session));
        let refreshed = session.interactive_scene_identity();
        assert!(refreshed.is_current(&session));
        session.revision += 1;
        assert!(!refreshed.is_current(&session));
    }
}
