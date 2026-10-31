//! Optional original CPU visibility core and opaque direct-image integration.
//!
//! Original Rust implementation from elementary slab, barycentric and binary-tree
//! mathematics. Geometry is already world-space, in millimetres. No B-rep kernel
//! or third-party ray-tracer implementation is used. See `docs/render-raytrace-core.md`.

use crate::job::CancelToken;
use solidcraft_geom::Vec3;

mod anisotropic_faces;
pub mod area;
mod bvh;
pub mod dielectric;
pub mod emission;
pub mod image;
pub mod lighting;
pub mod prepared;
mod progressive;
pub mod punctual;
pub mod reflection;
pub use bvh::{BuildLimits, BuildProgress, Bvh, BvhBuilder, MAX_DEPTH, MAX_MEMORY_BYTES, MAX_TRIANGLES};

/// Finite coordinate admission, not a CAD accuracy guarantee.
pub const MAX_COORDINATE_MM: f64 = 1e12;
/// Maximum finite ray interval endpoint in millimetres.
pub const MAX_DISTANCE_MM: f64 = 4e12;
/// Maximum work charged to one explicitly owned budget.
pub const MAX_WORK: u64 = 1_000_000_000;
/// Maximum incremental builder operations in one call.
pub const MAX_BUILD_STEP: usize = 4096;

/// Structured failures; exhausting work never returns a truncated hit result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    InvalidInput(&'static str),
    MemoryLimit,
    AllocationFailed,
    WorkLimit,
    Cancelled,
    BuildIncomplete,
    Internal(&'static str),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(f, "invalid ray tracing input: {reason}"),
            Self::MemoryLimit => f.write_str("ray tracing memory admission exceeded"),
            Self::AllocationFailed => f.write_str("ray tracing allocation failed"),
            Self::WorkLimit => f.write_str("ray tracing work budget exhausted"),
            Self::Cancelled => f.write_str("ray tracing cancelled"),
            Self::BuildIncomplete => f.write_str("ray tracing BVH build is incomplete"),
            Self::Internal(reason) => write!(f, "invalid ray tracing internal state: {reason}"),
        }
    }
}
impl std::error::Error for TraceError {}
pub type Result<T> = std::result::Result<T, TraceError>;

fn check_cancel(cancel: &CancelToken) -> Result<()> {
    if cancel.is_cancelled() { Err(TraceError::Cancelled) } else { Ok(()) }
}

/// Caller-owned cumulative work; can span many rays in one image job.
#[derive(Debug)]
pub struct WorkBudget {
    limit: u64,
    used: u64,
    protected: Option<ProtectedWork>,
}

/// One owned ledger, not another spendable budget. Keeping the policy and actual
/// counters here preserves the final report after the image task drops its film.
#[derive(Debug)]
struct ProtectedWork {
    plan: crate::job::ProtectedPreviewPlan,
    ordinary_used: u64,
    presentation_used: u64,
    presentation_limit: u64,
    aggregate_limit: u64,
    accepted_snapshots: u32,
    next_milestone_index: u32,
}

impl WorkBudget {
    pub fn new(limit: u64) -> Result<Self> {
        if limit == 0 || limit > crate::job::MAX_CPU_WORK {
            return Err(TraceError::InvalidInput("work limit outside 1..=MAX_WORK"));
        }
        Ok(Self { limit, used: 0, protected: None })
    }

