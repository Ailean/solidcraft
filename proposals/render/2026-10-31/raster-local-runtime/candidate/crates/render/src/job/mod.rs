//! Product-image work and its device-independent, session-scoped protocol.
//! Hosts own execution; documents and history never contain these values.

use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{BackendId, Camera, ExecutorId};

pub mod adaptive;
pub mod aging;
pub mod bloom;
mod cpu_budget;
pub(crate) mod encoding;
pub mod finishing;
mod host;
pub(crate) mod image;
pub mod interactive;
pub mod led_strip;
mod lens;
#[cfg(test)]
mod light_union_tests;
mod lighting;
pub mod linear_exr;
mod linear_exr_decode;
pub mod looks;
pub mod photographic;
mod png;
pub mod presentation;
mod punctual;
mod raster_lights;
mod regrade;
#[cfg(test)]
mod rt2_tests;
pub mod textures;
pub mod volume;

pub use cpu_budget::{CpuBudget, CpuBudgetLimits, MAX_CPU_BODIES, MAX_CPU_SAMPLES_PER_PIXEL, MAX_CPU_TRIANGLES, MAX_CPU_WORK, NATIVE_CPU_POOL_BYTES};
#[cfg(feature = "raytrace-cpu")]
pub(crate) use host::FRAME_METADATA_BYTES;
pub(crate) use host::Reservation;
pub use host::{
    CpuHost, HostKind, ImageHost, JobEnvironmentInfo, JobHandle, JobStatus, LinearAlphaConvention, LinearColourEncoding, LinearMasterInfo,
    PartialPreviewInfo, PartialPreviewProjection, PreviewInfo, RenderFrame, RenderPartialPreview, RenderPreview,
};
pub use image::{Admission, ImageTask, TaskPoll, admit, admit_geometry, admit_geometry_with_limits};
pub use lens::ThinLens;
pub use lighting::{AreaLight, AreaLights, MAX_AREA_LIGHTS, StudioLighting, Sun, ToneMap};
pub use png::PngTask;
pub use punctual::{PunctualDiagnostics, PunctualLightOptions, SpotConeOptions};
pub use raster_lights::{
    deserialize as deserialize_raster_lights, deserialize_areas as deserialize_raster_area_lights,
    deserialize_punctual as deserialize_raster_punctual_lights, validate as validate_raster_lights,
    validate_areas as validate_raster_area_lights, validate_punctual as validate_raster_punctual_lights,
};

/// Native path-tracing ceilings, further restricted by the actual device.
/// Kept in the device-independent layer so saved intent and UI agree with execution.
pub const GPU_PATH_LIMITS: crate::RenderLimits =
    crate::RenderLimits { max_dimension: 8192, max_pixels: 16_777_216, max_scratch_bytes: 8 * 1024 * 1024 * 1024 };
pub const MAX_GPU_PATH_TRIANGLES: u64 = 8_000_000;
pub const MAX_GPU_PATH_BODIES: usize = 65_536;
pub const MAX_GPU_DIELECTRICS: usize = 32;

/// Runtime sampling intent, separate from saved documents and device resources.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JobOptions {
    #[serde(skip_serializing_if = "aging::AgingStack::is_empty")]
    pub aging: aging::AgingStack,
    pub nested_media: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<volume::HomogeneousVolume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<adaptive::AdaptiveSampling>,
    /// Conditional Fresnel strata per primary sample; one keeps the old stream.
    pub transmission_samples: u32,

    #[serde(default, skip_serializing_if = "finishing::Finishing::is_identity")]
    pub finishing: finishing::Finishing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<finishing::Backdrop>,
    #[serde(default)]
    pub linear_exr: bool,
    pub samples: u32,
    pub max_depth: u32,
    pub seed: u32,
    pub exposure_ev: f64,
    pub lens: Option<ThinLens>,
    pub lighting: Option<StudioLighting>,
    pub tone_map: ToneMap,
    /// Optional final spatial filter. Raw progressive accumulation remains unchanged.
    pub denoise: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            aging: Default::default(),
            nested_media: false,
            volume: None,
            adaptive: None,
            transmission_samples: 1,
            finishing: Default::default(),
            backdrop: None,
            linear_exr: false,
            samples: 64,
            max_depth: 8,
            seed: 0,
            exposure_ev: 0.0,
            lens: None,
            lighting: None,
            tone_map: ToneMap::default(),
            denoise: false,
        }
    }
}

impl JobOptions {
    pub fn validate(&self) -> Result<()> {
        self.aging.validate()?;
        if let Some(volume) = self.volume {
            volume.validate()?;
            if volume.active() && self.transmission_samples != 1 {
                return Err(JobError::new(
                    ErrorCode::InvalidRequest,
                    "volume transport requires one transmission stratum; glass combinations are not supported",
                ));
            }
            if volume.active() && self.denoise {
                return Err(JobError::new(
                    ErrorCode::InvalidRequest,
                    "volume scattering has no surface-denoiser guides; disable denoising explicitly",
                ));
            }
        }
        if let Some(adaptive) = self.adaptive {
            adaptive.validate(self.samples)?;
        }
        if !matches!(self.transmission_samples, 1 | 2 | 4) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "transmission_samples must be 1, 2 or 4"));
        }
        self.finishing.validate()?;
        if let Some(backdrop) = self.backdrop {
            backdrop.validate()?;
        }
        if let Some(lighting) = self.lighting {
            lighting.validate()?;
        }
        if let Some(lens) = self.lens {
            lens.validate()?;
        }
        if !(1..=4096).contains(&self.samples)
            || !(1..=16).contains(&self.max_depth)
            || !self.exposure_ev.is_finite()
            || !(-32.0..=32.0).contains(&self.exposure_ev)
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "samples must be 1..4096, depth 1..16 and exposure -32..32 EV"));
        }
        Ok(())
    }
}

/// One bounded quantum of image work. Implementations check cancellation between quanta.
pub trait ImageWork: Send {
    fn sampling_report(&self) -> Option<adaptive::SamplingReport> {
        None
    }
    /// Optional raw linear companion, moved exactly once after PNG completion.
    /// The host validates intent, bounds, hashes and retains its memory lease.
    fn take_linear_exr(&mut self) -> Option<Vec<u8>> {
        None
    }
    fn step(&mut self, units: usize) -> Result<TaskPoll>;
}
impl ImageWork for ImageTask {
    fn step(&mut self, units: usize) -> Result<TaskPoll> {
        self.step(units)
    }
}

/// Device-independent executor boundary. The host owns admission leases and publication;
/// an executor owns computation only. GPU implementations live above the render crate.
pub trait ImageExecutor: Send + Sync {
    fn supports_aging(&self) -> bool {
        false
    }
    fn supports_photographic(&self) -> bool {
        false
    }
    fn supports_subsurface(&self) -> bool {
        false
    }
    fn supports_nested_media(&self) -> bool {
        false
    }
    fn supports_volume(&self) -> bool {
        false
    }
    fn supports_adaptive_sampling(&self) -> bool {
        false
    }
    fn supports_transmission_sampling(&self) -> bool {
        false
    }
    fn supports_bloom(&self) -> bool {
        false
    }
    fn supports_finishing(&self) -> bool {
        false
    }
    fn supports_linear_exr(&self) -> bool {
        false
    }
    /// Host-owned logical allocation pool. This is a ceiling, not eager allocation.
    /// Registration snapshots and validates it; unknown executors retain 512 MiB.
    fn memory_limit_bytes(&self) -> u64 {
        512 * 1024 * 1024
    }
    fn descriptor(&self) -> Result<crate::ExecutorDescriptor>;
    fn admit(&self, scene: &crate::ProductScene, request: &ImageRequest, options: JobOptions) -> Result<Admission>;
    fn begin(&self, scene: crate::ProductScene, request: ImageRequest, options: JobOptions, cancel: CancelToken) -> Result<Box<dyn ImageWork>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobId {
    pub session: u64,
    pub serial: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Queued,
    Preparing,
    Running,
    Encoding,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

impl Phase {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed | Self::Failed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    LimitExceeded,
    AllocationFailed,
    HostUnavailable,
    QueueFull,
    TimedOut,
    Cancelled,
    InvalidScene,
    EncodingFailed,
    WrongOwner,
    UnknownJob,
    NotCompleted,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
}

impl JobError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for JobError {}

pub type Result<T> = std::result::Result<T, JobError>;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(JobError::new(ErrorCode::Cancelled, "render cancelled")) } else { Ok(()) }
    }
}

/// Per-request execution choice, independent of viewport graphics presets.
/// Keep both variants parseable in builds without optional raytracing, so those
/// builds return an explicit capability error before capturing model geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Integrator {
    #[default]
    Raster,
    Raytrace,
}

