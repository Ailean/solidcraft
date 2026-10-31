//! Opt-in opaque direct image integration; immutable world geometry, one host reservation.
use super::anisotropic_faces::{self, PlanarTarget};
use super::bvh::TraceCursor;
use super::dielectric::{self, Certificate, MAX_BOUNDARIES, MAX_OVERRIDES, MAX_PATHS};
use super::lighting::{DirectionalLight, Lighting, StudioPreset, map_to_display};
use super::prepared::{PreparedPrimitive, PreparedScene, SharedHit};
use super::progressive::{self, Progressive};
use super::reflection;
use super::{BuildLimits, Bvh, BvhBuilder, Ray, TraceError, Triangle, WorkBudget};
use crate::job::{self, Admission, CancelToken, ErrorCode, ImageRequest, JobError, Phase, Progress, RaytraceOptions, TaskPoll};
use crate::{
    ProductScene,
    film::primary::PrimaryFilm,
    surface::{OpaqueSurface, srgb8_to_linear},
};
use solidcraft_geom::{Vec3, srgb::encode8};

#[cfg(test)]
#[path = "environment_image_tests.rs"]
mod environment_image_tests;

#[cfg(test)]
#[path = "environment_reflection_tests.rs"]
mod environment_reflection_tests;

#[cfg(test)]
#[path = "anisotropic_reflection_image_tests.rs"]
mod anisotropic_reflection_tests;

fn error(e: TraceError) -> JobError {
    let code = match e {
        TraceError::Cancelled => ErrorCode::Cancelled,
        TraceError::MemoryLimit | TraceError::WorkLimit => ErrorCode::LimitExceeded,
        TraceError::AllocationFailed => ErrorCode::AllocationFailed,
        TraceError::InvalidInput(_) => ErrorCode::InvalidScene,
        _ => ErrorCode::Internal,
    };
    JobError::new(code, e.to_string())
}
fn invalid(message: &'static str) -> JobError {
    JobError::new(ErrorCode::InvalidScene, message)
}
fn limits(budget: job::CpuBudget) -> BuildLimits {
    let limits = budget.limits();
    BuildLimits {
        max_triangles: limits.max_triangles as usize,
        max_memory_bytes: limits.max_job_bytes,
        max_work: limits.max_work_units,
        ..Default::default()
    }
}
fn lighting(options: &RaytraceOptions) -> job::Result<Lighting> {
    options.validate()?;
    if let Some(preset) = options.preset {
        let preset = match preset {
            job::LightingPreset::Neutral => StudioPreset::Neutral,
            job::LightingPreset::WarmCool => StudioPreset::WarmCool,
            job::LightingPreset::KeyRim => StudioPreset::KeyRim,
        };
        return Lighting::preset(preset).map_err(error);
    }
    match &options.lights {
        None => Ok(Lighting::studio_default()),
        Some(inputs) => {
            let mut lights = Vec::new();
            lights.try_reserve_exact(inputs.len()).map_err(|_| error(TraceError::AllocationFailed))?;
            for l in inputs {
                lights.push(
                    DirectionalLight::new(
                        Vec3::new(l.direction[0], l.direction[1], l.direction[2]),
                        Vec3::new(l.color[0], l.color[1], l.color[2]),
                        l.intensity,
                    )
                    .map_err(error)?,
                );
            }
            Lighting::new(lights).map_err(error)
        }
    }
}
type Look = PreparedPrimitive;

fn certificates(
    scene: &ProductScene,
    options: &RaytraceOptions,
    nested: bool,
) -> job::Result<([Option<Certificate>; MAX_OVERRIDES], [Option<usize>; MAX_OVERRIDES])> {
    let mut result = [None; MAX_OVERRIDES];
    for (slot, input) in result.iter_mut().zip(&options.dielectrics) {
        *slot = Some(dielectric::certify(scene, input).map_err(error)?);
    }
    let parents = dielectric::parents(&result, nested).map_err(error)?;
    // Sheets within volumes are unsupported. Conservatively use the admitted
    // tiny sheet's bounds, rather than pretending to resolve medium nesting.
    for (i, certificate) in result.iter().enumerate() {
        if let Some(Certificate::ClosedBox(a)) = certificate {
            for (j, other) in result.iter().enumerate() {
                if i != j && matches!(other, Some(Certificate::ThinSheet)) {
                    let input = options.dielectrics.get(j).ok_or_else(|| invalid("sheet override index"))?;
                    let body = scene.meshes.get(input.body_index).ok_or_else(|| invalid("sheet body index"))?;
                    let bounds = body.mesh.bounds();
                    if dielectric::overlaps(*a, super::Bounds::new(bounds.min, bounds.max).map_err(error)?) {
                        return Err(invalid("thin sheets touching or inside dielectric volumes are unsupported"));
                    }
                }
            }
        }
    }
    Ok((result, parents))
}

/// Private construction policy; protected admission is selected before allocations.
#[derive(Clone, Copy)]
enum PreviewPolicy {
    Legacy,
    Shared(job::PartialPreviewPlan),
    Protected(job::ProtectedPreviewPlan),
}
impl PreviewPolicy {
    fn partial_plan(self) -> Option<job::PartialPreviewPlan> {
        match self {
            Self::Legacy => None,
            Self::Shared(plan) => Some(plan),
            Self::Protected(plan) => Some(plan.preview_plan()),
        }
    }
}

fn committed_frontier(frontier: crate::film::primary::PartialFrontier) -> job::Result<u64> {
    let pixels = u64::try_from(frontier.pixels).map_err(|_| error(TraceError::MemoryLimit))?;
    let prefix = u64::try_from(frontier.updated_prefix).map_err(|_| error(TraceError::MemoryLimit))?;
    u64::from(frontier.completed_passes).checked_mul(pixels).and_then(|n| n.checked_add(prefix)).ok_or_else(|| error(TraceError::WorkLimit))
}

pub fn admit(scene: &ProductScene, request: &ImageRequest, options: &RaytraceOptions) -> job::Result<Admission> {
    admit_with_nested(scene, request, options, false)
}

pub(crate) fn admit_with_nested(scene: &ProductScene, request: &ImageRequest, options: &RaytraceOptions, nested: bool) -> job::Result<Admission> {
    admit_with_policy(scene, request, options, nested, PreviewPolicy::Legacy)
}

pub(crate) fn admit_with_partial_preview(
    scene: &ProductScene,
    request: &ImageRequest,
    options: &RaytraceOptions,
    nested: bool,
    plan: job::PartialPreviewPlan,
) -> job::Result<Admission> {
    admit_with_policy(scene, request, options, nested, PreviewPolicy::Shared(plan))
}

pub(crate) fn admit_with_protected_partial_preview(
    scene: &ProductScene,
    request: &ImageRequest,
    options: &RaytraceOptions,
    nested: bool,
    plan: job::ProtectedPreviewPlan,
) -> job::Result<Admission> {
    admit_with_policy(scene, request, options, nested, PreviewPolicy::Protected(plan))
}

fn admit_with_policy(
    scene: &ProductScene,
    request: &ImageRequest,
    options: &RaytraceOptions,
    nested: bool,
    policy: PreviewPolicy,
) -> job::Result<Admission> {
    if let PreviewPolicy::Protected(plan) = policy {
        plan.validate()?;
        let progressive =
            options.progressive.ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "protected feedback requires progressive CPU sampling"))?;
        if plan != job::ProtectedPreviewPlan::with_cadence(request.width, request.height, progressive.samples_per_pixel, options.budget, request.cpu_progressive_every_pass)? {
            return Err(JobError::new(ErrorCode::InvalidRequest, "protected feedback plan differs from actual source sampling or budget"));
        }
    }
    let partial = policy.partial_plan();
    if let Some(plan) = partial {
        plan.validate()?;
        if plan != job::PartialPreviewPlan::new(request.width, request.height)?
            || options.progressive.is_none()
            || request.environment.is_some()
            || request.textures.is_some()
            || scene.has_transmission()
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview requires the exact CPU progressive source plan"));
        }
    }
    options.validate()?;
    if options.directional_shadow_transmission && nested {
        return Err(invalid("directional shadow transmission does not support nested media"));
    }
    if let Some(original) = &request.raytrace {
        original.validate()?;
    }
    if request.raytrace.as_ref().unwrap_or(&RaytraceOptions::default()) != options {
        return Err(JobError::new(ErrorCode::InvalidRequest, "raytrace options disagree with image request"));
    }
    if request.integrator != job::Integrator::Raytrace {
        return Err(JobError::new(ErrorCode::InvalidRequest, "raytrace task requires raytrace integrator"));
    }
    if request.transparent_background {
        return Err(invalid("raytrace supports opaque backgrounds only"));
    }
    if let Some(progressive) = options.progressive {
        progressive.validate_image_with_budget(request.width, request.height, options.budget)?;
    }
    let mut a = job::image::admit_raytrace_source(scene, request, options.budget)?;
    let r = options.reflections.as_ref().filter(|r| !r.bodies.is_empty()).map_or(1, |r| r.samples);
    u64::from(request.width)
        .checked_mul(u64::from(request.height))
        .and_then(|n| n.checked_mul(u64::from(options.primary_samples)))
        .and_then(|n| n.checked_mul(u64::from(r)))
        .ok_or_else(|| invalid("spatial attempt count overflow"))?;
    for (index, body) in scene.meshes.iter().enumerate() {
        if body.opacity != 1.0 || body.faces.iter().any(|f| f.opacity != 1.0) {
            return Err(invalid("raytrace requires opaque body and face appearances"));
        }
        let has_coat = body.surface.is_some_and(|surface| surface.clear_coat().is_some())
            || body.faces.iter().any(|face| face.surface.is_some_and(|surface| surface.clear_coat().is_some()));
        let selected_transport = options.reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|entry| entry.body == body.name))
            || options.anisotropic_faces.iter().flatten().any(|entry| entry.body_index == index)
            || options.dielectrics.iter().any(|entry| entry.body_index == index);
        if has_coat && selected_transport {
            return Err(invalid("CPU reflection, anisotropy and dielectric overrides do not support clear-coat transport"));
        }
    }
    if let Some(reflections) = &options.reflections {
        for override_ in &reflections.bodies {
            let count = scene.meshes.iter().filter(|body| body.name == override_.body).count();
            if count != 1 {
                return Err(JobError::new(ErrorCode::InvalidRequest, "reflection body must resolve to exactly one captured body"));
            }
        }
    }
    anisotropic_faces::prepare(scene, options, || Ok(()))?;
    certificates(scene, options, nested)?;
    for input in &options.dielectrics {
        if options.reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|v| v.body == input.body_name))
            || options.anisotropic_faces.iter().flatten().any(|v| v.body_index == input.body_index)
            || options.emissions.iter().flatten().any(|v| v.body_index == input.body_index)
        {
            return Err(invalid("dielectric boundaries cannot also select opaque reflection, anisotropy or emission"));
        }
        if let Some(values) = &options.surface_effects {
            for value in values.iter().filter(|v| v.body == input.body_name) {
                if !value.prepare()?.is_identity() {
                    return Err(invalid("nonidentity surface effects on dielectric boundaries are unsupported"));
                }
            }
        }
    }
    if let Some(values) = &options.surface_effects {
        for value in values {
            let mut bodies = scene.meshes.iter().enumerate().filter(|(_, body)| body.name == value.body);
            let (index, _) = bodies.next().ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "surface effect body missing"))?;
            if bodies.next().is_some() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "surface effect body must resolve uniquely"));
            }
            if !value.prepare()?.is_identity() && options.anisotropic_faces.iter().flatten().any(|target| target.body_index == index) {
                return Err(JobError::new(ErrorCode::InvalidRequest, "surface effects on a certified anisotropic body are not supported"));
            }
        }
    }
    if let Some(values) = &options.emissions {
        for value in values {
            if scene.meshes.get(value.body_index).is_none_or(|body| body.name != value.body) {
                return Err(invalid("surface emission target does not match captured body index and name"));
            }
        }
    }
    // Bill each task-owned options copy and the prepared emission vector in addition
    // to the unchanged baseline estimate. Record metadata has its independent lease.
    let emission_bytes = options
        .retained_emission_bytes()?
        .checked_add(request.raytrace.as_ref().map_or(Ok(0), RaytraceOptions::retained_emission_bytes)?)
        .and_then(|v| {
            v.checked_add(options.emissions.as_ref().map_or(0, Vec::len) as u64 * std::mem::size_of::<(usize, super::emission::Emission)>() as u64)
        })
        .ok_or_else(|| error(TraceError::MemoryLimit))?;
    // The task owns two independent options copies plus its prepared effect vector.
    // The original request Record remains billed by the same retained_bytes sum.
    let effect_bytes = options
        .retained_surface_effect_bytes()?
        .checked_add(request.raytrace.as_ref().map_or(Ok(0), RaytraceOptions::retained_surface_effect_bytes)?)
        .and_then(|v| {
            v.checked_add(
                options.surface_effects.as_ref().map_or(0, Vec::len) as u64
                    * std::mem::size_of::<(usize, crate::surface_patterns::PreparedEffect)>() as u64,
            )
        })
        .ok_or_else(|| error(TraceError::MemoryLimit))?;
    let optical_bytes = options
        .retained_dielectric_bytes()?
        .checked_add(request.raytrace.as_ref().map_or(Ok(0), RaytraceOptions::retained_dielectric_bytes)?)
        .and_then(|v| v.checked_add(if options.dielectrics.is_empty() { 0 } else { std::mem::size_of::<OpticalTask>() as u64 }))
        .ok_or_else(|| error(TraceError::MemoryLimit))?;
    let n = usize::try_from(a.triangles).map_err(|_| error(TraceError::MemoryLimit))?;
    let film_bytes = if let Some(progressive) = options.progressive {
        let checkpoint = a.encoded_limit.checked_add(job::FRAME_METADATA_BYTES).ok_or_else(|| error(TraceError::MemoryLimit))?;
        let previews = if let Some(plan) = partial {
            plan.additional_bytes()?
        } else {
            u64::from(progressive.checkpoint_slots())
                .checked_mul(checkpoint)
                .and_then(|bytes| bytes.checked_add(if progressive.checkpoint_slots() == 0 { 0 } else { 1024 }))
                .ok_or_else(|| error(TraceError::MemoryLimit))?
        };
        PrimaryFilm::required_bytes(request.width, request.height, progressive, options.budget)?
            .checked_add(previews)
            .and_then(|v| v.checked_add(std::mem::size_of::<Progressive>() as u64))
            .ok_or_else(|| error(TraceError::MemoryLimit))?
    } else {
        0
    };
    let extra = BvhBuilder::required_bytes(n, n, limits(options.budget))
        .map_err(error)?
        .checked_add(a.triangles.checked_mul(std::mem::size_of::<Look>() as u64).ok_or_else(|| error(TraceError::MemoryLimit))?)
        .and_then(|v| v.checked_add(emission_bytes))
        .and_then(|v| v.checked_add(effect_bytes))
        .and_then(|v| v.checked_add(optical_bytes))
        .and_then(|v| v.checked_add(film_bytes))
        // The resumable pixel owns two traversal cursors. Its fallible, fixed
        // heap slot is additional to the unchanged host/task-state allowance.
        .and_then(|v| v.checked_add(PendingPixelSlot::required_bytes()))
        .and_then(|v| {
            v.checked_add(8192 + if options.reflections.is_some() || options.anisotropic_faces.iter().any(Option::is_some) { 32768 } else { 0 })
        })
        .and_then(|v| {
            v.checked_add(options.area_lights.as_ref().map_or(0, Vec::capacity) as u64 * std::mem::size_of::<job::RectangleLightOptions>() as u64)
        })
        .and_then(|v| {
            v.checked_add(options.punctual_lights.as_ref().map_or(0, Vec::capacity) as u64 * std::mem::size_of::<job::PunctualLightOptions>() as u64)
        })
        .ok_or_else(|| error(TraceError::MemoryLimit))?;
    // Keep the conservative raster pixel/codec estimate; add every new owned buffer.
    a.preparation_bytes = a.preparation_bytes.checked_add(extra).ok_or_else(|| error(TraceError::MemoryLimit))?;
    a.estimated_total_bytes = a.estimated_total_bytes.checked_add(extra).ok_or_else(|| error(TraceError::MemoryLimit))?;
    if a.estimated_total_bytes > options.budget.limits().max_job_bytes {
        return Err(error(TraceError::MemoryLimit));
    }
    Ok(a)
}

