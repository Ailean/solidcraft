//! Caller-owned image execution host. One worker, two bounded queued inputs; browser pumping
//! uses the same incremental task without threads. Shared ownership never includes CAD state.

use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Condvar;
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    Admission, AlphaConvention, CancelToken, ColourEncoding, ErrorCode, FrameInfo, ImageExecutor, ImageRequest, ImageTask, ImageWork, Integrator,
    JobError, JobId, JobOptions, Phase, Progress, Result, TaskPoll, admit,
};
use crate::{
    Availability, ExecutorDescriptor, ExecutorId, ProductScene, RenderFormat, RenderLimits, RenderMode, RenderOperation, RenderProfile, ScratchLayout,
};

const MEMORY_LIMIT: u64 = 512 * 1024 * 1024;
const QUEUE_LIMIT: usize = 2;

#[cfg(test)]
struct CpuExecutor;
#[cfg(test)]
impl ImageExecutor for CpuExecutor {
    fn descriptor(&self) -> Result<ExecutorDescriptor> {
        Ok(ExecutorDescriptor {
            id: ExecutorId::new("cpu-product").map_err(|e| JobError::new(ErrorCode::Internal, e.to_string()))?,
            availability: Availability::Available,
            profiles: vec![RenderProfile {
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                formats: vec![RenderFormat::Png],
                features: vec![crate::RenderFeature::Pbr],
            }],
            limits: RenderLimits::default(),
            scratch: ScratchLayout { sample_scale: 2, sample_bytes_per_pixel: 8, output_bytes_per_pixel: 8, encoding_bytes_per_pixel: 8 },
        })
    }

    fn admit(&self, scene: &ProductScene, request: &ImageRequest, options: JobOptions) -> Result<Admission> {
        if options != JobOptions::default() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "CPU product does not support sampling or exposure controls"));
        }
        admit(scene, request)
    }
    fn begin(&self, scene: ProductScene, request: ImageRequest, _options: JobOptions, cancel: CancelToken) -> Result<Box<dyn ImageWork>> {
        Ok(Box::new(ImageTask::new(scene, request, cancel)?))
    }
}

// Completion notification precedes destruction of the worker's temporary handle.
// Lease tests join their owned, idle worker before checking exact retained bytes.
#[cfg(all(test, feature = "raytrace-cpu", not(target_arch = "wasm32")))]
impl ImageHost {
    pub(super) fn join_worker_for_test(&mut self) -> Result<()> {
        self.0.shared.stop.store(true, Ordering::Release);
        self.0.shared.wake.notify_all();
        let inner = Arc::get_mut(&mut self.0).ok_or_else(|| JobError::new(ErrorCode::Internal, "test must own its host"))?;
        if let Some(worker) = inner._worker.take() {
            worker.join().map_err(|_| JobError::new(ErrorCode::Internal, "test worker panicked"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "host_regrade_tests.rs"]
mod regrade_tests;
#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    NativeWorker,
    Cooperative,
}

struct Budget {
    used: AtomicU64,
    limit: u64,
}
pub(crate) struct Reservation {
    budget: Arc<Budget>,
    bytes: u64,
}

impl Budget {
    fn reserve(self: &Arc<Self>, bytes: u64) -> Result<Reservation> {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let next = used.checked_add(bytes).filter(|n| *n <= self.limit).ok_or_else(|| {
                JobError::new(ErrorCode::LimitExceeded, format!("image host admission pool is full ({} MiB)", self.limit / (1024 * 1024)))
            })?;
            match self.used.compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(current) => used = current,
            }
        }
        Ok(Reservation { budget: self.clone(), bytes })
    }
}
impl Reservation {
    /// Merge unique portions of one admission without a second pool debit.
    fn merge_from(&mut self, donor: &mut Self) -> Result<()> {
        if !Arc::ptr_eq(&self.budget, &donor.budget) {
            return Err(JobError::new(ErrorCode::WrongOwner, "cannot merge different image admission pools"));
        }
        let bytes = self
            .bytes
            .checked_add(donor.bytes)
            .filter(|v| *v <= self.budget.limit)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "image reservation merge overflow"))?;
        self.bytes = bytes;
        donor.bytes = 0;
        Ok(())
    }
    /// Transfer an already admitted portion without changing the pool's total.
    /// CPU progressive admission includes every possible checkpoint up front.
    fn split(&mut self, bytes: u64) -> Result<Self> {
        if bytes > self.bytes {
            return Err(JobError::new(ErrorCode::LimitExceeded, "preview exceeds its prepaid reservation"));
        }
        self.bytes -= bytes;
        Ok(Self { budget: self.budget.clone(), bytes })
    }

    pub(crate) fn shrink(&mut self, bytes: u64) -> Result<()> {
        if bytes > self.bytes {
            return Err(JobError::new(ErrorCode::LimitExceeded, "completed PNG exceeds its reservation"));
        }
        self.budget.used.fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Retained runtime input capacities survive work/PNG eviction through status handles.
fn retained_request_bytes(request: &ImageRequest) -> Result<u64> {
    let existing = request.raytrace.as_ref().map_or(Ok(0), |options| options.retained_bytes())?;
    existing
        .checked_add(request.raster_lights_retained_bytes()?)
        .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "retained request size overflow"))
}

#[cfg(all(test, feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod light_union_lease_tests {
    use super::super::{DirectionalLightOptions, PunctualLightOptions, RaytraceOptions, led_strip::LedStripOptions};
    use super::*;
    fn request() -> Result<ImageRequest> {
        let mut options = RaytraceOptions {
            primary_samples: 16,
            lights: Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 }; 8]),
            punctual_lights: Some(vec![
                PunctualLightOptions {
                    position_mm: [0.0, 0.0, 10.0],
                    color: [1.0; 3],
                    intensity: 1.0,
                    reference_distance_mm: 10.0,
                    minimum_distance_mm: 0.001,
                    cone: None
                };
                8
            ]),
            ..Default::default()
        };
        let strip = LedStripOptions {
            center_mm: [0.0, 0.0, 10.0],
            length_mm: 8.0,
            width_mm: 0.5,
            length_axis: [1.0, 0.0, 0.0],
            width_axis: [0.0, -1.0, 0.0],
            color: [1.0; 3],
            intensity: 2.0,
        };
        for _ in 0..4 {
            strip.append_to(&mut options, 64)?;
        }
        Ok(ImageRequest {
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            studio_weights: None,
            environment: None,
            textures: None,
            integrator: Integrator::Raytrace,
            raytrace: Some(options),
            camera: Default::default(),
            width: 8,
            height: 8,
            background: [0; 3],
            transparent_background: false,
        })
    }
    fn scene() -> ProductScene {
        ProductScene { meshes: Vec::new(), bounds: solidcraft_geom::Aabb3::EMPTY }
    }
    #[test]
    fn normalized_union_uses_one_pool_and_keeps_record_lease_after_frame_eviction() {
        let mut host = CpuHost::for_platform().unwrap();
        let budget = host.0.shared.budget.clone();
        let request = request().unwrap();
        let bytes = retained_request_bytes(&request).unwrap();
        let capacities = request
            .raytrace
            .as_ref()
            .map(|o| {
                (o.lights.as_ref().unwrap().capacity(), o.punctual_lights.as_ref().unwrap().capacity(), o.area_lights.as_ref().unwrap().capacity())
            })
            .unwrap();
        let job = host.submit(JobId { session: 1724, serial: 1, generation: 1 }, 1, scene(), request).unwrap();
        assert_eq!(job.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase, Phase::Completed);
        // Synchronize this test's completed worker before asserting final Arc release;
        // publication wakes waiters just before Running drops its temporary handle.
        host.0.shared.stop.store(true, Ordering::Release);
        host.0.shared.wake.notify_all();
        Arc::get_mut(&mut host.0).unwrap()._worker.take().unwrap().join().unwrap();
        let retained = job.0.request.raytrace.as_ref().unwrap();
        assert_eq!(
            capacities,
            (
                retained.lights.as_ref().unwrap().capacity(),
                retained.punctual_lights.as_ref().unwrap().capacity(),
                retained.area_lights.as_ref().unwrap().capacity()
            )
        );
        assert!(Arc::ptr_eq(&job.0._request_reservation.as_ref().unwrap().budget, &budget));
        assert_eq!(job.0._request_reservation.as_ref().unwrap().bytes, bytes);
        let frame = job.frame().unwrap().unwrap();
        job.0.data.lock().unwrap().frame = None;
        assert!(host.admitted_bytes() > bytes);
        drop(frame);
        assert_eq!(host.admitted_bytes(), bytes);
        let held = job.clone();
        drop(job);
        assert_eq!(host.admitted_bytes(), bytes);
        drop(held);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
    #[test]
    fn failed_union_metadata_reservation_releases_work_and_does_not_enqueue() {
        let host = CpuHost::for_platform().unwrap();
        let request = request().unwrap();
        let work = admit(&scene(), &request).unwrap().estimated_total_bytes;
        let fill = host.0.shared.budget.reserve(host.pool_limit_bytes() - work).unwrap();
        let before = host.admitted_bytes();
        let result = host.submit(JobId { session: 1724, serial: 2, generation: 1 }, 1, scene(), request);
        assert_eq!(result.err().unwrap().code, ErrorCode::LimitExceeded);
        assert_eq!(host.admitted_bytes(), before);
        assert!(host.0.shared.queue.lock().unwrap().is_empty());
        drop(fill);
        assert_eq!(host.admitted_bytes(), 0);
    }

    fn raster_local_pair() -> (ImageRequest, ProductScene) {
        let mut input = request().unwrap();
        input.integrator = Integrator::Raster;
        input.raytrace = None;
        let mut points = Vec::with_capacity(8);
        points.push(PunctualLightOptions {
            position_mm: [0.0, 0.0, 10.0],
            color: [1.0; 3],
            intensity: 1.0,
            reference_distance_mm: 10.0,
            minimum_distance_mm: 0.001,
            cone: None,
        });
        let mut areas = Vec::with_capacity(4);
        areas.push(super::super::RectangleLightOptions {
            center_mm: [0.0, 0.0, 10.0],
            edge_u_mm: [2.0, 0.0, 0.0],
            edge_v_mm: [0.0, -2.0, 0.0],
            color: [1.0; 3],
            intensity: 1.0,
        });
        input.raster_punctual_lights = Some(points);
        input.raster_area_lights = Some(areas);
        let mesh = solidcraft_geom::Mesh {
            positions: vec![
                solidcraft_geom::Vec3::new(-2.0, -2.0, 0.0),
                solidcraft_geom::Vec3::new(2.0, -2.0, 0.0),
                solidcraft_geom::Vec3::new(0.0, 2.0, 0.0),
            ],
            normals: vec![solidcraft_geom::Vec3::Z; 3],
            triangles: vec![[0, 1, 2]],
            tri_face: vec![0],
            ..Default::default()
        };
        let bounds = mesh.bounds();
        let scene = ProductScene {
            meshes: vec![crate::ProductMesh {
                name: "Raster local lease fixture".into(),
                mesh: Arc::new(mesh),
                color: [160; 3],
                opacity: 1.0,
                surface: None,
                transmission: None,
                faces: Vec::new(),
            }],
            bounds,
        };
        (input, scene)
    }

    #[test]
    fn raster_local_record_keeps_original_capacity_lease_after_real_completion_and_frame_drop() {
        let mut host = CpuHost::for_platform().unwrap();
        let budget = host.0.shared.budget.clone();
        let (input, scene) = raster_local_pair();
        let capacities = (input.raster_punctual_lights.as_ref().unwrap().capacity(), input.raster_area_lights.as_ref().unwrap().capacity());
        let bytes = (capacities.0 * std::mem::size_of::<PunctualLightOptions>()
            + capacities.1 * std::mem::size_of::<super::super::RectangleLightOptions>()) as u64;
        assert_eq!(retained_request_bytes(&input).unwrap(), bytes);
        let job = host.submit(JobId { session: 1725, serial: 1, generation: 1 }, 1, scene, input).unwrap();
        assert_eq!(job.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase, Phase::Completed);
        host.0.shared.stop.store(true, Ordering::Release);
        host.0.shared.wake.notify_all();
        Arc::get_mut(&mut host.0).unwrap()._worker.take().unwrap().join().unwrap();
        assert_eq!(job.0.request.raster_punctual_lights.as_ref().unwrap().capacity(), capacities.0);
        assert_eq!(job.0.request.raster_area_lights.as_ref().unwrap().capacity(), capacities.1);
        assert!(Arc::ptr_eq(&job.0._request_reservation.as_ref().unwrap().budget, &budget));
        assert_eq!(job.0._request_reservation.as_ref().unwrap().bytes, bytes);
        let copied = job.status().unwrap().request;
        assert_eq!(copied.raster_punctual_lights, job.0.request.raster_punctual_lights);
        assert_eq!(copied.raster_area_lights, job.0.request.raster_area_lights);
        drop(copied);
        let frame = job.frame().unwrap().unwrap();
        job.0.data.lock().unwrap().frame = None;
        drop(frame);
        assert_eq!(host.admitted_bytes(), bytes);
        let held = job.clone();
        drop(job);
        assert_eq!(host.admitted_bytes(), bytes);
        drop(held);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }

    #[test]
    fn raster_local_metadata_reservation_failure_releases_work_without_enqueuing() {
        let host = CpuHost::for_platform().unwrap();
        let (input, scene) = raster_local_pair();
        let work = host.admit(&scene, &input, JobOptions::default()).unwrap().estimated_total_bytes;
        let fill = host.0.shared.budget.reserve(host.pool_limit_bytes() - work).unwrap();
        let before = host.admitted_bytes();
        let failed = host.submit(JobId { session: 1725, serial: 2, generation: 1 }, 1, scene, input);
        assert_eq!(failed.err().unwrap().code, ErrorCode::LimitExceeded);
        assert_eq!(host.admitted_bytes(), before);
        assert!(host.0.shared.queue.lock().unwrap().is_empty());
        drop(fill);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

fn admit_request_sum(work_bytes: u64, request_bytes: u64) -> Result<()> {
    if work_bytes.checked_add(request_bytes).is_none_or(|sum| sum > super::image::MAX_JOB_BYTES) {
        return Err(JobError::new(ErrorCode::LimitExceeded, "render work and retained request exceed 384 MiB per-job admission"));
    }
    Ok(())
}

/// Borrow encoded bytes through the frame, so retaining the data also retains its admission lease.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearColourEncoding {
    SceneLinearSrgbD65,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearAlphaConvention {
    CoveragePremultiplied,
}
#[derive(Clone, Debug, Serialize)]
pub struct LinearMasterInfo {
    pub encoding: LinearColourEncoding,
    pub alpha: LinearAlphaConvention,
    pub bytes: u64,
    pub sha256: String,
}
struct RetainedPresentation {
    info: super::presentation::MasterPresentation,
    _reservation: Reservation,
}

pub struct RenderFrame {
    // Derivatives point directly at the original, never form a retention chain.
    source_master: Option<Arc<RenderFrame>>,
    presentation: Option<Arc<RetainedPresentation>>,
    pub info: FrameInfo,
    png: Vec<u8>,
    exr: Option<(Vec<u8>, LinearMasterInfo)>,
    _reservation: Reservation,
}
impl RenderFrame {
    pub fn master_source(self: &Arc<Self>) -> Result<Arc<Self>> {
        if let Some(source) = &self.source_master {
            return Ok(source.clone());
        }
        if self.exr.is_some() {
            return Ok(self.clone());
        }
        Err(JobError::new(ErrorCode::InvalidRequest, "completed image has no retained raw linear master"))
    }
    pub fn presentation(&self) -> Option<&super::presentation::MasterPresentation> {
        self.presentation.as_deref().map(|retained| &retained.info)
    }
    pub fn linear_exr(&self) -> Option<&[u8]> {
        self.exr.as_ref().map(|(bytes, _)| bytes.as_slice()).or_else(|| self.source_master.as_ref().and_then(|source| source.linear_exr()))
    }
    pub fn linear_master_info(&self) -> Option<&LinearMasterInfo> {
        self.exr.as_ref().map(|(_, info)| info).or_else(|| self.source_master.as_ref().and_then(|source| source.linear_master_info()))
    }
    pub fn png(&self) -> &[u8] {
        &self.png
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewInfo {
    pub frame_sequence: u32,
    pub samples_per_pixel: u32,
    pub frame: FrameInfo,
}

/// Immutable checkpoint bytes retain their own transferred host lease. Keeping
/// an older preview alive never makes its memory disappear from pool accounting.
pub struct RenderPreview {
    pub info: PreviewInfo,
    png: Vec<u8>,
    _reservation: CheckpointLease,
}

/// Two reusable, immutable complete-pass slots. No consumer acknowledgement or
/// queue: externally pinned slots are diagnosed and never stall final work.
struct CheckpointCredits {
    state: Mutex<CheckpointCreditState>,
    slot_bytes: u64,
    capacity: u64,
}
struct CheckpointCreditState {
    free: Reservation,
    closed: bool,
}
impl CheckpointCredits {
    fn new(reservation: &mut Reservation, slot_bytes: u64, slots: u32) -> Result<Arc<Self>> {
        if !(1..=2).contains(&slots) || slot_bytes == 0 {
            return Err(JobError::new(ErrorCode::InvalidRequest, "checkpoint credits require one or two finite slots"));
        }
        let capacity = slot_bytes
            .checked_mul(u64::from(slots))
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "checkpoint credit capacity overflow"))?;
        let free = reservation.split(capacity)?;
        Ok(Arc::new(Self { state: Mutex::new(CheckpointCreditState { free, closed: false }), slot_bytes, capacity }))
    }
    fn acquire(self: &Arc<Self>) -> Result<Option<CheckpointLease>> {
        let mut state = self.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "checkpoint credit lock poisoned"))?;
        if state.closed {
            return Err(JobError::new(ErrorCode::Cancelled, "checkpoint credits are closed"));
        }
        let needed = self.slot_bytes.checked_add(1024).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "checkpoint credit bound overflow"))?;
        if state.free.bytes < needed {
            return Ok(None);
        }
        let reservation = state.free.split(self.slot_bytes)?;
        Ok(Some(CheckpointLease { reservation, credits: self.clone() }))
    }
    fn close(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => error.into_inner(),
        };
        state.closed = true;
        let _ = state.free.shrink(0);
    }
}
struct CheckpointLease {
    reservation: Reservation,
    credits: Arc<CheckpointCredits>,
}
impl std::ops::Deref for CheckpointLease {
    type Target = Reservation;
    fn deref(&self) -> &Reservation {
        &self.reservation
    }
}
impl Drop for CheckpointLease {
    fn drop(&mut self) {
        let mut state = match self.credits.state.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.closed = true;
                let _ = state.free.shrink(0);
                state
            }
        };
        if !state.closed {
            if state.free.bytes.checked_add(self.reservation.bytes).is_some_and(|n| n <= self.credits.capacity)
                && state.free.merge_from(&mut self.reservation).is_ok()
            {
                return;
            }
            state.closed = true;
            let _ = state.free.shrink(0);
        }
    }
}