    pub(crate) fn new_protected(plan: crate::job::ProtectedPreviewPlan) -> Result<Self> {
        plan.validate().map_err(|_| TraceError::InvalidInput("invalid protected feedback plan"))?;
        let presentation_limit = plan.presentation_limit().map_err(|_| TraceError::WorkLimit)?;
        let aggregate_limit = plan.aggregate_limit().map_err(|_| TraceError::WorkLimit)?;
        if aggregate_limit > crate::job::MAX_PROTECTED_CPU_WORK {
            return Err(TraceError::WorkLimit);
        }
        Ok(Self {
            limit: plan.ordinary_limit(),
            used: 0,
            protected: Some(ProtectedWork {
                plan,
                ordinary_used: 0,
                presentation_used: 0,
                presentation_limit,
                aggregate_limit,
                accepted_snapshots: 0,
                next_milestone_index: 0,
            }),
        })
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    /// Legacy meaning is unchanged. Protected ordinary callers retain B-O;
    /// presentation and aggregate capacity have separate, explicit getters.
    pub fn remaining(&self) -> u64 {
        self.ordinary_remaining()
    }

    pub(crate) fn ordinary_remaining(&self) -> u64 {
        self.limit - self.protected.as_ref().map_or(self.used, |state| state.ordinary_used)
    }

    pub(crate) fn presentation_remaining(&self) -> u64 {
        self.protected.as_ref().map_or(0, |state| state.presentation_limit - state.presentation_used)
    }

    pub(crate) fn aggregate_remaining(&self) -> u64 {
        self.protected.as_ref().map_or(self.limit, |state| state.aggregate_limit) - self.used
    }

    pub(crate) fn protected_report(&self) -> Option<crate::job::ProtectedPreviewReport> {
        let state = self.protected.as_ref()?;
        let quota_limit = state.plan.quota_limit();
        Some(crate::job::ProtectedPreviewReport {
            policy_version: 1,
            ordinary_limit: self.limit,
            presentation_limit: state.presentation_limit,
            aggregate_limit: state.aggregate_limit,
            ordinary_used: state.ordinary_used,
            presentation_used: state.presentation_used,
            accepted_snapshots: state.accepted_snapshots,
            quota_limit,
            next_milestone_index: state.next_milestone_index,
            quota_exhausted: state.accepted_snapshots == quota_limit,
            milestones_complete: state.next_milestone_index == quota_limit,
        })
    }

    /// Read-only eligibility at a real committed camera frontier. Source progress
    /// is not a work credit: it selects a milestone, never reserves ordinary work.
    fn protected_ready(&self, committed: u64) -> Result<bool> {
        let state = self.protected.as_ref().ok_or(TraceError::InvalidInput("protected feedback policy missing"))?;
        if committed >= state.plan.total_commits()
            || state.accepted_snapshots >= state.plan.quota_limit()
            || state.next_milestone_index >= state.plan.quota_limit()
        {
            return Ok(false);
        }
        let milestone = state.plan.milestone(state.next_milestone_index).map_err(|_| TraceError::Internal("invalid protected milestone"))?;
        let cost = state.plan.snapshot_work().map_err(|_| TraceError::WorkLimit)?;
        Ok(committed >= milestone && self.presentation_remaining() >= cost && self.aggregate_remaining() >= cost)
    }

    /// Debit ONE W and coalesce all reached milestones in the same transaction.
    /// A wait/refusal changes no counters. Cancellation after a debit is not a refund.
    fn charge_presentation(&mut self, committed: u64, cancel: &CancelToken) -> Result<bool> {
        check_cancel(cancel)?;
        if !self.protected_ready(committed)? {
            return Ok(false);
        }
        let state = self.protected.as_ref().ok_or(TraceError::InvalidInput("protected feedback policy missing"))?;
        let cost = state.plan.snapshot_work().map_err(|_| TraceError::WorkLimit)?;
        let presentation_used = state.presentation_used.checked_add(cost).filter(|v| *v <= state.presentation_limit).ok_or(TraceError::WorkLimit)?;
        let used = self.used.checked_add(cost).filter(|v| *v <= state.aggregate_limit).ok_or(TraceError::WorkLimit)?;
        let accepted = state.accepted_snapshots.checked_add(1).filter(|n| *n <= state.plan.quota_limit()).ok_or(TraceError::WorkLimit)?;
        let next = state.plan.reached_milestones(committed).map_err(|_| TraceError::Internal("invalid protected reached frontier"))?;
        check_cancel(cancel)?;
        let state = self.protected.as_mut().ok_or(TraceError::Internal("protected ledger disappeared"))?;
        state.presentation_used = presentation_used;
        state.accepted_snapshots = accepted;
        state.next_milestone_index = next;
        self.used = used;
        Ok(true)
    }

    fn tick(&mut self, cancel: &CancelToken) -> Result<()> {
        self.charge(1, cancel)
    }

    fn charge(&mut self, units: u64, cancel: &CancelToken) -> Result<()> {
        check_cancel(cancel)?;
        if let Some(state) = &mut self.protected {
            let ordinary = state.ordinary_used.checked_add(units).filter(|v| *v <= self.limit).ok_or(TraceError::WorkLimit)?;
            let used = self.used.checked_add(units).filter(|v| *v <= state.aggregate_limit).ok_or(TraceError::WorkLimit)?;
            state.ordinary_used = ordinary;
            self.used = used;
        } else {
            self.used = self.used.checked_add(units).filter(|used| *used <= self.limit).ok_or(TraceError::WorkLimit)?;
        }
        Ok(())
    }

    /// Private test boundary only: the ordinary allowance is fixed before any
    /// real task work, preserving Q and the same O/V ledger. No production caller.
    #[cfg(test)]
    fn new_protected_for_test(plan: crate::job::ProtectedPreviewPlan, ordinary: u64) -> Result<Self> {
        let mut work = Self::new_protected(plan)?;
        if ordinary == 0 || ordinary > work.limit {
            return Err(TraceError::InvalidInput("test ordinary limit outside profile"));
        }
        let state = work.protected.as_mut().ok_or(TraceError::Internal("test protected policy missing"))?;
        state.aggregate_limit =
            ordinary.checked_add(state.presentation_limit).filter(|v| *v <= crate::job::MAX_PROTECTED_CPU_WORK).ok_or(TraceError::WorkLimit)?;
        work.limit = ordinary;
        Ok(work)
    }
}

fn bounded(point: Vec3) -> bool {
    point.is_finite() && [point.x, point.y, point.z].into_iter().all(|v| v.abs() <= MAX_COORDINATE_MM)
}

fn unit(vector: Vec3) -> Option<Vec3> {
    let scale = vector.x.abs().max(vector.y.abs()).max(vector.z.abs());
    if !vector.is_finite() || scale == 0.0 {
        return None;
    }
    let scaled = vector / scale;
    Some(scaled / scaled.len2().sqrt())
}

fn component(vector: Vec3, axis: usize) -> f64 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

/// Admitted ray with a unit direction and closed finite distance interval.
/// No implicit origin offset, back-face culling or fixed world-space epsilon.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    origin: Vec3,
    direction: Vec3,
    min: f64,
    max: f64,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3, min: f64, max: f64) -> Result<Self> {
        if !bounded(origin) || !min.is_finite() || !max.is_finite() || min < 0.0 || min > max || max > MAX_DISTANCE_MM {
            return Err(TraceError::InvalidInput("ray origin or distance interval"));
        }
        let direction = unit(direction).ok_or(TraceError::InvalidInput("finite nonzero ray direction required"))?;
        Ok(Self { origin, direction, min, max })
    }
    pub fn origin(self) -> Vec3 {
        self.origin
    }
    pub fn direction(self) -> Vec3 {
        self.direction
    }
    pub fn interval(self) -> [f64; 2] {
        [self.min, self.max]
    }
    pub fn at(self, distance: f64) -> Result<Vec3> {
        if !distance.is_finite() || distance < self.min || distance > self.max {
            return Err(TraceError::InvalidInput("distance outside admitted ray interval"));
        }
        Ok(self.origin + self.direction * distance)
    }
}