struct PendingPixel {
    surface_cursor: usize,
    surface_index: Option<usize>,
    surface_ready: bool,
    emission_cursor: usize,
    punctual_index: usize,
    punctual_shadow: bool,
    punctual_ready: bool,
    punctual_value: Vec3,
    area_light: usize,
    area_index: u32,
    area_shadow: bool,
    area_ready: bool,
    area_value: Vec3,

    /// Proof from the primary generator, revoked for any selected reflector.
    /// A jittered/time-varying primary generator MUST supply false.
    repeatable_primary: bool,
    environment_sample: u32,
    environment_sum: Vec3,
    environment_cursor: Option<TraceCursor>,
    environment_value: Vec3,
    ray: Ray,
    cursor: TraceCursor,
    hit: Option<super::Hit>,
    visibility: [bool; 8],
    directional_transmittance: [Vec3; 8],
    directional_complete: u8,
    directional_active: bool,
    directional_required: bool,
    light: usize,
    bounce: u8,
    throughput: Vec3,
    radiance: Vec3,
}

/// A commit/reset may abandon neither an active connection nor an unfinished rig.
fn ensure_directional_drained(pending: Option<&PendingPixel>, lights: usize) -> job::Result<()> {
    if lights > super::lighting::MAX_LIGHTS {
        return Err(invalid("directional light count exceeds fixed visibility storage"));
    }
    let required = ((1u16 << lights) - 1) as u8;
    if pending.is_some_and(|pending| pending.directional_active || (pending.directional_required && pending.directional_complete != required)) {
        return Err(invalid("directional visibility connection has not drained"));
    }
    Ok(())
}

fn begin_directional_shadow(
    owner: &mut dielectric::DirectVisibilityOwner,
    pending: &mut PendingPixel,
    ray: Ray,
    work: &mut WorkBudget,
    cancel: &CancelToken,
) -> job::Result<()> {
    if pending.light >= super::lighting::MAX_LIGHTS || pending.directional_active {
        return Err(invalid("invalid directional connection start"));
    }
    let bit = 1u8 << pending.light;
    if pending.directional_complete & bit != 0 {
        return Err(invalid("directional connection already completed"));
    }
    pending.directional_required = true;
    owner.begin(ray, dielectric::MAX_DIRECT_CROSSINGS, work, cancel).map_err(error)?;
    pending.directional_active = true;
    Ok(())
}

/// One reusable, fallibly allocated continuation. The Vec header stays in the
/// 4096-byte host slot; its exact single-element capacity is billed by admit.
/// Keeping an empty slot between samples avoids any per-ray allocation.
struct PendingPixelSlot(Vec<PendingPixel>);

impl PendingPixelSlot {
    fn required_bytes() -> u64 {
        std::mem::size_of::<PendingPixel>() as u64
    }

    fn new(cancel: &CancelToken) -> job::Result<Self> {
        cancel.check()?;
        let mut slot = Vec::new();
        slot.try_reserve_exact(1).map_err(|_| error(TraceError::AllocationFailed))?;
        if slot.capacity() != 1 {
            return Err(error(TraceError::MemoryLimit));
        }
        cancel.check()?;
        Ok(Self(slot))
    }

    fn as_ref(&self) -> Option<&PendingPixel> {
        self.0.first()
    }

    fn as_mut(&mut self) -> Option<&mut PendingPixel> {
        self.0.first_mut()
    }