/// Explicit scene-reflection transport, separate from opaque roughness bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionMode {
    Mirror,
    RoughSpecular,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReflectionBody {
    pub body: String,
    pub mode: ReflectionMode,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReflectionOptions {
    #[serde(deserialize_with = "bounded_reflection_bodies")]
    pub bodies: Vec<ReflectionBody>,
    pub max_bounces: u8,
    pub samples: u8,
}
impl Default for ReflectionOptions {
    fn default() -> Self {
        Self { bodies: Vec::new(), max_bounces: 2, samples: 4 }
    }
}
fn bounded_reflection_bodies<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<ReflectionBody>, D::Error> {
    struct Bodies;
    impl<'de> serde::de::Visitor<'de> for Bodies {
        type Value = Vec<ReflectionBody>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most64 reflection body overrides")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == 64 {
                    return Err(serde::de::Error::custom("at most64 reflection bodies"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Bodies)
}
impl ReflectionOptions {
    pub fn validate(&self) -> Result<()> {
        if !(1..=8).contains(&self.max_bounces) || !(1..=64).contains(&self.samples) || self.bodies.len() > 64 || self.bodies.capacity() > 64 {
            return Err(JobError::new(ErrorCode::InvalidRequest, "reflection bounds: 1..8 bounces, 1..64 samples, at most64 body slots"));
        }
        for (index, body) in self.bodies.iter().enumerate() {
            if body.body.is_empty()
                || body.body.len() > 128
                || body.body.capacity() > 128
                || self.bodies.iter().take(index).any(|other| other.body == body.body)
            {
                return Err(JobError::new(ErrorCode::InvalidRequest, "reflection body names must be unique, nonempty and bounded to128 bytes"));
            }
        }
        Ok(())
    }
}

/// Linear-light runtime inputs. These are never design settings or GPU presets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectionalLightOptions {
    pub direction: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingPreset {
    Neutral,
    WarmCool,
    KeyRim,
}

impl LightingPreset {
    /// Portable source values shared by CPU transport and light inspection.
    pub fn directional_lights(self) -> Vec<DirectionalLightOptions> {
        let light = |direction, color, intensity| DirectionalLightOptions { direction, color, intensity };
        match self {
            Self::Neutral => crate::studio::LIGHTS.into_iter().map(|(d, intensity)| light([d.x, d.y, d.z], [1.0; 3], intensity)).collect(),
            Self::WarmCool => vec![
                light([0.6, 0.0, 0.8], [1.0, 0.72, 0.48], 3.0),
                light([-0.8, 0.0, 0.6], [0.45, 0.65, 1.0], 1.0),
                light([0.0, -0.6, 0.8], [1.0; 3], 1.5),
            ],
            Self::KeyRim => vec![light([0.6, -0.4, 0.8], [1.0; 3], 3.0), light([-0.6, 0.4, 0.8], [0.6, 0.8, 1.0], 2.0)],
        }
    }
}

/// Full world-space side vectors; U cross V is the emitting side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RectangleLightOptions {
    pub center_mm: [f64; 3],
    pub edge_u_mm: [f64; 3],
    pub edge_v_mm: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
}

impl RectangleLightOptions {
    /// Canonical numerical domain shared by CPU transport and schematic Raster.
    pub(crate) fn validate(&self) -> Result<()> {
        use solidcraft_geom::Vec3;
        let vec = |v: [f64; 3]| Vec3::new(v[0], v[1], v[2]);
        let c = vec(self.center_mm);
        let u = vec(self.edge_u_mm);
        let v = vec(self.edge_v_mm);
        if !c.is_finite()
            || !u.is_finite()
            || !v.is_finite()
            || !self.color.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            || !self.intensity.is_finite()
            || !(0.0..=64.0).contains(&self.intensity)
            || !(1e-6..=1e6).contains(&u.len())
            || !(1e-6..=1e6).contains(&v.len())
            || (u.dot(v) / (u.len() * v.len())).abs() > 1e-10
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid finite rectangular light"));
        }
        for a in [-0.5, 0.5] {
            for b in [-0.5, 0.5] {
                let p = c + u * a + v * b;
                if !p.is_finite() || [p.x, p.y, p.z].into_iter().any(|x| x.abs() > 1e9) {
                    return Err(JobError::new(ErrorCode::InvalidRequest, "rectangular light corner exceeds world bounds"));
                }
            }
        }
        Ok(())
    }
}

/// Explicit visual rotation about a certified planar face's outward normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnisotropicOrientation {
    #[default]
    Zero,
    QuarterTurn,
}

/// Runtime-only captured planar face selector; no persistent naming is invented.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnisotropicFaceOptions {
    pub body_index: usize,
    #[serde(deserialize_with = "bounded_anisotropic_body_name")]
    pub body_name: String,
    pub face: u32,
    pub tangent: [f64; 3],
    pub orientation: AnisotropicOrientation,
    pub roughness_x: f64,
    pub roughness_y: f64,
}

fn bounded_anisotropic_body_name<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    struct Name;
    impl serde::de::Visitor<'_> for Name {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("captured body name of 1..1024 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<String, E> {
            if value.is_empty() || value.len() > 1024 {
                return Err(E::custom("captured body name of 1..1024 bytes required"));
            }
            let mut name = String::new();
            name.try_reserve_exact(value.len()).map_err(E::custom)?;
            name.push_str(value);
            Ok(name)
        }
    }
    d.deserialize_str(Name)
}

/// Emission is relative linear-sRGB radiance, independent of reflected color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmissionSides {
    #[default]
    Front,
    Both,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceEmission {
    pub body_index: usize,
    #[serde(deserialize_with = "bounded_emission_body")]
    pub body: String,
    pub color: [f64; 3],
    pub strength: f64,
    #[serde(default)]
    pub sides: EmissionSides,
}

fn bounded_emission_body<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    struct Name;
    impl<'de> serde::de::Visitor<'de> for Name {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a nonempty captured body name of at most 256 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<String, E> {
            if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(E::custom("invalid surface emission body name"));
            }
            let mut name = String::new();
            name.try_reserve_exact(value.len()).map_err(E::custom)?;
            name.push_str(value);
            Ok(name)
        }
    }
    d.deserialize_str(Name)
}

impl SurfaceEmission {
    pub fn validate(&self) -> Result<()> {
        if self.body.is_empty()
            || self.body.len() > 256
            || self.body.capacity() > 256
            || self.body.chars().any(char::is_control)
            || !self.color.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            || !self.strength.is_finite()
            || !(0.0..=64.0).contains(&self.strength)
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid bounded surface emission"));
        }
        Ok(())
    }
}

fn bounded_emissions<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<SurfaceEmission>>, D::Error> {
    struct Emitters;
    impl<'de> serde::de::Visitor<'de> for Emitters {
        type Value = Vec<SurfaceEmission>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 32 surface emitters")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<SurfaceEmission>()? {
                if values.len() == 32 {
                    return Err(serde::de::Error::custom("at most 32 surface emitters"));
                }
                value.validate().map_err(serde::de::Error::custom)?;
                values.try_reserve_exact(1).map_err(serde::de::Error::custom)?;
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Emitters).map(Some)
}

/// Explicit runtime optics; never inferred from opacity or saved into a design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DielectricBoundary {
    ThinSheet,
    ClosedBox,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DielectricOptions {
    pub body_index: usize,
    #[serde(deserialize_with = "bounded_dielectric_body_name")]
    pub body_name: String,
    pub boundary: DielectricBoundary,
    pub ior: f64,
    /// Per-channel Beer-Lambert extinction, inverse millimetres.
    pub absorption_per_mm: [f64; 3],
    /// Virtual normal thickness for ThinSheet only; ClosedBox requires zero.
    pub thickness_mm: f64,
}

/// Explicit constant secondary-ray environment; independent of display background.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaytraceEnvironmentOptions {
    pub color: [f64; 3],
    pub intensity: f64,
}

fn bounded_dielectric_body_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<String, D::Error> {
    struct Name;
    impl serde::de::Visitor<'_> for Name {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("captured body name of 1..1024 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<String, E> {
            if value.is_empty() || value.len() > 1024 {
                return Err(E::custom("captured body name of 1..1024 bytes required"));
            }
            let mut name = String::new();
            name.try_reserve_exact(value.len()).map_err(E::custom)?;
            name.push_str(value);
            Ok(name)
        }
    }
    deserializer.deserialize_str(Name)
}