impl RenderPreview {
    pub fn png(&self) -> &[u8] {
        &self.png
    }
}

/// Metadata for either runtime environment contract; wire objects retain their original shape.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum JobEnvironmentInfo {
    Cpu(crate::lighting::environment_input::EnvironmentInfo),
    Gpu(crate::lighting::environment::EnvironmentInfo),
}

#[derive(Clone, Debug, Serialize)]
pub struct JobStatus {
    /// Completed CPU passes whose immutable snapshot could not be published
    /// because both admitted slots remained pinned by external consumers.
    pub cpu_preview_retention_skips: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub punctual_diagnostics: Option<super::PunctualDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<super::presentation::MasterPresentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<super::adaptive::SamplingReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linear_master: Option<LinearMasterInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewInfo>,
    pub textures: Option<super::textures::TextureSummary>,
    pub options: JobOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<JobEnvironmentInfo>,
    pub job: JobId,
    pub scene_generation: u64,
    pub request: ImageRequest,
    pub resolved: crate::ResolvedRequest,
    pub progress: Progress,
    pub error: Option<JobError>,
    pub frame: Option<FrameInfo>,
}

struct Data {
    cpu_preview_retention_skips: u32,
    partial_preview: Option<Arc<RenderPartialPreview>>,
    punctual_diagnostics: Option<super::PunctualDiagnostics>,
    sampling: Option<super::adaptive::SamplingReport>,
    progress: Progress,
    error: Option<JobError>,
    frame: Option<Arc<RenderFrame>>,
    preview: Option<Arc<RenderPreview>>,
}
struct Record {
    partial: Option<Arc<PartialCredits>>,
    presentation: Option<Arc<RetainedPresentation>>,
    textures: Option<super::textures::TextureSummary>,
    options: JobOptions,
    environment: Option<JobEnvironmentInfo>,
    id: JobId,
    scene_generation: u64,
    request: ImageRequest,
    // Rust fields drop in declaration order: payload before its retained credits.
    _request_reservation: Option<Reservation>,
    resolved: crate::ResolvedRequest,
    cancel: CancelToken,
    data: Mutex<Data>,
    #[cfg(not(target_arch = "wasm32"))]
    done: Condvar,
}

impl Record {
    fn notify(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.done.notify_all();
    }
}

#[derive(Clone)]
pub struct JobHandle(Arc<Record>);

impl JobHandle {
    /// Internal admitted-work identity. Equal public IDs need not be one record.
    pub(super) fn same_record(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn id(&self) -> JobId {
        self.0.id
    }
    pub fn status(&self) -> Result<JobStatus> {
        let data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
        Ok(JobStatus {
            cpu_preview_retention_skips: data.cpu_preview_retention_skips,
            punctual_diagnostics: data.punctual_diagnostics,
            presentation: self.0.presentation.as_deref().map(|retained| retained.info.clone()),
            sampling: data.sampling.clone(),
            linear_master: data.frame.as_ref().and_then(|frame| frame.linear_master_info().cloned()),
            preview: data.preview.as_ref().map(|preview| preview.info.clone()),
            textures: self.0.textures.clone(),
            options: self.0.options,
            environment: self.0.environment.clone(),
            job: self.0.id,
            scene_generation: self.0.scene_generation,
            request: self.0.request.try_clone_for_host()?,
            resolved: self.0.resolved.clone(),
            progress: data.progress,
            error: data.error.clone(),
            frame: data.frame.as_ref().map(|f| f.info.clone()),
        })
    }
    pub fn cancel(&self) -> Result<JobStatus> {
        if let Some(credits) = &self.0.partial {
            credits.close();
        }
        let mut released_partial = None;
        {
            let mut data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
            if !data.progress.phase.terminal() {
                self.0.cancel.cancel();
                released_partial = data.partial_preview.take();
                data.preview = None;
                data.progress.phase = if data.progress.phase == Phase::Queued { Phase::Cancelled } else { Phase::Cancelling };
            }
        }
        drop(released_partial);
        self.0.notify();
        self.status()
    }

    /// Native application helper; engine status commands remain non-blocking.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_timeout(&self, timeout: std::time::Duration) -> Result<JobStatus> {
        let data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
        let (data, timed) = self
            .0
            .done
            .wait_timeout_while(data, timeout, |d| !d.progress.phase.terminal())
            .map_err(|_| JobError::new(ErrorCode::Internal, "render completion lock poisoned"))?;
        if timed.timed_out() && !data.progress.phase.terminal() {
            return Err(JobError::new(ErrorCode::TimedOut, "render wait timed out"));
        }
        drop(data);
        self.status()
    }
    pub fn frame(&self) -> Result<Option<Arc<RenderFrame>>> {
        let data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render result lock poisoned"))?;
        let frame = data.frame.clone();
        if frame.as_ref().is_some_and(|f| f.info.job != self.0.id || f.info.scene_generation != self.0.scene_generation) {
            return Err(JobError::new(ErrorCode::WrongOwner, "render frame identity does not match its job"));
        }
        Ok(frame)
    }

    /// A provisional complete-pass image, never the final/export frame slot.
    pub fn preview(&self) -> Result<Option<Arc<RenderPreview>>> {
        let data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render preview lock poisoned"))?;
        let preview = data.preview.clone();
        if preview.as_ref().is_some_and(|p| p.info.frame.job != self.0.id || p.info.frame.scene_generation != self.0.scene_generation) {
            return Err(JobError::new(ErrorCode::WrongOwner, "render preview identity does not match its job"));
        }
        Ok(preview)
    }
}

#[derive(Clone, Copy)]
enum CpuPreviewPolicy {
    Shared(super::PartialPreviewPlan),
    Protected(super::ProtectedPreviewPlan),
}
impl CpuPreviewPolicy {
    fn preview_plan(self) -> super::PartialPreviewPlan {
        match self {
            Self::Shared(plan) => plan,
            Self::Protected(plan) => plan.preview_plan(),
        }
    }
}
struct Submission {
    id: JobId,
    scene_generation: u64,
    scene: ProductScene,
    request: ImageRequest,
    options: JobOptions,
    environment: Option<crate::lighting::environment_input::EnvironmentInput>,
}
struct Work {
    regrade: Option<(Arc<RenderFrame>, super::presentation::Presentation)>,
    request: Option<ImageRequest>,
    executor: Option<Arc<dyn ImageExecutor>>,
    environment: Option<crate::lighting::environment_input::EnvironmentInput>,
    scene: ProductScene,
    handle: JobHandle,
    reservation: Reservation,
}
// One host advance hashes at most four 16 KiB chunks, independently of the
// integrator's quantum. The unchanged work reservation covers the sole PNG and
// this fixed state until publication; it is not another image task or pool.
const PNG_HASH_CHUNK_BYTES: usize = 16_384;
const PNG_HASH_CHUNKS_PER_ADVANCE: usize = 4;
pub(crate) const FRAME_METADATA_BYTES: u64 = 1024;

struct PendingPng {
    png: Vec<u8>,
    digest: Sha256,
    hashed: usize,
    preview_samples: Option<u32>,
    exr: Option<Vec<u8>>,
    png_digest: Option<sha2::digest::Output<Sha256>>,
    exr_hashed: usize,
}
// Keep the new fixed completion state within the existing metadata allowance
// even if the hashing dependency's representation changes on another target.
const _: [(); 1] = [(); (std::mem::size_of::<PendingPng>() <= 512) as usize];
const _: [(); 1] = [(); (std::mem::size_of::<Running>()
    + std::mem::size_of::<RenderFrame>()
    + std::mem::size_of::<RenderPreview>()
    + 64
    + 2 * crate::MAX_RENDER_ID_BYTES
    + 32
    <= FRAME_METADATA_BYTES as usize) as usize];

impl PendingPng {
    fn new(png: Vec<u8>) -> Self {
        Self { png, digest: Sha256::new(), hashed: 0, preview_samples: None, exr: None, png_digest: None, exr_hashed: 0 }
    }

    #[cfg(feature = "raytrace-cpu")]
    fn preview(png: Vec<u8>, samples: u32) -> Self {
        Self { preview_samples: Some(samples), ..Self::new(png) }
    }

    fn finish_png_digest(&mut self) {
        if self.hashed == self.png.len() && self.png_digest.is_none() {
            // PNG and EXR are scanned serially. Keep the completed PNG output,
            // then reuse a fresh digest state for EXR without another allocation.
            self.png_digest = Some(std::mem::take(&mut self.digest).finalize());
        }
    }

    fn step(&mut self, units: usize, cancel: &CancelToken) -> Result<bool> {
        cancel.check()?;
        self.finish_png_digest();
        for _ in 0..units.min(PNG_HASH_CHUNKS_PER_ADVANCE) {
            let (bytes, hashed) = if self.hashed < self.png.len() {
                (&self.png, &mut self.hashed)
            } else if let Some(exr) = self.exr.as_ref().filter(|exr| self.exr_hashed < exr.len()) {
                (exr, &mut self.exr_hashed)
            } else {
                break;
            };
            let end = hashed.saturating_add(PNG_HASH_CHUNK_BYTES).min(bytes.len());
            self.digest.update(bytes.get(*hashed..end).ok_or_else(|| JobError::new(ErrorCode::Internal, "missing completed image digest chunk"))?);
            *hashed = end;
            cancel.check()?;
            self.finish_png_digest();
        }
        Ok(self.hashed == self.png.len() && self.exr.as_ref().is_none_or(|exr| self.exr_hashed == exr.len()))
    }
    fn progress(&self) -> Progress {
        Progress {
            phase: Phase::Encoding,
            done: self.hashed as u64 + self.exr_hashed as u64,
            total: self.png.len() as u64 + self.exr.as_ref().map_or(0, |bytes| bytes.len() as u64),
        }
    }
}

