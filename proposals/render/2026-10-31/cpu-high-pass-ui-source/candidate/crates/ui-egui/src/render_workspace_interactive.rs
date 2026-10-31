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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PartialCursor {
    sequence: u64,
    completed_passes: u32,
    updated_prefix: u64,
}

/// The source frontier is not a uniform samples-per-pixel count.
pub(super) fn partial_sample_count(completed_passes: u32, updated_prefix: u64, index: u64, pixels: u64, target: u32) -> Result<u32, String> {
    if pixels == 0
        || index >= pixels
        || updated_prefix >= pixels
        || target == 0
        || completed_passes > target
        || (completed_passes == target && updated_prefix != 0)
    {
        return Err(crate::i18n::tr("render.error_provisional_identity").into());
    }
    completed_passes
        .checked_add(u32::from(index < updated_prefix))
        .filter(|count| *count <= target)
        .ok_or_else(|| crate::i18n::tr("render.error_provisional_identity").into())
}

/// Alpha denotes knowledge only in this additive provisional PNG contract.
/// Physical primary coverage has already been composited by the renderer.
pub(super) fn validate_partial_pixel(count: u32, covered: u32, pixel: egui::Color32, background: [u8; 3]) -> Result<(), String> {
    if covered > count
        || (count == 0 && pixel.a() != 0)
        || (count > 0 && pixel.a() != 255)
        || (count > 0 && covered == 0 && pixel != egui::Color32::from_rgb(background[0], background[1], background[2]))
    {
        return Err(crate::i18n::tr("render.error_provisional_identity").into());
    }
    Ok(())
}