/// Conservative world-space box; empty boxes exist only inside the builder.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    pub fn new(min: Vec3, max: Vec3) -> Result<Self> {
        if !bounded(min) || !bounded(max) || min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(TraceError::InvalidInput("finite ordered bounding box required"));
        }
        Ok(Self { min, max })
    }
    pub fn min(self) -> Vec3 {
        self.min
    }
    pub fn max(self) -> Vec3 {
        self.max
    }
    fn empty() -> Self {
        Self { min: Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY), max: Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }
    fn include(self, other: Self) -> Self {
        Self { min: self.min.min(other.min), max: self.max.max(other.max) }
    }
    fn point(point: Vec3) -> Self {
        Self { min: point, max: point }
    }

    /// Closed slab overlap. Parallel (including signed-zero) components never divide.
    pub fn intersect(self, ray: &Ray) -> Option<[f64; 2]> {
        self.clip(ray, ray.max)
    }

    fn clip(self, ray: &Ray, maximum: f64) -> Option<[f64; 2]> {
        let (mut near, mut far) = (ray.min, ray.max.min(maximum));
        for axis in 0..3 {
            let (lo, hi, origin, direction) =
                (component(self.min, axis), component(self.max, axis), component(ray.origin, axis), component(ray.direction, axis));
            if direction == 0.0 {
                if origin < lo || origin > hi {
                    return None;
                }
            } else {
                // Division may produce infinity for an almost-parallel ray; the
                // finite caller interval clips it. No reciprocal times zero.
                let a = (lo - origin) / direction;
                let b = (hi - origin) / direction;
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some([near, far])
    }
}

/// Validated triangle. Coincident/collinear vertices are retained but never hit.
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    vertices: [Vec3; 3],
    normal: Option<Vec3>,
}