fn bounded_dielectrics<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<DielectricOptions>, D::Error> {
    struct Inputs;
    impl<'de> serde::de::Visitor<'de> for Inputs {
        type Value = Vec<DielectricOptions>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most eight dielectric overrides")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error> {
            let mut inputs = Vec::new();
            inputs.try_reserve_exact(8).map_err(serde::de::Error::custom)?;
            while let Some(input) = sequence.next_element()? {
                if inputs.len() == 8 {
                    return Err(serde::de::Error::custom("at most eight dielectric overrides"));
                }
                inputs.push(input);
            }
            Ok(inputs)
        }
    }
    deserializer.deserialize_seq(Inputs)
}

/// CPU camera sampling, independent of the registered GPU's `progressive` wire flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CpuProgressiveOptions {
    pub samples_per_pixel: u32,
    pub seed: u32,
}

impl Default for CpuProgressiveOptions {
    fn default() -> Self {
        Self { samples_per_pixel: 16, seed: 0 }
    }
}

impl CpuProgressiveOptions {
    pub const MAX_PIXELS: u64 = 1_048_576;
    pub const MAX_CAMERA_SAMPLES: u64 = 16_777_216;

    pub fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.samples_per_pixel) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "CPU progressive samples_per_pixel must be 1..64"));
        }
        Ok(())
    }

    /// Compatibility entry point: a caller that supplies no profile retains Compact.
    pub fn validate_image(&self, width: u32, height: u32) -> Result<()> {
        self.validate_image_with_budget(width, height, CpuBudget::Compact)
    }

    pub fn validate_image_with_budget(&self, width: u32, height: u32, budget: CpuBudget) -> Result<()> {
        budget.validate_image(width, height, Some(*self))
    }

    /// Nonfinal checkpoints whose PNG allocations may remain externally retained.
    #[cfg(feature = "raytrace-cpu")]
    pub(crate) fn preview_count(&self) -> u32 {
        if self.samples_per_pixel <= 1 { 0 } else { (self.samples_per_pixel - 1).ilog2() + 1 }
    }
}

/// Runtime CPU dielectric transport policy, independent of resource and preview budgets.
/// StrictTir preserves deterministic TIR and refuses positive dielectric-depth tails.
/// It does not promise convergence or general physical glass support.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuDielectricTransport {
    #[default]
    LegacyBounded,
    StrictTir,
}

impl CpuDielectricTransport {
    pub fn is_legacy(&self) -> bool {
        *self == Self::LegacyBounded
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RaytraceOptions {
    #[serde(skip_serializing_if = "CpuDielectricTransport::is_legacy")]
    pub dielectric_transport: CpuDielectricTransport,
    #[serde(skip_serializing_if = "CpuBudget::is_compact")]
    pub budget: CpuBudget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progressive: Option<CpuProgressiveOptions>,
    #[serde(deserialize_with = "bounded_dielectrics", skip_serializing_if = "Vec::is_empty")]
    pub dielectrics: Vec<DielectricOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<RaytraceEnvironmentOptions>,
    #[serde(deserialize_with = "crate::surface_patterns::deserialize_effects", skip_serializing_if = "Option::is_none")]
    pub surface_effects: Option<Vec<crate::surface_patterns::SurfaceEffect>>,
    #[serde(deserialize_with = "bounded_emissions", skip_serializing_if = "Option::is_none")]
    pub emissions: Option<Vec<SurfaceEmission>>,
    /// Transient spatial pixel-box samples; independent of reflection attempts.
    #[serde(skip_serializing_if = "single_primary_sample")]
    pub primary_samples: u8,
    pub reflections: Option<ReflectionOptions>,
    pub anisotropic_faces: [Option<AnisotropicFaceOptions>; 2],
    #[serde(deserialize_with = "punctual::bounded_punctual", skip_serializing_if = "Option::is_none")]
    pub punctual_lights: Option<Vec<PunctualLightOptions>>,
    #[serde(deserialize_with = "bounded_area_lights", skip_serializing_if = "Option::is_none")]
    pub area_lights: Option<Vec<RectangleLightOptions>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_samples: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    pub preset: Option<LightingPreset>,
    #[serde(deserialize_with = "bounded_lights", skip_serializing_if = "Option::is_none")]
    pub lights: Option<Vec<DirectionalLightOptions>>,
    pub exposure_ev: f64,
    pub shadows: bool,
}

impl Default for RaytraceOptions {
    fn default() -> Self {
        Self {
            dielectric_transport: CpuDielectricTransport::LegacyBounded,
            budget: CpuBudget::Compact,
            progressive: None,
            punctual_lights: None,
            area_lights: None,
            area_samples: None,
            seed: None,
            primary_samples: 1,
            emissions: None,
            surface_effects: None,
            dielectrics: Vec::new(),
            environment: None,
            anisotropic_faces: [None, None],
            reflections: None,
            preset: None,
            lights: None,
            exposure_ev: 0.0,
            shadows: true,
        }
    }
}

fn single_primary_sample(samples: &u8) -> bool {
    *samples == 1
}

fn bounded_lights<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<DirectionalLightOptions>>, D::Error> {
    struct Lights;
    impl<'de> serde::de::Visitor<'de> for Lights {
        type Value = Vec<DirectionalLightOptions>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most eight directional lights")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error> {
            let mut lights = Vec::new();
            while let Some(light) = sequence.next_element()? {
                if lights.len() == 8 {
                    return Err(serde::de::Error::custom("at most eight directional lights"));
                }
                lights.push(light);
            }
            Ok(lights)
        }
    }
    deserializer.deserialize_seq(Lights).map(Some)
}

fn bounded_area_lights<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<RectangleLightOptions>>, D::Error> {
    struct AreaLights;
    impl<'de> serde::de::Visitor<'de> for AreaLights {
        type Value = Vec<RectangleLightOptions>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most four rectangular lights")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
            let mut lights = Vec::new();
            while let Some(light) = seq.next_element()? {
                if lights.len() == 4 {
                    return Err(serde::de::Error::custom("at most four rectangular lights"));
                }
                if lights.is_empty() {
                    lights.try_reserve_exact(4).map_err(serde::de::Error::custom)?;
                    if lights.capacity() > 4 {
                        return Err(serde::de::Error::custom("rectangular light capacity exceeds four"));
                    }
                }
                lights.push(light);
            }
            Ok(lights)
        }
    }
    struct Optional;
    impl<'de> serde::de::Visitor<'de> for Optional {
        type Value = Option<Vec<RectangleLightOptions>>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an optional bounded rectangular-light list")
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<Self::Value, D::Error> {
            d.deserialize_seq(AreaLights).map(Some)
        }
    }
    deserializer.deserialize_option(Optional)
}