    fn is_none(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    fn is_some(&self) -> bool {
        !self.is_none()
    }

    fn insert(&mut self, value: PendingPixel) -> job::Result<()> {
        if self.0.capacity() != 1 || !self.0.is_empty() {
            return Err(invalid("pending pixel slot is unavailable"));
        }
        // Capacity is already admitted and reserved: this cannot allocate.
        self.0.push(value);
        Ok(())
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn release(&mut self) {
        self.0 = Vec::new();
    }
}

#[derive(Clone, Copy)]
struct OpticalPath {
    ray: Ray,
    throughput: Vec3,
    medium: Option<usize>,
    /// Boundary or opaque reflection position, before the numerical spawn offset.
    segment_start: Vec3,
    boundaries: u8,
    /// Opaque reflection depth is independent of the dielectric interface count.
    bounce: u8,
}

struct DielectricPixel {
    /// A whole Fresnel tree belongs to one absolute camera sample. Fixed paths
    /// keep None; work quanta and presentation never enter this identity.
    camera_sample: Option<(usize, u32)>,
    path: OpticalPath,
    stack: [Option<OpticalPath>; MAX_BOUNDARIES as usize + 1],
    stack_len: usize,
    paths: u16,
    primary_check: usize,
    selector: usize,
    selected: Option<usize>,
    ready: bool,
}

/// At most2 crossings per certified convex box and1 per certified planar sheet
/// on a single straight camera segment. Equal indices preserve direction; no
/// selected opaque reflector may generate another segment. Certificates and
/// strict parent validation precede this bounded, allocation-free policy.
fn straight_boundary_limit(options: &RaytraceOptions, certificates: &[Option<Certificate>; MAX_OVERRIDES]) -> job::Result<Option<u8>> {
    if options.dielectrics.is_empty()
        || options.dielectrics.iter().any(|input| input.ior != 1.0)
        || options.reflections.as_ref().is_some_and(|r| !r.bodies.is_empty())
    {
        return Ok(None);
    }
    let mut limit = 0_u8;
    for (index, input) in options.dielectrics.iter().enumerate() {
        let certificate = certificates.get(index).copied().flatten().ok_or_else(|| invalid("missing straight optical certificate"))?;
        let crossings = match (input.boundary, certificate) {
            (job::DielectricBoundary::ClosedBox, Certificate::ClosedBox(_)) => 2,
            (job::DielectricBoundary::ThinSheet, Certificate::ThinSheet) => 1,
            _ => return Err(invalid("straight optical certificate disagrees with boundary")),
        };
        limit = limit.checked_add(crossings).filter(|n| *n <= 16).ok_or_else(|| invalid("straight optical crossing bound exceeds sixteen"))?;
    }
    Ok(Some(limit))
}

/// One fallibly reserved optional slot. Branches do not copy PendingPixel, own
/// lighting state, encode pixels, or allocate outside the existing job reservation.
struct OpticalTask {
    /// None retains the original eight-event branching model unchanged.
    straight_limit: Option<u8>,
    /// Captured runtime dielectric policy; fixed and progressive use the same transport.
    deterministic_tir: bool,
    /// Actual deterministic continuations CREATED, not completed rays or paths.
    tir_continuations: u64,
    certificates: [Option<Certificate>; MAX_OVERRIDES],
    parents: [Option<usize>; MAX_OVERRIDES],
    nested: bool,
    pending: Option<DielectricPixel>,
}

impl DielectricPixel {
    fn new(ray: Ray) -> Self {
        Self {
            path: OpticalPath { ray, throughput: Vec3::new(1.0, 1.0, 1.0), medium: None, segment_start: ray.origin(), boundaries: 0, bounce: 0 },
            camera_sample: None,
            stack: [None; MAX_BOUNDARIES as usize + 1],
            stack_len: 0,
            paths: 1,
            primary_check: 0,
            selector: 0,
            selected: None,
            ready: false,
        }
    }

    fn check_camera_sample(&self, actual: Option<(usize, u32)>) -> job::Result<()> {
        if self.camera_sample != actual {
            return Err(invalid("optical tree differs from its absolute camera sample"));
        }
        Ok(())
    }

    fn push(&mut self, path: OpticalPath) -> job::Result<()> {
        if !bounded_radiance(path.throughput) {
            return Err(invalid("dielectric branch throughput exceeds finite bound"));
        }
        if path.throughput == Vec3::ZERO {
            return Ok(());
        }
        if self.paths >= MAX_PATHS {
            return Err(error(TraceError::WorkLimit));
        }
        *self.stack.get_mut(self.stack_len).ok_or_else(|| error(TraceError::MemoryLimit))? = Some(path);
        self.stack_len += 1;
        self.paths += 1;
        Ok(())
    }

    fn advance(&mut self) -> job::Result<bool> {
        if self.stack_len == 0 {
            return Ok(false);
        }
        self.stack_len -= 1;
        self.path = self.stack.get_mut(self.stack_len).and_then(Option::take).ok_or_else(|| invalid("optical path stack"))?;
        self.selector = 0;
        self.selected = None;
        self.ready = false;
        Ok(true)
    }
}

/// Reset only per-hit transport state; retain the shared sum across Fresnel leaves.
fn resume_optical_path(pending: &mut PendingPixel, path: OpticalPath, lights: usize) -> job::Result<()> {
    ensure_directional_drained(Some(pending), lights)?;
    pending.ray = path.ray;
    pending.throughput = path.throughput;
    pending.bounce = path.bounce;
    pending.repeatable_primary = false;
    pending.cursor = TraceCursor::new(path.ray, false);
    pending.hit = None;
    pending.surface_cursor = 0;
    pending.surface_index = None;
    pending.surface_ready = false;
    pending.emission_cursor = 0;
    pending.light = 0;
    pending.visibility = [true; 8];
    pending.directional_transmittance = [Vec3::ZERO; 8];
    pending.directional_complete = 0;
    pending.directional_active = false;
    pending.directional_required = false;
    pending.punctual_index = 0;
    pending.punctual_shadow = false;
    pending.punctual_ready = false;
    pending.punctual_value = Vec3::ZERO;
    pending.area_light = 0;
    pending.area_index = 0;
    pending.area_shadow = false;
    pending.area_ready = false;
    pending.area_value = Vec3::ZERO;
    pending.environment_sample = 0;
    pending.environment_sum = Vec3::ZERO;
    pending.environment_cursor = None;
    pending.environment_value = Vec3::ZERO;
    Ok(())
}

pub struct ImageTask {
    punctual_diagnostics: job::PunctualDiagnostics,
    progressive: Option<Progressive>,
    last_pass_boundary: u32,
    optical: Vec<OpticalTask>,
    anisotropic_targets: [Option<PlanarTarget>; 2],
    environment: Option<crate::lighting::environment_input::EnvironmentState>,
    visibility_owner: dielectric::DirectVisibilityOwner,
    request: ImageRequest,
    cancel: CancelToken,
    admission: Admission,
    lights: Lighting,
    emissions: Vec<(usize, super::emission::Emission)>,
    effects: Vec<(usize, crate::surface_patterns::PreparedEffect)>,
    work: WorkBudget,
    clip: (f64, f64),
    triangles: Vec<Triangle>,
    looks: Vec<Look>,
    body: usize,
    triangle: usize,
    face_cursor: usize,
    chosen: Option<crate::FaceAppearance>,
    builder: Option<BvhBuilder>,
    bvh: Option<Bvh>,
    build_used: u64,
    pixels: Vec<u8>,
    pixel: usize,
    sample: u8,
    spatial: u8,
    primary_coverage: u16,
    spatial_radiance: Vec3,
    finish_spatial: bool,
    emit_spatial_pixel: bool,
    pixel_radiance: Vec3,
    pending: PendingPixelSlot,
    encoder: Option<job::encoding::PngEncoding>,
    done: bool,
}
// Same host slot: a future feature build must prove the actual target layout.
const _: [(); 1] = [(); (std::mem::size_of::<ImageTask>() <= 4096) as usize];

/// CPU continuations. Preview bytes are complete PNGs but acquire a transferable
/// host lease and identity only when the owning host publishes them.
pub enum ImagePoll {
    Pending(Progress),
    Preview { png: Vec<u8>, samples_per_pixel: u32 },
    Complete(Vec<u8>),
}

/// Additive host-only poll; the public legacy ImagePoll contract is unchanged.
pub(crate) enum PartialImagePoll {
    Pending(Progress),
    Complete(Vec<u8>),
    Preview { png: Vec<u8>, covered: Vec<u32>, completed_passes: u32, updated_prefix: u64 },
}

pub(crate) enum PartialBegin {
    Started,
    Exhausted,
}

enum ImageAdvance {
    Legacy(ImagePoll),
    Partial { png: Vec<u8>, covered: Vec<u32>, completed_passes: u32, updated_prefix: u64 },
}

impl ImageTask {
    pub fn punctual_diagnostics(&self) -> Option<job::PunctualDiagnostics> {
        (!self.punctual_diagnostics.is_empty()).then_some(self.punctual_diagnostics)
    }

    pub fn new(scene: ProductScene, request: ImageRequest, cancel: CancelToken, options: RaytraceOptions) -> job::Result<Self> {
        Self::new_with_environment(scene, request, cancel, options, None)
    }
    pub fn new_with_environment(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        input: Option<crate::lighting::environment_input::EnvironmentInput>,
    ) -> job::Result<Self> {
        Self::new_with_environment_and_nested(scene, request, cancel, options, input, false)
    }

    pub(crate) fn new_with_environment_and_nested(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        input: Option<crate::lighting::environment_input::EnvironmentInput>,
        nested: bool,
    ) -> job::Result<Self> {
        Self::new_with_policy(scene, request, cancel, options, input, nested, PreviewPolicy::Legacy)
    }

    /// Compatibility helper for the original five regression sources. Activate
    /// the real serialized production policy before ordinary admission/construction.
    #[cfg(test)]
    fn new_with_deterministic_tir(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        nested: bool,
    ) -> job::Result<Self> {
        let mut options = options;
        options.dielectric_transport = job::CpuDielectricTransport::StrictTir;
        let mut request = request;
        request.raytrace = Some(options.clone());
        Self::new_with_environment_and_nested(scene, request, cancel, options, None, nested)
    }

    pub(crate) fn new_with_environment_and_nested_and_partial(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        input: Option<crate::lighting::environment_input::EnvironmentInput>,
        nested: bool,
        plan: job::PartialPreviewPlan,
    ) -> job::Result<Self> {
        Self::new_with_policy(scene, request, cancel, options, input, nested, PreviewPolicy::Shared(plan))
    }

    pub(crate) fn new_with_environment_and_nested_and_protected_partial(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        input: Option<crate::lighting::environment_input::EnvironmentInput>,
        nested: bool,
        plan: job::ProtectedPreviewPlan,
    ) -> job::Result<Self> {
        Self::new_with_policy(scene, request, cancel, options, input, nested, PreviewPolicy::Protected(plan))
    }

    fn new_with_policy(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        options: RaytraceOptions,
        input: Option<crate::lighting::environment_input::EnvironmentInput>,
        nested: bool,
        policy: PreviewPolicy,
    ) -> job::Result<Self> {
        cancel.check()?;
        if options.directional_shadow_transmission && input.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "directional shadow transmission does not support HDR environments"));
        }
        let mut admission = admit_with_policy(&scene, &request, &options, nested, policy)?;
        let partial = policy.partial_plan();
        let mut optical = Vec::new();
        if !options.dielectrics.is_empty() {
            let (certificates, parents) = certificates(&scene, &options, nested)?;
            let straight_limit = straight_boundary_limit(&options, &certificates)?;
            optical.try_reserve_exact(1).map_err(|_| error(TraceError::AllocationFailed))?;
            if optical.capacity() != 1 {
                return Err(error(TraceError::MemoryLimit));
            }
            optical.push(OpticalTask {
                straight_limit,
                deterministic_tir: options.dielectric_transport == job::CpuDielectricTransport::StrictTir,
                tir_continuations: 0,
                certificates,
                parents,
                nested,
                pending: None,
            });
        }
        if input.is_some() {
            options.validate_environment_reflections(&scene)?;
        }
        let extra = input.as_ref().map_or(Ok(0), |e| e.required_bytes()).map_err(environment_error)?;
        admission.estimated_total_bytes = admission
            .estimated_total_bytes
            .checked_add(extra)
            .filter(|v| *v <= options.budget.limits().max_job_bytes)
            .ok_or_else(|| error(TraceError::MemoryLimit))?;
        admission.preparation_bytes = admission.preparation_bytes.checked_add(extra).ok_or_else(|| error(TraceError::MemoryLimit))?;
        let environment = input.map(|e| e.begin()).transpose().map_err(environment_error)?;
        let lights = lighting(&options)?;
        let mut work = match policy {
            PreviewPolicy::Protected(plan) => WorkBudget::new_protected(plan),
            PreviewPolicy::Legacy | PreviewPolicy::Shared(_) => WorkBudget::new(options.budget.limits().max_work_units),
        }
        .map_err(error)?;
        let anisotropic_targets = anisotropic_faces::prepare(&scene, &options, || work.tick(&cancel).map_err(error))?;
        let mut effects = Vec::new();
        if let Some(values) = &options.surface_effects {
            effects.try_reserve_exact(values.len()).map_err(|_| error(TraceError::AllocationFailed))?;
            if effects.capacity() != values.len() {
                return Err(error(TraceError::MemoryLimit));
            }
            for value in values {
                work.tick(&cancel).map_err(error)?;
                let effect = value.prepare()?;
                let index = scene
                    .meshes
                    .iter()
                    .position(|body| body.name == value.body)
                    .ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "surface effect body missing"))?;
                // Identity is omitted to preserve the existing pixel path and bytes.
                if !effect.is_identity() {
                    effects.push((index, effect));
                }
            }
        }
        let mut emissions = Vec::new();
        if let Some(values) = &options.emissions {
            emissions.try_reserve_exact(values.len()).map_err(|_| error(TraceError::AllocationFailed))?;
            if emissions.capacity() != values.len() {
                return Err(error(TraceError::MemoryLimit));
            }
            for value in values {
                work.tick(&cancel).map_err(error)?;
                // Skip zero terms to preserve the baseline floating-point path exactly.
                if value.strength != 0.0 && value.color.iter().any(|v| *v != 0.0) {
                    emissions.push((value.body_index, super::emission::Emission::new(value)?));
                }
            }
        }
        let radius =
            if scene.meshes.is_empty() { 10.0 } else { scene.bounds.diagonal().max(10.0) + scene.bounds.center().dist(request.camera.target) };
        if !radius.is_finite() {
            return Err(invalid("invalid product bounds"));
        }
        let clip = request.camera.clip(radius);
        let mut triangles = Vec::new();
        let mut looks = Vec::new();
        let mut pixels = Vec::new();
        triangles.try_reserve_exact(admission.triangles as usize).map_err(|_| error(TraceError::AllocationFailed))?;
        looks.try_reserve_exact(admission.triangles as usize).map_err(|_| error(TraceError::AllocationFailed))?;
        pixels.try_reserve_exact(request.width as usize * request.height as usize * 4).map_err(|_| error(TraceError::AllocationFailed))?;
        // Allocator capacities are checked, not assumed equal to requested capacities.
        let actual = BvhBuilder::required_bytes(admission.triangles as usize, triangles.capacity(), limits(options.budget)).map_err(error)?;
        if actual != BvhBuilder::required_bytes(admission.triangles as usize, admission.triangles as usize, limits(options.budget)).map_err(error)?
            || looks.capacity() != admission.triangles as usize
            || pixels.capacity() != request.width as usize * request.height as usize * 4
        {
            return Err(error(TraceError::MemoryLimit));
        }
        let pending = PendingPixelSlot::new(&cancel)?;
        let budget = options.budget;
        let progressive = options
            .progressive
            .map(|options| -> job::Result<Progressive> {
                let bytes = PrimaryFilm::required_bytes(request.width, request.height, options, budget)?;
                let film = PrimaryFilm::new(request.width, request.height, options, bytes, &cancel, budget)?;
                Ok(Progressive { film, options, resolving: false, resolve_pixel: 0, partial_plan: partial, partial: None, last_partial: None })
            })
            .transpose()?;
        Ok(Self {
            punctual_diagnostics: Default::default(),
            progressive,
            last_pass_boundary: 0,
            optical,
            anisotropic_targets,
            environment,
            visibility_owner: dielectric::DirectVisibilityOwner::new(scene, options),
            request,
            cancel,
            admission,
            lights,
            emissions,
            effects,
            clip,
            work,
            triangles,
            looks,
            body: 0,
            triangle: 0,
            face_cursor: 0,
            chosen: None,
            builder: None,
            bvh: None,
            build_used: 0,
            pixels,
            pixel: 0,
            sample: 0,
            spatial: 0,
            primary_coverage: 0,
            spatial_radiance: Vec3::ZERO,
            finish_spatial: false,
            emit_spatial_pixel: false,
            pixel_radiance: Vec3::ZERO,
            pending,
            encoder: None,
            done: false,
        })
    }
    pub fn progress(&self) -> Progress {
        let (phase, done, total) = if self.done {
            (Phase::Completed, 1, 1)
        } else if let Some(partial) = self.progressive.as_ref().and_then(|p| p.partial.as_ref()) {
            if let Some(encoder) = &self.encoder {
                (Phase::Encoding, encoder.row() as u64, u64::from(partial.plan.height()))
            } else {
                (Phase::Encoding, partial.next as u64, partial.plan.pixels())
            }
        } else if let Some(e) = &self.encoder {
            (Phase::Encoding, e.row() as u64, self.request.height as u64)
        } else if let Some(progressive) = &self.progressive {
            let count = u64::from(self.request.width) * u64::from(self.request.height);
            if !progressive.film.ready() {
                (Phase::Preparing, progressive.film.initialized() as u64, count)
            } else if progressive.resolving {
                (Phase::Encoding, progressive.resolve_pixel as u64, count)
            } else if self.bvh.is_some() {
                let partial = if self.pixel as u64 == count { 0 } else { self.pixel as u64 };
                (Phase::Running, u64::from(progressive.film.samples()) * count + partial, u64::from(progressive.options.samples_per_pixel) * count)
            } else {
                (Phase::Preparing, self.triangles.len() as u64, self.admission.triangles)
            }
        } else if self.bvh.is_some() {
            (Phase::Running, self.pixel as u64, self.request.width as u64 * self.request.height as u64)
        } else {
            (Phase::Preparing, self.triangles.len() as u64, self.admission.triangles)
        };
        Progress { phase, done, total }
    }
    fn prepare_one(&mut self) -> job::Result<()> {
        self.work.tick(&self.cancel).map_err(error)?;
        let Some(body) = self.visibility_owner.source().meshes.get(self.body) else {
            let triangles = std::mem::take(&mut self.triangles);
            let expected =
                BvhBuilder::required_bytes(triangles.len(), triangles.capacity(), limits(self.visibility_owner.options().budget)).map_err(error)?;
            let builder = BvhBuilder::new(triangles, limits(self.visibility_owner.options().budget), &self.cancel).map_err(error)?;
            if builder.admitted_bytes() > expected {
                return Err(error(TraceError::MemoryLimit));
            }
            self.builder = Some(builder);
            return Ok(());
        };
        let Some(indices) = body.mesh.triangles.get(self.triangle) else {
            self.body += 1;
            self.triangle = 0;
            return Ok(());
        };
        let mut vertices = [Vec3::ZERO; 3];
        let mut normals = [Vec3::ZERO; 3];
        for ((p, n), i) in vertices.iter_mut().zip(normals.iter_mut()).zip(indices) {
            *p = *body.mesh.positions.get(*i as usize).ok_or_else(|| invalid("triangle vertex index"))?;
            *n = body.mesh.normals.get(*i as usize).copied().unwrap_or(Vec3::ZERO);
        }
        let face = body.mesh.tri_face.get(self.triangle).copied();
        let mut color = body.color;
        let mut surface = body.surface;
        if let Some(look) = body.faces.get(self.face_cursor) {
            self.face_cursor += 1;
            if Some(look.face) == face {
                self.chosen = Some(look.clone());
                self.face_cursor = body.faces.len();
            } else {
                return Ok(());
            }
        }
        if let Some(look) = self.chosen.take() {
            color = look.color;
            surface = look.surface;
        }
        self.face_cursor = 0;
        let surface = match surface {
            Some(s) => s,
            None => OpaqueSurface::new(Vec3::new(srgb8_to_linear(color[0]), srgb8_to_linear(color[1]), srgb8_to_linear(color[2])), 0.0, 0.7, 1.5)
                .map_err(|_| invalid("invalid scalar surface"))?,
        };
        self.triangles.push(Triangle::new(vertices).map_err(error)?);
        self.looks.push(Look { body_index: self.body, face, surface, normals, vertices });
        self.triangle += 1;
        Ok(())
    }
    fn sample_count(&self) -> u8 {
        self.visibility_owner.options().reflections.as_ref().filter(|r| !r.bodies.is_empty()).map_or(1, |r| r.samples)
    }
    fn finish_sample(&mut self, radiance: Vec3) -> job::Result<()> {
        ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
        // A Fresnel leaf is not a camera observation. The existing transport
        // drains every reflected/transmitted branch into one linear sum first.
        // Reject a premature commit rather than averaging branches into film.
        if self.progressive.is_some() && self.optical.first().is_some_and(|o| o.pending.is_some()) {
            return Err(invalid("progressive film cannot commit an undrained optical tree"));
        }
        if let Some(progressive) = &mut self.progressive {
            if self.primary_coverage > 1 {
                return Err(invalid("progressive camera sample has duplicate primary coverage"));
            }
            let ordinal = progressive.film.samples();
            progressive.film.add(self.pixel, ordinal, [radiance.x, radiance.y, radiance.z], self.primary_coverage == 1, &self.cancel, &mut || {
                self.work.tick(&self.cancel).map_err(error)
            })?;
            self.pending.clear();
            self.primary_coverage = 0;
            self.pixel += 1;
            return Ok(());
        }
        if self.visibility_owner.options().primary_samples != 1 {
            return self.finish_spatial_sample(radiance);
        }
        // Only the current fixed-centre, nonselected primary path is identical
        // across reflection samples: directional inputs and the environment's
        // fixed u-grid do not depend on self.sample, and this path never bounces.
        // Secondary nonselected hits and selected null samples still require R.
        let repeats =
            if self.sample == 0 && self.pending.as_ref().is_some_and(|p| p.repeatable_primary && p.bounce == 0) { self.sample_count() } else { 1 };
        // Reuse the shading result, but retain the original addition order,
        // per-add bound and final divisor exactly. At most64 scalar additions,
        // no repeated BVH/BSDF/environment work or new allocation/work budget.
        for _ in 0..repeats {
            self.pixel_radiance += radiance;
            if !self.pixel_radiance.is_finite()
                || [self.pixel_radiance.x, self.pixel_radiance.y, self.pixel_radiance.z].into_iter().any(|v| !(0.0..=1e12).contains(&v))
            {
                return Err(invalid("reflection radiance exceeds finite transport bound"));
            }
            self.sample += 1;
        }
        self.pending.clear();
        if self.sample == self.sample_count() {
            let display = map_to_display(self.pixel_radiance / self.sample as f64, self.visibility_owner.options().exposure_ev).map_err(error)?;
            self.pixels.extend_from_slice(&[encode8(display.x), encode8(display.y), encode8(display.z), 255]);
            self.pixel += 1;
            self.sample = 0;
            self.pixel_radiance = Vec3::ZERO;
        }
        Ok(())
    }
    fn finish_spatial_sample(&mut self, radiance: Vec3) -> job::Result<()> {
        ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
        // Same per-R-add finite bound and denominator, including zero/null events.
        self.pixel_radiance += radiance;
        if !bounded_radiance(self.pixel_radiance) {
            return Err(invalid("reflection radiance exceeds finite transport bound"));
        }
        self.sample = self.sample.checked_add(1).ok_or_else(|| invalid("reflection sample overflow"))?;
        if self.sample > self.sample_count() {
            return Err(invalid("reflection sample count exceeded"));
        }
        self.finish_spatial = self.sample == self.sample_count();
        let retain = !self.finish_spatial && self.pending.as_ref().is_some_and(|p| p.repeatable_primary && p.bounce == 0 && p.hit.is_some());
        if retain {
            // A completed primary, never a traversal/shading intermediate. The
            // existing light cursor has no other len+1 state (at most 8 lights).
            let completed = self.lights.lights().len() + 1;
            self.pending.as_mut().ok_or_else(|| invalid("missing reusable primary"))?.light = completed;
        } else {
            self.pending.clear();
        }
        Ok(())
    }
    fn spatial_transition(&mut self) -> job::Result<bool> {
        if self.finish_spatial {
            ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
            self.work.tick(&self.cancel).map_err(error)?;
            self.spatial_radiance +=
                self.pixel_radiance / f64::from(self.sample_count()) / f64::from(self.visibility_owner.options().primary_samples);
            if !bounded_radiance(self.spatial_radiance) {
                return Err(invalid("spatial radiance exceeds finite transport bound"));
            }
            self.pixel_radiance = Vec3::ZERO;
            self.sample = 0;
            self.spatial = self.spatial.checked_add(1).ok_or_else(|| invalid("spatial sample overflow"))?;
            self.finish_spatial = false;
            self.emit_spatial_pixel = self.spatial == self.visibility_owner.options().primary_samples;
            return Ok(true);
        }
        if self.emit_spatial_pixel {
            ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
            self.work.tick(&self.cancel).map_err(error)?;
            let attempts = u16::from(self.visibility_owner.options().primary_samples)
                .checked_mul(u16::from(self.sample_count()))
                .filter(|n| *n <= 1024)
                .ok_or_else(|| invalid("spatial coverage bound"))?;
            let rgba = composite_spatial(
                self.spatial_radiance,
                u32::from(self.primary_coverage),
                u32::from(attempts),
                self.request.background,
                self.visibility_owner.options().exposure_ev,
            )?;
            self.cancel.check()?;
            self.pixels.extend_from_slice(&rgba);
            self.pixel += 1;
            self.spatial = 0;
            self.primary_coverage = 0;
            self.spatial_radiance = Vec3::ZERO;
            self.emit_spatial_pixel = false;
            return Ok(true);
        }
        Ok(false)
    }
    fn pixel_one(&mut self) -> job::Result<()> {
        if self.visibility_owner.options().primary_samples != 1 && self.pending.as_ref().is_some_and(|p| p.light > self.lights.lights().len()) {
            let pending = self.pending.as_ref().ok_or_else(|| invalid("missing reusable primary"))?;
            if pending.light != self.lights.lights().len() + 1
                || !pending.repeatable_primary
                || pending.directional_active
                || pending.bounce != 0
                || pending.hit.is_none()
                || self.sample == 0
                || self.sample >= self.sample_count()
                || self.spatial >= self.visibility_owner.options().primary_samples
                || self.finish_spatial
                || self.emit_spatial_pixel
            {
                return Err(invalid("invalid reusable primary state"));
            }
            if !pending.surface_ready
                || pending.surface_cursor != self.effects.len()
                || pending.emission_cursor != self.emissions.len()
                || pending.punctual_index != self.visibility_owner.options().punctual_lights.as_ref().map_or(0, Vec::len)
                || pending.area_light != self.visibility_owner.options().area_lights.as_ref().map_or(0, Vec::len)
                || pending.punctual_shadow
                || pending.punctual_ready
                || pending.area_shadow
                || pending.area_ready
                || pending.environment_cursor.is_some()
                || pending.environment_sample != self.environment.as_ref().map_or(0, |e| e.samples)
            {
                return Err(invalid("incomplete reusable primary shading"));
            }
            let radiance = pending.radiance;
            // One actual scalar continuation per logical R; no bulk R loop or
            // fabricated traversal debit. Check cancel/limit before mutations.
            self.work.tick(&self.cancel).map_err(error)?;
            let bvh = self.bvh.as_ref().ok_or_else(|| invalid("missing reusable primary BVH"))?;
            ensure_directional_drained(Some(pending), self.lights.lights().len())?;
            let shared = resolved_surface_hit(
                &PreparedScene { bvh, primitives: &self.looks, source: self.visibility_owner.source() },
                pending,
                &self.effects,
            )?;
            if self.optical.first().is_some_and(|o| o.pending.is_some())
                || self.visibility_owner.options().dielectrics.iter().any(|input| input.body_index == shared.body_index)
            {
                return Err(invalid("optical primary cannot reuse shading"));
            }
            if self.visibility_owner.options().reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|entry| entry.body == shared.body_name)) {
                return Err(invalid("selected primary cannot reuse shading"));
            }
            self.primary_coverage =
                self.primary_coverage.checked_add(1).filter(|n| *n <= 1024).ok_or_else(|| invalid("spatial coverage overflow"))?;
            return self.finish_spatial_sample(radiance);
        }
        if self.visibility_owner.options().primary_samples != 1 && self.spatial_transition()? {
            return Ok(());
        }
        let bvh = self.bvh.as_ref().ok_or_else(|| invalid("missing image BVH"))?;
        if self.pending.is_none() {
            self.work.tick(&self.cancel).map_err(error)?;
            let width = self.request.width as usize;
            // This proof belongs to the generator for a fixed (pixel, spatial),
            // not the camera/request globally. Future per-R jitter must revoke it.
            let (ray, repeatable_primary) = if let Some(progressive) = &self.progressive {
                let [dx, dy] = progressive::variates(progressive.options.seed, self.pixel as u64, progressive.film.samples(), 0);
                (primary_ray(&self.request, (self.pixel % width) as f64 + dx, (self.pixel / width) as f64 + dy, self.clip).map_err(error)?, false)
            } else if self.visibility_owner.options().primary_samples == 1 {
                // Preserve the exact legacy centre path, including its reuse proof.
                (primary_ray(&self.request, (self.pixel % width) as f64 + 0.5, (self.pixel / width) as f64 + 0.5, self.clip).map_err(error)?, true)
            } else {
                let [dx, dy] = primary_offset(self.visibility_owner.options().primary_samples, self.spatial)?;
                (primary_ray(&self.request, (self.pixel % width) as f64 + dx, (self.pixel / width) as f64 + dy, self.clip).map_err(error)?, true)
            };
            if let Some(optical) = self.optical.first_mut() {
                if optical.pending.is_some() {
                    return Err(invalid("previous optical sample was not drained"));
                }
                let mut state = DielectricPixel::new(ray);
                state.camera_sample = self.progressive.as_ref().map(|p| (self.pixel, p.film.samples()));
                optical.pending = Some(state);
            }
            self.pending.insert(PendingPixel {
                surface_cursor: 0,
                surface_index: None,
                surface_ready: false,
                emission_cursor: 0,
                punctual_index: 0,
                punctual_shadow: false,
                punctual_ready: false,
                punctual_value: Vec3::ZERO,
                area_light: 0,
                area_index: 0,
                area_shadow: false,
                area_ready: false,
                area_value: Vec3::ZERO,

                repeatable_primary,
                environment_sample: 0,
                environment_sum: Vec3::ZERO,
                environment_cursor: None,
                environment_value: Vec3::ZERO,
                ray,
                cursor: TraceCursor::new(ray, false),
                hit: None,
                visibility: [true; 8],
                directional_transmittance: [Vec3::ZERO; 8],
                directional_complete: 0,
                directional_active: false,
                directional_required: false,
                light: 0,
                bounce: 0,
                throughput: Vec3::new(1.0, 1.0, 1.0),
                radiance: Vec3::ZERO,
            })?;
            return Ok(());
        }
        if let Some(optical) = self.optical.first_mut() {
            let state = optical.pending.as_mut().ok_or_else(|| invalid("missing optical primary state"))?;
            state.check_camera_sample(self.progressive.as_ref().map(|p| (self.pixel, p.film.samples())))?;
            if state.primary_check < self.visibility_owner.options().dielectrics.len() {
                self.work.tick(&self.cancel).map_err(error)?;
                if let Some(Certificate::ClosedBox(bounds)) = optical.certificates.get(state.primary_check).copied().flatten()
                    && (dielectric::contains(bounds, state.path.ray.origin()) || dielectric::contains(bounds, self.request.camera.eye()))
                {
                    return Err(invalid("camera or near plane inside a dielectric volume is unsupported"));
                }
                state.primary_check += 1;
                return Ok(());
            }
            if state.primary_check != self.visibility_owner.options().dielectrics.len() {
                return Err(invalid("invalid primary certificate cursor"));
            }
        }
        if self.pending.as_ref().is_some_and(|pending| pending.directional_active) {
            if !self.visibility_owner.options().directional_shadow_transmission {
                return Err(invalid("directional cursor active without its runtime mode"));
            }
            let result = self.visibility_owner.step(&mut self.work, &self.cancel).map_err(error)?;
            if let Some(transmittance) = result {
                let pending = self.pending.as_mut().ok_or_else(|| invalid("missing directional pixel state"))?;
                if pending.light >= super::lighting::MAX_LIGHTS {
                    return Err(invalid("directional visibility light index"));
                }
                *pending.directional_transmittance.get_mut(pending.light).ok_or_else(|| invalid("directional visibility result index"))? =
                    transmittance;
                pending.directional_complete |= 1u8 << pending.light;
                pending.directional_active = false;
                pending.light += 1;
            }
            return Ok(());
        }
        let scene = PreparedScene { bvh, primitives: &self.looks, source: self.visibility_owner.source() };
        let pending = self.pending.as_mut().ok_or_else(|| invalid("missing pixel state"))?;
        let primary_segment =
            pending.bounce == 0 && self.optical.first().and_then(|o| o.pending.as_ref()).is_none_or(|state| state.path.boundaries == 0);
        if pending.hit.is_none() {
            let Some(hit) = pending.cursor.step(bvh, &mut self.work, &self.cancel).map_err(error)? else {
                return Ok(());
            };
            let Some(hit) = hit else {
                if self.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|state| state.path.medium.is_some()) {
                    return Err(invalid("dielectric path escaped without its certified exit"));
                }
                if primary_segment {
                    if let Some(optical) = self.optical.first_mut() {
                        optical.pending = None;
                    }
                    if self.visibility_owner.options().primary_samples != 1 || self.progressive.is_some() {
                        // A miss is one of S*R attempts, never the completed pixel.
                        return self.finish_sample(Vec3::ZERO);
                    }
                    let [r, g, b] = self.request.background;
                    self.pixels.extend_from_slice(&[r, g, b, 255]);
                    self.pixel += 1;
                    self.sample = 0;
                    self.pixel_radiance = Vec3::ZERO;
                    self.pending.clear();
                    return Ok(());
                }
                if let Some(environment) = &self.environment {
                    let emitted =
                        environment.view().map_err(environment_error)?.radiance_linear_srgb(pending.ray.direction()).map_err(environment_error)?;
                    pending.radiance += multiply(pending.throughput, emitted);
                }
                if self.environment.is_none()
                    && let Some(environment) = self.visibility_owner.options().environment
                {
                    let [r, g, b] = environment.color;
                    pending.radiance += multiply(pending.throughput, Vec3::new(r, g, b) * environment.intensity);
                }
                let radiance = pending.radiance;
                if !bounded_radiance(radiance) {
                    return Err(invalid("secondary environment radiance exceeds finite bound"));
                }
                return self.finish_transport_path(radiance);
            };
            if primary_segment && (self.visibility_owner.options().primary_samples != 1 || self.progressive.is_some()) {
                self.primary_coverage =
                    self.primary_coverage.checked_add(1).filter(|n| *n <= 1024).ok_or_else(|| invalid("spatial coverage overflow"))?;
            }
            pending.hit = Some(hit);
            pending.light = 0;
            let shared = scene.resolve_hit(pending.ray, hit).map_err(error)?;
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            pending.repeatable_primary &= primary_segment && mode.is_none();
            if let Some(optical) = self.optical.first_mut() {
                let state = optical.pending.as_mut().ok_or_else(|| invalid("missing optical hit state"))?;
                state.selector = 0;
                state.selected = None;
                state.ready = false;
                // Traversal consumed this continuation; selector work begins next call.
                return Ok(());
            }
            pending.surface_ready = self.effects.is_empty();
            if !pending.surface_ready {
                // Traversal already consumed this continuation's unit. Scan one
                // effect per following call, before beginning any lighting stage.
                return Ok(());
            }
            if self.visibility_owner.options().shadows && !self.lights.lights().is_empty() && mode != Some(job::ReflectionMode::Mirror) {
                let light = self.lights.lights().first().ok_or_else(|| invalid("missing first light"))?;
                let ray = shared.spawn(light.direction()).map_err(error)?;
                if self.visibility_owner.options().directional_shadow_transmission {
                    begin_directional_shadow(&mut self.visibility_owner, pending, ray, &mut self.work, &self.cancel)?;
                } else {
                    pending.cursor = TraceCursor::new(ray, true);
                }
                return Ok(());
            }
            pending.light = self.lights.lights().len();
            return Ok(());
        } else if self.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|state| !state.ready) {
            return self.dielectric_hit_one();
        } else if !pending.surface_ready {
            self.work.tick(&self.cancel).map_err(error)?;
            if let Some((body_index, _)) = self.effects.get(pending.surface_cursor) {
                let hit = pending.hit.ok_or_else(|| invalid("missing surface effect hit"))?;
                let look = self.looks.get(hit.primitive_index).ok_or_else(|| invalid("missing surface effect appearance"))?;
                let index = pending.surface_cursor;
                pending.surface_cursor += 1;
                if *body_index == look.body_index {
                    pending.surface_index = Some(index);
                    pending.surface_cursor = self.effects.len();
                }
                return Ok(());
            }
            if pending.surface_cursor != self.effects.len() {
                return Err(invalid("invalid surface effect scan state"));
            }
            pending.surface_ready = true;
            let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            if self.visibility_owner.options().shadows && !self.lights.lights().is_empty() && mode != Some(job::ReflectionMode::Mirror) {
                let light = self.lights.lights().first().ok_or_else(|| invalid("missing first surface light"))?;
                let ray = shared.spawn(light.direction()).map_err(error)?;
                if self.visibility_owner.options().directional_shadow_transmission {
                    begin_directional_shadow(&mut self.visibility_owner, pending, ray, &mut self.work, &self.cancel)?;
                } else {
                    pending.cursor = TraceCursor::new(ray, true);
                }
            } else {
                pending.light = self.lights.lights().len();
            }
            return Ok(());
        } else if pending.light < self.lights.lights().len() {
            if self.visibility_owner.options().directional_shadow_transmission {
                let light = self.lights.lights().get(pending.light).ok_or_else(|| invalid("missing directional light"))?;
                let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
                let ray = shared.spawn(light.direction()).map_err(error)?;
                begin_directional_shadow(&mut self.visibility_owner, pending, ray, &mut self.work, &self.cancel)?;
                return Ok(());
            }
            let Some(shadow) = pending.cursor.step(bvh, &mut self.work, &self.cancel).map_err(error)? else {
                return Ok(());
            };
            *pending.visibility.get_mut(pending.light).ok_or_else(|| invalid("light visibility index"))? = shadow.is_none();
            pending.light += 1;
            if let Some(light) = self.lights.lights().get(pending.light) {
                let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
                pending.cursor = TraceCursor::new(shared.spawn(light.direction()).map_err(error)?, true);
                return Ok(());
            }
            return Ok(());
        }
        if let Some((body_index, emission)) = self.emissions.get(pending.emission_cursor) {
            self.work.tick(&self.cancel).map_err(error)?;
            let hit = pending.hit.ok_or_else(|| invalid("missing emission hit"))?;
            let look = self.looks.get(hit.primitive_index).ok_or_else(|| invalid("missing emission appearance"))?;
            pending.emission_cursor += 1;
            if *body_index == look.body_index {
                let emitted = emission.radiance(hit.geometry.geometric_normal.dot(-pending.ray.direction()) > 0.0);
                pending.radiance += multiply(pending.throughput, emitted);
                if !bounded_radiance(pending.radiance) {
                    return Err(invalid("emission path radiance exceeds finite bound"));
                }
                pending.emission_cursor = self.emissions.len();
            }
            return Ok(());
        }
        if pending.punctual_index < self.visibility_owner.options().punctual_lights.as_ref().map_or(0, Vec::len) {
            let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            let face = self.looks.get(shared.primitive_index).ok_or_else(|| invalid("punctual face missing"))?.face;
            let target = anisotropic_faces::target_index(self.visibility_owner.options(), shared.body_index, face)
                .map(|i| self.anisotropic_targets.get(i).copied().flatten().ok_or_else(|| invalid("punctual anisotropic target missing")))
                .transpose()?;
            punctual_one(
                pending,
                self.visibility_owner.options(),
                bvh,
                (shared, target, mode),
                &mut self.work,
                &self.cancel,
                &mut self.punctual_diagnostics,
            )?;
            return Ok(());
        }
        if pending.area_light < self.visibility_owner.options().area_lights.as_ref().map_or(0, Vec::len) {
            let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            let face = self.looks.get(shared.primitive_index).ok_or_else(|| invalid("area face missing"))?.face;
            let target = anisotropic_faces::target_index(self.visibility_owner.options(), shared.body_index, face)
                .map(|i| self.anisotropic_targets.get(i).copied().flatten().ok_or_else(|| invalid("area anisotropic target missing")))
                .transpose()?;
            area_one(
                pending,
                self.visibility_owner.options(),
                bvh,
                (shared, target, mode),
                &mut self.work,
                &self.cancel,
                area_sample_key(
                    self.pixel as u64,
                    self.spatial,
                    self.visibility_owner.options().primary_samples,
                    self.progressive.as_ref().map_or(0, |progress| progress.film.samples()),
                )?,
            )?;
            return Ok(());
        }
        if let Some(cursor) = &mut pending.environment_cursor {
            let Some(blocker) = cursor.step(bvh, &mut self.work, &self.cancel).map_err(error)? else {
                return Ok(());
            };
            if blocker.is_none() {
                pending.environment_sum += pending.environment_value;
            }
            pending.environment_cursor = None;
            pending.environment_sample += 1;
            return Ok(());
        }
        if pending.light == self.lights.lights().len() {
            self.work.tick(&self.cancel).map_err(error)?;
            let shared = resolved_surface_hit(&scene, pending, &self.effects)?;
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            let view = -pending.ray.direction();
            let geometric = shared.oriented_geometric_normal();
            // Disjoint estimators: selected reflectors use BSDF transport to the environment;
            // only nonselected opaque hits use environment next-event estimates.
            if mode.is_none()
                && let Some(environment) = &self.environment
            {
                if pending.environment_sample < environment.samples {
                    let side = match environment.samples {
                        1 => 1,
                        4 => 2,
                        16 => 4,
                        _ => 8,
                    };
                    let i = pending.environment_sample;
                    let u = if let Some(progressive) = &self.progressive {
                        let [dx, dy] = progressive::variates(
                            progressive.options.seed,
                            self.pixel as u64,
                            progressive.film.samples(),
                            0x1000 + u64::from(pending.bounce) * 128 + u64::from(i),
                        );
                        [(f64::from(i / side) + dx) / f64::from(side), (f64::from(i % side) + dy) / f64::from(side)]
                    } else if side == 1 {
                        [0.25, 0.375]
                    } else {
                        [(f64::from(i / side) + 0.5) / f64::from(side), (f64::from(i % side) + 0.5) / f64::from(side)]
                    };
                    let sample = environment
                        .view()
                        .map_err(environment_error)?
                        .direct_sample_linear_srgb(u, shared.surface, shared.shading_normal, geometric, view)
                        .map_err(environment_error)?;
                    pending.environment_value = sample.contribution_linear_srgb;
                    if self.visibility_owner.options().shadows && sample.contribution_linear_srgb.len2() > 0.0 {
                        pending.environment_cursor = Some(TraceCursor::new(shared.spawn(sample.direction).map_err(error)?, true));
                    } else {
                        pending.environment_sum += pending.environment_value;
                        pending.environment_sample += 1;
                    }
                    return Ok(());
                }
                pending.radiance += multiply(pending.throughput, pending.environment_sum / f64::from(environment.samples));
            }
            let face = self.looks.get(shared.primitive_index).ok_or_else(|| invalid("missing captured face"))?.face;
            let target_index = anisotropic_faces::target_index(self.visibility_owner.options(), shared.body_index, face);
            if mode != Some(job::ReflectionMode::Mirror) {
                let direct = if self.visibility_owner.options().directional_shadow_transmission {
                    pending.directional_required = true;
                    ensure_directional_drained(Some(pending), self.lights.lights().len())?;
                    self.lights
                        .shade_linear_transmitted(shared.surface, shared.shading_normal, view, |index, light| {
                            if geometric.dot(light) <= 0.0 {
                                return Ok(Vec3::ZERO);
                            }
                            if index >= super::lighting::MAX_LIGHTS || pending.directional_complete & (1u8 << index) == 0 {
                                return Err(TraceError::Internal("directional visibility result is incomplete"));
                            }
                            pending.directional_transmittance.get(index).copied().ok_or(TraceError::Internal("directional visibility result index"))
                        })
                        .map_err(error)?
                } else {
                    let visible = |index: usize, light: Vec3| {
                        Ok(geometric.dot(light) > 0.0 && *pending.visibility.get(index).ok_or(TraceError::Internal("light visibility"))?)
                    };
                    if let Some(index) = target_index {
                        self.anisotropic_targets
                            .get(index)
                            .copied()
                            .flatten()
                            .ok_or_else(|| invalid("missing certified anisotropic face"))?
                            .shade(&self.lights, shared.surface, geometric, view, visible)
                            .map_err(error)?
                    } else {
                        self.lights.shade_linear(shared.surface, shared.shading_normal, view, visible).map_err(error)?
                    }
                };
                pending.radiance += multiply(pending.throughput, direct);
            }
            let max_bounces = self.visibility_owner.options().reflections.as_ref().map_or(0, |r| r.max_bounces);
            if let Some(mode) = mode.filter(|_| pending.bounce < max_bounces) {
                let samples = self.visibility_owner.options().reflections.as_ref().map_or(1, |r| u64::from(r.samples));
                let u = if let Some(progressive) = &self.progressive {
                    progressive::variates(progressive.options.seed, self.pixel as u64, progressive.film.samples(), 0x100 + u64::from(pending.bounce))
                } else if self.visibility_owner.options().primary_samples == 1 {
                    reflection::variates(self.pixel as u64, self.sample as u64, pending.bounce as u64, samples).map_err(error)?
                } else {
                    let key = spatial_key(self.pixel as u64, self.spatial, self.visibility_owner.options().primary_samples)?;
                    reflection::variates(key, self.sample as u64, pending.bounce as u64, samples).map_err(error)?
                };
                let sampled = match (mode, target_index) {
                    (job::ReflectionMode::Mirror, _) => reflection::mirror(shared.surface, shared.shading_normal, view),
                    (job::ReflectionMode::RoughSpecular, Some(index)) => self
                        .anisotropic_targets
                        .get(index)
                        .copied()
                        .flatten()
                        .ok_or_else(|| invalid("missing anisotropic reflection target"))?
                        .sample(shared.surface, geometric, view, u),
                    (job::ReflectionMode::RoughSpecular, None) => reflection::rough_specular(shared.surface, shared.shading_normal, view, u),
                }
                .map_err(error)?;
                if let Some(sample) = sampled.filter(|s| s.direction.dot(geometric) > 0.0) {
                    pending.throughput = multiply(pending.throughput, sample.weight);
                    if !pending.throughput.is_finite() {
                        return Err(invalid("nonfinite reflection throughput"));
                    }
                    if pending.throughput.len2() > 0.0 {
                        pending.ray = shared.spawn(sample.direction).map_err(error)?;
                        pending.cursor = TraceCursor::new(pending.ray, false);
                        pending.hit = None;
                        pending.surface_cursor = 0;
                        pending.surface_index = None;
                        pending.surface_ready = false;
                        pending.emission_cursor = 0;
                        pending.light = 0;
                        pending.visibility = [true; 8];
                        pending.directional_transmittance = [Vec3::ZERO; 8];
                        pending.directional_complete = 0;
                        pending.directional_active = false;
                        pending.directional_required = false;
                        pending.punctual_index = 0;
                        pending.punctual_shadow = false;
                        pending.punctual_ready = false;
                        pending.punctual_value = Vec3::ZERO;
                        pending.area_light = 0;
                        pending.area_index = 0;
                        pending.area_shadow = false;
                        pending.area_ready = false;
                        pending.area_value = Vec3::ZERO;

                        pending.environment_sample = 0;
                        pending.environment_sum = Vec3::ZERO;
                        pending.environment_cursor = None;
                        pending.environment_value = Vec3::ZERO;
                        pending.bounce += 1;
                        if let Some(optical) = self.optical.first_mut() {
                            let state = optical.pending.as_mut().ok_or_else(|| invalid("missing reflected optical path"))?;
                            state.path.ray = pending.ray;
                            state.path.throughput = pending.throughput;
                            state.path.segment_start = shared.position;
                            state.path.bounce = pending.bounce;
                            state.selector = 0;
                            state.selected = None;
                            state.ready = false;
                        }
                        return Ok(());
                    }
                }
            }
            let radiance = pending.radiance;
            return self.finish_transport_path(radiance);
        }
        Ok(())
    }
    /// Complete a transport branch. Only a drained optical stack completes one R sample.
    fn finish_transport_path(&mut self, radiance: Vec3) -> job::Result<()> {
        ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
        if let Some(optical) = self.optical.first_mut() {
            let state = optical.pending.as_mut().ok_or_else(|| invalid("missing optical sample"))?;
            state.check_camera_sample(self.progressive.as_ref().map(|p| (self.pixel, p.film.samples())))?;
            if state.advance()? {
                let pending = self.pending.as_mut().ok_or_else(|| invalid("missing optical continuation"))?;
                resume_optical_path(pending, state.path, self.lights.lights().len())?;
                return Ok(());
            }
            optical.pending = None;
        }
        self.finish_sample(radiance)
    }

    /// Each selector and each interface is a separate charged continuation. Traversal has
    /// already yielded with the captured hit, and opaque hits rejoin the shared light stages.
    fn dielectric_hit_one(&mut self) -> job::Result<()> {
        ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
        self.work.tick(&self.cancel).map_err(error)?;
        let optical = self.optical.first_mut().ok_or_else(|| invalid("missing optical task"))?;
        let state = optical.pending.as_mut().ok_or_else(|| invalid("missing optical sample"))?;
        let pending = self.pending.as_mut().ok_or_else(|| invalid("missing optical hit"))?;
        if state.ready {
            return Err(invalid("optical hit already consumed"));
        }
        let hit = pending.hit.ok_or_else(|| invalid("missing captured optical hit"))?;
        let look = self.looks.get(hit.primitive_index).ok_or_else(|| invalid("missing optical appearance"))?;
        if let Some(input) = self.visibility_owner.options().dielectrics.get(state.selector) {
            let index = state.selector;
            state.selector += 1;
            if input.body_index == look.body_index {
                state.selected = Some(index);
                state.selector = self.visibility_owner.options().dielectrics.len();
            }
            return Ok(());
        }
        if state.selector != self.visibility_owner.options().dielectrics.len() {
            return Err(invalid("invalid optical selector state"));
        }
        let bvh = self.bvh.as_ref().ok_or_else(|| invalid("missing optical BVH"))?;
        let scene = PreparedScene { bvh, primitives: &self.looks, source: self.visibility_owner.source() };
        // Optics always uses the certified geometric boundary, before optional opaque
        // shading-normal effects. Distance starts before the numerical spawn offset.
        let mut shared = scene.resolve_hit(pending.ray, hit).map_err(error)?;
        let exact_face = if optical.nested {
            match state.selected.and_then(|index| optical.certificates.get(index).copied().flatten()) {
                Some(Certificate::ClosedBox(bounds)) => {
                    let face =
                        dielectric::exact_box_face(bounds, shared.vertices, shared.geometric_normal, shared.position, pending.ray).map_err(error)?;
                    shared.position = face.point;
                    Some(face)
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(index) = state.path.medium {
            let medium = self.visibility_owner.options().dielectrics.get(index).ok_or_else(|| invalid("active dielectric medium"))?;
            let distance = shared.position.dist(state.path.segment_start);
            pending.throughput = multiply(pending.throughput, dielectric::attenuation(medium.absorption_per_mm, distance).map_err(error)?);
            if !bounded_radiance(pending.throughput) {
                return Err(invalid("dielectric throughput exceeds finite bound"));
            }
        }
        state.ready = true;
        let Some(index) = state.selected else {
            pending.surface_ready = self.effects.is_empty();
            if !pending.surface_ready {
                return Ok(());
            }
            let mode = self
                .visibility_owner
                .options()
                .reflections
                .as_ref()
                .and_then(|r| r.bodies.iter().find(|entry| entry.body == shared.body_name).map(|entry| entry.mode));
            if self.visibility_owner.options().shadows && !self.lights.lights().is_empty() && mode != Some(job::ReflectionMode::Mirror) {
                let light = self.lights.lights().first().ok_or_else(|| invalid("missing optical opaque light"))?;
                let ray = shared.spawn(light.direction()).map_err(error)?;
                if self.visibility_owner.options().directional_shadow_transmission {
                    begin_directional_shadow(&mut self.visibility_owner, pending, ray, &mut self.work, &self.cancel)?;
                } else {
                    pending.cursor = TraceCursor::new(ray, true);
                }
            } else {
                pending.light = self.lights.lights().len();
            }
            return Ok(());
        };
        let medium = self.visibility_owner.options().dielectrics.get(index).ok_or_else(|| invalid("selected dielectric medium"))?;
        if medium.body_index != shared.body_index || medium.body_name != shared.body_name {
            return Err(invalid("optical captured identity changed"));
        }
        pending.repeatable_primary = false;
        state.path.ray = pending.ray;
        state.path.throughput = pending.throughput;
        state.path.bounce = pending.bounce;
        if let Some(limit) = optical.straight_limit {
            // The active transmitted path was popped immediately; a nonzero
            // stack or opaque bounce would invalidate the one-segment proof.
            // Preserve all real interfaces/Beer/parent checks below. No tint or
            // environment shortcut and no expansion of the branching stack.
            if state.path.bounce != 0 || state.stack_len != 0 || state.paths != u16::from(state.path.boundaries) + 1 {
                return Err(invalid("unexpected branch in certified straight optical transport"));
            }
            if state.path.boundaries >= limit {
                return Err(invalid("certified straight optical crossing bound exceeded"));
            }
        } else {
            if state.path.boundaries == MAX_BOUNDARIES && !optical.deterministic_tir {
                // Unchanged legacy policy; strict mode must first
                // distinguish a deterministic TIR from another real split.
                let radiance = pending.radiance;
                return self.finish_transport_path(radiance);
            }
            if state.path.boundaries > MAX_BOUNDARIES {
                return Err(invalid("invalid dielectric boundary depth"));
            }
        }
        let normal = shared.oriented_geometric_normal();
        let direction = state.path.ray.direction();
        let certificate = optical.certificates.get(index).copied().flatten().ok_or_else(|| invalid("missing dielectric certificate"))?;
        let (optics, reflected_weight, transmitted_weight, next_medium) = match certificate {
            Certificate::ThinSheet => {
                if state.path.medium.is_some() {
                    return Err(invalid("thin sheet inside dielectric medium"));
                }
                let (optics, reflected, transmitted) = dielectric::thin_sheet(direction, normal, medium).map_err(error)?;
                (optics, reflected, transmitted, None)
            }
            Certificate::ClosedBox(bounds) => {
                let boundary_axes = (0..3)
                    .filter(|axis| {
                        let p = super::component(shared.position, *axis);
                        p == super::component(bounds.min(), *axis) || p == super::component(bounds.max(), *axis)
                    })
                    .count();
                if boundary_axes > 1 {
                    return Err(invalid("dielectric box edge or corner has an ambiguous interface normal"));
                }
                let parent = optical.parents.get(index).copied().ok_or_else(|| invalid("missing dielectric parent"))?;
                let parent_ior = match parent {
                    Some(parent) => {
                        self.visibility_owner.options().dielectrics.get(parent).ok_or_else(|| invalid("missing parent optical material"))?.ior
                    }
                    None => 1.0,
                };
                let (incident, transmitted, next_medium) = if shared.front_face {
                    if state.path.medium != parent {
                        return Err(invalid("dielectric entry does not match certified parent"));
                    }
                    (parent_ior, medium.ior, Some(index))
                } else {
                    if state.path.medium != Some(index) {
                        return Err(invalid("dielectric exit without matching entry"));
                    }
                    (medium.ior, parent_ior, parent)
                };
                let optics = dielectric::interface(direction, normal, incident, transmitted).map_err(error)?;
                let f = optics.reflectance;
                let t = (1.0 - f) * (incident / transmitted).powi(2);
                (optics, Vec3::new(f, f, f), Vec3::new(t, t, t), next_medium)
            }
        };
        let spawn = |direction, reflected| match exact_face {
            Some(face) => face.spawn(direction, reflected).map_err(error),
            None => shared.spawn(direction).map_err(error),
        };
        if optical.deterministic_tir && state.path.throughput == Vec3::ZERO {
            // Exact zero after real Beer attenuation owes no outgoing ray or
            // positive residual; retain all previously accumulated branches.
            let radiance = pending.radiance;
            return self.finish_transport_path(radiance);
        }
        if optical.deterministic_tir && optics.transmitted.is_none() {
            if optical.straight_limit.is_some() || optics.reflectance != 1.0 || reflected_weight != Vec3::new(1.0, 1.0, 1.0) {
                return Err(invalid("strict deterministic TIR disagrees with its interface"));
            }
            // REAL ray, normal-aware spawn and next traversal, with the same
            // already-paid interface tick. Do not push/pop a one-child tree,
            // consume branch depth/paths511, clear the shared radiance sum or
            // reset ordinary work. The waiting Fresnel branches remain intact.
            let path = OpticalPath {
                ray: spawn(optics.reflected, true)?,
                throughput: dielectric::multiply(state.path.throughput, reflected_weight),
                medium: state.path.medium,
                segment_start: shared.position,
                boundaries: state.path.boundaries,
                bounce: pending.bounce,
            };
            optical.tir_continuations = optical.tir_continuations.checked_add(1).ok_or_else(|| error(TraceError::WorkLimit))?;
            state.path = path;
            state.selector = 0;
            state.selected = None;
            state.ready = false;
            // This creates the real new TraceCursor; its following steps pay
            // every original BVH/triangle operation and check cancellation.
            resume_optical_path(pending, path, self.lights.lights().len())?;
            return Ok(());
        }
        if optical.deterministic_tir && optical.straight_limit.is_none() && state.path.boundaries == MAX_BOUNDARIES {
            return Err(JobError::new(ErrorCode::LimitExceeded, "strict optical non-TIR branch-depth budget exhausted with positive continuation"));
        }
        let next_depth = state.path.boundaries + 1;
        state.push(OpticalPath {
            ray: spawn(optics.reflected, true)?,
            throughput: dielectric::multiply(state.path.throughput, reflected_weight),
            medium: state.path.medium,
            segment_start: shared.position,
            boundaries: next_depth,
            bounce: pending.bounce,
        })?;
        if let Some(direction) = optics.transmitted {
            state.push(OpticalPath {
                ray: spawn(direction, false)?,
                throughput: dielectric::multiply(state.path.throughput, transmitted_weight),
                medium: next_medium,
                segment_start: shared.position,
                boundaries: next_depth,
                bounce: pending.bounce,
            })?;
        }
        let radiance = pending.radiance;
        self.finish_transport_path(radiance)
    }

    /// Legacy final-only callers keep their exact contract. Progressive callers
    /// must consume `step_with_previews`, so no checkpoint is silently discarded.
    pub fn step(&mut self, units: usize) -> job::Result<TaskPoll> {
        self.cancel.check()?;
        if self.progressive.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "CPU progressive tasks require step_with_previews"));
        }
        match self.step_with_previews(units)? {
            ImagePoll::Pending(progress) => Ok(TaskPoll::Pending(progress)),
            ImagePoll::Complete(png) => Ok(TaskPoll::Complete(png)),
            ImagePoll::Preview { .. } => Err(invalid("fixed image produced a progressive checkpoint")),
        }
    }

    fn release_completed_work(&mut self) -> job::Result<()> {
        ensure_directional_drained(self.pending.as_ref(), self.lights.lights().len())?;
        let bounds = self.visibility_owner.source().bounds;
        let old = self.visibility_owner.options();
        let options = RaytraceOptions {
            dielectric_transport: old.dielectric_transport,
            directional_shadow_transmission: old.directional_shadow_transmission,
            budget: old.budget,
            progressive: old.progressive,
            primary_samples: old.primary_samples,
            area_samples: old.area_samples,
            seed: old.seed,
            preset: old.preset,
            exposure_ev: old.exposure_ev,
            shadows: old.shadows,
            ..Default::default()
        };
        self.visibility_owner = dielectric::DirectVisibilityOwner::new(ProductScene { meshes: Vec::new(), bounds }, options);
        self.done = true;
        self.progressive = None;
        self.anisotropic_targets = [None, None];
        self.environment = None;
        self.looks = Vec::new();
        self.bvh = None;
        self.builder = None;
        self.triangles = Vec::new();
        self.pixels = Vec::new();
        self.pending.release();
        self.request.raytrace = None;
        self.emissions = Vec::new();
        self.optical = Vec::new();
        self.effects = Vec::new();
        self.lights = Lighting::new(Vec::new()).map_err(error)?;
        self.encoder = None;
        Ok(())
    }

    pub(crate) fn protected_preview_report(&self) -> Option<job::ProtectedPreviewReport> {
        self.work.protected_report()
    }

    pub(crate) fn partial_preview_ready(&self, plan: job::PartialPreviewPlan) -> job::Result<bool> {
        self.cancel.check()?;
        plan.validate()?;
        if (plan.source_width(), plan.source_height()) != (self.request.width, self.request.height) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview source dimensions changed"));
        }
        let Some(progressive) = &self.progressive else { return Ok(false) };
        if progressive.partial_plan != Some(plan) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview plan differs from the task policy"));
        }
        if self.done
            || !progressive.film.ready()
            || !self.pending.is_none()
            || self.optical.first().is_some_and(|o| o.pending.is_some())
            || self.encoder.is_some()
            || progressive.partial.is_some()
            || progressive.resolving
            || !self.pixels.is_empty()
        {
            return Ok(false);
        }
        let frontier = progressive.film.partial_frontier()?;
        if frontier.completed_passes >= progressive.options.samples_per_pixel {
            return Ok(false);
        }
        let safe = if frontier.updated_prefix == 0 {
            frontier.completed_passes > 0 && self.pixel == frontier.pixels
        } else {
            self.pixel == frontier.updated_prefix
        };
        if !safe {
            return Ok(false);
        }
        if self.work.protected.is_some() && !self.work.protected_ready(committed_frontier(frontier)?).map_err(error)? {
            return Ok(false);
        }
        progressive::represented_changed(plan, progressive.last_partial, frontier)
    }

    pub(crate) fn completed_pass_preview_ready(&self, plan: job::PartialPreviewPlan) -> job::Result<Option<u32>> {
        if self.work.protected.is_none() || !self.partial_preview_ready(plan)? {
            return Ok(None);
        }
        let frontier = self.progressive.as_ref().ok_or_else(|| invalid("missing progressive film"))?.film.partial_frontier()?;
        Ok((frontier.updated_prefix == 0 && frontier.completed_passes > 0).then_some(frontier.completed_passes))
    }

    /// Host must already hold its one in-flight credit. No host lease or clock
    /// lives here; an insufficient optional debit does not mutate trace state.
    pub(crate) fn begin_partial_preview(&mut self, plan: job::PartialPreviewPlan) -> job::Result<PartialBegin> {
        if !self.partial_preview_ready(plan)? {
            if self.protected_preview_report().is_some_and(|report| report.quota_exhausted) {
                return Ok(PartialBegin::Exhausted);
            }
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview requires a fresh safe completed pixel"));
        }
        let frontier = self.progressive.as_ref().ok_or_else(|| invalid("missing progressive film"))?.film.partial_frontier()?;
        if self.work.protected.is_some() {
            if !self.work.charge_presentation(committed_frontier(frontier)?, &self.cancel).map_err(error)? {
                return Err(JobError::new(ErrorCode::InvalidRequest, "protected snapshot has no reached unconsumed milestone"));
            }
        } else {
            let units = plan.snapshot_work()?;
            if self.work.remaining() < units {
                return Ok(PartialBegin::Exhausted);
            }
            self.work.charge(units, &self.cancel).map_err(error)?;
        }
        let count = usize::try_from(plan.pixels()).map_err(|_| error(TraceError::MemoryLimit))?;
        let mut covered = Vec::new();
        covered.try_reserve_exact(count).map_err(|_| error(TraceError::AllocationFailed))?;
        if covered.capacity() != count {
            return Err(error(TraceError::MemoryLimit));
        }
        self.cancel.check()?;
        let progressive = self.progressive.as_mut().ok_or_else(|| invalid("missing progressive film"))?;
        progressive.partial = Some(progressive::PartialResolve { plan, frontier, resume_pixel: self.pixel, next: 0, covered });
        Ok(PartialBegin::Started)
    }

    /// One prepaid representative resolve, encoder setup, or encoding row.
    /// Work was debited before allocation; cancel/continuation bounds remain real.
    fn partial_preview_one(&mut self) -> job::Result<Option<ImageAdvance>> {
        self.cancel.check()?;
        let (plan, frontier, resume_pixel, next) = {
            let partial = self.progressive.as_ref().and_then(|p| p.partial.as_ref()).ok_or_else(|| invalid("missing partial resolve state"))?;
            (partial.plan, partial.frontier, partial.resume_pixel, partial.next)
        };
        let count = usize::try_from(plan.pixels()).map_err(|_| error(TraceError::MemoryLimit))?;
        if next < count {
            let index = plan.source_index(next)?;
            let value = self.progressive.as_ref().ok_or_else(|| invalid("missing partial film"))?.film.partial_pixel(index)?;
            let (rgba, covered) = match value {
                crate::film::primary::PartialPixel::Unknown => ([0; 4], 0),
                crate::film::primary::PartialPixel::Sampled { linear_premultiplied, covered, count } => {
                    let [r, g, b] = linear_premultiplied;
                    (
                        composite_spatial(Vec3::new(r, g, b), covered, count, self.request.background, self.visibility_owner.options().exposure_ev)?,
                        covered,
                    )
                }
            };
            if next.checked_mul(4) != Some(self.pixels.len()) {
                return Err(invalid("partial RGBA cursor mismatch"));
            }
            self.cancel.check()?;
            let partial = self.progressive.as_mut().and_then(|p| p.partial.as_mut()).ok_or_else(|| invalid("missing partial cell state"))?;
            if partial.covered.len() != next || partial.covered.capacity() != count {
                return Err(invalid("partial coverage cursor or capacity mismatch"));
            }
            self.pixels.extend_from_slice(&rgba);
            partial.covered.push(covered);
            partial.next += 1;
            return Ok(None);
        }
        if next != count {
            return Err(invalid("partial display cursor exceeded its plan"));
        }
        if self.encoder.is_none() {
            let limit = plan.encoded_limit()?;
            let capacity = usize::try_from(limit).map_err(|_| error(TraceError::MemoryLimit))?;
            let mut output = Vec::new();
            output.try_reserve_exact(capacity).map_err(|_| error(TraceError::AllocationFailed))?;
            if output.capacity() != capacity {
                return Err(error(TraceError::MemoryLimit));
            }
            self.cancel.check()?;
            self.encoder = Some(job::encoding::PngEncoding::with_output(plan.width(), plan.height(), limit, self.cancel.clone(), output)?);
            return Ok(None);
        }
        let png = self.encoder.as_mut().ok_or_else(|| invalid("missing partial encoder"))?.step(&self.pixels)?;
        let Some(png) = png else { return Ok(None) };
        self.cancel.check()?;
        let progressive = self.progressive.as_mut().ok_or_else(|| invalid("missing completed partial film"))?;
        if progressive.film.partial_frontier()? != frontier || self.pixel != resume_pixel || !self.pending.is_none() {
            return Err(invalid("partial snapshot changed its captured sample frontier"));
        }
        let partial = progressive.partial.take().ok_or_else(|| invalid("missing completed partial output"))?;
        if partial.covered.len() != count || partial.covered.capacity() != count {
            return Err(invalid("partial covered output differs from its plan"));
        }
        let updated_prefix = u64::try_from(frontier.updated_prefix).map_err(|_| error(TraceError::MemoryLimit))?;
        progressive.last_partial = Some(frontier);
        self.encoder = None;
        self.pixels.clear();
        // Preserve the complete-pass sentinel too: after host hashing, the
        // ordinary continuation must still charge its tick before P -> 0.
        self.pixel = resume_pixel;
        Ok(Some(ImageAdvance::Partial { png, covered: partial.covered, completed_passes: frontier.completed_passes, updated_prefix }))
    }

    pub(crate) fn step_with_partial_previews(&mut self, units: usize) -> job::Result<PartialImagePoll> {
        if self.progressive.as_ref().is_none_or(|p| p.partial_plan.is_none()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial polling requires the CPU partial policy"));
        }
        match self.advance_image(units)? {
            ImageAdvance::Legacy(ImagePoll::Pending(progress)) => Ok(PartialImagePoll::Pending(progress)),
            ImageAdvance::Legacy(ImagePoll::Complete(png)) => Ok(PartialImagePoll::Complete(png)),
            ImageAdvance::Partial { png, covered, completed_passes, updated_prefix } => {
                Ok(PartialImagePoll::Preview { png, covered, completed_passes, updated_prefix })
            }
            ImageAdvance::Legacy(ImagePoll::Preview { .. }) => Err(invalid("partial policy produced a legacy checkpoint")),
        }
    }

    fn resolve_progressive_one(&mut self) -> job::Result<()> {
        self.work.tick(&self.cancel).map_err(error)?;
        let progressive = self.progressive.as_mut().ok_or_else(|| invalid("missing progressive resolve state"))?;
        let count = self.request.width as usize * self.request.height as usize;
        if progressive.resolve_pixel < count {
            let (rgb, covered, samples) = progressive.film.pixel(progressive.resolve_pixel)?;
            let rgba = composite_spatial(
                Vec3::new(rgb[0], rgb[1], rgb[2]),
                covered,
                samples,
                self.request.background,
                self.visibility_owner.options().exposure_ev,
            )?;
            self.cancel.check()?;
            self.pixels.extend_from_slice(&rgba);
            progressive.resolve_pixel += 1;
        } else {
            let capacity = usize::try_from(self.admission.encoded_limit).map_err(|_| error(TraceError::MemoryLimit))?;
            let mut output = Vec::new();
            output.try_reserve_exact(capacity).map_err(|_| error(TraceError::AllocationFailed))?;
            if output.capacity() != capacity {
                return Err(error(TraceError::MemoryLimit));
            }
            self.cancel.check()?;
            self.encoder = Some(job::encoding::PngEncoding::with_output(
                self.request.width,
                self.request.height,
                self.admission.encoded_limit,
                self.cancel.clone(),
                output,
            )?);
        }
        Ok(())
    }

    /// At most four continuations/encoding rows. One cumulative work budget spans
    /// geometry preparation, every absolute camera sample and every checkpoint.
    pub fn step_with_previews(&mut self, units: usize) -> job::Result<ImagePoll> {
        if self.progressive.as_ref().is_some_and(|p| p.partial_plan.is_some()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial CPU tasks require their additive internal poll"));
        }
        match self.advance_image(units)? {
            ImageAdvance::Legacy(poll) => Ok(poll),
            ImageAdvance::Partial { .. } => Err(invalid("legacy policy produced a partial snapshot")),
        }
    }

    fn advance_image(&mut self, units: usize) -> job::Result<ImageAdvance> {
        self.cancel.check()?;
        if units == 0 || units > 16384 || self.done {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid raytrace step or completed task"));
        }
        for _ in 0..units.min(4) {
            if self.progressive.as_ref().is_some_and(|p| p.partial.is_some()) {
                if let Some(preview) = self.partial_preview_one()? {
                    return Ok(preview);
                }
            } else if self.progressive.as_ref().is_some_and(|p| !p.film.ready()) {
                self.progressive
                    .as_mut()
                    .ok_or_else(|| invalid("missing progressive initialization"))?
                    .film
                    .initialize_one(&self.cancel, &mut || self.work.tick(&self.cancel).map_err(error))?;
            } else if let Some(encoder) = &mut self.encoder {
                if self.progressive.is_some() {
                    self.work
                        .charge(if encoder.row() < self.request.height as usize { u64::from(self.request.width) } else { 1 }, &self.cancel)
                        .map_err(error)?;
                }
                if let Some(png) = encoder.step(&self.pixels)? {
                    self.cancel.check()?;
                    if let Some(progressive) = &mut self.progressive {
                        // The host hashes these exact bytes incrementally. Debit
                        // their full scan here, before handing ownership over.
                        self.work.charge(png.len() as u64, &self.cancel).map_err(error)?;
                        let samples_per_pixel = progressive.film.samples();
                        if samples_per_pixel < progressive.options.samples_per_pixel {
                            progressive.resolving = false;
                            progressive.resolve_pixel = 0;
                            self.encoder = None;
                            self.pixels.clear();
                            self.pixel = 0;
                            return Ok(ImageAdvance::Legacy(ImagePoll::Preview { png, samples_per_pixel }));
                        }
                    }
                    self.release_completed_work()?;
                    return Ok(ImageAdvance::Legacy(ImagePoll::Complete(png)));
                }
            } else if self.progressive.as_ref().is_some_and(|p| p.resolving) {
                self.resolve_progressive_one()?;
            } else if self.environment.as_ref().is_some_and(|e| !e.ready()) {
                self.work.tick(&self.cancel).map_err(error)?;
                self.environment
                    .as_mut()
                    .ok_or_else(|| invalid("missing environment preparation"))?
                    .prepare_one(|| self.cancel.is_cancelled())
                    .map_err(environment_error)?;
            } else if self.bvh.is_some() {
                if self.pixel == self.request.width as usize * self.request.height as usize {
                    if let Some(progressive) = &mut self.progressive {
                        let samples = progressive.film.samples();
                        if self.request.cpu_progressive_every_pass && progressive.partial_plan.is_some() && samples < progressive.options.samples_per_pixel && samples > self.last_pass_boundary
                        {
                            self.last_pass_boundary = samples;
                            return Ok(ImageAdvance::Legacy(ImagePoll::Pending(self.progress())));
                        }
                        self.work.tick(&self.cancel).map_err(error)?;
                        if samples == progressive.options.samples_per_pixel || (progressive.partial_plan.is_none() && (self.request.cpu_progressive_every_pass || samples.is_power_of_two())) {
                            progressive.resolving = true;
                        } else {
                            self.pixel = 0;
                        }
                    } else {
                        self.encoder = Some(job::encoding::PngEncoding::new(
                            self.request.width,
                            self.request.height,
                            self.admission.encoded_limit,
                            self.cancel.clone(),
                        )?);
                    }
                } else {
                    self.pixel_one()?;
                }
            } else if let Some(builder) = &mut self.builder {
                let progress = builder.step(1, &self.cancel).map_err(error)?;
                for _ in self.build_used..progress.work_used {
                    self.work.tick(&self.cancel).map_err(error)?;
                }
                self.build_used = progress.work_used;
                if progress.complete {
                    self.bvh = Some(self.builder.take().ok_or_else(|| invalid("missing builder"))?.finish().map_err(error)?);
                }
            } else {
                self.prepare_one()?;
            }
        }
        Ok(ImageAdvance::Legacy(ImagePoll::Pending(self.progress())))
    }
}
/// Apply the already selected deterministic effect to the shared opaque hit.
/// The captured geometric normal, ray origin, visibility and topology are unchanged.
fn resolved_surface_hit<'a>(
    scene: &PreparedScene<'a>,
    pending: &PendingPixel,
    effects: &[(usize, crate::surface_patterns::PreparedEffect)],
) -> job::Result<SharedHit<'a>> {
    if !pending.surface_ready || pending.surface_cursor != effects.len() {
        return Err(invalid("surface effect is not completely resolved"));
    }
    let mut shared = scene.resolve_hit(pending.ray, pending.hit.ok_or_else(|| invalid("missing prepared surface hit"))?).map_err(error)?;
    if let Some(index) = pending.surface_index {
        let (body_index, effect) = effects.get(index).ok_or_else(|| invalid("missing selected surface effect"))?;
        if *body_index != shared.body_index {
            return Err(invalid("selected surface effect belongs to another body"));
        }
        let changed = effect.apply(shared.surface, shared.shading_normal, shared.oriented_geometric_normal(), shared.position)?;
        shared.surface = changed.surface;
        shared.shading_normal = changed.shading_normal;
    }
    Ok(shared)
}