impl Triangle {
    pub fn new(vertices: [Vec3; 3]) -> Result<Self> {
        if !vertices.into_iter().all(bounded) {
            return Err(TraceError::InvalidInput("triangle vertex coordinates"));
        }
        let [a, b, c] = vertices;
        let ab = b - a;
        let ac = c - a;
        let scale = [ab.x, ab.y, ab.z, ac.x, ac.y, ac.z].into_iter().map(f64::abs).fold(0.0, f64::max);
        let normal = if scale == 0.0 { None } else { unit((ab / scale).cross(ac / scale)) };
        Ok(Self { vertices, normal })
    }
    pub fn vertices(self) -> [Vec3; 3] {
        self.vertices
    }
    pub fn geometric_normal(self) -> Option<Vec3> {
        self.normal
    }
    pub fn bounds(self) -> Bounds {
        let [a, b, c] = self.vertices;
        let min = a.min(b).min(c);
        let max = a.max(b).max(c);
        // One representable outward step, not a fixed millimetre epsilon.
        Bounds {
            min: Vec3::new(min.x.next_down(), min.y.next_down(), min.z.next_down()),
            max: Vec3::new(max.x.next_up(), max.y.next_up(), max.z.next_up()),
        }
    }
    fn centroid(self) -> Vec3 {
        let [a, b, c] = self.vertices;
        a / 3.0 + b / 3.0 + c / 3.0
    }

    /// Two-sided closed-edge barycentric intersection by dominant-direction shear.
    /// At equal distance the BVH's input ordinal resolves the tie. Zero projected
    /// area is a miss, including geometry collapsed by f64 arithmetic.
    pub fn intersect(self, ray: &Ray) -> Option<TriangleHit> {
        let normal = self.normal?;
        let d = ray.direction;
        let kz = if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() {
            0
        } else if d.y.abs() >= d.z.abs() {
            1
        } else {
            2
        };
        let kx = (kz + 1) % 3;
        let ky = (kx + 1) % 3;
        let dominant = component(d, kz);
        let sx = component(d, kx) / dominant;
        let sy = component(d, ky) / dominant;
        let projected = self.vertices.map(|v| {
            let delta = v - ray.origin;
            let z = component(delta, kz);
            [component(delta, kx) - sx * z, component(delta, ky) - sy * z, z / dominant]
        });
        let [[ax, ay, az], [bx, by, bz], [cx, cy, cz]] = projected;
        let weights = [bx * cy - by * cx, cx * ay - cy * ax, ax * by - ay * bx];
        let [u, v, w] = weights;
        if (u < 0.0 || v < 0.0 || w < 0.0) && (u > 0.0 || v > 0.0 || w > 0.0) {
            return None;
        }
        let determinant = u + v + w;
        if determinant == 0.0 {
            return None;
        }
        let barycentric = weights.map(|value| value / determinant);
        let [u, v, w] = barycentric;
        // Divide before multiplying by depth: very small triangle areas need not
        // underflow their otherwise well-represented hit distance.
        // An exact coordinate plane has a direct depth equation. Summing
        // depths along another dominant ray axis can cancel large tangential
        // terms into a false positive. No interval or edge policy changes.
        let axis_plane = (0..3).find_map(|axis| {
            let [a, b, c] = self.vertices.map(|point| component(point, axis));
            (a == b && b == c).then_some((axis, a))
        });
        let distance = if let Some((axis, coordinate)) = axis_plane {
            (coordinate - component(ray.origin, axis)) / component(d, axis)
        } else {
            u * az + v * bz + w * cz
        };
        if !distance.is_finite() || distance < ray.min || distance > ray.max {
            return None;
        }
        Some(TriangleHit { distance, barycentric, geometric_normal: normal, front_face: d.dot(normal) < 0.0 })
    }
}

/// Geometric hit, without material/shading-normal or persistent topology identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleHit {
    pub distance: f64,
    pub barycentric: [f64; 3],
    pub geometric_normal: Vec3,
    pub front_face: bool,
}

/// Captured input ordinal, not a CAD face identifier. RT2 owns the appearance mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub primitive_index: usize,
    pub geometry: TriangleHit,
}

#[cfg(test)]
mod planar_shadow_tests;
#[cfg(test)]
mod tests;