impl RaytraceOptions {
    /// Dynamic dielectric capacities in one task or immutable request copy.
    pub(crate) fn retained_dielectric_bytes(&self) -> Result<u64> {
        let limit = || JobError::new(ErrorCode::LimitExceeded, "dielectric request capacity overflow");
        let mut bytes = (self.dielectrics.capacity() as u64).checked_mul(std::mem::size_of::<DielectricOptions>() as u64).ok_or_else(limit)?;
        for input in &self.dielectrics {
            bytes = bytes.checked_add(input.body_name.capacity() as u64).ok_or_else(limit)?;
        }
        Ok(bytes)
    }
    /// Dynamic surface-effect capacities in one task or retained request copy.
    pub(crate) fn retained_surface_effect_bytes(&self) -> Result<u64> {
        let limit = || JobError::new(ErrorCode::LimitExceeded, "surface effect capacity overflow");
        let slots = self.surface_effects.as_ref().map_or(0, Vec::capacity) as u64;
        let mut bytes = slots.checked_mul(std::mem::size_of::<crate::surface_patterns::SurfaceEffect>() as u64).ok_or_else(limit)?;
        if let Some(values) = &self.surface_effects {
            for value in values {
                bytes = bytes.checked_add(value.body.capacity() as u64).ok_or_else(limit)?;
            }
        }
        Ok(bytes)
    }
    /// Dynamic emission capacities in one options copy; included in both task and Record admission.
    pub(crate) fn retained_emission_bytes(&self) -> Result<u64> {
        let limit = || JobError::new(ErrorCode::LimitExceeded, "surface emission capacity overflow");
        let slots = self.emissions.as_ref().map_or(0, Vec::capacity) as u64;
        let mut bytes = slots.checked_mul(std::mem::size_of::<SurfaceEmission>() as u64).ok_or_else(limit)?;
        if let Some(values) = &self.emissions {
            for value in values {
                bytes = bytes.checked_add(value.body.capacity() as u64).ok_or_else(limit)?;
            }
        }
        Ok(bytes)
    }
    /// One capacity calculation shared by host and engine preflight.
    pub fn retained_bytes(&self) -> Result<u64> {
        self.validate()?;
        let limit = || JobError::new(ErrorCode::LimitExceeded, "raytrace request capacity overflow");
        let mut bytes = self
            .lights
            .as_ref()
            .map_or(Some(0), |l| (l.capacity() as u64).checked_mul(std::mem::size_of::<DirectionalLightOptions>() as u64))
            .ok_or_else(limit)?;
        for (capacity, size) in [
            (self.area_lights.as_ref().map_or(0, Vec::capacity), std::mem::size_of::<RectangleLightOptions>()),
            (self.punctual_lights.as_ref().map_or(0, Vec::capacity), std::mem::size_of::<PunctualLightOptions>()),
        ] {
            bytes = bytes.checked_add((capacity as u64).checked_mul(size as u64).ok_or_else(limit)?).ok_or_else(limit)?;
        }
        if let Some(reflections) = &self.reflections {
            let slots = (reflections.bodies.capacity() as u64).checked_mul(std::mem::size_of::<ReflectionBody>() as u64).ok_or_else(limit)?;
            bytes = bytes.checked_add(slots).ok_or_else(limit)?;
            for body in &reflections.bodies {
                bytes = bytes.checked_add(body.body.capacity() as u64).ok_or_else(limit)?;
            }
        }
        for target in self.anisotropic_faces.iter().flatten() {
            bytes = bytes.checked_add(target.body_name.capacity() as u64).ok_or_else(limit)?;
        }
        bytes = bytes.checked_add(self.retained_emission_bytes()?).ok_or_else(limit)?;
        bytes = bytes.checked_add(self.retained_surface_effect_bytes()?).ok_or_else(limit)?;
        bytes = bytes.checked_add(self.retained_dielectric_bytes()?).ok_or_else(limit)?;
        Ok(bytes)
    }
    /// No environment NEE on selected reflectors; rough transport must be purely metallic.
    /// Rejecting every nonmetallic effective face prevents silently dropping its diffuse HDR.
    pub fn validate_environment_reflections(&self, scene: &crate::ProductScene) -> Result<()> {
        self.validate()?;
        if self.environment.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "choose either the HDR input or constant secondary environment"));
        }
        if let Some(reflections) = &self.reflections {
            for selector in &reflections.bodies {
                let mut bodies = scene.meshes.iter().filter(|b| b.name == selector.body);
                let body = bodies.next().ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "environment reflection body missing"))?;
                if bodies.next().is_some() {
                    return Err(JobError::new(ErrorCode::InvalidRequest, "ambiguous environment reflection body"));
                }
                if selector.mode == ReflectionMode::RoughSpecular
                    && (body.surface.is_none_or(|s| s.metallic() != 1.0) || body.faces.iter().any(|f| f.surface.is_none_or(|s| s.metallic() != 1.0)))
                {
                    return Err(JobError::new(
                        ErrorCode::InvalidRequest,
                        "HDR rough reflection requires fully metallic body and face surfaces; diffuse/specular partition is not supported",
                    ));
                }
            }
        }
        for target in self.anisotropic_faces.iter().flatten() {
            let mode = self.reflections.as_ref().and_then(|r| r.bodies.iter().find(|b| b.body == target.body_name).map(|b| b.mode));
            if mode != Some(ReflectionMode::RoughSpecular) {
                return Err(JobError::new(
                    ErrorCode::InvalidRequest,
                    "HDR anisotropy requires a selected fully metallic rough reflector; anisotropic HDR direct lighting is not supported",
                ));
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        // Opaque reflection has a separate bounded continuation policy. Do not
        // advertise strict dielectric tails across that unqualified consumer.
        if self.dielectric_transport == CpuDielectricTransport::StrictTir && self.reflections.as_ref().is_some_and(|r| !r.bodies.is_empty()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "strict CPU dielectric transport does not support selected opaque reflections yet"));
        }
        if let Some(progressive) = self.progressive {
            self.budget.validate_samples(progressive.samples_per_pixel)?;
            if self.primary_samples != 1 || self.reflections.as_ref().is_some_and(|r| !r.bodies.is_empty() && r.samples != 1) {
                return Err(JobError::new(
                    ErrorCode::InvalidRequest,
                    "CPU progressive requires primary_samples: 1 and one reflection attempt per camera sample",
                ));
            }
            // Smooth certified dielectric trees now complete one real camera
            // sample before film accumulation. Anisotropic progressive transport
            // remains unsupported; geometry/identity certificates run at capture.
            if self.anisotropic_faces.iter().any(Option::is_some) {
                return Err(JobError::new(ErrorCode::InvalidRequest, "CPU progressive does not support anisotropic inputs; use fixed sampling"));
            }
        }
        if self.environment.is_some_and(|environment| {
            !environment.color.into_iter().all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                || !environment.intensity.is_finite()
                || !(0.0..=64.0).contains(&environment.intensity)
        }) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid constant raytrace environment"));
        }
        if self.dielectrics.len() > 8 || self.dielectrics.capacity() > 8 {
            return Err(JobError::new(ErrorCode::InvalidRequest, "at most eight dielectric overrides"));
        }
        for (index, input) in self.dielectrics.iter().enumerate() {
            if input.body_name.is_empty()
                || input.body_name.len() > 1024
                || input.body_name.capacity() > 1024
                || !input.ior.is_finite()
                || !(1.0..=3.0).contains(&input.ior)
                || !input.absorption_per_mm.into_iter().all(|v| v.is_finite() && (0.0..=100.0).contains(&v))
                || !input.thickness_mm.is_finite()
                || !(0.0..=10000.0).contains(&input.thickness_mm)
                || (input.boundary == DielectricBoundary::ClosedBox && input.thickness_mm != 0.0)
                || self.dielectrics.iter().take(index).any(|previous| previous.body_index == input.body_index)
            {
                return Err(JobError::new(ErrorCode::InvalidRequest, "invalid or duplicate dielectric override"));
            }
        }
        if let Some(values) = &self.surface_effects {
            if values.capacity() > 32 {
                return Err(JobError::new(ErrorCode::InvalidRequest, "at most 32 surface-effect reserved slots"));
            }
            crate::surface_patterns::validate_effects(values)?;
        }
        if let Some(values) = &self.emissions {
            if values.len() > 32 || values.capacity() > 32 {
                return Err(JobError::new(ErrorCode::InvalidRequest, "at most 32 surface emitters"));
            }
            for (i, value) in values.iter().enumerate() {
                value.validate()?;
                if values.iter().take(i).any(|other| other.body_index == value.body_index) {
                    return Err(JobError::new(ErrorCode::InvalidRequest, "duplicate surface emission target"));
                }
            }
        }
        let area_count = self.area_lights.as_ref().map_or(0, Vec::len);
        if self.area_lights.as_ref().is_some_and(|v| v.len() > 4 || v.capacity() > 4) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "at most four rectangular lights and reserved slots"));
        }
        if area_count == 0 && (self.area_samples.is_some() || self.seed.is_some()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "area_samples and seed require nonempty area_lights"));
        }
        if self.area_samples.is_some_and(|n| !matches!(n, 1 | 4 | 16 | 64)) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "area_samples must be 1, 4, 16 or 64"));
        }
        let directional_count = self.lights.as_ref().map_or_else(|| if self.preset == Some(LightingPreset::KeyRim) { 2 } else { 3 }, Vec::len);
        if directional_count
            .checked_add(area_count)
            .and_then(|n| n.checked_add(self.punctual_lights.as_ref().map_or(0, Vec::len)))
            .is_none_or(|n| n > 20)
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "at most twenty combined directional, punctual and rectangular lights"));
        }
        if let Some(lights) = &self.area_lights {
            for light in lights {
                light.validate()?;
            }
        }
        if let Some(lights) = &self.punctual_lights {
            if lights.len() > 8 || lights.capacity() > 8 {
                return Err(JobError::new(ErrorCode::InvalidRequest, "at most eight point/spot sources and reserved slots"));
            }
            for light in lights {
                light.validate()?;
            }
        }
        if !matches!(self.primary_samples, 1 | 4 | 16) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "primary samples must be exactly 1, 4 or 16"));
        }

        if let Some(reflections) = &self.reflections {
            reflections.validate()?;
        }
        for (i, target) in self.anisotropic_faces.iter().enumerate().filter_map(|(i, target)| target.as_ref().map(|target| (i, target))) {
            if target.body_name.is_empty()
                || target.body_name.len() > 1024
                || target.body_name.capacity() > 1024
                || !target.tangent.iter().all(|v| v.is_finite())
                || !target.tangent.iter().any(|v| *v != 0.0)
                || ![target.roughness_x, target.roughness_y].into_iter().all(|v| v.is_finite() && (crate::surface::MIN_ROUGHNESS..=1.0).contains(&v))
            {
                return Err(JobError::new(ErrorCode::InvalidRequest, "invalid bounded anisotropic planar face options"));
            }
            if self.anisotropic_faces.iter().skip(i + 1).flatten().any(|other| target.body_index == other.body_index && target.face == other.face) {
                return Err(JobError::new(ErrorCode::InvalidRequest, "duplicate anisotropic captured face"));
            }
        }
        if self.reflections.as_ref().is_some_and(|r| {
            r.bodies.iter().any(|b| b.mode == ReflectionMode::Mirror && self.anisotropic_faces.iter().flatten().any(|t| t.body_name == b.body))
        }) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "delta mirror cannot have anisotropic rough axes"));
        }
        if self.preset.is_some() && self.lights.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "choose either a lighting preset or custom lights"));
        }
        if !self.exposure_ev.is_finite() || !(-32.0..=32.0).contains(&self.exposure_ev) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "raytrace exposure_ev must be finite and within -32..32"));
        }
        if let Some(lights) = &self.lights {
            if lights.len() > 8 || lights.capacity() > 8 {
                return Err(JobError::new(ErrorCode::InvalidRequest, "at most eight directional lights"));
            }
            for light in lights {
                let direction_valid = light.direction.iter().all(|n| n.is_finite() && n.abs() <= 1e9) && light.direction.iter().any(|n| *n != 0.0);
                if !direction_valid
                    || !light.color.iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n))
                    || !light.intensity.is_finite()
                    || !(0.0..=64.0).contains(&light.intensity)
                {
                    return Err(JobError::new(ErrorCode::InvalidRequest, "invalid bounded directional light"));
                }
            }
        }
        Ok(())
    }
}