#[cfg(test)]
#[test]
fn sequential_png_and_exr_hashing_preserves_independent_payload_digests() {
    let quantum_bytes = PNG_HASH_CHUNK_BYTES * PNG_HASH_CHUNKS_PER_ADVANCE;
    for units in [1, 3] {
        for exr_length in [None, Some(0), Some(quantum_bytes + 29)] {
            let png: Vec<u8> = (0..quantum_bytes + 13).map(|index| (index as u8).wrapping_mul(17).wrapping_add(3)).collect();
            let exr: Option<Vec<u8>> = exr_length.map(|length| (0..length).map(|index| (index as u8).wrapping_mul(29).wrapping_add(101)).collect());
            let expected_png = Sha256::digest(&png);
            let expected_exr = exr.as_ref().map(Sha256::digest);
            if let Some(expected_exr) = &expected_exr {
                assert_ne!(&expected_png, expected_exr, "the distinct payloads must not share a digest");
            }
            let png_length = png.len();
            let exr_length = exr.as_ref().map_or(0, Vec::len);
            assert!(png_length > quantum_bytes);
            assert!(exr_length == 0 || exr_length > quantum_bytes);
            let chunks = png_length.div_ceil(PNG_HASH_CHUNK_BYTES) + exr_length.div_ceil(PNG_HASH_CHUNK_BYTES);
            let steps = chunks.div_ceil(units);
            let mut pending = PendingPng::new(png);
            pending.exr = exr;
            let cancel = CancelToken::default();
            assert!(!pending.step(0, &cancel).unwrap());
            assert_eq!((pending.hashed, pending.exr_hashed), (0, 0));
            assert!(pending.png_digest.is_none());
            let mut crossed_within_step = false;
            let mut saw_png_only_boundary = false;
            for step in 0..steps {
                let before_png = pending.hashed;
                let before_exr = pending.exr_hashed;
                let done = pending.step(units, &cancel).unwrap();
                assert_eq!(done, step + 1 == steps);
                let advanced = pending.hashed + pending.exr_hashed - before_png - before_exr;
                assert!(advanced > 0 && advanced <= units * PNG_HASH_CHUNK_BYTES);
                if pending.hashed == png_length {
                    assert_eq!(pending.png_digest.as_ref(), Some(&expected_png));
                    saw_png_only_boundary |= pending.exr_hashed == 0;
                }
                crossed_within_step |= before_png < png_length && pending.hashed == png_length && pending.exr_hashed > 0;
                assert_eq!(pending.progress().done, (pending.hashed + pending.exr_hashed) as u64);
                assert_eq!(pending.progress().total, (png_length + exr_length) as u64);
            }
            assert_eq!((pending.hashed, pending.exr_hashed), (png_length, exr_length));
            if exr_length > 0 {
                assert_eq!(crossed_within_step, units == 3);
                assert_eq!(saw_png_only_boundary, units == 1);
            }
            for extra_units in [0, 1] {
                assert!(pending.step(extra_units, &cancel).unwrap());
                assert_eq!(pending.png_digest.as_ref(), Some(&expected_png));
                let actual_exr = pending.exr.as_ref().map(|_| pending.digest.clone().finalize());
                assert_eq!(actual_exr, expected_exr, "EXR must start from a fresh SHA state and survive repeated completed steps");
                assert_eq!((pending.hashed, pending.exr_hashed), (png_length, exr_length));
            }
        }
    }
}


struct Running {
    source_master: Option<Arc<RenderFrame>>,
    task: Option<WorkTask>,
    pending_png: Option<PendingPng>,
    handle: JobHandle,
    reservation: Option<Reservation>,
    next_preview_sequence: u32,
    next_checkpoint_pass: u32,
    checkpoints: Option<Arc<CheckpointCredits>>,
    // Task/PNG allocations above die before the in-flight credit below.
    partial: Option<TaskSlot<PartialRunning>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(credits) = &self.checkpoints {
            credits.close();
        }
    }
}

/// One fallibly allocated task, keeping the dispatch enum small in either build.
/// Its bounded metadata allocation fits the existing per-job task allowance.
struct TaskSlot<T>(Vec<T>);
impl<T> TaskSlot<T> {
    fn new(task: T) -> Result<Self> {
        if std::mem::size_of::<T>() > 4096 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "render task metadata exceeds its admission allowance"));
        }
        let mut tasks = Vec::new();
        tasks.try_reserve_exact(1).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "could not reserve render task slot"))?;
        if tasks.capacity() != 1 {
            return Err(JobError::new(ErrorCode::LimitExceeded, "render task slot exceeds its bounded capacity"));
        }
        tasks.push(task);
        Ok(Self(tasks))
    }
    fn get_mut(&mut self) -> Result<&mut T> {
        self.0.first_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing render task slot"))
    }
}
enum WorkTask {
    Regrade(TaskSlot<super::regrade::RegradeTask>),
    Registered(Box<dyn ImageWork>),
    Raster(TaskSlot<ImageTask>),
    #[cfg(feature = "raytrace-cpu")]
    Raytrace(TaskSlot<crate::raytrace::image::ImageTask>),
}

enum WorkPoll {
    #[cfg(feature = "raytrace-cpu")]
    Partial {
        png: Vec<u8>,
        covered: Vec<u32>,
        completed_passes: u32,
        updated_prefix: u64,
    },
    Existing(TaskPoll),
    #[cfg(feature = "raytrace-cpu")]
    Preview {
        png: Vec<u8>,
        samples_per_pixel: u32,
    },
}

impl WorkTask {
    fn protected_report(&self) -> Option<super::ProtectedPreviewReport> {
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(slot) => slot.0.first().and_then(|task| task.protected_preview_report()),
            _ => None,
        }
    }
    fn partial_ready(&mut self, plan: super::PartialPreviewPlan) -> Result<bool> {
        let _ = &plan;
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(task) => task.get_mut()?.partial_preview_ready(plan),
            _ => Ok(false),
        }
    }
    fn completed_pass_ready(&self, plan: super::PartialPreviewPlan) -> Result<Option<u32>> {
        let _ = &plan;
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(slot) => slot.0.first().map_or(Ok(None), |task| task.completed_pass_preview_ready(plan)),
            _ => Ok(None),
        }
    }
    fn begin_partial(&mut self, plan: super::PartialPreviewPlan) -> Result<bool> {
        let _ = &plan;
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(task) => match task.get_mut()?.begin_partial_preview(plan)? {
                crate::raytrace::image::PartialBegin::Started => Ok(true),
                crate::raytrace::image::PartialBegin::Exhausted => Ok(false),
            },
            _ => Err(JobError::new(ErrorCode::InvalidRequest, "partial presentation requires the builtin CPU task")),
        }
    }
    fn step_partial(&mut self, units: usize) -> Result<WorkPoll> {
        let _ = &units;
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(task) => Ok(match task.get_mut()?.step_with_partial_previews(units)? {
                crate::raytrace::image::PartialImagePoll::Pending(progress) => WorkPoll::Existing(TaskPoll::Pending(progress)),
                crate::raytrace::image::PartialImagePoll::Complete(png) => WorkPoll::Existing(TaskPoll::Complete(png)),
                crate::raytrace::image::PartialImagePoll::Preview { png, covered, completed_passes, updated_prefix } => {
                    WorkPoll::Partial { png, covered, completed_passes, updated_prefix }
                }
            }),
            _ => Err(JobError::new(ErrorCode::InvalidRequest, "partial presentation requires the builtin CPU task")),
        }
    }
    fn punctual_diagnostics(&self) -> Option<super::PunctualDiagnostics> {
        match self {
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(slot) => slot.0.first().and_then(|task| task.punctual_diagnostics()),
            _ => None,
        }
    }

    fn source_master(&self) -> Result<Option<Arc<RenderFrame>>> {
        match self {
            Self::Regrade(slot) => {
                slot.0.first().map(|task| Some(task.source())).ok_or_else(|| JobError::new(ErrorCode::Internal, "missing master task slot"))
            }
            _ => Ok(None),
        }
    }
    fn sampling_report(&self) -> Option<super::adaptive::SamplingReport> {
        match self {
            Self::Registered(task) => task.sampling_report(),
            _ => None,
        }
    }
    fn take_linear_exr(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Registered(task) => task.take_linear_exr(),
            _ => None,
        }
    }
    fn new(
        scene: ProductScene,
        request: ImageRequest,
        cancel: CancelToken,
        environment: Option<crate::lighting::environment_input::EnvironmentInput>,
        job_options: JobOptions,
        partial: Option<CpuPreviewPolicy>,
        report_sink: Option<&PartialCredits>,
    ) -> Result<Self> {
        let _ = (&environment, &job_options, &partial, &report_sink);
        match request.integrator {
            Integrator::Raster => ImageTask::new(scene, request, cancel).and_then(TaskSlot::new).map(Self::Raster),
            Integrator::Raytrace => {
                #[cfg(feature = "raytrace-cpu")]
                {
                    let options = request.raytrace.clone().unwrap_or_default();
                    let task = match partial {
                        Some(CpuPreviewPolicy::Shared(plan)) => crate::raytrace::image::ImageTask::new_with_environment_and_nested_and_partial(
                            scene,
                            request,
                            cancel,
                            options,
                            environment,
                            job_options.nested_media,
                            plan,
                        ),
                        Some(CpuPreviewPolicy::Protected(plan)) => {
                            crate::raytrace::image::ImageTask::new_with_environment_and_nested_and_protected_partial(
                                scene,
                                request,
                                cancel,
                                options,
                                environment,
                                job_options.nested_media,
                                plan,
                            )
                        }
                        None => crate::raytrace::image::ImageTask::new_with_environment_and_nested(
                            scene,
                            request,
                            cancel,
                            options,
                            environment,
                            job_options.nested_media,
                        ),
                    };
                    Self::raytrace_with_slot(task?, report_sink, TaskSlot::new)
                }
                #[cfg(not(feature = "raytrace-cpu"))]
                {
                    Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing is not compiled in; enable raytrace-cpu"))
                }
            }
        }
    }

    fn step(&mut self, units: usize) -> Result<WorkPoll> {
        match self {
            Self::Regrade(task) => task.get_mut()?.step(units).map(WorkPoll::Existing),
            Self::Registered(task) => task.step(units).map(WorkPoll::Existing),
            Self::Raster(task) => task.get_mut()?.step(units).map(WorkPoll::Existing),
            #[cfg(feature = "raytrace-cpu")]
            Self::Raytrace(task) => Ok(match task.get_mut()?.step_with_previews(units)? {
                crate::raytrace::image::ImagePoll::Pending(progress) => WorkPoll::Existing(TaskPoll::Pending(progress)),
                crate::raytrace::image::ImagePoll::Complete(png) => WorkPoll::Existing(TaskPoll::Complete(png)),
                crate::raytrace::image::ImagePoll::Preview { png, samples_per_pixel } => WorkPoll::Preview { png, samples_per_pixel },
            }),
        }
    }
}

fn terminal(handle: &JobHandle, error: JobError) {
    if let Some(credits) = &handle.0.partial {
        credits.close();
    }
    let mut released_partial = None;
    if let Ok(mut data) = handle.0.data.lock() {
        if data.progress.phase == Phase::Completed {
            return;
        }
        if handle.0.cancel.is_cancelled() || error.code == ErrorCode::Cancelled {
            data.progress.phase = Phase::Cancelled;
            data.error = None;
        } else {
            data.progress.phase = Phase::Failed;
            data.error = Some(error);
        }
        data.frame = None;
        data.preview = None;
        released_partial = data.partial_preview.take();
    }
    drop(released_partial);
    handle.0.notify();
}

impl Running {
    fn begin(work: Work) -> Option<Self> {
        Self::begin_with_partial_factory(work, |credits| TaskSlot::new(PartialRunning::new(credits)))
    }