fn bounded_radiance(v: Vec3) -> bool {
    v.is_finite() && [v.x, v.y, v.z].into_iter().all(|c| (0.0..=1e12).contains(&c))
}
fn primary_offset(samples: u8, ordinal: u8) -> job::Result<[f64; 2]> {
    let side = match samples {
        1 => 1,
        4 => 2,
        16 => 4,
        _ => return Err(invalid("invalid spatial sample count")),
    };
    if ordinal >= samples {
        return Err(invalid("invalid spatial sample ordinal"));
    }
    Ok([(f64::from(ordinal % side) + 0.5) / f64::from(side), (f64::from(ordinal / side) + 0.5) / f64::from(side)])
}
fn spatial_key(pixel: u64, ordinal: u8, samples: u8) -> job::Result<u64> {
    primary_offset(samples, ordinal)?;
    pixel
        .checked_mul(u64::from(samples))
        .and_then(|n| n.checked_add(u64::from(ordinal)))
        .map(|n| n ^ 0x4141_5052_494d_4152)
        .ok_or_else(|| invalid("spatial seed overflow"))
}
/// Distinguish film and spatial ordinals before sampling an emitter. The legacy
/// single-primary first pass keeps its original key and reference image bytes.
fn area_sample_key(pixel: u64, spatial: u8, spatial_samples: u8, film_sample: u32) -> job::Result<u64> {
    primary_offset(spatial_samples, spatial)?;
    if film_sample >= job::MAX_CPU_SAMPLES_PER_PIXEL {
        return Err(invalid("area-light film sample ordinal exceeds the admitted cap"));
    }
    if spatial_samples == 1 && film_sample == 0 {
        return Ok(pixel);
    }
    if film_sample < 4096 {
        // Preserve every historical ordinal and fixed-primary key exactly.
        return pixel
            .checked_mul(4096 * 16)
            .and_then(|key| key.checked_add(u64::from(film_sample) * 16 + u64::from(spatial)))
            .map(|key| key ^ 0x4152_4541_5341_4d50)
            .ok_or_else(|| invalid("area-light sample identity overflow"));
    }
    if pixel >= (1_u64 << 26) || film_sample >= (1_u32 << 20) {
        return Err(invalid("area-light extended sample identity exceeds its fields"));
    }
    let mode = match spatial_samples {
        1 => 0_u64,
        4 => 1,
        16 => 2,
        _ => return Err(invalid("area-light primary mode")),
    };
    // Separate extended ordinals from old keys and reserve ten low bits for
    // the existing light/emitter RNG offsets. No target/budget/cadence key.
    Ok((1_u64 << 63) | (pixel << 36) | (u64::from(film_sample) << 16) | (mode << 14) | (u64::from(spatial) << 10))
}