/// Fully resolved input; no mutable session, topology graph or GPU resources.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRequest {
    /// Resolved Raster-only capture of RenderSetup.exposure_ev, never a second authority.
    #[serde(default, skip_serializing_if = "zero_raster_exposure")]
    pub raster_exposure_ev: f64,
    /// Explicit transient opaque Raster rig; None keeps the original implicit studio.
    #[serde(default, deserialize_with = "deserialize_raster_lights", skip_serializing_if = "Option::is_none")]
    pub raster_lights: Option<Vec<DirectionalLightOptions>>,
    /// Runtime-only local rig. Any explicit local collection suppresses implicit studio lights.
    #[serde(default, deserialize_with = "deserialize_raster_punctual_lights", skip_serializing_if = "Option::is_none")]
    pub raster_punctual_lights: Option<Vec<PunctualLightOptions>>,
    #[serde(default, deserialize_with = "deserialize_raster_area_lights", skip_serializing_if = "Option::is_none")]
    pub raster_area_lights: Option<Vec<RectangleLightOptions>>,
    /// Transient visibility of the three implicit studio lights; never saved in a design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub studio_weights: Option<[f64; 3]>,
    /// Captured GPU resources stay in admitted work, never in status or saved data.
    #[serde(skip)]
    pub environment: Option<Arc<crate::lighting::environment::EnvironmentSource>>,
    #[serde(skip)]
    pub textures: Option<Arc<textures::RuntimeTextures>>,
    #[serde(default)]
    pub integrator: Integrator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raytrace: Option<RaytraceOptions>,
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
    pub background: [u8; 3],
    pub transparent_background: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColourEncoding {
    Srgb,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaConvention {
    Straight,
}

impl ImageRequest {
    pub fn has_local_raster_lights(&self) -> bool {
        self.raster_punctual_lights.is_some() || self.raster_area_lights.is_some()
    }

    pub fn has_explicit_raster_lights(&self) -> bool {
        self.raster_lights.is_some() || self.has_local_raster_lights()
    }

    /// Only the new local-rig route charges each bounded light evaluation.
    /// Existing directional and implicit routes retain their original pixel budget.
    pub(super) fn raster_light_work_factor(&self) -> Result<u64> {
        if !self.has_local_raster_lights() {
            return Ok(1);
        }
        self.validate_raster_request()?;
        let count = self.raster_lights.as_ref().map_or(0, Vec::len)
            .checked_add(self.raster_punctual_lights.as_ref().map_or(0, Vec::len))
            .and_then(|n| n.checked_add(self.raster_area_lights.as_ref().map_or(0, Vec::len)))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster light work overflow"))?;
        u64::try_from(count.max(1)).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "Raster light work overflow"))
    }

    /// Reject explicit Raster intent before allocating or entering another producer.
    pub fn validate_raster_request(&self) -> Result<()> {
        if !self.raster_exposure_ev.is_finite() || !(-32.0..=32.0).contains(&self.raster_exposure_ev) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "Raster exposure must be finite and within -32..32 EV"));
        }
        if self.raster_exposure_ev != 0.0
            && (self.integrator != Integrator::Raster || self.raytrace.is_some() || self.environment.is_some() || self.textures.is_some())
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "Raster exposure cannot enter another integrator or captured GPU resources"));
        }
        raster_lights::validate(&self.raster_lights)?;
        raster_lights::validate_punctual(&self.raster_punctual_lights)?;
        raster_lights::validate_areas(&self.raster_area_lights)?;
        if self.has_explicit_raster_lights()
            && (self.integrator != Integrator::Raster
                || self.raytrace.is_some()
                || self.studio_weights.is_some()
                || self.environment.is_some()
                || self.textures.is_some())
        {
            return Err(JobError::new(
                ErrorCode::InvalidRequest,
                "explicit Raster lights require Raster without raytrace, GPU resources or studio_weights",
            ));
        }
        Ok(())
    }

    pub fn raster_lights_retained_bytes(&self) -> Result<u64> {
        raster_lights::retained_bytes(&self.raster_lights)?
            .checked_add(raster_lights::local_retained_bytes(&self.raster_punctual_lights, &self.raster_area_lights)?)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster light retained size overflow"))
    }

    pub(super) fn raster_lights_task_bytes(&self) -> Result<u64> {
        raster_lights::task_bytes(&self.raster_lights)?
            .checked_add(raster_lights::local_task_bytes(&self.raster_punctual_lights, &self.raster_area_lights)?)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster task light size overflow"))
    }

    /// Internal status/task copy: the new Vec allocation is bounded and fallible.
    pub(super) fn try_clone_for_host(&self) -> Result<Self> {
        self.validate_raster_request()?;
        // Preserve the old path, including existing unrelated payload semantics.
        if !self.has_explicit_raster_lights() {
            return Ok(self.clone());
        }
        Ok(Self {
            raster_exposure_ev: self.raster_exposure_ev,
            raster_lights: raster_lights::try_copy(&self.raster_lights)?,
            raster_punctual_lights: raster_lights::try_copy_punctual(&self.raster_punctual_lights)?,
            raster_area_lights: raster_lights::try_copy_areas(&self.raster_area_lights)?,
            studio_weights: self.studio_weights,
            environment: self.environment.clone(),
            textures: self.textures.clone(),
            integrator: self.integrator,
            raytrace: None, // validated absent above, never clone-then-clear
            camera: self.camera,
            width: self.width,
            height: self.height,
            background: self.background,
            transparent_background: self.transparent_background,
        })
    }
    pub fn cpu_budget(&self) -> CpuBudget {
        if self.integrator == Integrator::Raytrace {
            self.raytrace.as_ref().map_or(CpuBudget::Compact, |options| options.budget)
        } else {
            CpuBudget::Compact
        }
    }

    /// Small GPU resource summaries survive work completion and keep their own host lease.
    pub fn retained_resource_metadata_bytes(&self) -> u64 {
        let environment = self
            .environment
            .as_ref()
            .map_or(0, |source| (std::mem::size_of::<crate::lighting::environment::EnvironmentInfo>() + source.info().source_sha256.len()) as u64);
        environment + if self.textures.is_some() { std::mem::size_of::<textures::TextureSummary>() as u64 } else { 0 }
    }
}

