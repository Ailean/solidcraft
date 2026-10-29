//! Opt-in CPU checkpoints. This cache and its capture receiver are app-wide,
//! never part of saved setups, final results or export ownership.
use super::ImageMethod;
use crate::{SolveApp, theme::Tokens, ui_kit};
use serde_json::{Value, json};
#[cfg(not(target_arch = "wasm32"))]
use solidcraft_engine::render_interactive::Snapshot;
use solidcraft_engine::{
    render::job::{
        FrameInfo, HostKind, Integrator, JobId,
        interactive::{CaptureTicket, State as Phase, VisualRevision},
    },
    render_interactive::{InteractiveRender, SceneIdentity},
    schema::render_ui::Action,
};
use std::{sync::mpsc, time::Duration};

struct Intent {
    scene: SceneIdentity,
    uid: u64,
    revision: u64,
    camera: solidcraft_engine::render::Camera,
    stale: bool,
    request: Result<Value, String>,
}

pub(super) struct Pending {
    pub(super) uid: u64,
    pub(super) ticket: CaptureTicket,
    pub(super) receiver: mpsc::Receiver<Result<bool, String>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct PixelKey {
    job: JobId,
    sequence: u32,
    complete: bool,
}
struct Cached {
    key: PixelKey,
    revision: VisualRevision,
    info: FrameInfo,
    samples: u32,
    stale: bool,
    texture: Result<egui::TextureHandle, String>,
}

pub(super) struct State {
    context: Option<InteractiveRender>,
    pub(super) pending: Option<Pending>,
    issued: Option<CaptureTicket>,
    observed: Option<Intent>,
    visual: u64,
    clock: Duration,
    phase: Phase,
    user_paused: bool,
    final_active: bool,
    error: Option<String>,
    frame: Option<Cached>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            context: None,
            pending: None,
            issued: None,
            observed: None,
            visual: 0,
            clock: Duration::ZERO,
            phase: Phase::Off,
            user_paused: false,
            final_active: false,
            error: None,
            frame: None,
        }
    }
}
impl State {
    /// Never drop a live receiver to make room for a second capture thread.
    pub(super) fn reset_document(&mut self) {
        if let Some(context) = self.context.take() {
            let _ = context.disable();
        }
        self.issued = None;
        self.observed = None;
        self.phase = Phase::Off;
        self.user_paused = false;
        self.final_active = false;
        self.error = None;
        self.frame = None;
    }
    #[cfg(test)]
    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }
    fn revision(&self) -> Option<VisualRevision> {
        self.observed.as_ref().map(|intent| VisualRevision { scene_generation: intent.revision, visual_revision: self.visual })
    }
    fn drain(&mut self, app: &SolveApp) {
        let Some(pending) = self.pending.take() else { return };
        let reply = match pending.receiver.try_recv() {
            Ok(reply) => reply,
            Err(mpsc::TryRecvError::Empty) => {
                self.pending = Some(pending);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(app.tr("render.interactive_error").into()),
        };
        if pending.uid != app.session.uid
            || self.issued != Some(pending.ticket)
            || self.revision() != Some(pending.ticket.revision())
            || self.context.as_ref().is_none_or(|context| !context.is_current(&app.session))
        {
            return;
        }
        // Final preemption or a user pause invalidates the outstanding ticket;
        // an expected rejected capture is not a fresh preparation failure.
        if self.user_paused
            || self.final_active
            || self.context.as_ref().is_some_and(|context| matches!(context.state(), Ok(Phase::Paused | Phase::Off)))
        {
            return;
        }
        match reply {
            Err(error) => {
                if let Some(context) = &self.context {
                    let _ = context.capture_failed(pending.ticket, error.clone());
                }
                self.error = Some(error);
                self.phase = Phase::Error;
            }
            Ok(false) => {
                self.error = Some(app.tr("render.interactive_error").into());
                self.phase = Phase::Error;
            }
            Ok(true) => {}
        }
    }
    fn observe(&mut self, app: &SolveApp) -> Result<(), String> {
        // The exact-input stamp never computes world placements/meshes.
        // Preserve a current stamp across request-only edits; its weak identities
        // prevent pointer reuse without retaining CAD data or changing COW behavior.
        let stale = app.session.model.is_stale();
        let request = if stale { Err(app.tr("render.error_finish_gesture").into()) } else { super::interactive_request(app) };
        let current_scene = self.observed.as_ref().is_some_and(|old| old.scene.is_current(&app.session));
        if current_scene
            && self.observed.as_ref().is_some_and(|old| {
                old.uid == app.session.uid
                    && old.revision == app.session.revision
                    && old.camera == app.cam
                    && old.stale == stale
                    && old.request == request
            })
        {
            return Ok(());
        }
        let scene = match self.observed.take() {
            Some(old) if current_scene => old.scene,
            _ => app.session.interactive_scene_identity(),
        };
        let intent = Intent { scene, uid: app.session.uid, revision: app.session.revision, camera: app.cam, stale, request };
        let recovering = self.error.is_some() || self.phase == Phase::Error;
        self.visual = self.visual.checked_add(1).ok_or_else(|| app.tr("render.interactive_error").to_string())?;
        self.observed = Some(intent);
        self.issued = None;
        self.error = None;
        if self.user_paused {
            if let Some(frame) = &mut self.frame {
                frame.stale = true;
            }
        } else {
            self.frame = None;
        }
        if let (Some(context), Some(revision)) = (&self.context, self.revision()) {
            context.edit(revision, self.clock).map_err(|error| error.to_string())?;
            if recovering && !self.user_paused {
                context.resume(self.clock).map_err(|error| error.to_string())?;
            }
        }
        if !stale && let Some(Err(error)) = self.observed.as_ref().map(|intent| &intent.request) {
            self.error = Some(error.clone());
        }
        Ok(())
    }
    fn update(&mut self, app: &mut SolveApp, ctx: &egui::Context, dispatch: bool) -> Result<(), String> {
        self.clock = monotonic(self.clock, ctx.input(|input| input.time));
        self.drain(app);
        if self.context.is_none() {
            return Ok(());
        }
        if !super::active(app)
            || app.render_workspace.view.method != ImageMethod::CpuRaytrace
            || self.context.as_ref().is_some_and(|context| !context.is_current(&app.session))
        {
            self.reset_document();
            return Ok(());
        }
        self.observe(app)?;
        let final_active = finals_active(app);
        let suspend = final_active.as_ref().copied().unwrap_or(true) || app.render_workspace.view.result;
        self.final_active = suspend;
        let waiting_for_model = self.observed.as_ref().is_some_and(|intent| intent.stale);
        let context = self.context.as_ref().ok_or_else(|| app.tr("render.interactive_error").to_string())?;
        context.set_final_active(suspend || waiting_for_model, self.clock).map_err(|error| error.to_string())?;
        if suspend || waiting_for_model {
            self.issued = None;
        }
        if suspend && !self.user_paused {
            self.frame = None;
        }
        if let Err(error) = final_active {
            self.error = Some(error);
        }
        if waiting_for_model {
            self.phase = if self.user_paused || suspend { Phase::Paused } else { Phase::Updating };
            if let Some(frame) = &mut self.frame {
                frame.stale = true;
            }
            return Ok(());
        }
        if self.error.is_some() {
            // Request errors must cancel an already attached old capture, too.
            context.pause().map_err(|error| error.to_string())?;
            self.phase = Phase::Error;
            return Ok(());
        }
        self.phase = context.state().map_err(|error| error.to_string())?;
        if self.phase == Phase::Error {
            self.error = context.error().map_err(|error| error.to_string())?.or_else(|| Some(app.tr("render.interactive_error").into()));
            self.frame = None;
            return Ok(());
        }
        self.cache(ctx)?;
        if !dispatch || self.pending.is_some() || suspend || self.user_paused {
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ticket) = self
            .context
            .as_ref()
            .ok_or_else(|| app.tr("render.interactive_error").to_string())?
            .capture(self.clock)
            .map_err(|error| error.to_string())?
        {
            let request = self
                .observed
                .as_ref()
                .and_then(|intent| intent.request.as_ref().ok())
                .ok_or_else(|| app.tr("render.interactive_error").to_string())?;
            let snapshot = self
                .context
                .as_ref()
                .ok_or_else(|| app.tr("render.interactive_error").to_string())?
                .snapshot(&app.session, ticket, request)
                .map_err(|error| error.to_string())?;
            self.issued = Some(ticket);
            self.pending = Some(spawn(snapshot)?);
            self.phase = Phase::Updating;
        }
        Ok(())
    }
    fn cache(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.user_paused || self.final_active {
            return Ok(());
        }
        let Some(context) = &self.context else { return Ok(()) };
        let Some(job) = context.current().map_err(|error| error.to_string())? else { return Ok(()) };
        let Some(ticket) = self.issued else { return Ok(()) };
        if job.id() != ticket.job_id() || self.revision() != Some(ticket.revision()) {
            return Ok(());
        }
        let status = job.status().map_err(|error| error.to_string())?;
        if status.request.integrator != Integrator::Raytrace || status.scene_generation != ticket.revision().scene_generation {
            return Err(crate::i18n::tr("render.interactive_error").into());
        }
        // Prefer an immutable checkpoint. A completed 16-SPP preview is also
        // provisional and is never inserted into view.job/completed/result slots.
        let preview = job.preview().map_err(|error| error.to_string())?;
        if let Some(preview) = preview {
            let key = PixelKey { job: job.id(), sequence: preview.info.frame_sequence, complete: false };
            self.cache_png(ctx, key, ticket.revision(), &preview.info.frame, preview.info.samples_per_pixel, (preview.png(), &status.request))?;
        } else if let Some(frame) = job.frame().map_err(|error| error.to_string())? {
            let key = PixelKey { job: job.id(), sequence: 0, complete: true };
            let samples = status.request.raytrace.as_ref().and_then(|options| options.progressive).map_or(0, |options| options.samples_per_pixel);
            self.cache_png(ctx, key, ticket.revision(), &frame.info, samples, (frame.png(), &status.request))?;
        }
        Ok(())
    }
    fn cache_png(
        &mut self,
        ctx: &egui::Context,
        key: PixelKey,
        revision: VisualRevision,
        info: &FrameInfo,
        samples: u32,
        payload: (&[u8], &solidcraft_engine::render::job::ImageRequest),
    ) -> Result<(), String> {
        let (png, request) = payload;
        if self.frame.as_ref().is_some_and(|frame| frame.key == key && frame.revision == revision) {
            return Ok(());
        }
        if info.job != key.job
            || info.scene_generation != revision.scene_generation
            || !(1..=16).contains(&samples)
            || (info.width, info.height) != (request.width, request.height)
            || info.width > 1024
            || info.height > 1024
        {
            return Err(crate::i18n::tr("render.error_provisional_identity").into());
        }
        let texture = super::decode_render_image(
            png,
            info.width,
            info.height,
            super::captured_image_limits(request),
            ctx.input(|input| input.max_texture_side),
        )
        .map(|image| ctx.load_texture("sc_render_interactive", image, egui::TextureOptions::LINEAR));
        let error = texture.as_ref().err().cloned();
        self.frame = Some(Cached { key, revision, info: info.clone(), samples, stale: false, texture });
        if let Some(error) = error {
            return Err(error);
        }
        Ok(())
    }
}