    // The private factory keeps post-construction setup fallible without losing
    // the real ledger. It runs only after report capture and outside every lock.
    fn begin_with_partial_factory(
        mut work: Work,
        make_partial: impl FnOnce(Arc<PartialCredits>) -> Result<TaskSlot<PartialRunning>>,
    ) -> Option<Self> {
        let checkpoints = (|| {
            let slots = work
                .handle
                .0
                .request
                .raytrace
                .as_ref()
                .and_then(|options| options.progressive)
                .filter(|_| work.handle.0.partial.is_none() && work.executor.is_none())
                .map_or(0, |options| options.samples_per_pixel.saturating_sub(1).min(2));
            if slots == 0 {
                return Ok(None);
            }
            let slot_bytes = u64::from(work.handle.0.request.width)
                .checked_mul(u64::from(work.handle.0.request.height))
                .and_then(|n| n.checked_mul(8))
                .and_then(|n| n.checked_add(65_536 + FRAME_METADATA_BYTES))
                .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "checkpoint slot overflow"))?;
            CheckpointCredits::new(&mut work.reservation, slot_bytes, slots).map(Some)
        })();
        let checkpoints = match checkpoints {
            Ok(credits) => credits,
            Err(error) => {
                terminal(&work.handle, error);
                return None;
            }
        };
        if work.handle.0.cancel.is_cancelled() {
            terminal(&work.handle, JobError::new(ErrorCode::Cancelled, "render cancelled"));
            return None;
        }
        if let Ok(mut data) = work.handle.0.data.lock()
            && !work.handle.0.cancel.is_cancelled()
        {
            data.progress.phase = Phase::Preparing;
        }
        let task = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some((source, presentation)) = work.regrade {
                super::regrade::RegradeTask::new(source, presentation, work.handle.0.cancel.clone()).and_then(TaskSlot::new).map(WorkTask::Regrade)
            } else if let Some(executor) = work.executor {
                let request = work.request.ok_or_else(|| JobError::new(ErrorCode::Internal, "missing registered executor request"))?;
                executor.begin(work.scene, request, work.handle.0.options, work.handle.0.cancel.clone()).map(WorkTask::Registered)
            } else {
                WorkTask::new(
                    work.scene,
                    work.handle.0.request.try_clone_for_host()?,
                    work.handle.0.cancel.clone(),
                    work.environment,
                    work.handle.0.options,
                    work.handle.0.partial.as_ref().map(|credits| credits.policy()),
                    work.handle.0.partial.as_deref(),
                )
            }
        }))
        .unwrap_or_else(|_| Err(JobError::new(ErrorCode::Internal, "image executor failed while preparing the job")));
        let task = match task {
            Ok(task) => task,
            Err(error) => {
                terminal(&work.handle, error);
                return None;
            }
        };
        // The raw producer report was captured before the first task-slot wrap.
        // Only now attempt the additional partial runtime slot.
        let partial = work.handle.0.partial.as_ref().map(|credits| make_partial(credits.clone())).transpose();
        match partial {
            Ok(partial) => Some(Self {
                partial,
                source_master: None,
                task: Some(task),
                pending_png: None,
                handle: work.handle,
                reservation: Some(work.reservation),
                next_preview_sequence: 1,
                next_checkpoint_pass: 1,
                checkpoints,
            }),
            Err(error) => {
                terminal(&work.handle, error);
                // The real task's buffers die before work's remaining lease.
                None
            }
        }
    }
    fn advance(&mut self, units: usize) -> Result<bool> {
        self.handle.0.cancel.check()?;
        if self.pending_png.is_some() {
            return self.advance_png(units);
        }
        if let Some(partial) = self.partial.as_mut() {
            let partial = partial.get_mut()?;
            let task = self.task.as_ref().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing partial CPU task"))?;
            let uniform_pass = task.completed_pass_ready(partial.credits.plan)?;
            let automatic_pass = uniform_pass.filter(|pass| *pass > partial.last_pass_attempt);
            if let Some(pass) = automatic_pass {
                partial.last_pass_attempt = pass;
                partial.credits.request()?;
            }
            if partial.lease.is_none()
                && partial.credits.requested.load(Ordering::Acquire)
                && !partial.credits.exhausted.load(Ordering::Acquire)
                && (uniform_pass.is_none() || automatic_pass.is_some())
            {
                let task = self.task.as_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing partial CPU task"))?;
                if task.partial_ready(partial.credits.plan)? {
                    let mut lease = partial.credits.acquire()?;
                    if lease.is_none() && automatic_pass.is_some() {
                        let released = self
                            .handle
                            .0
                            .data
                            .lock()
                            .map_err(|_| JobError::new(ErrorCode::Internal, "partial retirement lock poisoned"))?
                            .partial_preview
                            .take();
                        drop(released);
                        lease = partial.credits.acquire()?;
                    }
                    if lease.is_none() && automatic_pass.is_some() && partial.credits.retained_slots_blocked()? {
                        let mut data = self
                            .handle
                            .0
                            .data
                            .lock()
                            .map_err(|_| JobError::new(ErrorCode::Internal, "partial retention diagnostic lock poisoned"))?;
                        self.handle.0.cancel.check()?;
                        data.cpu_preview_retention_skips = data
                            .cpu_preview_retention_skips
                            .checked_add(1)
                            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial retention diagnostic overflow"))?;
                    }
                    if let Some(lease) = lease {
                        // Keep the lease past any producer error until task buffers drop.
                        partial.lease = Some(lease);
                        let begin = task.begin_partial(partial.credits.plan);
                        partial.credits.store_report(task.protected_report())?;
                        if !begin? {
                            if partial.credits.protected.is_some() && !partial.credits.exhausted.load(Ordering::Acquire) {
                                return Err(JobError::new(ErrorCode::Internal, "protected feedback refused without real quota exhaustion"));
                            }
                            partial.credits.exhaust();
                            drop(partial.lease.take());
                        }
                    }
                }
            }
        }
        let task = self.task.as_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing render task"))?;
        let result = if self.partial.is_some() { task.step_partial(units) } else { task.step(units) };
        if let Some(credits) = &self.handle.0.partial {
            credits.store_report(task.protected_report())?;
        }
        let result = result?;
        let mut data = self.handle.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
        data.punctual_diagnostics = self.task.as_ref().and_then(WorkTask::punctual_diagnostics);
        // The final publication check is under the same lock as cancellation.
        if self.handle.0.cancel.is_cancelled() {
            data.progress.phase = Phase::Cancelled;
            data.frame = None;
            data.preview = None;
            let released = data.partial_preview.take();
            drop(data);
            if let Some(credits) = &self.handle.0.partial {
                credits.close();
            }
            drop(released);
            self.handle.0.notify();
            return Ok(false);
        }
        match result {
            #[cfg(feature = "raytrace-cpu")]
            WorkPoll::Partial { png, covered, completed_passes, updated_prefix } => {
                drop(data);
                self.capture_partial(png, covered, completed_passes, updated_prefix)?;
                Ok(true)
            }
            WorkPoll::Existing(TaskPoll::Pending(progress)) => {
                data.progress = progress;
                Ok(true)
            }
            WorkPoll::Existing(TaskPoll::Complete(png)) => {
                // Drop every integrator-owned capture/scratch buffer first, but
                // keep the original admission lease while hashing the sole PNG.
                self.source_master = self.task.as_ref().map(WorkTask::source_master).transpose()?.flatten();
                data.sampling = self.task.as_ref().and_then(WorkTask::sampling_report);
                let exr = self.task.as_mut().and_then(WorkTask::take_linear_exr);
                if exr.is_some() != self.handle.0.options.linear_exr {
                    return Err(JobError::new(ErrorCode::Internal, "linear master availability differs from the admitted request"));
                }
                if let Some(bytes) = &exr {
                    let limit = super::linear_exr::encoded_limit(self.handle.0.request.width, self.handle.0.request.height)?;
                    if bytes.len() < 8 || bytes.capacity() as u64 > limit || bytes.get(..8) != Some(&[0x76, 0x2f, 0x31, 0x01, 2, 0, 0, 0]) {
                        return Err(JobError::new(ErrorCode::LimitExceeded, "linear master exceeds its bound or has an invalid signature"));
                    }
                }
                self.task = None;
                let mut pending = PendingPng::new(png);
                pending.exr = exr;
                data.progress = pending.progress();
                self.pending_png = Some(pending);
                Ok(true)
            }
            #[cfg(feature = "raytrace-cpu")]
            WorkPoll::Preview { png, samples_per_pixel } => {
                // Retain the integrator/film and the entire remaining work lease.
                // Hash the checkpoint before making immutable bytes observable.
                let pending = PendingPng::preview(png, samples_per_pixel);
                data.progress = pending.progress();
                self.pending_png = Some(pending);
                Ok(true)
            }
        }
    }

    fn advance_png(&mut self, units: usize) -> Result<bool> {
        // No status/data lock is held while scanning the encoded image.
        let pending = self.pending_png.as_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing pending PNG"))?;
        if !pending.step(units, &self.handle.0.cancel)? {
            let progress = pending.progress();
            let mut data = self.handle.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
            self.handle.0.cancel.check()?;
            data.progress = progress;
            return Ok(true);
        }
        let PendingPng { png, digest, preview_samples, exr, png_digest, .. } =
            self.pending_png.take().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing completed PNG digest"))?;
        let png_digest = png_digest.ok_or_else(|| JobError::new(ErrorCode::Internal, "missing finalized PNG digest"))?;
        if self.partial.as_ref().and_then(|slot| slot.0.first()).is_some_and(|partial| partial.pending.is_some()) {
            return self.publish_partial(png, png_digest);
        }
        let info = FrameInfo {
            job: self.handle.0.id,
            scene_generation: self.handle.0.scene_generation,
            backend: self.handle.0.resolved.backend.clone(),
            executor: self.handle.0.resolved.executor.clone(),
            width: self.handle.0.request.width,
            height: self.handle.0.request.height,
            encoding: ColourEncoding::Srgb,
            alpha: AlphaConvention::Straight,
            png_bytes: png.len() as u64,
            png_sha256: format!("{:x}", png_digest),
        };
        // Include the working digest/Vec metadata, completed frame layout,
        // actual hex allocation, bounded copied IDs and Arc/header allowance.
        // The PNG allocation itself is retained once and billed by capacity below.
        let frame_layout = if preview_samples.is_some() { std::mem::size_of::<RenderPreview>() } else { std::mem::size_of::<RenderFrame>() };
        let checkpoint_metadata =
            if preview_samples.is_some() { std::mem::size_of::<CheckpointCredits>() + 2 * std::mem::size_of::<usize>() } else { 0 };
        let fixed_metadata = std::mem::size_of::<Running>() + frame_layout + checkpoint_metadata + 2 * crate::MAX_RENDER_ID_BYTES + 32;
        let linear_digest_bytes = if exr.is_some() { 64 } else { 0 };
        if fixed_metadata
            .checked_add(info.png_sha256.capacity())
            .and_then(|n| n.checked_add(linear_digest_bytes))
            .is_none_or(|bytes| bytes > FRAME_METADATA_BYTES as usize)
        {
            return Err(JobError::new(ErrorCode::LimitExceeded, "PNG digest metadata exceeds its existing admission allowance"));
        }
        let mut data = self.handle.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?;
        // Cancellation shares this lock; a late cancel must win before frame
        // installation. The digest is complete; checkpoints retain task scratch,
        // while the final-only path has already released it.
        self.handle.0.cancel.check()?;
        if let Some(samples_per_pixel) = preview_samples {
            let options = self
                .handle
                .0
                .request
                .raytrace
                .as_ref()
                .and_then(|options| options.progressive)
                .ok_or_else(|| JobError::new(ErrorCode::Internal, "preview requires CPU progressive intent"))?;
            let frame_sequence = self.next_preview_sequence;
            let expected = Some(self.next_checkpoint_pass);
            if expected != Some(samples_per_pixel) || samples_per_pixel >= options.samples_per_pixel || self.task.is_none() {
                return Err(JobError::new(ErrorCode::Internal, "invalid progressive checkpoint sequence"));
            }
            let next_sequence = frame_sequence.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::Internal, "preview sequence exhausted"))?;
            let credits = self.checkpoints.as_ref().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing checkpoint credits"))?;
            if (png.capacity() as u64).checked_add(FRAME_METADATA_BYTES).is_none_or(|bytes| bytes > credits.slot_bytes) {
                return Err(JobError::new(ErrorCode::LimitExceeded, "checkpoint PNG exceeds its admitted slot"));
            }
            let mut reservation = credits.acquire()?;
            if reservation.is_none() {
                // Retire the host's obsolete slot before treating a consumer as
                // pinned. A headless client never has to acknowledge previews.
                let released = data.preview.take();
                drop(data);
                drop(released);
                reservation = credits.acquire()?;
                data = self.handle.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render preview publication lock poisoned"))?;
                self.handle.0.cancel.check()?;
            }
            self.next_checkpoint_pass = self
                .next_checkpoint_pass
                .checked_add(1)
                .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "checkpoint pass ordinal exhausted"))?;
            let Some(reservation) = reservation else {
                data.cpu_preview_retention_skips = data
                    .cpu_preview_retention_skips
                    .checked_add(1)
                    .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "checkpoint retention diagnostic overflow"))?;
                return Ok(true);
            };
            data.preview = Some(Arc::new(RenderPreview {
                info: PreviewInfo { frame_sequence, samples_per_pixel, frame: info },
                png,
                _reservation: reservation,
            }));
            self.next_preview_sequence = next_sequence;
            let pixels = u64::from(self.handle.0.request.width) * u64::from(self.handle.0.request.height);
            data.progress =
                Progress { phase: Phase::Running, done: pixels * u64::from(samples_per_pixel), total: pixels * u64::from(options.samples_per_pixel) };
            self.handle.0.notify();
            return Ok(true);
        }
        let exr = exr.map(|bytes| {
            let info = LinearMasterInfo {
                bytes: bytes.len() as u64,
                sha256: format!("{:x}", digest.finalize()),
                encoding: LinearColourEncoding::SceneLinearSrgbD65,
                alpha: LinearAlphaConvention::CoveragePremultiplied,
            };
            (bytes, info)
        });
        let retained = png.capacity() as u64 + exr.as_ref().map_or(0, |(bytes, _)| bytes.capacity() as u64) + FRAME_METADATA_BYTES;
        let mut reservation = self.reservation.take().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing render reservation"))?;
        reservation.shrink(retained)?;
        data.frame = Some(Arc::new(RenderFrame {
            info,
            png,
            exr,
            source_master: self.source_master.take(),
            presentation: self.handle.0.presentation.clone(),
            _reservation: reservation,
        }));
        data.preview = None;
        let released = data.partial_preview.take();
        data.progress = Progress { phase: Phase::Completed, done: 1, total: 1 };
        if let Some(credits) = &self.checkpoints {
            credits.close();
        }
        drop(data);
        if let Some(credits) = &self.handle.0.partial {
            credits.close();
        }
        drop(released);
        self.handle.0.notify();
        Ok(false)
    }

    fn guarded_step(&mut self, units: usize) -> bool {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.advance(units)));
        match result {
            Ok(Ok(more)) => more,
            Ok(Err(e)) => {
                terminal(&self.handle, e);
                false
            }
            Err(_) => {
                terminal(&self.handle, JobError::new(ErrorCode::Internal, "image host encountered an internal error"));
                false
            }
        }
    }
}

struct Shared {
    queue: Mutex<VecDeque<Work>>,
    active: Mutex<Option<Weak<Record>>>,
    stop: AtomicBool,
    budget: Arc<Budget>,
    #[cfg(not(target_arch = "wasm32"))]
    wake: Condvar,
}

struct Owner {
    executor: Option<Arc<dyn ImageExecutor>>,
    shared: Arc<Shared>,
    kind: HostKind,
    running: Mutex<Option<Running>>,
    #[cfg(not(target_arch = "wasm32"))]
    _worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Ok(active) = self.shared.active.lock()
            && let Some(record) = active.as_ref().and_then(Weak::upgrade)
        {
            let _ = JobHandle(record).cancel();
        }
        if let Ok(mut queue) = self.shared.queue.lock() {
            for work in queue.iter() {
                let _ = work.handle.cancel();
            }
            queue.clear();
        }
        if let Ok(mut running) = self.running.lock()
            && let Some(work) = running.take()
        {
            let _ = work.handle.cancel();
            terminal(&work.handle, JobError::new(ErrorCode::Cancelled, "render host closed"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.shared.wake.notify_all();
        // Detaching cannot stall the UI; the bounded worker observes stop/cancel at its next unit.
    }
}

#[derive(Clone)]
pub struct ImageHost(Arc<Owner>);

/// Builtin CPU constructors share the registered image host lifetime machinery.
pub type CpuHost = ImageHost;

impl ImageHost {
    fn shared(limit: u64) -> Arc<Shared> {
        Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            active: Mutex::new(None),
            stop: AtomicBool::new(false),
            budget: Arc::new(Budget { used: AtomicU64::new(0), limit }),
            #[cfg(not(target_arch = "wasm32"))]
            wake: Condvar::new(),
        })
    }

    pub fn cooperative() -> Self {
        Self(Arc::new(Owner {
            executor: None,
            shared: Self::shared(MEMORY_LIMIT),
            kind: HostKind::Cooperative,
            running: Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            _worker: None,
        }))
    }