fn zero_raster_exposure(value: &f64) -> bool {
    *value == 0.0
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameInfo {
    pub job: JobId,
    pub scene_generation: u64,
    pub backend: BackendId,
    pub executor: ExecutorId,
    pub width: u32,
    pub height: u32,
    pub encoding: ColourEncoding,
    pub alpha: AlphaConvention,
    /// Exact encoded PNG length, not pixel storage or reserved Vec capacity.
    pub png_bytes: u64,
    /// Cached lowercase SHA-256 of those encoded bytes, computed before completion.
    pub png_sha256: String,
}
#[cfg(test)]
mod runtime_request_roundtrip_tests {

    fn raster_local_test_request() -> ImageRequest {
        serde_json::from_value(serde_json::json!({"camera":crate::Camera::default(),"width":8,"height":8,"background":[0,0,0],"transparent_background":false})).unwrap()
    }

    fn raster_local_test_point() -> PunctualLightOptions {
        PunctualLightOptions { position_mm:[0.0,0.0,10.0], color:[1.0;3], intensity:2.0, reference_distance_mm:10.0, minimum_distance_mm:0.001, cone:None }
    }

    fn raster_local_test_rectangle() -> RectangleLightOptions {
        RectangleLightOptions { center_mm:[0.0,0.0,10.0], edge_u_mm:[2.0,0.0,0.0], edge_v_mm:[0.0,-2.0,0.0], color:[1.0;3], intensity:2.0 }
    }

    #[test]
    fn raster_local_wire_omission_null_empty_and_host_copy_keep_distinct_intent() {
        let legacy = raster_local_test_request();
        let wire = serde_json::to_value(&legacy).unwrap();
        assert!(wire.get("raster_punctual_lights").is_none());
        assert!(wire.get("raster_area_lights").is_none());
        assert!(!legacy.has_explicit_raster_lights());
        let mut null = wire.clone();
        null["raster_punctual_lights"] = serde_json::Value::Null;
        null["raster_area_lights"] = serde_json::Value::Null;
        let decoded: ImageRequest = serde_json::from_value(null).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
        for key in ["raster_punctual_lights", "raster_area_lights"] {
            let mut explicit = wire.clone();
            explicit[key] = serde_json::json!([]);
            let input: ImageRequest = serde_json::from_value(explicit).unwrap();
            assert!(input.has_explicit_raster_lights() && input.has_local_raster_lights());
            assert!(input.raster_lights.is_none(), "no implicit directional materialisation");
            assert_eq!(input.raster_light_work_factor().unwrap(), 1);
            let copy = input.try_clone_for_host().unwrap();
            assert_eq!(serde_json::to_value(copy).unwrap(), serde_json::to_value(input).unwrap());
        }
    }

    #[test]
    fn raster_local_eight_shared_point_spot_four_areas_and_directional_caps_are_independent() {
        let mut input = raster_local_test_request();
        let point = raster_local_test_point();
        let mut spot = point;
        spot.cone = Some(SpotConeOptions { emitted_axis:[0.0,0.0,-1.0], inner_half_angle_radians:0.2, outer_half_angle_radians:0.5 });
        input.raster_lights = Some(vec![DirectionalLightOptions { direction:[0.0,0.0,1.0], color:[1.0;3], intensity:1.0 };8]);
        assert_eq!(input.raster_light_work_factor().unwrap(), 1, "old directional work path unchanged");
        input.raster_punctual_lights = Some(vec![point,spot,point,spot,point,spot,point,spot]);
        input.raster_area_lights = Some(vec![raster_local_test_rectangle();4]);
        input.validate_raster_request().unwrap();
        assert_eq!(input.raster_light_work_factor().unwrap(), 20);
        input.raster_punctual_lights.as_mut().unwrap().push(point);
        assert_eq!(input.validate_raster_request().unwrap_err().code, ErrorCode::InvalidRequest);
        assert!(serde_json::from_value::<ImageRequest>(serde_json::to_value(&input).unwrap()).is_err());
        input.raster_punctual_lights = Some(vec![point]);
        input.raster_area_lights.as_mut().unwrap().push(raster_local_test_rectangle());
        assert!(input.validate_raster_request().is_err());
        assert!(serde_json::from_value::<ImageRequest>(serde_json::to_value(input).unwrap()).is_err());
    }

    #[test]
    fn raster_local_spare_capacities_refuse_before_copy_or_task_reservation() {
        let mut input = raster_local_test_request();
        input.raster_punctual_lights = Some(Vec::with_capacity(9));
        assert!(input.try_clone_for_host().is_err());
        assert!(input.raster_lights_retained_bytes().is_err());
        assert!(input.raster_lights_task_bytes().is_err());
        input.raster_punctual_lights = None;
        input.raster_area_lights = Some(Vec::with_capacity(5));
        assert!(input.try_clone_for_host().is_err());
        assert!(input.raster_lights_retained_bytes().is_err());
        assert!(input.raster_lights_task_bytes().is_err());
    }

    #[test]
    fn raster_local_fallible_fixed_dto_copy_preserves_values_and_accounts_both_families() {
        let mut input = raster_local_test_request();
        input.raster_punctual_lights = Some(vec![raster_local_test_point()]);
        input.raster_area_lights = Some(vec![raster_local_test_rectangle()]);
        let expected = (input.raster_punctual_lights.as_ref().unwrap().capacity()*std::mem::size_of::<PunctualLightOptions>()
            + input.raster_area_lights.as_ref().unwrap().capacity()*std::mem::size_of::<RectangleLightOptions>()) as u64;
        assert_eq!(input.raster_lights_retained_bytes().unwrap(), expected);
        let task_fee = (8*std::mem::size_of::<PunctualLightOptions>()+4*std::mem::size_of::<RectangleLightOptions>()) as u64;
        assert_eq!(input.raster_lights_task_bytes().unwrap(), task_fee);
        let copy = input.try_clone_for_host().unwrap();
        assert_eq!(copy.raster_punctual_lights, input.raster_punctual_lights);
        assert_eq!(copy.raster_area_lights, input.raster_area_lights);
        assert!(copy.raster_lights_retained_bytes().unwrap() <= task_fee);
        input.raster_punctual_lights.as_mut().unwrap()[0].intensity = 0.0;
        input.raster_area_lights.as_mut().unwrap()[0].intensity = 0.0;
        assert_eq!(copy.raster_punctual_lights.as_ref().unwrap()[0].intensity, 2.0);
        assert_eq!(copy.raster_area_lights.as_ref().unwrap()[0].intensity, 2.0);
    }

    #[test]
    fn raster_local_empty_intent_still_refuses_other_producers_and_implicit_weights() {
        for family in [0,1] {
            let mut input = raster_local_test_request();
            if family == 0 { input.raster_punctual_lights = Some(Vec::new()); }
            else { input.raster_area_lights = Some(Vec::new()); }
            input.studio_weights = Some([0.0;3]);
            assert!(input.validate_raster_request().is_err());
            input.studio_weights = None;
            input.raytrace = Some(Default::default());
            assert!(input.validate_raster_request().is_err());
            input.raytrace = None;
            input.integrator = Integrator::Raytrace;
            assert!(input.validate_raster_request().is_err());
            assert!(super::interactive::cpu_request(&input).is_err());
        }
    }

    #[test]
    fn raster_local_shared_rectangle_validator_preserves_canonical_bounds_and_error_order() {
        let mut rectangle = raster_local_test_rectangle();
        rectangle.color = [0.0;3];
        rectangle.intensity = 0.0;
        rectangle.edge_u_mm = [1.0,1.0,0.0];
        assert_eq!(rectangle.validate().unwrap_err().message, "invalid finite rectangular light");
        rectangle = raster_local_test_rectangle();
        rectangle.center_mm = [1e9,0.0,0.0];
        assert_eq!(rectangle.validate().unwrap_err().message, "rectangular light corner exceeds world bounds");
        let mut input = raster_local_test_request();
        input.raster_area_lights = Some(vec![rectangle.clone()]);
        assert!(input.validate_raster_request().is_err());
        let options = RaytraceOptions { area_lights:Some(vec![rectangle]), ..Default::default() };
        assert_eq!(options.validate().unwrap_err().message, "rectangular light corner exceeds world bounds");
    }


    #[test]
    fn raster_light_wire_is_nullable_bounded_strict_and_fallibly_copied() {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Rig {
            #[serde(default, deserialize_with = "super::raster_lights::deserialize", skip_serializing_if = "Option::is_none")]
            raster_lights: Option<Vec<DirectionalLightOptions>>,
        }
        use serde_json::json;
        for value in [json!({}), json!({"raster_lights":null})] {
            let rig: Rig = serde_json::from_value(value).unwrap();
            assert!(rig.raster_lights.is_none());
            assert_eq!(serde_json::to_value(rig).unwrap(), json!({}));
        }
        let light = json!({"direction":[0.0,0.0,1.0],"color":[0.25,0.5,1.0],"intensity":2.0});
        for count in [0, 1, 8] {
            let rig: Rig = serde_json::from_value(json!({"raster_lights":vec![light.clone();count]})).unwrap();
            assert_eq!(rig.raster_lights.as_ref().unwrap().len(), count);
            assert!(rig.raster_lights.as_ref().unwrap().capacity() <= 8);
            let copy = super::raster_lights::try_copy(&rig.raster_lights).unwrap();
            assert_eq!(copy, rig.raster_lights);
            assert!(copy.as_ref().unwrap().capacity() <= 8);
            assert_eq!(serde_json::to_value(rig).unwrap(), json!({"raster_lights":vec![light.clone();count]}));
        }
        assert!(serde_json::from_value::<Rig>(json!({"raster_lights":vec![light.clone();9]})).is_err());
        for value in [
            json!(true),
            json!({}),
            json!([{"direction":[0,0,0],"color":[1,1,1],"intensity":0}]),
            json!([{"direction":[0,0,1],"color":[1,1,1],"intensity":1,"shadows":true}]),
        ] {
            assert!(serde_json::from_value::<Rig>(json!({"raster_lights":value})).is_err());
        }
        let mut excessive = Vec::with_capacity(9);
        excessive.push(DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 });
        assert!(super::raster_lights::try_copy(&Some(excessive)).is_err());
        for intensity in [f64::NAN, f64::INFINITY, -1.0, 65.0] {
            let invalid = Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity }]);
            assert!(super::raster_lights::try_copy(&invalid).is_err());
        }
    }

    use super::*;

    #[test]
    fn dielectric_transport_wire_policy_preserves_legacy_and_rejects_hostile_values() {
        let legacy: RaytraceOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(legacy.dielectric_transport, CpuDielectricTransport::LegacyBounded);
        assert!(serde_json::to_value(&legacy).unwrap().get("dielectric_transport").is_none());
        let strict: RaytraceOptions = serde_json::from_value(serde_json::json!({"dielectric_transport":"strict_tir"})).unwrap();
        strict.validate().unwrap();
        assert_eq!(serde_json::to_value(&strict).unwrap()["dielectric_transport"], "strict_tir");
        for value in [serde_json::json!(null), serde_json::json!(true), serde_json::json!(7), serde_json::json!("unlimited"), serde_json::json!({})] {
            assert!(serde_json::from_value::<RaytraceOptions>(serde_json::json!({"dielectric_transport":value})).is_err());
        }
    }

    #[test]
    fn strict_dielectric_policy_refuses_unqualified_opaque_continuations_without_mutation() {
        let options = RaytraceOptions {
            dielectric_transport: CpuDielectricTransport::StrictTir,
            reflections: Some(ReflectionOptions {
                bodies: vec![ReflectionBody { body: "Body".into(), mode: ReflectionMode::Mirror }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let before = options.clone();
        assert_eq!(options.validate().unwrap_err().code, ErrorCode::InvalidRequest);
        assert_eq!(options, before);
        RaytraceOptions { dielectric_transport: CpuDielectricTransport::LegacyBounded, ..options }.validate().unwrap();
    }
    #[test]
    fn default_preset_and_empty_lights_requests_roundtrip_without_invalid_null_sequence() {
        for options in [
            RaytraceOptions::default(),
            RaytraceOptions { preset: Some(LightingPreset::Neutral), ..Default::default() },
            RaytraceOptions { lights: Some(Vec::new()), ..Default::default() },
        ] {
            let value = serde_json::to_value(&options).unwrap();
            if options.lights.is_none() {
                assert!(value.get("lights").is_none());
            } else {
                assert_eq!(value.get("lights").unwrap(), &serde_json::json!([]));
            }
            let decoded: RaytraceOptions = serde_json::from_value(value).unwrap();
            assert_eq!(decoded, options);
            assert!(decoded.validate().is_ok());
        }
        assert_eq!(serde_json::from_value::<RaytraceOptions>(serde_json::json!({})).unwrap(), RaytraceOptions::default());
    }

    #[test]
    fn cpu_progressive_options_are_optional_strict_and_bounded_before_capture() {
        use serde_json::json;
        assert!(serde_json::to_value(RaytraceOptions::default()).unwrap().get("progressive").is_none());
        assert_eq!(serde_json::from_value::<RaytraceOptions>(json!({"progressive":null})).unwrap(), RaytraceOptions::default());
        let options: RaytraceOptions = serde_json::from_value(json!({"progressive":{"samples_per_pixel":3,"seed":u32::MAX}})).unwrap();
        options.validate().unwrap();
        assert_eq!(serde_json::from_value::<RaytraceOptions>(serde_json::to_value(&options).unwrap()).unwrap(), options);
        for invalid in [json!({"seed":-1}), json!({"seed":4294967296_u64}), json!({"seed":0.5}), json!({"adaptive":true})] {
            assert!(serde_json::from_value::<CpuProgressiveOptions>(invalid).is_err());
        }
        for samples_per_pixel in [0, 65, u32::MAX] {
            assert!(CpuProgressiveOptions { samples_per_pixel, seed: 0 }.validate().is_err());
        }
        for samples_per_pixel in [1, 2, 3, 16, 64] {
            CpuProgressiveOptions { samples_per_pixel, seed: 0 }.validate_image(512, 512).unwrap();
        }
        CpuProgressiveOptions::default().validate_image(1024, 1024).unwrap();
        assert!(CpuProgressiveOptions { samples_per_pixel: 17, seed: 0 }.validate_image(1024, 1024).is_err());
        for (width, height) in [(0, 1), (1, 0), (1025, 1024), (4097, 1), (u32::MAX, u32::MAX)] {
            assert!(CpuProgressiveOptions { samples_per_pixel: 1, seed: 0 }.validate_image(width, height).is_err());
        }
    }

    #[test]
    fn progressive_rejects_incompatible_sampling_without_changing_fixed_aa() {
        let progressive = Some(CpuProgressiveOptions::default());
        for primary_samples in [4, 16] {
            let fixed = RaytraceOptions { primary_samples, ..Default::default() };
            fixed.validate().unwrap();
            assert!(RaytraceOptions { progressive, ..fixed }.validate().is_err());
        }
        let reflections = ReflectionOptions {
            bodies: vec![ReflectionBody { body: "Body".into(), mode: ReflectionMode::RoughSpecular }],
            samples: 4,
            max_bounces: 2,
        };
        assert!(RaytraceOptions { progressive, reflections: Some(reflections), ..Default::default() }.validate().is_err());
        let area = RectangleLightOptions {
            center_mm: [0.0, 0.0, 10.0],
            edge_u_mm: [1.0, 0.0, 0.0],
            edge_v_mm: [0.0, -1.0, 0.0],
            color: [1.0; 3],
            intensity: 1.0,
        };
        RaytraceOptions { progressive, area_lights: Some(vec![area]), ..Default::default() }.validate().unwrap();
        let dielectric = DielectricOptions {
            body_index: 0,
            body_name: "Body".into(),
            boundary: DielectricBoundary::ThinSheet,
            ior: 1.5,
            absorption_per_mm: [0.0; 3],
            thickness_mm: 1.0,
        };
        // Superseded global rejection: smooth optical trees are implemented per
        // camera sample; capture-time certification remains separately required.
        RaytraceOptions { progressive, dielectrics: vec![dielectric], ..Default::default() }.validate().unwrap();
        let face = AnisotropicFaceOptions {
            body_index: 0,
            body_name: "Body".into(),
            face: 0,
            tangent: [1.0, 0.0, 0.0],
            orientation: AnisotropicOrientation::Zero,
            roughness_x: 0.2,
            roughness_y: 0.6,
        };
        assert!(RaytraceOptions { progressive, anisotropic_faces: [Some(face), None], ..Default::default() }.validate().is_err());
        RaytraceOptions { progressive, area_lights: Some(Vec::new()), ..Default::default() }.validate().unwrap();
    }

    #[test]
    fn progressive_rectangle_and_led_admission_preserves_source_limits_and_atomic_rejection() {
        let strip = led_strip::LedStripOptions {
            center_mm: [0.0, 0.0, 10.0],
            length_mm: 2.0,
            width_mm: 2.0,
            length_axis: [1.0, 0.0, 0.0],
            width_axis: [0.0, -1.0, 0.0],
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
        };
        for samples in [1, 4, 16, 64] {
            let mut options = RaytraceOptions {
                progressive: Some(CpuProgressiveOptions { samples_per_pixel: 3, seed: 97 }),
                lights: Some(Vec::new()),
                ..Default::default()
            };
            for count in 1..=4 {
                strip.append_to(&mut options, samples).unwrap();
                options.validate().unwrap();
                assert_eq!(options.area_lights.as_ref().unwrap().len(), count);
                assert_eq!(
                    options.retained_bytes().unwrap(),
                    (options.area_lights.as_ref().unwrap().capacity() * std::mem::size_of::<RectangleLightOptions>()) as u64
                );
            }
            let before = options.clone();
            let capacity = options.area_lights.as_ref().unwrap().capacity();
            assert!(strip.append_to(&mut options, samples).is_err());
            assert_eq!(options, before);
            assert_eq!(options.area_lights.as_ref().unwrap().capacity(), capacity);
            options.area_lights.as_mut().unwrap().pop();
            let before = options.clone();
            let conflicting = if samples == 1 { 4 } else { 1 };
            assert!(strip.append_to(&mut options, conflicting).is_err());
            assert_eq!(options, before);
            for invalid in [0, 2, 65, u32::MAX] {
                let mut hostile = options.clone();
                hostile.area_samples = Some(invalid);
                assert!(hostile.validate().is_err());
            }
            let mut hostile = options.clone();
            hostile.area_lights.as_mut().unwrap()[0].intensity = f64::NAN;
            assert!(hostile.validate().is_err());
        }
        let mut maximum = RaytraceOptions {
            progressive: Some(CpuProgressiveOptions::default()),
            lights: Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 }; 8]),
            punctual_lights: Some(vec![
                PunctualLightOptions {
                    position_mm: [0.0, 0.0, 10.0],
                    color: [1.0; 3],
                    intensity: 1.0,
                    reference_distance_mm: 10.0,
                    minimum_distance_mm: 0.001,
                    cone: None,
                };
                8
            ]),
            ..Default::default()
        };
        for _ in 0..4 {
            strip.append_to(&mut maximum, 16).unwrap();
        }
        maximum.validate().unwrap();
        maximum.area_lights.as_mut().unwrap().push(strip.rectangle().unwrap());
        assert!(maximum.validate().is_err());
        for options in [
            RaytraceOptions { progressive: Some(CpuProgressiveOptions::default()), area_samples: Some(16), ..Default::default() },
            RaytraceOptions { progressive: Some(CpuProgressiveOptions::default()), seed: Some(7), ..Default::default() },
        ] {
            assert!(options.validate().is_err());
        }
    }
}