pub(super) fn monotonic(previous: Duration, seconds: f64) -> Duration {
    if !seconds.is_finite() || seconds < 0.0 {
        return previous;
    }
    match Duration::try_from_secs_f64(seconds) {
        Ok(now) => previous.max(now),
        Err(_) => previous,
    }
}
fn finals_active(app: &SolveApp) -> Result<bool, String> {
    let mut active = false;
    for session in std::iter::once(&app.session).chain(app.docs.background()) {
        active |= session.render_final_active().map_err(|error| error.to_string())?;
    }
    Ok(active)
}

pub(super) fn ready(app: &SolveApp) -> Result<(), String> {
    if cfg!(target_arch = "wasm32") || !super::active(app) || app.render_workspace.view.method != ImageMethod::CpuRaytrace {
        return Err(app.tr("render.interactive_requires_cpu").into());
    }
    let host = app.session.render_job_host().ok_or_else(|| app.tr("render.interactive_requires_cpu").to_string())?;
    if host.kind() != HostKind::NativeWorker || !host.is_available() || host.validate_integrator(Integrator::Raytrace).is_err() {
        return Err(app.tr("render.interactive_requires_cpu").into());
    }
    if finals_active(app)? || app.render_workspace.view.result {
        return Err(app.tr("render.interactive_final_active").into());
    }
    Ok(())
}