fn partial_advances(previous: Option<PartialCursor>, next: PartialCursor) -> bool {
    next.sequence != 0
        && previous.is_none_or(|old| {
            next.sequence > old.sequence && (next.completed_passes, next.updated_prefix) > (old.completed_passes, old.updated_prefix)
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PartialSampling {
    plan: solidcraft_engine::render::job::PartialPreviewPlan,
    cursor: PartialCursor,
}

impl PartialSampling {
    pub(super) fn source_unknown(&self) -> bool {
        self.cursor.completed_passes == 0
    }

    pub(super) fn sample_range(&self) -> (u32, u32) {
        (self.cursor.completed_passes, self.cursor.completed_passes + u32::from(self.cursor.updated_prefix != 0))
    }

    pub(super) fn sampled_source_pixels(&self) -> u64 {
        if self.source_unknown() { self.cursor.updated_prefix } else { u64::from(self.plan.source_width()) * u64::from(self.plan.source_height()) }
    }

    fn uniform_samples(&self) -> Option<u32> {
        (self.cursor.completed_passes != 0 && self.cursor.updated_prefix == 0).then_some(self.cursor.completed_passes)
    }
}

pub(super) struct PartialExpectation<'a> {
    pub(super) job: JobId,
    pub(super) scene_generation: u64,
    pub(super) request: &'a solidcraft_engine::render::job::ImageRequest,
    pub(super) final_dimensions: [u32; 2],
}

/// Only the actual caller's current job/ticket may construct this expectation.
/// Validating a descriptor is not authorization to attach a renderer or final.
pub(super) fn validate_partial_metadata(
    expected: &PartialExpectation<'_>,
    info: &solidcraft_engine::render::job::PartialPreviewInfo,
    png_len: usize,
    covered_len: usize,
    previous: Option<PartialSampling>,
) -> Result<Option<PartialSampling>, String> {
    use solidcraft_engine::render::job::{AlphaConvention, ColourEncoding, PartialPreviewPlan, PartialPreviewProjection};
    let invalid = || crate::i18n::tr("render.error_provisional_identity").to_string();
    let progressive = expected.request.raytrace.as_ref().and_then(|options| options.progressive).ok_or_else(invalid)?;
    progressive
        .validate_image_with_budget(expected.request.width, expected.request.height, expected.request.cpu_budget())
        .map_err(|error| error.to_string())?;
    let plan = PartialPreviewPlan::new(expected.request.width, expected.request.height).map_err(|error| error.to_string())?;
    plan.validate().map_err(|error| error.to_string())?;
    let source_pixels = u64::from(plan.source_width()) * u64::from(plan.source_height());
    if expected.request.integrator != Integrator::Raytrace
        || expected.request.transparent_background
        || expected.final_dimensions.contains(&0)
        || info.frame.job != expected.job
        || info.frame.scene_generation != expected.scene_generation
        || info.frame.executor.as_str() != "cpu-raytrace"
        || info.frame.encoding != ColourEncoding::Srgb
        || info.frame.alpha != AlphaConvention::Straight
        || (info.source_width, info.source_height) != (plan.source_width(), plan.source_height())
        || (info.frame.width, info.frame.height) != (plan.width(), plan.height())
        || info.projection != PartialPreviewProjection::TopLeftRepresentativeV1
        || u64::try_from(covered_len).map_err(|_| invalid())? != plan.pixels()
        || info.frame.png_bytes != u64::try_from(png_len).map_err(|_| invalid())?
        || info.frame.png_bytes > plan.encoded_limit().map_err(|error| error.to_string())?
        || info.frame.png_sha256.len() != 64
        || !info.frame.png_sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || (info.completed_passes == 0 && info.updated_prefix == 0)
    {
        return Err(invalid());
    }
    // This validates the global frontier even when reduced representatives hide
    // the last unobserved source pixel. No fake uniform SPP from the preview size.
    partial_sample_count(info.completed_passes, info.updated_prefix, 0, source_pixels, progressive.samples_per_pixel)?;
    let cursor = PartialCursor { sequence: info.sequence, completed_passes: info.completed_passes, updated_prefix: info.updated_prefix };
    if cursor.sequence == 0 {
        return Err(invalid());
    }
    if let Some(old) = previous {
        if old.plan != plan {
            return Err(invalid());
        }
        if cursor.sequence <= old.cursor.sequence {
            return Ok(None);
        }
    }
    if !partial_advances(previous.map(|old| old.cursor), cursor) {
        return Err(invalid());
    }
    Ok(Some(PartialSampling { plan, cursor }))
}

/// Bounded O(M) borrowed-sidecar check. The shared plan supplies every source index.
pub(super) fn validate_partial_pixels(
    sampling: PartialSampling,
    target: u32,
    covered: &[u32],
    image: &egui::ColorImage,
    background: [u8; 3],
) -> Result<(), String> {
    let invalid = || crate::i18n::tr("render.error_provisional_identity").to_string();
    if image.size != [sampling.plan.width() as usize, sampling.plan.height() as usize]
        || u64::try_from(image.pixels.len()).map_err(|_| invalid())? != sampling.plan.pixels()
        || covered.len() != image.pixels.len()
    {
        return Err(invalid());
    }
    let source_pixels = u64::from(sampling.plan.source_width()) * u64::from(sampling.plan.source_height());
    for (index, (pixel, coverage)) in image.pixels.iter().zip(covered).enumerate() {
        let source_index = sampling.plan.source_index(index).map_err(|error| error.to_string())?;
        let count = partial_sample_count(
            sampling.cursor.completed_passes,
            sampling.cursor.updated_prefix,
            u64::try_from(source_index).map_err(|_| invalid())?,
            source_pixels,
            target,
        )?;
        validate_partial_pixel(count, *coverage, *pixel, background)?;
    }
    Ok(())
}

pub(super) fn decode_partial_image(
    request: &solidcraft_engine::render::job::ImageRequest,
    info: &solidcraft_engine::render::job::PartialPreviewInfo,
    png: &[u8],
    covered: &[u32],
    sampling: PartialSampling,
    max_texture_side: usize,
) -> Result<egui::ColorImage, String> {
    let invalid = || crate::i18n::tr("render.error_provisional_identity").to_string();
    if (info.frame.width, info.frame.height) != (sampling.plan.width(), sampling.plan.height())
        || super::display_dimensions(info.frame.width, info.frame.height, max_texture_side)? != (info.frame.width, info.frame.height)
    {
        return Err(crate::i18n::tr("render.error_display_fit").into());
    }
    let image = super::decode_render_image(png, info.frame.width, info.frame.height, super::captured_image_limits(request), max_texture_side)?;
    let target = request.raytrace.as_ref().and_then(|options| options.progressive).ok_or_else(invalid)?.samples_per_pixel;
    validate_partial_pixels(sampling, target, covered, &image, request.background)?;
    Ok(image)
}

pub(super) fn partial_request_due(previous: Option<(JobId, Duration)>, job: JobId, now: Duration) -> bool {
    previous.is_none_or(|(old, when)| old != job || now.saturating_sub(when) >= Duration::from_millis(250))
}

fn capture_dimensions(request: &Value) -> Result<[u32; 2], String> {
    let invalid = || crate::i18n::tr("render.error_provisional_identity").to_string();
    let settings = request.get("settings").ok_or_else(invalid)?;
    let dimension = |key: &str| {
        settings.get(key).and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()).filter(|value| *value != 0).ok_or_else(invalid)
    };
    Ok([dimension("width")?, dimension("height")?])
}

/// Frozen with the real issued ticket; never refreshed to bless paused pixels.
/// A weak prepared-world identity does not retain CAD or prepare cold placements.
struct GeometryAlignment {
    ticket: CaptureTicket,
    uid: u64,
    world: std::sync::Weak<solidcraft_engine::doc::ModelState>,
    document: std::sync::Weak<solidcraft_engine::doc::Document>,
    visibility: solidcraft_engine::doc::Visibility,
    pending_moves: std::collections::BTreeMap<u64, solidcraft_engine::doc::Mat>,
    active_component: u64,
    active_occurrence_path: Option<Vec<u64>>,
    active_sketch: Option<u64>,
}

impl GeometryAlignment {
    #[cfg(not(target_arch = "wasm32"))]
    fn capture(app: &SolveApp, ticket: CaptureTicket) -> Option<Self> {
        if !geometry_context_supported(app) {
            return None;
        }
        let world = app.session.interactive_cached_world_state()?;
        Some(Self {
            ticket,
            uid: app.session.uid,
            world: std::sync::Arc::downgrade(&world),
            document: std::sync::Arc::downgrade(&app.session.doc),
            visibility: app.session.visibility.clone(),
            pending_moves: app.session.pending_moves.clone(),
            active_component: app.session.active_component,
            active_occurrence_path: app.session.active_occurrence_path.clone(),
            active_sketch: app.session.active_sketch,
        })
    }

    fn is_current(&self, app: &SolveApp, job: JobId, revision: VisualRevision) -> bool {
        self.ticket.job_id() == job
            && self.ticket.revision() == revision
            && geometry_context_supported(app)
            && self.uid == app.session.uid
            && self.document.upgrade().is_some_and(|accepted| {
                // Render/appearance revisions may differ. Only ownership, placement
                // and folder membership metadata can change the pickable scene.
                std::sync::Arc::ptr_eq(&accepted, &app.session.doc)
                    || (accepted.marker == app.session.doc.marker
                        && accepted.occurrences == app.session.doc.occurrences
                        && accepted.body_components == app.session.doc.body_components
                        && accepted.body_offsets == app.session.doc.body_offsets
                        && accepted.browser_groups == app.session.doc.browser_groups
                        && accepted.components.iter().map(|component| (component.id, component.parent)).eq(app
                            .session
                            .doc
                            .components
                            .iter()
                            .map(|component| (component.id, component.parent))))
            })
            && self.visibility == app.session.visibility
            && self.pending_moves == app.session.pending_moves
            && self.active_component == app.session.active_component
            && self.active_occurrence_path == app.session.active_occurrence_path
            && self.active_sketch == app.session.active_sketch
            && app
                .session
                .interactive_cached_world_state()
                .is_some_and(|world| std::sync::Weak::ptr_eq(&self.world, &std::sync::Arc::downgrade(&world)))
    }
}

/// These paths can substitute pickable geometry outside the accepted product
/// world. Without a cache-only shown-world authority they cannot certify input.
fn geometry_context_supported(app: &SolveApp) -> bool {
    !app.session.model.is_stale()
        && app.session.active_sketch.is_none()
        && app.open_meshing.is_none()
        && !app.open_meshing_next
        && app.session.section.is_none()
        && app.render_workspace.view.draft.stage.is_empty()
        && !app.preview.active
        && !app.preview.busy
        && !app.preview.rolled_back
        && app.preview.bounds.is_empty()
        && app.preview.effective_replaced().is_empty()
}

struct Cached {
    key: PixelKey,
    revision: VisualRevision,
    info: FrameInfo,
    samples: u32,
    target_samples: u32,
    partial: Option<PartialSampling>,
    final_dimensions: [u32; 2],
    // Accepted job camera, never the latest observed intent after Pause.
    camera: solidcraft_engine::render::Camera,
    geometry: Option<std::sync::Arc<GeometryAlignment>>,
    stale: bool,
    texture: Result<egui::TextureHandle, String>,
}

pub(super) struct State {
    context: Option<InteractiveRender>,
    pub(super) pending: Option<Pending>,
    issued: Option<CaptureTicket>,
    issued_dimensions: Option<[u32; 2]>,
    issued_geometry: Option<std::sync::Arc<GeometryAlignment>>,
    partial_request: Option<(JobId, Duration)>,
    partial_exhausted: bool,
    protected_feedback_snapshot: Option<(JobId, solidcraft_engine::render::job::ProtectedPreviewReport)>,
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
            issued_dimensions: None,
            issued_geometry: None,
            partial_request: None,
            partial_exhausted: false,
            protected_feedback_snapshot: None,
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
        self.issued_dimensions = None;
        self.issued_geometry = None;
        self.partial_request = None;
        self.partial_exhausted = false;
        self.protected_feedback_snapshot = None;
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
        self.issued_dimensions = None;
        self.issued_geometry = None;
        self.partial_request = None;
        self.partial_exhausted = false;
        self.protected_feedback_snapshot = None;
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
            self.issued_dimensions = None;
            self.issued_geometry = None;
            self.partial_request = None;
            self.partial_exhausted = false;
            self.protected_feedback_snapshot = None;
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
            self.issued_dimensions = Some(capture_dimensions(request)?);
            self.issued_geometry = GeometryAlignment::capture(app, ticket).map(std::sync::Arc::new);
            self.partial_request = None;
            self.partial_exhausted = false;
            self.protected_feedback_snapshot = None;
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
        // Demand is advisory, not capture authority. False attempts still throttle.
        if !status.progress.phase.terminal() && partial_request_due(self.partial_request, job.id(), self.clock) {
            self.partial_request = Some((job.id(), self.clock));
            let _ = job.request_partial_preview().map_err(|error| error.to_string())?;
        }
        // Report counters belong to this actual admitted job. None is not a
        // theoretical zero ledger; old/shared-cap routes retain their own cause.
        let report = job.protected_preview_report().map_err(|error| error.to_string())?;
        self.protected_feedback_snapshot = report.map(|report| (job.id(), report));
        self.partial_exhausted =
            if let Some(report) = report { report.quota_exhausted } else { job.partial_preview_exhausted().map_err(|error| error.to_string())? };
        let partial = job.partial_preview().map_err(|error| error.to_string())?;
        // Read the real terminal after the partial slot: completion can race the
        // earlier status. Terminal16 stays provisional, never a final export.
        if let Some(frame) = job.frame().map_err(|error| error.to_string())? {
            let key = PixelKey { job: job.id(), sequence: 0, complete: true };
            let samples = status.request.raytrace.as_ref().and_then(|options| options.progressive).map_or(0, |options| options.samples_per_pixel);
            self.cache_png(ctx, key, ticket.revision(), &frame.info, samples, (frame.png(), &status.request))?;
        } else if let Some(partial) = partial {
            self.cache_partial_png(ctx, &job, ticket, &status.request, &partial)?;
        } else if let Some(preview) = job.preview().map_err(|error| error.to_string())? {
            let key = PixelKey { job: job.id(), sequence: preview.info.frame_sequence, complete: false };
            self.cache_png(ctx, key, ticket.revision(), &preview.info.frame, preview.info.samples_per_pixel, (preview.png(), &status.request))?;
        }
        Ok(())
    }
    fn cache_partial_png(
        &mut self,
        ctx: &egui::Context,
        job: &solidcraft_engine::render::job::JobHandle,
        ticket: CaptureTicket,
        request: &solidcraft_engine::render::job::ImageRequest,
        publication: &solidcraft_engine::render::job::RenderPartialPreview,
    ) -> Result<(), String> {
        let invalid = || crate::i18n::tr("render.error_provisional_identity").to_string();
        let final_dimensions = self.issued_dimensions.ok_or_else(invalid)?;
        let expected = PartialExpectation { job: job.id(), scene_generation: ticket.revision().scene_generation, request, final_dimensions };
        let previous = self.frame.as_ref().filter(|old| old.key.job == job.id() && old.revision == ticket.revision()).and_then(|old| old.partial);
        let Some(sampling) = validate_partial_metadata(&expected, &publication.info, publication.png().len(), publication.covered().len(), previous)?
        else {
            return Ok(());
        };
        let info = &publication.info.frame;
        let image = decode_partial_image(
            request,
            &publication.info,
            publication.png(),
            publication.covered(),
            sampling,
            ctx.input(|input| input.max_texture_side),
        )?;
        // Decoding can overlap worker cancellation/completion. Do not adopt a
        // late partial, and leave a real terminal for the next normal refresh.
        let status = job.status().map_err(|error| error.to_string())?;
        if status.progress.phase.terminal()
            || self.issued != Some(ticket)
            || self.revision() != Some(ticket.revision())
            || self.context.as_ref().ok_or_else(invalid)?.current().map_err(|error| error.to_string())?.is_none_or(|current| current.id() != job.id())
        {
            return Ok(());
        }
        let texture = ctx.load_texture("sc_render_interactive_partial", image, egui::TextureOptions::NEAREST);
        self.frame = Some(Cached {
            key: PixelKey { job: job.id(), sequence: 0, complete: false },
            revision: ticket.revision(),
            info: info.clone(),
            samples: 0,
            target_samples: request.raytrace.as_ref().and_then(|options| options.progressive).ok_or_else(invalid)?.samples_per_pixel,
            partial: Some(sampling),
            final_dimensions,
            camera: request.camera,
            geometry: self.issued_geometry.clone(),
            stale: false,
            texture: Ok(texture),
        });
        // The cache retains neither source Arc, PNG nor covered sidecar. Ending
        // this borrow is not a credit ACK: host/external owners may still retain it.
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
        let progressive = request
            .raytrace
            .as_ref()
            .and_then(|options| options.progressive)
            .ok_or_else(|| crate::i18n::tr("render.error_provisional_identity").to_string())?;
        progressive.validate_image_with_budget(request.width, request.height, request.cpu_budget()).map_err(|error| error.to_string())?;
        if self.frame.as_ref().is_some_and(|frame| frame.partial.is_none() && frame.key == key && frame.revision == revision) {
            return Ok(());
        }
        if info.job != key.job
            || info.scene_generation != revision.scene_generation
            || !(1..=progressive.samples_per_pixel).contains(&samples)
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
        self.frame = Some(Cached {
            key,
            revision,
            info: info.clone(),
            samples,
            target_samples: progressive.samples_per_pixel,
            partial: None,
            final_dimensions: self.issued_dimensions.ok_or_else(|| crate::i18n::tr("render.error_provisional_identity").to_string())?,
            camera: request.camera,
            geometry: self.issued_geometry.clone(),
            stale: false,
            texture,
        });
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
                state.issued_dimensions = None;
                state.issued_geometry = None;
                state.partial_request = None;
                state.partial_exhausted = false;
                state.protected_feedback_snapshot = None;
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
                state.issued_dimensions = None;
                state.issued_geometry = None;
                state.partial_request = None;
                state.partial_exhausted = false;
                state.protected_feedback_snapshot = None;
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

/// Reserve the canvas while an active CPU preview awaits its current pixels.
/// This never turns a waiting state into a painted image or retains old pixels.
pub(super) fn awaiting_current_frame(app: &SolveApp) -> bool {
    let state = &app.render_workspace.interactive;
    super::active(app)
        && app.render_workspace.view.method == ImageMethod::CpuRaytrace
        && !app.render_workspace.view.result
        && state.context.as_ref().is_some_and(|context| context.is_current(&app.session))
        && !state.user_paused
        && !state.final_active
        && state.error.is_none()
        && matches!(state.phase, Phase::Updating | Phase::Refining)
        && state.frame.is_none()
}

/// Pausing an Unknown source must not restore invisible geometry hit targets.
/// Pure UI visibility policy, shared by pixel paint and the Unknown-only guard.
/// These flags describe an existing cached frame, never a renderer/job capability.
#[derive(Clone, Copy)]
pub(super) struct FrameVisibility {
    pub(super) has_context: bool,
    pub(super) final_active: bool,
    pub(super) has_error: bool,
    pub(super) paused: bool,
    pub(super) stale: bool,
    pub(super) revision_matches: bool,
    pub(super) texture_ok: bool,
}

impl FrameVisibility {
    pub(super) fn paintable(self) -> bool {
        self.has_context && !self.final_active && !self.has_error && self.texture_ok && (self.paused || (!self.stale && self.revision_matches))
    }

    pub(super) fn blocks_unknown(self, source_unknown: bool) -> bool {
        source_unknown && self.paintable()
    }
}

fn frame_visibility(state: &State, frame: &Cached) -> FrameVisibility {
    FrameVisibility {
        has_context: state.context.is_some(),
        final_active: state.final_active,
        has_error: state.error.is_some(),
        paused: state.user_paused,
        stale: frame.stale,
        revision_matches: state.revision() == Some(frame.revision),
        texture_ok: frame.texture.is_ok(),
    }
}

fn input_frame(app: &SolveApp) -> Option<&Cached> {
    let state = &app.render_workspace.interactive;
    (super::active(app)
        && app.render_workspace.view.method == ImageMethod::CpuRaytrace
        && !app.render_workspace.view.result
        && state.context.as_ref().is_some_and(|context| context.is_current(&app.session)))
    .then_some(state.frame.as_ref())
    .flatten()
}

/// Match accepted camera and authored aspect without using the latest visual
/// revision: a paused appearance/light edit alone need not move geometry targets.
fn camera_framing_matches(app: &SolveApp, frame: &Cached) -> bool {
    let [width, height] = frame.final_dimensions;
    let draft = &app.render_workspace.view.draft;
    frame.camera == app.cam
        && width != 0
        && height != 0
        && draft.width != 0
        && draft.height != 0
        && u64::from(width) * u64::from(draft.height) == u64::from(height) * u64::from(draft.width)
}

pub(super) fn blocks_geometry_input(app: &SolveApp) -> bool {
    awaiting_current_frame(app)
        || input_frame(app).is_some_and(|frame| {
            let visibility = frame_visibility(&app.render_workspace.interactive, frame);
            visibility.blocks_unknown(frame.partial.is_some_and(|sampling| sampling.source_unknown()))
                || (visibility.paintable()
                    && (!camera_framing_matches(app, frame)
                        || frame.geometry.as_ref().is_none_or(|geometry| !geometry.is_current(app, frame.key.job, frame.revision))))
        })
}

/// Use exactly the painted pixel rectangle for CAD projection and pointer bounds.
/// Shared navigation keeps its existing viewport; no cached camera is installed.
pub(super) fn geometry_input_rect(app: &SolveApp, rect: egui::Rect) -> egui::Rect {
    input_frame(app)
        .filter(|frame| frame_visibility(&app.render_workspace.interactive, frame).paintable())
        .and_then(|frame| pixel_rect(&frame.info, rect))
        .unwrap_or(rect)
}

fn pixel_rect(info: &FrameInfo, rect: egui::Rect) -> Option<egui::Rect> {
    if info.width == 0 || info.height == 0 || !rect.is_finite() || !rect.is_positive() {
        return None;
    }
    let scale = (rect.width() / info.width as f32).min(rect.height() / info.height as f32);
    let target = egui::Rect::from_center_size(rect.center(), egui::vec2(info.width as f32, info.height as f32) * scale);
    (target.is_finite() && target.is_positive()).then_some(target)
}

fn paint_unknown_underlay(ui: &egui::Ui, rect: egui::Rect) {
    let tokens = Tokens::get();
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, tokens.file_background);
    // Fixed64 theme cells independent of source pixel count; no model or final fill.
    for y in 0..8 {
        for x in 0..8 {
            if (x + y) % 2 == 0 {
                let min = rect.min + egui::vec2(rect.width() * x as f32 / 8.0, rect.height() * y as f32 / 8.0);
                let max = rect.min + egui::vec2(rect.width() * (x + 1) as f32 / 8.0, rect.height() * (y + 1) as f32 / 8.0);
                painter.rect_filled(egui::Rect::from_min_max(min, max), 0.0, tokens.file_border);
            }
        }
    }
}

pub(super) fn paint(app: &SolveApp, ui: &egui::Ui, rect: egui::Rect) -> bool {
    let state = &app.render_workspace.interactive;
    let Some(frame) = &state.frame else { return false };
    if !frame_visibility(state, frame).paintable() {
        return false;
    }
    let Ok(texture) = &frame.texture else { return false };
    let Some(target) = pixel_rect(&frame.info, rect) else { return false };
    if frame.partial.is_some_and(|sampling| sampling.source_unknown()) {
        paint_unknown_underlay(ui, target);
    }
    ui.painter().with_clip_rect(rect).image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Tokens::get().file_image_tint,
    );
    true
}