    /// A cooperative host over `executor` (pumped by its caller).
    #[cfg(test)]
    fn cooperative_with(executor: Arc<dyn ImageExecutor>) -> Self {
        Self(Arc::new(Owner {
            executor: Some(executor),
            shared: Self::shared(MEMORY_LIMIT),
            kind: HostKind::Cooperative,
            running: Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            _worker: None,
        }))
    }

    pub fn for_platform() -> Result<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Self::cooperative())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pool = if usize::BITS >= 64 { super::NATIVE_CPU_POOL_BYTES } else { MEMORY_LIMIT };
            let shared = Self::shared(pool);
            let worker_shared = shared.clone();
            let worker = std::thread::Builder::new()
                .name("solidcraft-render".into())
                .spawn(move || native_worker(worker_shared))
                .map_err(|e| JobError::new(ErrorCode::HostUnavailable, format!("could not start CPU image worker: {e}")))?;
            Ok(Self(Arc::new(Owner { executor: None, shared, kind: HostKind::NativeWorker, running: Mutex::new(None), _worker: Some(worker) })))
        }
    }

    /// Start an isolated bounded native worker. GPU discovery occurs before registration.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_executor(executor: Arc<dyn ImageExecutor>) -> Result<Self> {
        let descriptor = executor.descriptor()?;
        if descriptor.profiles.len() != 1
            || descriptor
                .profiles
                .iter()
                .any(|p| p.mode != RenderMode::Preview || p.operation != RenderOperation::Image || p.formats != [RenderFormat::Png])
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "image worker currently accepts one Preview/Image/PNG profile"));
        }
        let limit = executor.memory_limit_bytes();
        let ceiling = if usize::BITS >= 64 { super::NATIVE_CPU_POOL_BYTES } else { MEMORY_LIMIT };
        if limit == 0 || limit > ceiling {
            return Err(JobError::new(ErrorCode::InvalidRequest, "registered image pool exceeds supported address space"));
        }
        let shared = Self::shared(limit);
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("solidcraft-render".into())
            .spawn(move || native_worker(worker_shared))
            .map_err(|e| JobError::new(ErrorCode::HostUnavailable, format!("could not start image worker: {e}")))?;
        Ok(Self(Arc::new(Owner { executor: Some(executor), shared, kind: HostKind::NativeWorker, running: Mutex::new(None), _worker: Some(worker) })))
    }

    pub fn kind(&self) -> HostKind {
        self.0.kind
    }

    pub fn same_host(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn is_available(&self) -> bool {
        !self.0.shared.stop.load(Ordering::Acquire)
    }
    pub fn admitted_bytes(&self) -> u64 {
        self.0.shared.budget.used.load(Ordering::Acquire)
    }

    /// Internal completed-film adapter: the existing host budget, no new pool.
    pub(crate) fn reserve_film_snapshot(&self, bytes: u64) -> Result<Reservation> {
        if self.0.executor.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "CPU film snapshots require the builtin CPU host"));
        }
        if !self.is_available() {
            return Err(JobError::new(ErrorCode::HostUnavailable, "CPU image host has stopped"));
        }
        if bytes > super::image::MAX_JOB_BYTES {
            return Err(JobError::new(ErrorCode::LimitExceeded, "film snapshot exceeds per-job admission"));
        }
        self.0.shared.budget.reserve(bytes)
    }

    pub fn descriptor(&self) -> Result<ExecutorDescriptor> {
        self.descriptor_for(Integrator::Raster)
    }

    pub fn job_limit_bytes(&self) -> u64 {
        if self.0.executor.is_some() { self.0.shared.budget.limit } else { super::image::MAX_JOB_BYTES }
    }

    pub fn pool_limit_bytes(&self) -> u64 {
        self.0.shared.budget.limit
    }

    pub fn admit(&self, scene: &ProductScene, request: &ImageRequest, options: JobOptions) -> Result<Admission> {
        self.admit_with_policy(scene, request, options, None)
    }
    /// Same canonical policy and complete retained/HDR accounting as protected submit.
    /// Non-reserving estimate: submission still admits against the live pool.
    pub fn admit_cpu_protected_partial_with_environment_and_options(
        &self,
        scene: &ProductScene,
        request: &ImageRequest,
        options: JobOptions,
        environment: Option<&crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<Admission> {
        self.validate_submission_environment(scene, request, environment)?;
        let policy = Self::protected_preview_policy(request)?;
        let (mut admission, request_bytes) = self.admit_submission(scene, request, options, environment, Some(policy))?;
        admission.estimated_total_bytes = admission
            .estimated_total_bytes
            .checked_add(request_bytes)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "retained request admission overflow"))?;
        Ok(admission)
    }

    fn protected_preview_policy(request: &ImageRequest) -> Result<CpuPreviewPolicy> {
        let samples = request
            .raytrace
            .as_ref()
            .and_then(|v| v.progressive)
            .ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "protected previews require CPU progressive intent"))?
            .samples_per_pixel;
        let plan = super::ProtectedPreviewPlan::new(request.width, request.height, samples, request.cpu_budget())?;
        Ok(CpuPreviewPolicy::Protected(plan))
    }

    fn validate_submission_environment(
        &self,
        scene: &ProductScene,
        request: &ImageRequest,
        environment: Option<&crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<()> {
        if self.0.executor.is_some() && environment.is_some() {
            return Err(JobError::new(ErrorCode::InvalidRequest, "registered image executors do not accept CPU environments"));
        }
        if environment.is_some() && request.integrator != Integrator::Raytrace {
            return Err(JobError::new(ErrorCode::InvalidRequest, "environment lighting requires CPU raytrace"));
        }
        self.validate_integrator(request.integrator)?;
        if environment.is_some() {
            request.raytrace.as_ref().unwrap_or(&super::RaytraceOptions::default()).validate_environment_reflections(scene)?;
        }
        Ok(())
    }

    fn admit_submission(
        &self,
        scene: &ProductScene,
        request: &ImageRequest,
        options: JobOptions,
        environment: Option<&crate::lighting::environment_input::EnvironmentInput>,
        partial: Option<CpuPreviewPolicy>,
    ) -> Result<(Admission, u64)> {
        if environment.is_some() && request.raytrace.as_ref().is_some_and(|options| options.directional_shadow_transmission) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "directional shadow transmission does not support HDR environments"));
        }
        let metadata_bytes =
            (if environment.is_some() { std::mem::size_of::<crate::lighting::environment_input::EnvironmentInfo>() as u64 } else { 0 })
                + request.retained_resource_metadata_bytes();
        let request_bytes = retained_request_bytes(request)?
            .checked_add(metadata_bytes)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "environment metadata overflow"))?;
        let mut admission = self.admit_with_policy(scene, request, options, partial)?;
        let environment_bytes =
            environment.map_or(Ok(0), |value| value.required_bytes()).map_err(|error| JobError::new(ErrorCode::InvalidRequest, error.to_string()))?;
        admission.estimated_total_bytes = admission
            .estimated_total_bytes
            .checked_add(environment_bytes)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "environment admission overflow"))?;
        admission.preparation_bytes = admission
            .preparation_bytes
            .checked_add(environment_bytes)
            .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "environment admission overflow"))?;
        self.check_request_sum(admission.estimated_total_bytes, request_bytes, request.cpu_budget())?;
        Ok((admission, request_bytes))
    }

    fn admit_with_policy(
        &self,
        scene: &ProductScene,
        request: &ImageRequest,
        options: JobOptions,
        partial: Option<CpuPreviewPolicy>,
    ) -> Result<Admission> {
        request.validate_raster_request()?;
        if self.0.executor.is_some() && (request.has_explicit_raster_lights() || request.raster_exposure_ev != 0.0) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "registered executors do not accept Raster-only lights or exposure"));
        }
        if let Some(policy) = partial {
            let plan = policy.preview_plan();
            if self.0.executor.is_some()
                || request.integrator != Integrator::Raytrace
                || request.transparent_background
                || request.environment.is_some()
                || request.textures.is_some()
                || request.raytrace.as_ref().and_then(|v| v.progressive).is_none()
                || plan != super::PartialPreviewPlan::new(request.width, request.height)?
            {
                return Err(JobError::new(ErrorCode::InvalidRequest, "partial previews require the canonical builtin CPU progressive request"));
            }
            if let CpuPreviewPolicy::Protected(protected) = policy {
                let samples = request
                    .raytrace
                    .as_ref()
                    .and_then(|v| v.progressive)
                    .ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "protected previews require a progressive sample target"))?
                    .samples_per_pixel;
                protected.validate()?;
                if protected != super::ProtectedPreviewPlan::new(request.width, request.height, samples, request.cpu_budget())? {
                    return Err(JobError::new(ErrorCode::InvalidRequest, "protected feedback plan does not match its admitted request"));
                }
            }
        }
        let scatter = |t: Option<crate::transmission::Transmission>| t.and_then(|v| v.subsurface()).is_some_and(|s| s.active());
        if scene.meshes.iter().any(|b| scatter(b.transmission) || b.faces.iter().any(|f| scatter(f.transmission)))
            && !self.0.executor.as_ref().is_some_and(|executor| executor.supports_subsurface())
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "installed image producer does not implement interior subsurface random walks"));
        }

        self.validate_integrator(request.integrator)?;
        options.validate()?;
        if let Some(executor) = &self.0.executor {
            if request.raytrace.is_some() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executors do not accept CPU raytrace options"));
            }
            if options.aging.active() && !executor.supports_aging() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "this executor cannot evaluate ordered surface aging"));
            }
            if options.finishing.photographic.active() && !executor.supports_photographic() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "this executor cannot apply photographic lens effects"));
            }
            if options.nested_media && !executor.supports_nested_media() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executor does not implement certified nested media"));
            }
            if options.volume.is_some() && !executor.supports_volume() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executor does not implement bounded homogeneous volume transport"));
            }
            if options.adaptive.is_some() && !executor.supports_adaptive_sampling() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executor does not implement independent-pilot adaptive sampling"));
            }
            if options.transmission_samples != 1 && !executor.supports_transmission_sampling() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executor does not implement transmission strata"));
            }
            if options.linear_exr && !executor.supports_linear_exr() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "this executor cannot retain a raw linear EXR"));
            }
            if options.finishing.bloom.is_some_and(|b| b.active()) && !executor.supports_bloom() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered executor does not implement display bloom"));
            }
            if (!options.finishing.is_identity() || options.backdrop.is_some()) && !executor.supports_finishing() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "this executor cannot apply the requested finishing/backdrop"));
            }
            let admission = executor.admit(scene, request, options)?;
            if admission.estimated_total_bytes > self.job_limit_bytes() {
                return Err(JobError::new(ErrorCode::LimitExceeded, "registered image job exceeds its admitted host pool"));
            }
            Ok(admission)
        } else {
            request.cpu_budget().validate_execution()?;
            // Only the existing nesting opt-in is admitted on the CPU raytrace path.
            // Every other JobOptions field retains its exact default-only contract.
            if (JobOptions { nested_media: false, ..options }) != JobOptions::default() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "CPU images use raytrace options, not progressive sampling controls"));
            }
            if let Some(policy) = partial {
                #[cfg(feature = "raytrace-cpu")]
                {
                    let raytrace = request
                        .raytrace
                        .as_ref()
                        .ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "partial previews require CPU progressive options"))?;
                    return match policy {
                        CpuPreviewPolicy::Shared(plan) => {
                            crate::raytrace::image::admit_with_partial_preview(scene, request, raytrace, options.nested_media, plan)
                        }
                        CpuPreviewPolicy::Protected(plan) => {
                            crate::raytrace::image::admit_with_protected_partial_preview(scene, request, raytrace, options.nested_media, plan)
                        }
                    };
                }
                #[cfg(not(feature = "raytrace-cpu"))]
                {
                    let _ = policy;
                    return Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing is not compiled in; enable raytrace-cpu"));
                }
            }
            if !options.nested_media {
                return admit(scene, request);
            }
            if request.integrator != Integrator::Raytrace || request.environment.is_some() || request.textures.is_some() || scene.has_transmission() {
                return Err(JobError::new(ErrorCode::InvalidRequest, "CPU nested media require explicit raytrace dielectric overrides"));
            }
            #[cfg(feature = "raytrace-cpu")]
            {
                let defaults = super::RaytraceOptions::default();
                let raytrace = request.raytrace.as_ref().unwrap_or(&defaults);
                crate::raytrace::image::admit_with_nested(scene, request, raytrace, true)
            }
            #[cfg(not(feature = "raytrace-cpu"))]
            {
                Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing is not compiled in; enable raytrace-cpu"))
            }
        }
    }

    fn check_request_sum(&self, work: u64, request: u64, budget: super::CpuBudget) -> Result<()> {
        if self.0.executor.is_none() {
            if budget == super::CpuBudget::Compact {
                return admit_request_sum(work, request);
            }
            if work.checked_add(request).is_none_or(|n| n > budget.limits().max_job_bytes) {
                return Err(JobError::new(ErrorCode::LimitExceeded, "CPU work and retained request exceed the selected profile"));
            }
            return Ok(());
        }
        if work.checked_add(request).is_none_or(|n| n > self.job_limit_bytes()) {
            return Err(JobError::new(ErrorCode::LimitExceeded, "registered image job exceeds its admitted host pool"));
        }
        Ok(())
    }

    /// Check the execution choice without capturing geometry or reserving memory.
    pub fn validate_integrator(&self, integrator: Integrator) -> Result<()> {
        if self.0.executor.is_some() && integrator != Integrator::Raster {
            return Err(JobError::new(ErrorCode::InvalidRequest, "registered image executors do not accept CPU raytrace requests"));
        }
        if !self.is_available() {
            return Err(JobError::new(ErrorCode::HostUnavailable, "image host has stopped"));
        }
        if integrator == Integrator::Raytrace && !cfg!(feature = "raytrace-cpu") {
            return Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing is not compiled in; enable raytrace-cpu"));
        }
        if integrator == Integrator::Raytrace && self.kind() != HostKind::NativeWorker {
            return Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing requires the native worker host"));
        }
        Ok(())
    }

    /// Select exactly one executor for this request. Never rely on registry order
    /// to choose between raster and raytracing implementations.
    pub fn descriptor_for(&self, integrator: Integrator) -> Result<ExecutorDescriptor> {
        if let Some(executor) = &self.0.executor {
            if integrator != Integrator::Raster {
                return Err(JobError::new(ErrorCode::InvalidRequest, "registered image executors do not accept CPU raytrace requests"));
            }
            let mut descriptor = executor.descriptor()?;
            if !self.is_available() {
                descriptor.availability = Availability::Unavailable { reason: "image host has stopped".into() };
            }
            return Ok(descriptor);
        }
        if integrator == Integrator::Raytrace && !cfg!(feature = "raytrace-cpu") {
            return Err(JobError::new(ErrorCode::HostUnavailable, "CPU raytracing is not compiled in; enable raytrace-cpu"));
        }
        Ok(ExecutorDescriptor {
            id: ExecutorId::new(match integrator {
                Integrator::Raster => "cpu-product",
                Integrator::Raytrace => "cpu-raytrace",
            })
            .map_err(|e| JobError::new(ErrorCode::Internal, e.to_string()))?,
            availability: if self.0.shared.stop.load(Ordering::Acquire) {
                Availability::Unavailable { reason: "image host has stopped".into() }
            } else if integrator == Integrator::Raytrace && self.kind() != HostKind::NativeWorker {
                Availability::Unavailable { reason: "CPU raytracing requires the native worker host".into() }
            } else {
                Availability::Available
            },
            profiles: vec![RenderProfile {
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                formats: vec![RenderFormat::Png],
                features: if integrator == Integrator::Raytrace && cfg!(feature = "raytrace-cpu") && self.kind() == HostKind::NativeWorker {
                    // Refraction is the certified thin-sheet/axis-aligned-box CPU preview
                    // contract. Admission still refuses unsupported geometry and combinations.
                    vec![crate::RenderFeature::Pbr, crate::RenderFeature::Hdri, crate::RenderFeature::Refraction]
                } else {
                    vec![crate::RenderFeature::Pbr]
                },
            }],
            limits: if integrator == Integrator::Raytrace && self.kind() == HostKind::NativeWorker && usize::BITS >= 64 {
                super::CpuBudget::Workstation.image_limits(false)
            } else {
                RenderLimits::default()
            },
            scratch: ScratchLayout { sample_scale: 2, sample_bytes_per_pixel: 8, output_bytes_per_pixel: 8, encoding_bytes_per_pixel: 8 },
        })
    }

    /// Resolve a CPU request against its explicit envelope, never the largest advertised one.
    pub fn descriptor_for_request(&self, request: &ImageRequest) -> Result<ExecutorDescriptor> {
        let mut descriptor = self.descriptor_for(request.integrator)?;
        if self.0.executor.is_none() && request.integrator == Integrator::Raytrace {
            let budget = request.cpu_budget();
            budget.validate_execution()?;
            budget.validate_image(request.width, request.height, request.raytrace.as_ref().and_then(|o| o.progressive))?;
            descriptor.limits = budget.image_limits(request.raytrace.as_ref().is_some_and(|o| o.progressive.is_some()));
        }
        Ok(descriptor)
    }

    pub fn submit(&self, id: JobId, scene_generation: u64, scene: ProductScene, request: ImageRequest) -> Result<JobHandle> {
        self.submit_with_environment(id, scene_generation, scene, request, None)
    }

    /// Captured HDR ownership stays in work, never serialized/cloned into status.
    pub fn submit_with_environment(
        &self,
        id: JobId,
        scene_generation: u64,
        scene: ProductScene,
        request: ImageRequest,
        environment: Option<crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<JobHandle> {
        self.submit_with_environment_and_options(id, scene_generation, scene, request, JobOptions::default(), environment)
    }

    pub fn submit_with_options(
        &self,
        id: JobId,
        scene_generation: u64,
        scene: ProductScene,
        request: ImageRequest,
        options: JobOptions,
    ) -> Result<JobHandle> {
        self.submit_with_environment_and_options(id, scene_generation, scene, request, options, None)
    }

    pub fn submit_with_environment_and_options(
        &self,
        id: JobId,
        scene_generation: u64,
        scene: ProductScene,
        request: ImageRequest,
        options: JobOptions,
        environment: Option<crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<JobHandle> {
        self.submit_with_policy(Submission { id, scene_generation, scene, request, options, environment }, None)
    }
    /// Transient CPU presentation policy. Final request and legacy observers are unchanged.
    pub fn submit_cpu_partial_with_environment_and_options(
        &self,
        id: JobId,
        scene_generation: u64,
        scene: ProductScene,
        request: ImageRequest,
        options: JobOptions,
        environment: Option<crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<JobHandle> {
        let plan = super::PartialPreviewPlan::new(request.width, request.height)?;
        self.submit_with_policy(Submission { id, scene_generation, scene, request, options, environment }, Some(CpuPreviewPolicy::Shared(plan)))
    }
    /// Opt-in protected feedback preserves ordinary work while admitting a finite extra quota.
    pub fn submit_cpu_protected_partial_with_environment_and_options(
        &self,
        id: JobId,
        scene_generation: u64,
        scene: ProductScene,
        request: ImageRequest,
        options: JobOptions,
        environment: Option<crate::lighting::environment_input::EnvironmentInput>,
    ) -> Result<JobHandle> {
        let policy = Self::protected_preview_policy(&request)?;
        self.submit_with_policy(Submission { id, scene_generation, scene, request, options, environment }, Some(policy))
    }
    fn submit_with_policy(&self, input: Submission, partial_plan: Option<CpuPreviewPolicy>) -> Result<JobHandle> {
        let Submission { id, scene_generation, scene, request, options, environment } = input;
        self.validate_submission_environment(&scene, &request, environment.as_ref())?;
        if id.session == 0 || id.serial == 0 || id.generation == 0 {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid render job identity"));
        }
        let environment_info = environment
            .as_ref()
            .map(|e| JobEnvironmentInfo::Cpu(e.metadata()))
            .or_else(|| request.environment.as_ref().map(|source| JobEnvironmentInfo::Gpu(source.info().clone())));
        let textures = request.textures.as_ref().map(|source| source.summary());
        let mut features = scene.required_features();
        if environment.is_some() || request.environment.is_some() {
            features.push(crate::RenderFeature::Hdri);
        }
        if request.integrator == Integrator::Raytrace
            && request.raytrace.as_ref().is_some_and(|options| !options.dielectrics.is_empty())
            && !features.contains(&crate::RenderFeature::Refraction)
        {
            features.push(crate::RenderFeature::Refraction);
        }
        let (Admission { estimated_total_bytes, .. }, request_bytes) =
            self.admit_submission(&scene, &request, options, environment.as_ref(), partial_plan)?;
        let mut backend = crate::builtin_backend();
        let descriptor = self.descriptor_for_request(&request)?;
        let expected_executor = descriptor.id.clone();
        backend.executors = vec![descriptor];
        let resolved = crate::resolve_request(
            &[backend],
            &crate::RenderRequest {
                backend: None,
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                width: u64::from(request.width),
                height: u64::from(request.height),
                format: Some(RenderFormat::Png),
                features,
                policy: Default::default(),
            },
        )
        .map_err(|e| JobError::new(ErrorCode::HostUnavailable, e.to_string()))?;
        if resolved.executor != expected_executor {
            return Err(JobError::new(ErrorCode::HostUnavailable, "CPU product executor is unavailable"));
        }
        let mut queue = self.0.shared.queue.lock().map_err(|_| JobError::new(ErrorCode::HostUnavailable, "render queue lock poisoned"))?;
        if self.0.shared.stop.load(Ordering::Acquire) {
            return Err(JobError::new(ErrorCode::HostUnavailable, "image host has stopped"));
        }
        queue.retain(|work| !work.handle.0.cancel.is_cancelled());
        if queue.len() >= QUEUE_LIMIT {
            return Err(JobError::new(ErrorCode::QueueFull, "image queue is full"));
        }
        self.check_request_sum(estimated_total_bytes, request_bytes, request.cpu_budget())?;
        let mut reservation = self.0.shared.budget.reserve(estimated_total_bytes)?;

        let partial_credits = partial_plan
            .map(|policy| match policy {
                CpuPreviewPolicy::Shared(plan) => PartialCredits::new(plan, &mut reservation),
                CpuPreviewPolicy::Protected(plan) => PartialCredits::new_protected(plan, &mut reservation),
            })
            .transpose()?;
        let request_reservation = if request_bytes == 0 { None } else { Some(self.0.shared.budget.reserve(request_bytes)?) };
        // Preserve the original CPU option capacities in the retained request and its lease.
        // Registered executors instead move resource Arcs into work and keep a small public copy.
        let (public_request, work_request) = if self.0.executor.is_some() {
            (ImageRequest { environment: None, textures: None, ..request.clone() }, Some(request))
        } else {
            (request, None)
        };
        let handle = JobHandle(Arc::new(Record {
            partial: partial_credits,
            presentation: None,
            textures,
            options,
            environment: environment_info,
            _request_reservation: request_reservation,
            id,
            scene_generation,
            request: public_request,
            resolved,
            cancel: CancelToken::default(),
            data: Mutex::new(Data {
                cpu_preview_retention_skips: 0,
                partial_preview: None,
                punctual_diagnostics: None,
                sampling: None,
                progress: Progress { phase: Phase::Queued, done: 0, total: 1 },
                error: None,
                frame: None,
                preview: None,
            }),
            #[cfg(not(target_arch = "wasm32"))]
            done: Condvar::new(),
        }));
        queue.push_back(Work {
            regrade: None,
            request: work_request,
            executor: self.0.executor.clone(),
            scene,
            handle: handle.clone(),
            reservation,
            environment,
        });
        drop(queue);
        #[cfg(not(target_arch = "wasm32"))]
        self.0.shared.wake.notify_one();
        Ok(handle)
    }

    /// Regrade the exact retained native master. No geometry, material, light or
    /// device work is submitted. The original frame and its lease remain shared.
    pub fn submit_regrade(&self, id: JobId, source: &JobHandle, presentation: super::presentation::Presentation) -> Result<JobHandle> {
        if id.session == 0 || id.serial == 0 || id.generation == 0 || id.session != source.id().session || id == source.id() {
            return Err(JobError::new(ErrorCode::WrongOwner, "regrade requires a new identity in the source document session"));
        }
        let frame = source.frame()?.ok_or_else(|| JobError::new(ErrorCode::NotCompleted, "regrade requires a completed frame"))?;
        let master = frame.master_source()?;
        let bytes = super::regrade::required_bytes(&master, presentation)?;
        let raw = master.linear_master_info().ok_or_else(|| JobError::new(ErrorCode::InvalidRequest, "raw master metadata is missing"))?;
        let origin = super::presentation::MasterPresentation {
            source_job: source.id(),
            master_job: master.info.job,
            master_sha256: raw.sha256.clone(),
            presentation,
            denoise_applied: false,
        };
        let metadata_bytes = (std::mem::size_of::<RetainedPresentation>() + origin.master_sha256.capacity() + 32) as u64;
        if bytes.checked_add(metadata_bytes).is_none_or(|n| n > self.job_limit_bytes()) {
            return Err(JobError::new(ErrorCode::LimitExceeded, "master presentation exceeds the installed host's resource budget"));
        }
        let limits = RenderLimits { max_scratch_bytes: self.job_limit_bytes(), ..super::GPU_PATH_LIMITS };
        let descriptor = ExecutorDescriptor {
            id: ExecutorId::new("linear-master").map_err(|e| JobError::new(ErrorCode::Internal, e.to_string()))?,
            availability: Availability::Available,
            profiles: vec![RenderProfile {
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                formats: vec![RenderFormat::Png],
                features: Vec::new(),
            }],
            limits,
            scratch: ScratchLayout { sample_scale: 1, sample_bytes_per_pixel: 32, output_bytes_per_pixel: 8, encoding_bytes_per_pixel: 8 },
        };
        let mut backend = crate::builtin_backend();
        backend.executors = vec![descriptor];
        let resolved = crate::resolve_request(
            &[backend],
            &crate::RenderRequest {
                backend: None,
                mode: RenderMode::Preview,
                operation: RenderOperation::Image,
                width: u64::from(frame.info.width),
                height: u64::from(frame.info.height),
                format: Some(RenderFormat::Png),
                features: Vec::new(),
                policy: Default::default(),
            },
        )
        .map_err(|e| JobError::new(ErrorCode::InvalidRequest, e.to_string()))?;
        let request = ImageRequest {
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            studio_weights: None,
            width: frame.info.width,
            height: frame.info.height,
            camera: source.0.request.camera,
            background: presentation.background,
            transparent_background: presentation.transparent_background,
            environment: None,
            textures: None,
            integrator: Integrator::Raster,
            raytrace: None,
        };
        let options = JobOptions {
            finishing: presentation.finishing,
            backdrop: presentation.backdrop,
            exposure_ev: presentation.exposure_ev,
            tone_map: presentation.tone_map,
            ..Default::default()
        };
        let mut queue = self.0.shared.queue.lock().map_err(|_| JobError::new(ErrorCode::HostUnavailable, "render queue lock poisoned"))?;
        if self.0.shared.stop.load(Ordering::Acquire) {
            return Err(JobError::new(ErrorCode::HostUnavailable, "image host has stopped"));
        }
        queue.retain(|work| !work.handle.0.cancel.is_cancelled());
        if queue.len() >= QUEUE_LIMIT {
            return Err(JobError::new(ErrorCode::QueueFull, "image queue is full"));
        }
        let reservation = self.0.shared.budget.reserve(bytes)?;
        let metadata = self.0.shared.budget.reserve(metadata_bytes)?;
        let origin = Arc::new(RetainedPresentation { info: origin, _reservation: metadata });
        let handle = JobHandle(Arc::new(Record {
            partial: None,
            presentation: Some(origin),
            textures: None,
            options,
            environment: None,
            _request_reservation: None,
            id,
            scene_generation: frame.info.scene_generation,
            request,
            resolved,
            cancel: CancelToken::default(),
            data: Mutex::new(Data {
                cpu_preview_retention_skips: 0,
                partial_preview: None,
                punctual_diagnostics: None,
                sampling: None,
                progress: Progress { phase: Phase::Queued, done: 0, total: 1 },
                error: None,
                frame: None,
                preview: None,
            }),
            #[cfg(not(target_arch = "wasm32"))]
            done: Condvar::new(),
        }));
        queue.push_back(Work {
            regrade: Some((master, presentation)),
            request: None,
            executor: None,
            environment: None,
            scene: ProductScene { meshes: Vec::new(), bounds: solidcraft_geom::Aabb3::EMPTY },
            handle: handle.clone(),
            reservation,
        });
        drop(queue);
        #[cfg(not(target_arch = "wasm32"))]
        self.0.shared.wake.notify_one();
        Ok(handle)
    }

    /// Cooperative callers pump a small quantum independently of polling a job's status.
    /// Native jobs run even when the UI is idle or a headless caller is not polling.
    pub fn pump(&self, units: usize) -> Result<bool> {
        if self.0.kind != HostKind::Cooperative {
            return Ok(false);
        }
        let mut running = self.0.running.lock().map_err(|_| JobError::new(ErrorCode::HostUnavailable, "cooperative host lock poisoned"))?;
        if running.is_none() {
            let work = self.0.shared.queue.lock().map_err(|_| JobError::new(ErrorCode::HostUnavailable, "render queue lock poisoned"))?.pop_front();
            if let Some(work) = work {
                *running = Running::begin(work);
            }
        }
        if let Some(work) = running.as_mut() {
            if let Ok(mut active) = self.0.shared.active.lock() {
                *active = Some(Arc::downgrade(&work.handle.0));
            }
            if !work.guarded_step(units) {
                *running = None;
                if let Ok(mut active) = self.0.shared.active.lock() {
                    *active = None;
                }
            }
        }
        Ok(running.is_some()
            || !self.0.shared.queue.lock().map_err(|_| JobError::new(ErrorCode::HostUnavailable, "render queue lock poisoned"))?.is_empty())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_worker(shared: Arc<Shared>) {
    loop {
        let mut queue = match shared.queue.lock() {
            Ok(q) => q,
            Err(_) => {
                shared.stop.store(true, Ordering::Release);
                return;
            }
        };
        while queue.is_empty() && !shared.stop.load(Ordering::Acquire) {
            queue = match shared.wake.wait(queue) {
                Ok(q) => q,
                Err(_) => {
                    shared.stop.store(true, Ordering::Release);
                    return;
                }
            };
        }
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        let work = queue.pop_front();
        drop(queue);
        let Some(mut running) = work.and_then(Running::begin) else { continue };
        if let Ok(mut active) = shared.active.lock() {
            *active = Some(Arc::downgrade(&running.handle.0));
        }
        loop {
            if shared.stop.load(Ordering::Acquire) {
                let _ = running.handle.cancel();
            }
            if !running.guarded_step(4096) {
                break;
            }
        }
        if let Ok(mut active) = shared.active.lock() {
            *active = None;
        }
    }
}
#[cfg(test)]
mod retained_request_tests {
    use super::*;
    #[test]
    fn retained_emitters_bill_vector_and_original_name_capacities_in_same_pool() {
        let mut request = request();
        let mut name = String::with_capacity(256);
        name.push_str("fixture");
        let mut emissions = Vec::with_capacity(32);
        emissions.push(super::super::SurfaceEmission {
            body_index: 0,
            body: name,
            color: [1.0; 3],
            strength: 1.0,
            sides: super::super::EmissionSides::Front,
        });
        let options = request.raytrace.as_mut().unwrap();
        options.emissions = Some(emissions);
        let expected = 8 * std::mem::size_of::<super::super::DirectionalLightOptions>() as u64
            + 32 * std::mem::size_of::<super::super::SurfaceEmission>() as u64
            + 256;
        assert_eq!(retained_request_bytes(&request).unwrap(), expected);
        let budget = Arc::new(Budget { used: AtomicU64::new(0), limit: MEMORY_LIMIT });
        let lease = budget.reserve(expected).unwrap();
        assert_eq!(budget.used.load(Ordering::Acquire), expected);
        assert!(admit_request_sum(super::super::image::MAX_JOB_BYTES - expected, expected).is_ok());
        assert!(admit_request_sum(super::super::image::MAX_JOB_BYTES - expected + 1, expected).is_err());
        drop(lease);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn emitter_request_lease_survives_real_completion_frame_eviction_and_last_handle() {
        let mut host = CpuHost::for_platform().unwrap();
        let mut request = request();
        let mut name = String::with_capacity(256);
        name.push_str("emitter");
        let mut values = Vec::with_capacity(32);
        values.push(super::super::SurfaceEmission {
            body_index: 0,
            body: name,
            color: [1.0; 3],
            strength: 1.0,
            sides: super::super::EmissionSides::Both,
        });
        request.raytrace.as_mut().unwrap().emissions = Some(values);
        let expected = retained_request_bytes(&request).unwrap();
        let mesh = solidcraft_geom::Mesh {
            positions: vec![solidcraft_geom::Vec3::ZERO, solidcraft_geom::Vec3::X, solidcraft_geom::Vec3::Y],
            normals: vec![solidcraft_geom::Vec3::Z; 3],
            triangles: vec![[0, 1, 2]],
            tri_face: vec![0],
            ..Default::default()
        };
        let bounds = mesh.bounds();
        let scene = ProductScene {
            meshes: vec![crate::ProductMesh {
                transmission: None,
                name: "emitter".into(),
                mesh: Arc::new(mesh),
                color: [255; 3],
                opacity: 1.0,
                surface: None,
                faces: Vec::new(),
            }],
            bounds,
        };
        let job = host.submit(JobId { session: 1695, serial: 1, generation: 1 }, 1, scene, request).unwrap();
        assert_eq!(job.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase, Phase::Completed);
        // Completion publication precedes the worker's last temporary handle release.
        // Join this test's worker before asserting exact lease ownership, as in the union test.
        host.0.shared.stop.store(true, Ordering::Release);
        host.0.shared.wake.notify_all();
        Arc::get_mut(&mut host.0).unwrap()._worker.take().unwrap().join().unwrap();
        let frame = job.frame().unwrap().unwrap();
        let held = job.clone();
        job.0.data.lock().unwrap().frame = None;
        drop(job);
        assert!(host.admitted_bytes() > expected);
        drop(frame);
        assert_eq!(host.admitted_bytes(), expected);
        drop(held);
        assert_eq!(host.admitted_bytes(), 0);
    }
    fn request() -> ImageRequest {
        let mut lights = Vec::with_capacity(8);
        lights.push(super::super::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 });
        ImageRequest {
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            studio_weights: None,
            environment: None,
            textures: None,
            integrator: Integrator::Raytrace,
            raytrace: Some(super::super::RaytraceOptions { lights: Some(lights), ..Default::default() }),
            camera: Default::default(),
            width: 8,
            height: 8,
            background: [0; 3],
            transparent_background: false,
        }
    }
    #[test]
    fn retained_lights_are_billed_by_original_capacity_and_sum_is_checked() {
        let request = request();
        let bytes = retained_request_bytes(&request).unwrap();
        assert_eq!(bytes, 8 * std::mem::size_of::<super::super::DirectionalLightOptions>() as u64);
        let cap = super::super::image::MAX_JOB_BYTES;
        assert!(admit_request_sum(cap - bytes, bytes).is_ok());
        assert_eq!(admit_request_sum(cap - bytes + 1, bytes).unwrap_err().code, ErrorCode::LimitExceeded);
        assert_eq!(admit_request_sum(u64::MAX, 1).unwrap_err().code, ErrorCode::LimitExceeded);
    }
    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn retained_lights_keep_the_same_pool_lease_after_png_eviction() {
        let host = CpuHost::for_platform().unwrap();
        let request = request();
        let expected = retained_request_bytes(&request).unwrap();
        let job = host
            .submit(
                JobId { session: 99, serial: 1, generation: 1 },
                1,
                ProductScene { meshes: Vec::new(), bounds: solidcraft_geom::Aabb3::EMPTY },
                request,
            )
            .unwrap();
        assert_eq!(job.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase, Phase::Completed);
        let frame = job.frame().unwrap().unwrap();
        let held_bytes = host.admitted_bytes();
        assert!(held_bytes > expected);
        job.0.data.lock().unwrap().frame = None;
        assert_eq!(host.admitted_bytes(), held_bytes);
        drop(frame);
        assert_eq!(host.admitted_bytes(), expected);
        let held = job.clone();
        drop(job);
        assert_eq!(host.admitted_bytes(), expected);
        drop(held);
    }
}

#[cfg(all(test, feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod environment_metadata_lease_tests {
    use super::*;
    #[test]
    fn captured_environment_metadata_survives_png_eviction_without_source_bytes() {
        use crate::lighting::{
            HdrSource,
            environment_input::{EnvironmentInfo, EnvironmentInput},
        };
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nPRIMARIES=0.64 0.33 0.30 0.60 0.15 0.06 0.3127 0.3290\n\n-Y 2 +X 4\n".to_vec();
        for _ in 0..8 {
            bytes.extend_from_slice(&[128, 64, 32, 129]);
        }
        let input = EnvironmentInput::new(HdrSource::new(bytes).unwrap(), 0.5, 2.0, 16).unwrap();
        let metadata = input.metadata();
        let expected = std::mem::size_of::<EnvironmentInfo>() as u64;
        let mut host = CpuHost::for_platform().unwrap();
        let budget = host.0.shared.budget.clone();
        let request = ImageRequest {
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            studio_weights: None,
            environment: None,
            textures: None,
            integrator: Integrator::Raytrace,
            raytrace: None,
            camera: Default::default(),
            width: 8,
            height: 8,
            background: [0; 3],
            transparent_background: false,
        };
        let job = host
            .submit_with_environment(
                JobId { session: 1693, serial: 7, generation: 1 },
                1,
                ProductScene { meshes: Vec::new(), bounds: solidcraft_geom::Aabb3::EMPTY },
                request,
                Some(input),
            )
            .unwrap();
        let status = job.wait_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase, Phase::Completed);
        assert!(matches!(status.environment, Some(JobEnvironmentInfo::Cpu(info)) if info.source_sha256 == metadata.source_sha256));
        host.join_worker_for_test().unwrap();
        let frame = job.frame().unwrap().unwrap();
        job.0.data.lock().unwrap().frame = None;
        assert_eq!(host.admitted_bytes(), frame._reservation.bytes + expected);
        drop(frame);
        assert_eq!(host.admitted_bytes(), expected);
        drop(host);
        drop(job);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
}

#[cfg(all(test, feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod surface_request_metadata_tests {
    use super::*;
    use solidcraft_geom::{Aabb3, Mesh};
    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn surface_request_lease_survives_completion_frame_eviction_and_held_handles() {
        use crate::{ProductMesh, surface_patterns::SurfaceEffect};
        use std::time::{Duration, Instant};
        fn settle(host: &CpuHost, expected: u64) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while host.admitted_bytes() != expected && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert_eq!(host.admitted_bytes(), expected);
        }
        let mut host = CpuHost::for_platform().unwrap();
        let mut meshes = Vec::new();
        let mut effects = Vec::with_capacity(32);
        for index in 0..32 {
            let mut name = String::with_capacity(256);
            name.push_str(&format!("{index:03}"));
            name.push_str(&"x".repeat(253));
            assert_eq!(name.capacity(), 256);
            meshes.push(ProductMesh {
                transmission: None,
                name: name.clone(),
                mesh: Arc::new(Mesh::default()),
                color: [100; 3],
                opacity: 1.0,
                surface: None,
                faces: Vec::new(),
            });
            effects.push(SurfaceEffect {
                body: name,
                frame: Default::default(),
                repeat_mm: [20.0; 3],
                roughness_amplitude: 0.0,
                bump_amplitude_mm: 0.0,
            });
        }
        let scene = ProductScene { meshes, bounds: Aabb3 { min: solidcraft_geom::Vec3::ZERO, max: solidcraft_geom::Vec3::ZERO } };
        let mut request = request();
        request.integrator = Integrator::Raytrace;
        request.raytrace = Some(super::super::RaytraceOptions { surface_effects: Some(effects), ..Default::default() });
        let metadata = retained_request_bytes(&request).unwrap();
        assert!(metadata >= 32 * 256);
        let mut oversized = request.clone();
        let mut overallocated = Vec::with_capacity(64);
        overallocated.extend(oversized.raytrace.as_mut().unwrap().surface_effects.take().unwrap());
        oversized.raytrace.as_mut().unwrap().surface_effects = Some(overallocated);
        assert!(host.submit(id(40, 1), 1, scene.clone(), oversized).is_err());
        assert_eq!(host.admitted_bytes(), 0);

        let job = host.submit(id(40, 2), 1, scene, request).unwrap();
        assert_eq!(job.wait_timeout(Duration::from_secs(10)).unwrap().progress.phase, Phase::Completed);
        // Wait for this host's last temporary worker handle before exact lease checks.
        host.0.shared.stop.store(true, Ordering::Release);
        host.0.shared.wake.notify_all();
        Arc::get_mut(&mut host.0).unwrap()._worker.take().unwrap().join().unwrap();
        let frame = job.frame().unwrap().unwrap();
        let png_bytes = frame._reservation.bytes;
        settle(&host, metadata + png_bytes);
        // Evict the record's frame under the same lock used by publication, while
        // an external frame and a cloned job handle retain their independent leases.
        job.0.data.lock().unwrap().frame = None;
        let held = job.clone();
        drop(job);
        assert_eq!(host.admitted_bytes(), metadata + png_bytes);
        drop(frame);
        settle(&host, metadata);
        assert_eq!(held.status().unwrap().request.raytrace.unwrap().surface_effects.unwrap().len(), 32);
        drop(held);
        settle(&host, 0);
    }

    fn request() -> ImageRequest {
        ImageRequest {
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            studio_weights: None,
            environment: None,
            textures: None,
            integrator: Default::default(),
            raytrace: None,
            camera: Default::default(),
            width: 16,
            height: 16,
            background: [40, 80, 120],
            transparent_background: false,
        }
    }

    fn id(owner: u64, serial: u64) -> JobId {
        JobId { session: owner, serial, generation: serial }
    }
}

/// Provisional source-film observation, separate from complete-pass previews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialPreviewProjection {
    TopLeftRepresentativeV1,
}
#[derive(Clone, Debug, Serialize)]
pub struct PartialPreviewInfo {
    pub frame: FrameInfo,
    pub sequence: u64,
    pub source_width: u32,
    pub source_height: u32,
    pub completed_passes: u32,
    pub updated_prefix: u64,
    pub projection: PartialPreviewProjection,
}
/// Fields drop in declaration order: payload allocations die before credit return.
pub struct RenderPartialPreview {
    pub info: PartialPreviewInfo,
    png: Vec<u8>,
    covered: Vec<u32>,
    _lease: ReturnReservation,
}
const _: [(); 1] = [(); (std::mem::size_of::<RenderPartialPreview>() + 2 * crate::MAX_RENDER_ID_BYTES + 256 <= 2048) as usize];
impl RenderPartialPreview {
    pub fn png(&self) -> &[u8] {
        &self.png
    }
    pub fn covered(&self) -> &[u32] {
        &self.covered
    }
}
struct PartialCreditState {
    report: Option<super::ProtectedPreviewReport>,
    free: Reservation,
    in_flight: bool,
    closed: bool,
}
struct PartialCredits {
    protected: Option<super::ProtectedPreviewPlan>,
    plan: super::PartialPreviewPlan,
    state: Mutex<PartialCreditState>,
    requested: AtomicBool,
    exhausted: AtomicBool,
    _metadata: Reservation,
}
const _: [(); 1] =
    [(); (std::mem::size_of::<PartialCredits>() + std::mem::size_of::<PartialRunning>() + std::mem::size_of::<TaskSlot<PartialRunning>>() + 128
        <= 1024) as usize];