pub(super) fn apply_command(app: &mut SolveApp, action: Action) -> Result<(), String> {
    if matches!(action, Action::On | Action::Resume) {
        ready(app)?;
    }
    let mut state = std::mem::take(&mut app.render_workspace.interactive);
    let result = (|| {
        match action {
            Action::Off => state.reset_document(),
            Action::On => {
                state.reset_document();
                state.context = Some(app.session.interactive_render().map_err(|error| error.to_string())?);
                state.observe(app)?;
                let revision = state.revision().ok_or_else(|| app.tr("render.interactive_error").to_string())?;
                state
                    .context
                    .as_ref()
                    .ok_or_else(|| app.tr("render.interactive_error").to_string())?
                    .enable(revision, state.clock)
                    .map_err(|error| error.to_string())?;
                state.phase = Phase::Updating;
            }
            Action::Pause => {
                let context = state.context.as_ref().ok_or_else(|| app.tr("render.interactive_requires_cpu").to_string())?;
                context.pause().map_err(|error| error.to_string())?;
                state.user_paused = true;
                state.issued = None;
                state.phase = Phase::Paused;
            }
            Action::Resume => {
                state.observe(app)?;
                if let Some(Err(error)) = state.observed.as_ref().map(|intent| &intent.request) {
                    return Err(error.clone());
                }
                let context = state.context.as_ref().ok_or_else(|| app.tr("render.interactive_requires_cpu").to_string())?;
                context.resume(state.clock).map_err(|error| error.to_string())?;
                state.user_paused = false;
                state.issued = None;
                state.error = None;
                state.frame = None;
                state.phase = Phase::Updating;
            }
        }
        Ok(())
    })();
    app.render_workspace.interactive = state;
    result
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(snapshot: Snapshot) -> Result<Pending, String> {
    let uid = snapshot.document_uid();
    let ticket = snapshot.ticket();
    let (sender, receiver) = mpsc::channel();
    let _worker = std::thread::Builder::new()
        .name("solidcraft-interactive-capture".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.run()))
                .map_err(|_| crate::i18n::tr("render.interactive_error").to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
    Ok(Pending { uid, ticket, receiver })
}

fn refresh(app: &mut SolveApp, ctx: &egui::Context, dispatch: bool) {
    let mut state = std::mem::take(&mut app.render_workspace.interactive);
    if let Err(error) = state.update(app, ctx, dispatch) {
        if let Some(context) = &state.context {
            let _ = context.pause();
        }
        state.error = Some(error);
        state.phase = Phase::Error;
        state.frame = None;
    }
    let poll = state.pending.is_some() || matches!(state.phase, Phase::Updating | Phase::Refining);
    app.render_workspace.interactive = state;
    if poll {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
pub(super) fn tick(app: &mut SolveApp, ctx: &egui::Context) {
    refresh(app, ctx, true);
}
pub(super) fn before_paint(app: &mut SolveApp, ctx: &egui::Context) {
    refresh(app, ctx, false);
}

pub(super) fn paint(app: &SolveApp, ui: &egui::Ui, rect: egui::Rect) -> bool {
    let state = &app.render_workspace.interactive;
    if state.context.is_none() || state.final_active || state.error.is_some() {
        return false;
    }
    let Some(frame) = &state.frame else { return false };
    if !state.user_paused && (frame.stale || state.revision() != Some(frame.revision)) {
        return false;
    }
    let Ok(texture) = &frame.texture else { return false };
    let scale = (rect.width() / frame.info.width as f32).min(rect.height() / frame.info.height as f32);
    let target = egui::Rect::from_center_size(rect.center(), egui::vec2(frame.info.width as f32, frame.info.height as f32) * scale);
    ui.painter().with_clip_rect(rect).image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Tokens::get().file_image_tint,
    );
    true
}

pub(crate) fn overlay(app: &mut SolveApp, ui: &egui::Ui, free: egui::Rect) {
    let width = (free.width() - Tokens::get().space_8 * 2.0).min(Tokens::get().form_card_width);
    if width < Tokens::get().control_height {
        return;
    }
    let card = ui_kit::Container::navigation_card(
        ui.ctx(),
        egui::Id::new("sc_render_interactive_controls"),
        free.center_top(),
        egui::Align2::CENTER_TOP,
        |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            ui.set_clip_rect(ui.clip_rect().intersect(free));
            controls(app, ui);
        },
    );
    crate::shell::register_panel(app, "render:interactive-card", Some(card.response.rect.intersect(free)), None);
    crate::scenario::publish_rect("render:interactive-card", card.response.rect.intersect(free));
}
pub(super) fn controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    let state = &app.render_workspace.interactive;
    let phase = state.phase;
    let running = state.context.is_some();
    let paused = state.user_paused;
    let key = match phase {
        Phase::Off => "render.interactive_off",
        Phase::Updating => "render.interactive_updating",
        Phase::Refining => "render.interactive_refining",
        Phase::Paused => "render.interactive_paused",
        Phase::TargetReached => "render.interactive_target",
        Phase::Error => "render.interactive_error",
    };
    let label = app.tr(key).to_string();
    let error = state.error.clone();
    let samples = state.frame.as_ref().map(|frame| frame.samples);
    let dimensions = state.frame.as_ref().map(|frame| (frame.info.width, frame.info.height));
    let stale = state.frame.as_ref().is_some_and(|frame| frame.stale);
    let final_active = state.final_active;
    ui_kit::text(ui, &label, ui_kit::Text::Inline);
    if running {
        ui_kit::text(ui, app.tr("render.interactive_provisional"), ui_kit::Text::Note("render.interactive_provisional"));
    }
    if let Some(samples) = samples {
        let label = crate::i18n::format("render.interactive_samples", &[("done", &samples.to_string()), ("total", "16")]);
        ui_kit::text(ui, &label, ui_kit::Text::Inline);
    }
    if let Some((width, height)) = dimensions {
        ui_kit::text(ui, &format!("{width} × {height} px"), ui_kit::Text::Inline);
    }
    if stale {
        ui_kit::text(ui, app.tr("render.interactive_stale"), ui_kit::Text::Note("render.interactive_stale"));
    }
    if final_active {
        ui_kit::text(ui, app.tr("render.interactive_final_active"), ui_kit::Text::Note("render.interactive_final_active"));
    }
    if let Some(error) = error {
        ui_kit::text(ui, &error, ui_kit::Text::Error("render.interactive_error"));
    }
    let reason = ready(app).err();
    if let Some(reason) = &reason {
        ui_kit::text(ui, reason, ui_kit::Text::Note("render.interactive_requires_cpu"));
    }
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        for (action_value, name, key, enabled) in if !running {
            vec![(Action::On, "on", "render.interactive_on", reason.is_none())]
        } else {
            vec![
                (
                    if paused || phase == Phase::Error { Action::Resume } else { Action::Pause },
                    if paused || phase == Phase::Error { "resume" } else { "pause" },
                    if paused || phase == Phase::Error { "render.interactive_resume" } else { "render.interactive_pause" },
                    !final_active && (reason.is_none() || !(paused || phase == Phase::Error)),
                ),
                (Action::Off, "off", "render.interactive_off", true),
            ]
        } {
            let label = app.tr(key).to_string();
            let response = ui_kit::button(ui, label, ui_kit::Button::Enabled(enabled));
            crate::scenario::publish_target(&format!("render:interactive:{name}"), response.rect);
            if response.clicked() {
                action = Some(action_value);
            }
        }
    });
    if let Some(action) = action {
        if let Err(error) = app.run("render.interactive", json!({"action":action})) {
            app.set_status(error, true);
        }
        ui.ctx().request_repaint();
    }
}
pub(super) fn command_ready(app: &SolveApp) -> Result<(), String> {
    if app.render_workspace.interactive.context.is_some() { Ok(()) } else { ready(app) }
}
pub(super) fn activate(app: &mut SolveApp) {
    let state = &app.render_workspace.interactive;
    let action = if state.context.is_none() {
        Action::On
    } else if state.user_paused || state.phase == Phase::Error {
        Action::Resume
    } else {
        Action::Pause
    };
    if let Err(error) = app.run("render.interactive", json!({"action":action})) {
        app.set_status(error, true);
    }
}
pub(super) fn inspect(app: &SolveApp) -> Value {
    let state = &app.render_workspace.interactive;
    json!({"enabled":state.context.is_some(),"phase":format!("{:?}",state.phase),"visual_revision":state.visual,
        "pending_capture":state.pending.is_some(),"paused":state.user_paused,"error":state.error,
        "samples":state.frame.as_ref().map(|frame| frame.samples),"stale":state.frame.as_ref().is_some_and(|frame|frame.stale),
        "job":state.frame.as_ref().map(|frame|frame.info.job),"provisional":true})
}

// Read-only private observations for original UI acceptance tests. No production
// command/API, host factory, renderer injection, worker gate or timing hook.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) fn test_current(app: &SolveApp) -> solidcraft_engine::Result<Option<solidcraft_engine::render::job::JobHandle>> {
    let Some(context) = app.render_workspace.interactive.context.as_ref() else { return Ok(None) };
    context.current()
}
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) fn test_cache(app: &SolveApp) -> Option<(FrameInfo, u32, bool, egui::TextureId)> {
    let frame = app.render_workspace.interactive.frame.as_ref()?;
    Some((frame.info.clone(), frame.samples, frame.key.complete, frame.texture.as_ref().ok()?.id()))
}

// Harness frames advance this clock even while Interactive is Off.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) fn test_clock(app: &SolveApp) -> Duration {
    app.render_workspace.interactive.clock
}
