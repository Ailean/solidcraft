//! Deterministic counter sampling and owned camera-film continuation.
//! No target sample count, work quantum or preview cadence enters the sampler.

use crate::{film::primary::PrimaryFilm, job::CpuProgressiveOptions};

pub(super) struct Progressive {
    pub film: PrimaryFilm,
    pub options: CpuProgressiveOptions,
    pub resolving: bool,
    pub resolve_pixel: usize,
    pub partial_plan: Option<crate::job::PartialPreviewPlan>,
    pub partial: Option<PartialResolve>,
    pub last_partial: Option<crate::film::primary::PartialFrontier>,
}

pub(super) struct PartialResolve {
    pub plan: crate::job::PartialPreviewPlan,
    pub frontier: crate::film::primary::PartialFrontier,
    pub resume_pixel: usize,
    pub next: usize,
    pub covered: Vec<u32>,
}

/// Read-only O(1) test for new represented data; no scan over the source film.
/// A completed global pass is meaningful even if its last source pixels are
/// not representatives: the receiver's global Unknown authority changes then.
pub(super) fn represented_changed(
    plan: crate::job::PartialPreviewPlan,
    before: Option<crate::film::primary::PartialFrontier>,
    now: crate::film::primary::PartialFrontier,
) -> crate::job::Result<bool> {
    use crate::job::{ErrorCode, JobError};
    plan.validate()?;
    let source_pixels = u64::from(plan.source_width()) * u64::from(plan.source_height());
    if u64::try_from(now.pixels).ok() != Some(source_pixels) || now.updated_prefix >= now.pixels {
        return Err(JobError::new(ErrorCode::InvalidScene, "partial film frontier differs from its source plan"));
    }
    let Some(before) = before else { return Ok(now.completed_passes != 0 || now.updated_prefix != 0) };
    if before.pixels != now.pixels
        || before.completed_passes > now.completed_passes
        || (before.completed_passes == now.completed_passes && before.updated_prefix > now.updated_prefix)
    {
        return Err(JobError::new(ErrorCode::InvalidScene, "partial film frontier went backwards"));
    }
    if before.completed_passes != now.completed_passes {
        return Ok(true);
    }
    if before.updated_prefix == now.updated_prefix {
        return Ok(false);
    }
    let w = u64::from(plan.source_width());
    let h = u64::from(plan.source_height());
    let pw = u64::from(plan.width());
    let ph = u64::from(plan.height());
    let lower = u64::try_from(before.updated_prefix).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "partial source index overflow"))?;
    let x = lower % w;
    let y = lower / w;
    let mut v = y.checked_mul(ph).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial row mapping overflow"))?.div_ceil(h);
    if v >= ph {
        return Ok(false);
    }
    let mut u = if v * h / ph == y {
        x.checked_mul(pw).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial column mapping overflow"))?.div_ceil(w)
    } else {
        0
    };
    if u >= pw {
        v += 1;
        u = 0;
    }
    if v >= ph {
        return Ok(false);
    }
    let display =
        v.checked_mul(pw).and_then(|n| n.checked_add(u)).ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "partial display index overflow"))?;
    let display = usize::try_from(display).map_err(|_| JobError::new(ErrorCode::LimitExceeded, "partial display address overflow"))?;
    Ok(plan.source_index(display)? < now.updated_prefix)
}

/// Integer-only mixing before conversion to exactly representable 53-bit unit
/// variates. Domains separate camera, BSDF and environment choices at each bounce.
pub(super) fn variates(seed: u32, pixel: u64, ordinal: u32, domain: u64) -> [f64; 2] {
    fn mix(mut value: u64) -> u64 {
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
    let key = mix(u64::from(seed) ^ 0x4350_5546_494c_4d00)
        ^ mix(pixel.wrapping_add(0x9e37_79b9_7f4a_7c15))
        ^ mix(u64::from(ordinal).wrapping_add(0xd1b5_4a32_d192_ed03))
        ^ mix(domain.wrapping_add(0x94d0_49bb_1331_11eb));
    [0_u64, 1].map(|axis| (mix(key ^ mix(axis.wrapping_add(0x632b_e59b_d9b4_e019))) >> 11) as f64 / 9_007_199_254_740_992.0)
}