/// Fixed representative presentation plan; fitting never grants source admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialPreviewPlan {
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
}
impl PartialPreviewPlan {
    pub fn source_width(&self) -> u32 {
        self.source_width
    }
    pub fn source_height(&self) -> u32 {
        self.source_height
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn new(source_width: u32, source_height: u32) -> Result<Self> {
        if source_width == 0 || source_height == 0 {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview source dimensions must be positive"));
        }
        let edge = source_width.max(source_height);
        let fit = |side: u32| {
            if edge <= 1024 { side } else { ((u64::from(side) * 1024 + u64::from(edge) / 2) / u64::from(edge)).max(1) as u32 }
        };
        Ok(Self { source_width, source_height, width: fit(source_width), height: fit(source_height) })
    }
    pub fn validate(&self) -> Result<()> {
        if *self != Self::new(self.source_width, self.source_height)? {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview dimensions differ from the fixed plan"));
        }
        Ok(())
    }
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
    pub fn source_index(&self, preview_index: usize) -> Result<usize> {
        self.validate()?;
        let index = u64::try_from(preview_index).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "partial preview index overflow"))?;
        if index >= self.pixels() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "partial preview index outside its plan"));
        }
        let x = (index % u64::from(self.width)) * u64::from(self.source_width) / u64::from(self.width);
        let y = (index / u64::from(self.width)) * u64::from(self.source_height) / u64::from(self.height);
        let source = y
            .checked_mul(u64::from(self.source_width))
            .and_then(|v| v.checked_add(x))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial source index overflow"))?;
        usize::try_from(source).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "partial source index exceeds address space"))
    }
    pub fn encoded_limit(&self) -> Result<u64> {
        self.validate()?;
        self.pixels()
            .checked_mul(8)
            .and_then(|v| v.checked_add(65_536))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial PNG capacity overflow"))
    }
    pub fn covered_bytes(&self) -> Result<u64> {
        self.validate()?;
        self.pixels().checked_mul(2).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial coverage capacity overflow"))
    }
    pub fn snapshot_work(&self) -> Result<u64> {
        self.validate()?;
        self.pixels()
            .checked_mul(12)
            .and_then(|v| v.checked_add(65_538))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial work bound overflow"))
    }
    /// Two retained payload slots; existing admitted RGBA/encoder scratch is reused.
    #[cfg(feature = "raytrace-cpu")]
    pub(crate) fn additional_bytes(&self) -> Result<u64> {
        self.slot_bytes()?
            .checked_mul(2)
            .and_then(|v| v.checked_add(1024))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial preview admission overflow"))
    }
    pub(crate) fn slot_bytes(&self) -> Result<u64> {
        self.encoded_limit()?
            .checked_add(self.covered_bytes()?)
            .and_then(|v| v.checked_add(2048))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial retained slot overflow"))
    }
}