impl PartialCredits {
    fn new(plan: super::PartialPreviewPlan, reservation: &mut Reservation) -> Result<Arc<Self>> {
        Self::new_with_protection(plan, None, reservation)
    }
    fn new_protected(plan: super::ProtectedPreviewPlan, reservation: &mut Reservation) -> Result<Arc<Self>> {
        plan.validate()?;
        Self::new_with_protection(plan.preview_plan(), Some(plan), reservation)
    }
    fn new_with_protection(
        plan: super::PartialPreviewPlan,
        protected: Option<super::ProtectedPreviewPlan>,
        reservation: &mut Reservation,
    ) -> Result<Arc<Self>> {
        plan.validate()?;
        let metadata = reservation.split(1024)?;
        let bytes = plan.slot_bytes()?.checked_mul(2).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial credit overflow"))?;
        let free = reservation.split(bytes)?;
        Ok(Arc::new(Self {
            plan,
            protected,
            state: Mutex::new(PartialCreditState { report: None, free, in_flight: false, closed: false }),
            requested: AtomicBool::new(false),
            exhausted: AtomicBool::new(false),
            _metadata: metadata,
        }))
    }
    fn request(&self) -> Result<bool> {
        let state = self.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "partial credit lock poisoned"))?;
        if state.closed || self.exhausted.load(Ordering::Acquire) {
            return Ok(false);
        }
        self.requested.store(true, Ordering::Release);
        Ok(true)
    }
    fn acquire(self: &Arc<Self>) -> Result<Option<ReturnReservation>> {
        let mut state = self.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "partial credit lock poisoned"))?;
        let bytes = self.plan.slot_bytes()?;
        if state.closed
            || state.in_flight
            || self.exhausted.load(Ordering::Acquire)
            || !self.requested.load(Ordering::Acquire)
            || state.free.bytes < bytes
        {
            return Ok(None);
        }
        let reservation = state.free.split(bytes)?;
        state.in_flight = true;
        self.requested.store(false, Ordering::Release);
        Ok(Some(ReturnReservation { reservation: Some(reservation), credits: self.clone(), in_flight: true }))
    }
    fn retained_slots_blocked(&self) -> Result<bool> {
        let state = self.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "partial retention lock poisoned"))?;
        Ok(!state.closed
            && !state.in_flight
            && !self.exhausted.load(Ordering::Acquire)
            && self.requested.load(Ordering::Acquire)
            && state.free.bytes < self.plan.slot_bytes()?)
    }
    fn exhaust(&self) {
        self.exhausted.store(true, Ordering::Release);
        self.requested.store(false, Ordering::Release);
    }
    fn close(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => error.into_inner(),
        };
        state.closed = true;
        state.in_flight = false;
        self.requested.store(false, Ordering::Release);
        // Atomic release only: no image destruction or user work under this lock.
        let _ = state.free.shrink(0);
    }
}
/// Unique lease; Arc payload clones never create another independent slot.
struct ReturnReservation {
    reservation: Option<Reservation>,
    credits: Arc<PartialCredits>,
    in_flight: bool,
}
impl ReturnReservation {
    fn published(&mut self) -> Result<()> {
        let mut state = self.credits.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "partial credit lock poisoned"))?;
        if state.closed || !state.in_flight || !self.in_flight {
            return Err(JobError::new(ErrorCode::Cancelled, "partial preview credit is no longer current"));
        }
        state.in_flight = false;
        self.in_flight = false;
        Ok(())
    }
}
impl Drop for ReturnReservation {
    fn drop(&mut self) {
        let Some(mut reservation) = self.reservation.take() else { return };
        let mut state = match self.credits.state.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.closed = true;
                let _ = state.free.shrink(0);
                state
            }
        };
        if self.in_flight {
            state.in_flight = false;
        }
        if !state.closed {
            let limit = self.credits.plan.slot_bytes().ok().and_then(|v| v.checked_mul(2));
            if state.free.bytes.checked_add(reservation.bytes).zip(limit).is_some_and(|(total, limit)| total <= limit) {
                if state.free.merge_from(&mut reservation).is_err() {
                    state.closed = true;
                    let _ = state.free.shrink(0);
                }
            } else {
                state.closed = true;
                let _ = state.free.shrink(0);
            }
        }
        drop(state);
        // A closed sink releases this uniquely held reservation globally.
    }
}
struct PartialPending {
    covered: Vec<u32>,
    completed_passes: u32,
    updated_prefix: u64,
}
// Error paths preserve the same payload-before-lease destruction order.
struct PartialPublication {
    png: Vec<u8>,
    pending: PartialPending,
    lease: ReturnReservation,
}
struct PartialRunning {
    last_pass_attempt: u32,
    pending: Option<PartialPending>,
    lease: Option<ReturnReservation>,
    credits: Arc<PartialCredits>,
    next_sequence: u64,
}
impl PartialRunning {
    fn new(credits: Arc<PartialCredits>) -> Self {
        Self { pending: None, lease: None, credits, next_sequence: 1, last_pass_attempt: 0 }
    }
}
impl JobHandle {
    /// Real producer usage, unavailable until a protected task was actually constructed.
    pub fn protected_preview_report(&self) -> Result<Option<super::ProtectedPreviewReport>> {
        self.0.partial.as_ref().map_or(Ok(None), |credits| {
            let state = credits.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "protected report lock poisoned"))?;
            Ok(state.report)
        })
    }
    /// Request presentation only; this is not a visual-authority certificate.
    pub fn request_partial_preview(&self) -> Result<bool> {
        let terminal = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render status lock poisoned"))?.progress.phase.terminal();
        if terminal {
            return Ok(false);
        }
        self.0.partial.as_ref().map_or(Ok(false), |credits| credits.request())
    }
    pub fn partial_preview_exhausted(&self) -> Result<bool> {
        Ok(self.0.partial.as_ref().is_some_and(|credits| credits.exhausted.load(Ordering::Acquire)))
    }
    pub fn partial_preview(&self) -> Result<Option<Arc<RenderPartialPreview>>> {
        let data = self.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render partial preview lock poisoned"))?;
        let preview = data.partial_preview.clone();
        if preview.as_ref().is_some_and(|p| p.info.frame.job != self.0.id || p.info.frame.scene_generation != self.0.scene_generation) {
            return Err(JobError::new(ErrorCode::WrongOwner, "partial preview identity does not match its job"));
        }
        Ok(preview)
    }
}
impl Running {
    #[cfg(feature = "raytrace-cpu")]
    fn capture_partial(&mut self, png: Vec<u8>, covered: Vec<u32>, completed_passes: u32, updated_prefix: u64) -> Result<()> {
        self.handle.0.cancel.check()?;
        let partial = self.partial.as_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "partial output without its policy"))?.get_mut()?;
        let plan = partial.credits.plan;
        let source_pixels = u64::from(plan.source_width()) * u64::from(plan.source_height());
        let target = self
            .handle
            .0
            .request
            .raytrace
            .as_ref()
            .and_then(|v| v.progressive)
            .ok_or_else(|| JobError::new(ErrorCode::Internal, "partial output without progressive intent"))?
            .samples_per_pixel;
        let covered_limit = plan.covered_bytes()?;
        if partial.lease.is_none()
            || partial.pending.is_some()
            || completed_passes >= target
            || updated_prefix >= source_pixels
            || (completed_passes == 0 && updated_prefix == 0)
            || covered.len() as u64 != plan.pixels()
            || (covered.capacity() as u64).checked_mul(4).is_none_or(|v| v > covered_limit)
            || png.capacity() as u64 > plan.encoded_limit()?
        {
            return Err(JobError::new(ErrorCode::LimitExceeded, "partial payload exceeds its admitted frontier or capacities"));
        }
        // This sole sidecar verification scan was included in the producer's precharge.
        for (index, count) in covered.iter().enumerate() {
            self.handle.0.cancel.check()?;
            let source = plan.source_index(index)? as u64;
            let samples = completed_passes
                .checked_add(u32::from(source < updated_prefix))
                .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial sample count overflow"))?;
            if *count > samples {
                return Err(JobError::new(ErrorCode::Internal, "partial coverage exceeds committed source samples"));
            }
        }
        partial.pending = Some(PartialPending { covered, completed_passes, updated_prefix });
        self.pending_png = Some(PendingPng::new(png));
        Ok(())
    }
    fn publish_partial(&mut self, png: Vec<u8>, digest: sha2::digest::Output<Sha256>) -> Result<bool> {
        self.handle.0.cancel.check()?;
        let partial = self.partial.as_mut().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing partial publication state"))?.get_mut()?;
        let pending = partial.pending.take().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing partial metadata"))?;
        let lease = partial.lease.take().ok_or_else(|| JobError::new(ErrorCode::Internal, "missing partial publication credit"))?;
        let mut publication = PartialPublication { png, pending, lease };
        let sequence = partial.next_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial preview sequence exhausted"))?;
        let plan = partial.credits.plan;
        let info = FrameInfo {
            job: self.handle.0.id,
            scene_generation: self.handle.0.scene_generation,
            backend: self.handle.0.resolved.backend.clone(),
            executor: self.handle.0.resolved.executor.clone(),
            width: plan.width(),
            height: plan.height(),
            encoding: ColourEncoding::Srgb,
            alpha: AlphaConvention::Straight,
            png_bytes: publication.png.len() as u64,
            png_sha256: format!("{:x}", digest),
        };
        let metadata = std::mem::size_of::<RenderPartialPreview>()
            .checked_add(64 + 2 * crate::MAX_RENDER_ID_BYTES)
            .and_then(|v| v.checked_add(info.png_sha256.capacity()));
        if metadata.is_none_or(|v| v > 2048) {
            return Err(JobError::new(ErrorCode::LimitExceeded, "partial payload metadata exceeds its slot"));
        }
        // No Data lock while touching the credit coordinator.
        publication.lease.published()?;
        let PartialPublication { png, pending, lease } = publication;
        let preview = Arc::new(RenderPartialPreview {
            info: PartialPreviewInfo {
                frame: info,
                sequence,
                source_width: plan.source_width(),
                source_height: plan.source_height(),
                completed_passes: pending.completed_passes,
                updated_prefix: pending.updated_prefix,
                projection: PartialPreviewProjection::TopLeftRepresentativeV1,
            },
            png,
            covered: pending.covered,
            _lease: lease,
        });
        let mut data = self.handle.0.data.lock().map_err(|_| JobError::new(ErrorCode::Internal, "render partial publication lock poisoned"))?;
        if self.handle.0.cancel.is_cancelled() || data.progress.phase.terminal() {
            drop(data);
            drop(preview);
            return Err(JobError::new(ErrorCode::Cancelled, "late partial preview refused"));
        }
        let released = data.partial_preview.replace(preview);
        partial.next_sequence = next;
        // A demand arriving during encoding remains pending; never clear it here.
        drop(data);
        drop(released);
        self.handle.0.notify();
        Ok(true)
    }
}