/// Place only our card in current-frame space; never move another workspace surface.
pub(super) fn overlay_bounds(app: &SolveApp, free: egui::Rect) -> egui::Rect {
    if !free.is_finite() || !free.is_positive() {
        return egui::Rect::NOTHING;
    }
    let gap = Tokens::get().space_8;
    let available = crate::shell::workspace_content(app, free).shrink(gap);
    let mut obstacles = Vec::new();
    for (id, (rect, edge)) in &app.shell.workspace_panels {
        if *id != "render:interactive-card" && edge.is_none() {
            obstacles.push(rect.expand(gap));
        }
    }
    if let Some(layout) = &app.viewport.layout {
        obstacles.extend([layout.axis_view, layout.navigation, layout.confirm].map(|rect| rect.expand(gap)));
    }
    crate::viewport_layout::free_rect(available, &obstacles)
}

pub(crate) fn overlay(app: &mut SolveApp, ui: &egui::Ui, free: egui::Rect) {
    let bounds = overlay_bounds(app, free);
    let t = Tokens::get();
    // Match navigation_card's real chrome, including theme-dependent borders.
    let chrome = ui_kit::Container::new(ui.ctx(), ui_kit::Surface::Navigation)
        .frame(ui.ctx(), t.overlay, crate::frame::Side::Left)
        .inner_margin(t.space_4)
        .total_margin()
        .sum();
    let width = (bounds.width() - chrome.x).min(t.form_card_width);
    let height = bounds.height() - chrome.y;
    if !bounds.is_finite() || width < t.control_height || height < t.control_height {
        return;
    }
    let card = ui_kit::Container::navigation_card(
        ui.ctx(),
        egui::Id::new("sc_render_interactive_controls"),
        bounds.left_top(),
        egui::Align2::LEFT_TOP,
        |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            ui.set_max_height(height);
            ui.set_clip_rect(ui.clip_rect().intersect(bounds));
            controls(app, ui);
        },
    );
    crate::shell::register_panel(app, "render:interactive-card", Some(card.response.rect.intersect(bounds)), None);
    crate::scenario::publish_rect("render:interactive-card", card.response.rect.intersect(bounds));
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
    let partial = state.frame.as_ref().and_then(|frame| frame.partial);
    let samples = state.frame.as_ref().and_then(|frame| frame.partial.map_or(Some(frame.samples), |sampling| sampling.uniform_samples()));
    let target_samples = state.frame.as_ref().map(|frame| frame.target_samples);
    let dimensions = state.frame.as_ref().map(|frame| (frame.info.width, frame.info.height));
    let stale = state.frame.as_ref().is_some_and(|frame| frame.stale);
    let final_active = state.final_active;
    let final_dimensions = state.frame.as_ref().map(|frame| frame.final_dimensions);
    let partial_exhausted = state.partial_exhausted;
    let protected_feedback =
        state.protected_feedback_snapshot.filter(|(job, _)| state.issued.is_some_and(|ticket| ticket.job_id() == *job)).map(|(_, report)| report);
    let reason = ready(app).err();
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
    // Actions remain outside the scroller. In a short free rectangle, status
    // text may scroll but must not push Pause/Resume/Off below the visible clip.
    let height = ui.available_height().max(0.0);
    if height > 0.0 {
        egui::ScrollArea::vertical()
            .id_salt("sc_render_interactive_controls_scroll")
            .min_scrolled_height(0.0)
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui_kit::text(ui, &label, ui_kit::Text::Inline);
                if running {
                    ui_kit::text(ui, app.tr("render.interactive_provisional"), ui_kit::Text::Note("render.interactive_provisional"));
                }
                if let Some(sampling) = partial {
                    if let Some(target) = target_samples {
                        let label = crate::i18n::format(
                            "render.cpu_pass_progress",
                            &[("current", &sampling.cursor.completed_passes.to_string()), ("target", &target.to_string())],
                        );
                        ui_kit::text(ui, &label, ui_kit::Text::Inline);
                    }
                    let (min, max) = sampling.sample_range();
                    let pixels = u64::from(sampling.plan.source_width()) * u64::from(sampling.plan.source_height());
                    let label = crate::i18n::format(
                        "render.interactive_partial_samples",
                        &[
                            ("min", &min.to_string()),
                            ("max", &max.to_string()),
                            ("known", &sampling.sampled_source_pixels().to_string()),
                            ("pixels", &pixels.to_string()),
                        ],
                    );
                    ui_kit::text(ui, &label, ui_kit::Text::Inline);
                } else if let (Some(samples), Some(target)) = (samples, target_samples) {
                    let label = crate::i18n::format("render.interactive_samples", &[("done", &samples.to_string()), ("total", &target.to_string())]);
                    ui_kit::text(ui, &label, ui_kit::Text::Inline);
                }
                if let Some(sampling) = partial {
                    if let (Some((width, height)), Some(final_dimensions)) = (dimensions, final_dimensions) {
                        let label = crate::i18n::format(
                            "render.interactive_partial_dimensions",
                            &[
                                ("display_width", &width.to_string()),
                                ("display_height", &height.to_string()),
                                ("source_width", &sampling.plan.source_width().to_string()),
                                ("source_height", &sampling.plan.source_height().to_string()),
                                ("final_width", &final_dimensions[0].to_string()),
                                ("final_height", &final_dimensions[1].to_string()),
                            ],
                        );
                        ui_kit::text(ui, &label, ui_kit::Text::Inline);
                    }
                } else if let Some((width, height)) = dimensions {
                    ui_kit::text(ui, &format!("{width} × {height} px"), ui_kit::Text::Inline);
                }
                if let Some(report) = protected_feedback.filter(|_| matches!(phase, Phase::Updating | Phase::Refining)) {
                    let label = crate::i18n::format(
                        "render.interactive_feedback_usage",
                        &[("started", &report.accepted_snapshots.to_string()), ("limit", &report.quota_limit.to_string())],
                    );
                    ui_kit::text(ui, &label, ui_kit::Text::Inline);
                    ui_kit::text(ui, app.tr("render.interactive_feedback_policy"), ui_kit::Text::Note("render.interactive_feedback_policy"));
                    let reason = if report.quota_exhausted {
                        Some("render.interactive_feedback_quota_exhausted")
                    } else if report.milestones_complete {
                        Some("render.interactive_feedback_milestones_complete")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        ui_kit::text(ui, app.tr(reason), ui_kit::Text::Note(reason));
                    }
                } else if protected_feedback.is_none() && partial_exhausted && phase != Phase::TargetReached {
                    ui_kit::text(ui, app.tr("render.interactive_partial_exhausted"), ui_kit::Text::Note("render.interactive_partial_exhausted"));
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
                if let Some(reason) = &reason {
                    ui_kit::text(ui, reason, ui_kit::Text::Note("render.interactive_requires_cpu"));
                }
            });
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
        "samples":state.frame.as_ref().and_then(|frame|frame.partial.map_or(Some(frame.samples), |sampling|sampling.uniform_samples())),
        "partial":state.frame.as_ref().and_then(|frame|frame.partial).map(|sampling|json!({
            "sequence":sampling.cursor.sequence,"completed_passes":sampling.cursor.completed_passes,"updated_prefix":sampling.cursor.updated_prefix,
            "source_width":sampling.plan.source_width(),"source_height":sampling.plan.source_height(),
            "sample_min":sampling.sample_range().0,"sample_max":sampling.sample_range().1,
            "source_sampled_pixels":sampling.sampled_source_pixels(),"source_unknown":sampling.source_unknown()
        })),
        "final_dimensions":state.frame.as_ref().map(|frame|frame.final_dimensions),
        // Diagnostic snapshot only, never a terminal audit or a new JobStatus field.
        "protected_feedback_snapshot":state.protected_feedback_snapshot.map(|(job, report)|json!({"job":job,"report":report})),
        "partial_exhausted":state.partial_exhausted,"stale":state.frame.as_ref().is_some_and(|frame|frame.stale),
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