/// Explicit protected-feedback aggregate ceiling; the ordinary MAX_CPU_WORK is unchanged.
pub const MAX_PROTECTED_CPU_WORK: u64 = 16_000_809_500_800;

/// Checked per-job feedback policy, not an admission token or an arbitrary work envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedPreviewPlan {
    preview: PartialPreviewPlan,
    samples_per_pixel: u32,
    budget: CpuBudget,
    total_commits: u64,
    quota_limit: u32,
}
impl ProtectedPreviewPlan {
    pub fn new(source_width: u32, source_height: u32, samples_per_pixel: u32, budget: CpuBudget) -> Result<Self> {
        budget.validate_image(source_width, source_height, Some(CpuProgressiveOptions { samples_per_pixel, seed: 0 }))?;
        let preview = PartialPreviewPlan::new(source_width, source_height)?;
        let total_commits = u64::from(source_width)
            .checked_mul(u64::from(source_height))
            .and_then(|v| v.checked_mul(u64::from(samples_per_pixel)))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "protected camera-commit count overflow"))?;
        let quota_limit = if total_commits <= 1 {
            0
        } else {
            u32::try_from((total_commits - 1).min(64)).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "protected quota overflow"))?
        };
        let plan = Self { preview, samples_per_pixel, budget, total_commits, quota_limit };
        plan.aggregate_limit()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<()> {
        if *self != Self::new(self.preview.source_width(), self.preview.source_height(), self.samples_per_pixel, self.budget)? {
            return Err(JobError::new(ErrorCode::InvalidRequest, "protected feedback plan differs from its canonical policy"));
        }
        Ok(())
    }
    pub fn preview_plan(&self) -> PartialPreviewPlan {
        self.preview
    }
    pub fn samples_per_pixel(&self) -> u32 {
        self.samples_per_pixel
    }
    pub fn budget(&self) -> CpuBudget {
        self.budget
    }
    pub fn total_commits(&self) -> u64 {
        self.total_commits
    }
    pub fn ordinary_limit(&self) -> u64 {
        self.budget.limits().max_work_units
    }
    pub fn quota_limit(&self) -> u32 {
        self.quota_limit
    }
    pub fn snapshot_work(&self) -> Result<u64> {
        self.preview.snapshot_work()
    }
    pub fn presentation_limit(&self) -> Result<u64> {
        self.snapshot_work()?
            .checked_mul(u64::from(self.quota_limit))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "protected presentation allowance overflow"))
    }
    pub fn aggregate_limit(&self) -> Result<u64> {
        self.ordinary_limit()
            .checked_add(self.presentation_limit()?)
            .filter(|v| *v <= MAX_PROTECTED_CPU_WORK)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "protected aggregate work exceeds its explicit ceiling"))
    }
    /// Zero-based, strictly increasing milestones below the final target.
    pub fn milestone(&self, index: u32) -> Result<u64> {
        if index >= self.quota_limit {
            return Err(JobError::new(ErrorCode::InvalidRequest, "protected milestone index outside its quota"));
        }
        if self.quota_limit == 1 {
            return Ok(1);
        }
        self.total_commits
            .checked_sub(2)
            .and_then(|v| v.checked_mul(u64::from(index)))
            .and_then(|v| v.checked_div(u64::from(self.quota_limit - 1)))
            .and_then(|v| v.checked_add(1))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "protected milestone arithmetic overflow"))
    }
}

/// Actual admitted policy and monotonic producer usage; never a saved design field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProtectedPreviewReport {
    pub policy_version: u32,
    pub ordinary_limit: u64,
    pub presentation_limit: u64,
    pub aggregate_limit: u64,
    pub ordinary_used: u64,
    pub presentation_used: u64,
    pub accepted_snapshots: u32,
    pub quota_limit: u32,
    pub next_milestone_index: u32,
    pub quota_exhausted: bool,
    pub milestones_complete: bool,
}