impl PartialCredits {
    fn policy(&self) -> CpuPreviewPolicy {
        self.protected.map_or(CpuPreviewPolicy::Shared(self.plan), CpuPreviewPolicy::Protected)
    }
    fn store_report(&self, report: Option<super::ProtectedPreviewReport>) -> Result<()> {
        let (plan, report) = match (self.protected, report) {
            (None, None) => return Ok(()),
            (Some(plan), Some(report)) => (plan, report),
            _ => return Err(JobError::new(ErrorCode::Internal, "producer work report differs from its admitted policy")),
        };
        let presentation = plan.presentation_limit()?;
        let aggregate = plan.aggregate_limit()?;
        let expected_v = plan.snapshot_work()?.checked_mul(u64::from(report.accepted_snapshots));
        let used = report.ordinary_used.checked_add(report.presentation_used);
        if report.policy_version != 1
            || report.ordinary_limit != plan.ordinary_limit()
            || report.presentation_limit != presentation
            || report.aggregate_limit != aggregate
            || report.quota_limit != plan.quota_limit()
            || report.ordinary_used > report.ordinary_limit
            || report.presentation_used > presentation
            || used.is_none_or(|v| v > aggregate)
            || expected_v != Some(report.presentation_used)
            || report.accepted_snapshots > report.next_milestone_index
            || report.next_milestone_index > report.quota_limit
            || report.quota_exhausted != (report.accepted_snapshots == report.quota_limit)
            || report.milestones_complete != (report.next_milestone_index == report.quota_limit)
        {
            return Err(JobError::new(ErrorCode::Internal, "protected work report violates its actual admission"));
        }
        let mut state = self.state.lock().map_err(|_| JobError::new(ErrorCode::Internal, "protected report lock poisoned"))?;
        if state.report.is_some_and(|previous| {
            previous.ordinary_used > report.ordinary_used
                || previous.presentation_used > report.presentation_used
                || previous.accepted_snapshots > report.accepted_snapshots
                || previous.next_milestone_index > report.next_milestone_index
        }) {
            drop(state);
            return Err(JobError::new(ErrorCode::Internal, "protected work report counters moved backwards"));
        }
        state.report = Some(report);
        if report.quota_exhausted {
            self.exhaust();
        }
        Ok(())
    }
}

impl WorkTask {
    #[cfg(feature = "raytrace-cpu")]
    fn raytrace_with_slot(
        task: crate::raytrace::image::ImageTask,
        report_sink: Option<&PartialCredits>,
        make_slot: impl FnOnce(crate::raytrace::image::ImageTask) -> Result<TaskSlot<crate::raytrace::image::ImageTask>>,
    ) -> Result<Self> {
        // ImageTask is already real, even if its host wrapper cannot be allocated.
        if let Some(credits) = report_sink {
            credits.store_report(task.protected_preview_report())?;
        }
        make_slot(task).map(Self::Raytrace)
    }
}