fn composite_spatial(premultiplied: Vec3, covered: u32, attempts: u32, background: [u8; 3], exposure: f64) -> job::Result<[u8; 4]> {
    if attempts == 0 || attempts > job::MAX_CPU_SAMPLES_PER_PIXEL || covered > attempts || !bounded_radiance(premultiplied) {
        return Err(invalid("invalid spatial coverage/radiance"));
    }
    if covered == 0 {
        if premultiplied != Vec3::ZERO {
            return Err(invalid("uncovered spatial radiance"));
        }
        return Ok([background[0], background[1], background[2], 255]);
    }
    let coverage = f64::from(covered) / f64::from(attempts);
    let geometry = map_to_display(premultiplied / coverage, exposure).map_err(error)?;
    let display = if covered == attempts {
        geometry
    } else {
        let bg = Vec3::new(srgb8_to_linear(background[0]), srgb8_to_linear(background[1]), srgb8_to_linear(background[2]));
        geometry * coverage + bg * (1.0 - coverage)
    };
    Ok([encode8(display.x), encode8(display.y), encode8(display.z), 255])
}

/// One sample preparation, one traversal operation OR one visible-sum operation.
/// Reuses the existing primary/directional cursor: no second task or heap state.
fn punctual_one(
    pending: &mut PendingPixel,
    options: &RaytraceOptions,
    bvh: &Bvh,
    context: (super::prepared::SharedHit<'_>, Option<PlanarTarget>, Option<job::ReflectionMode>),
    work: &mut WorkBudget,
    cancel: &CancelToken,
    diagnostics: &mut job::PunctualDiagnostics,
) -> job::Result<()> {
    let (shared, target, mode) = context;
    if pending.punctual_shadow {
        if let Some(blocker) = pending.cursor.step(bvh, work, cancel).map_err(error)? {
            if blocker.is_some() {
                pending.punctual_value = Vec3::ZERO;
            }
            pending.punctual_shadow = false;
            pending.punctual_ready = true;
        }
        return Ok(());
    }
    work.tick(cancel).map_err(error)?;
    if pending.punctual_ready {
        add_punctual_radiance(pending, pending.punctual_value, diagnostics);
        pending.punctual_index += 1;
        pending.punctual_ready = false;
        return Ok(());
    }
    let inputs = options.punctual_lights.as_ref().ok_or_else(|| invalid("punctual options missing"))?;
    if mode == Some(job::ReflectionMode::Mirror) {
        pending.punctual_index = inputs.len();
        return Ok(());
    }
    let light = inputs.get(pending.punctual_index).copied().ok_or_else(|| invalid("punctual source index"))?.light().map_err(error)?;
    let sample = light.sample(shared.position).map_err(error)?;
    if let Some(reason) = sample.omitted {
        match reason {
            super::punctual::OmittedSample::NearSource => diagnostics.near_source = diagnostics.near_source.saturating_add(1),
            super::punctual::OmittedSample::RadianceLimit => diagnostics.radiance_limit = diagnostics.radiance_limit.saturating_add(1),
        }
        pending.punctual_index += 1;
        return Ok(());
    }
    let geometric = shared.oriented_geometric_normal();
    let cosine = if target.is_some() { geometric } else { shared.shading_normal }.dot(sample.direction).max(0.0);
    let contribution = if geometric.dot(sample.direction) <= 0.0 || cosine == 0.0 || sample.incident_linear_srgb == Vec3::ZERO {
        Vec3::ZERO
    } else {
        let view = -pending.ray.direction();
        let f = if let Some(target) = target {
            target.direct_brdf(shared.surface, geometric, view, sample.direction).map_err(error)?
        } else {
            shared.surface.brdf(shared.shading_normal, view, sample.direction).map_err(|_| invalid("punctual opaque response"))?
        };
        multiply(f, sample.incident_linear_srgb) * cosine
    };
    if super::lighting::validate_radiance(contribution).is_err() {
        diagnostics.radiance_limit = diagnostics.radiance_limit.saturating_add(1);
        pending.punctual_index += 1;
        return Ok(());
    }
    pending.punctual_value = contribution;
    if options.shadows && contribution != Vec3::ZERO {
        let Some(ray) = sample.shadow_ray(shared).map_err(error)? else {
            diagnostics.shadow_offset_crossed = diagnostics.shadow_offset_crossed.saturating_add(1);
            pending.punctual_index += 1;
            return Ok(());
        };
        pending.cursor = TraceCursor::new(ray, true);
        pending.punctual_shadow = true;
    } else {
        add_punctual_radiance(pending, contribution, diagnostics);
        pending.punctual_index += 1;
    }
    Ok(())
}

fn add_punctual_radiance(pending: &mut PendingPixel, contribution: Vec3, diagnostics: &mut job::PunctualDiagnostics) {
    let candidate = pending.radiance + multiply(pending.throughput, contribution);
    if super::lighting::validate_radiance(candidate).is_ok() {
        pending.radiance = candidate;
    } else {
        diagnostics.radiance_limit = diagnostics.radiance_limit.saturating_add(1);
    }
}

fn multiply(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}
#[cfg(test)]
fn shadow_ray(ray: Ray, hit: super::Hit, light: Vec3, vertices: [Vec3; 3]) -> super::Result<Ray> {
    let mut n = hit.geometry.geometric_normal;
    if n.dot(light) < 0.0 {
        n = -n;
    }
    let [a, b, c] = hit.geometry.barycentric;
    let [va, vb, vc] = vertices;
    // Reconstruct from captured vertices, not a distant camera. Component-wise
    // rounding bounds avoid an unrelated large X translation displacing a Z plane.
    let p = va * a + vb * b + vc * c;
    let magnitude = |x: f64, y: f64, z: f64| x.abs() * a.abs() + y.abs() * b.abs() + z.abs() * c.abs();
    let e = Vec3::new(magnitude(va.x, vb.x, vc.x), magnitude(va.y, vb.y, vc.y), magnitude(va.z, vb.z, vc.z)) * (8.0 * f64::EPSILON);
    let distance = n.x.abs() * e.x + n.y.abs() * e.y + n.z.abs() * e.z;
    let shifted = p + n * distance;
    let outward = |v: f64, n: f64| {
        if n > 0.0 {
            v.next_up()
        } else if n < 0.0 {
            v.next_down()
        } else {
            v
        }
    };
    let _ = ray; // hit position comes from geometry, independent of camera distance.
    Ray::new(Vec3::new(outward(shifted.x, n.x), outward(shifted.y, n.y), outward(shifted.z, n.z)), light, 0.0_f64.next_up(), super::MAX_DISTANCE_MM)
}

fn primary_ray(request: &ImageRequest, x: f64, y: f64, clip: (f64, f64)) -> super::Result<Ray> {
    let camera = &request.camera;
    let (mut origin, direction) = camera.ray(x, y, request.width as f64, request.height as f64);
    if camera.fov == 0.0 {
        origin -= camera.back() * (camera.distance * 3.0);
    }
    let forward = -camera.back();
    let cosine = direction.dot(forward);
    if !cosine.is_finite() || cosine <= 0.0 {
        return Err(TraceError::InvalidInput("camera ray direction"));
    }
    let depth = (origin - camera.eye()).dot(forward);
    let start = (clip.0 - depth) / cosine;
    let length = (clip.1 - clip.0) / cosine;
    Ray::new(origin + direction * start, direction, 0.0, length)
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "deterministic_primary_tests.rs"]
mod deterministic_primary_tests;

#[cfg(test)]
#[path = "reflection_image_tests.rs"]
mod reflection_tests;

fn environment_error(e: crate::lighting::HdrError) -> JobError {
    use crate::lighting::HdrErrorCode;
    let code = match e.code {
        HdrErrorCode::Cancelled => ErrorCode::Cancelled,
        HdrErrorCode::LimitExceeded => ErrorCode::LimitExceeded,
        HdrErrorCode::AllocationFailed => ErrorCode::AllocationFailed,
        _ => ErrorCode::InvalidRequest,
    };
    JobError::new(code, e.to_string())
}

#[cfg(test)]
#[path = "primary_aa_tests.rs"]
mod primary_aa_tests;

// One bounded continuation operation, using the same cursor, work accumulator and task.
fn area_one(
    pending: &mut PendingPixel,
    options: &RaytraceOptions,
    bvh: &Bvh,
    context: (super::prepared::SharedHit<'_>, Option<PlanarTarget>, Option<job::ReflectionMode>),
    work: &mut WorkBudget,
    cancel: &CancelToken,
    pixel: u64,
) -> job::Result<()> {
    let (shared, target, mode) = context;
    if pending.area_shadow {
        if let Some(blocker) = pending.cursor.step(bvh, work, cancel).map_err(error)? {
            if blocker.is_some() {
                pending.area_value = Vec3::ZERO;
            }
            pending.area_shadow = false;
            pending.area_ready = true;
        }
        return Ok(());
    }
    work.tick(cancel).map_err(error)?;
    if pending.area_ready {
        pending.radiance += multiply(pending.throughput, pending.area_value);
        super::lighting::validate_radiance(pending.radiance).map_err(error)?;
        pending.area_index += 1;
        pending.area_ready = false;
        return Ok(());
    }
    let inputs = options.area_lights.as_ref().ok_or_else(|| invalid("area options missing"))?;
    if mode == Some(job::ReflectionMode::Mirror) {
        pending.area_light = inputs.len();
        return Ok(());
    }
    let count = options.area_samples.unwrap_or(16);
    if pending.area_index == count {
        pending.area_light += 1;
        pending.area_index = 0;
        return Ok(());
    }
    let input = inputs.get(pending.area_light).ok_or_else(|| invalid("area source index"))?;
    let vec = |[x, y, z]: [f64; 3]| Vec3::new(x, y, z);
    let light = super::area::RectangleLight::new(vec(input.center_mm), vec(input.edge_u_mm), vec(input.edge_v_mm), vec(input.color), input.intensity)
        .map_err(error)?;
    let Some(sample) =
        light.sample(shared.position, pixel, pending.area_light as u32, pending.area_index, count, options.seed.unwrap_or(0)).map_err(error)?
    else {
        pending.area_index += 1;
        return Ok(());
    };
    let geometric = shared.oriented_geometric_normal();
    let cosine = if target.is_some() { geometric } else { shared.shading_normal }.dot(sample.direction()).max(0.0);
    let contribution = if geometric.dot(sample.direction()) <= 0.0 || cosine == 0.0 {
        Vec3::ZERO
    } else {
        let view = -pending.ray.direction();
        let f = if let Some(target) = target {
            target.direct_brdf(shared.surface, geometric, view, sample.direction()).map_err(error)?
        } else {
            shared.surface.brdf(shared.shading_normal, view, sample.direction()).map_err(|_| invalid("area opaque response"))?
        };
        multiply(f, sample.incident()) * cosine
    };
    super::lighting::validate_radiance(contribution).map_err(error)?;
    pending.area_value = contribution;
    if options.shadows && contribution != Vec3::ZERO {
        let origin = shared.spawn(sample.direction()).map_err(error)?.origin();
        pending.cursor = TraceCursor::new(sample.shadow_ray(origin).map_err(error)?, true);
        pending.area_shadow = true;
    } else {
        pending.radiance += multiply(pending.throughput, contribution);
        super::lighting::validate_radiance(pending.radiance).map_err(error)?;
        pending.area_index += 1;
    }
    Ok(())
}

#[cfg(test)]
#[path = "punctual_image_tests.rs"]
mod punctual_image_tests;
