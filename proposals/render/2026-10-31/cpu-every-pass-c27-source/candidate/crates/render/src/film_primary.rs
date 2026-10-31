//! Bounded camera film: linear radiance sums and primary coverage, never display RGB.
//! Exactly one scanline-ordered update per pixel and absolute sample ordinal makes
//! a complete pass a state invariant. Initialization and updates are cancellable
//! single-pixel operations. The owning image job supplies its existing work lease.

use super::{invalid, limit};
use crate::job::{CancelToken, CpuBudget, CpuProgressiveOptions, ErrorCode, JobError, Result};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Pixel {
    sum: [f64; 3],
    covered: u32,
}

/// Observed scanline frontier; metadata only, never a caller-supplied authority.
/// Every pixel before updated_prefix has one more sample than completed_passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PartialFrontier {
    pub completed_passes: u32,
    pub updated_prefix: usize,
    pub pixels: usize,
}

/// A partial view never invents a zero sample for a pixel not yet observed.
/// Sampled linear radiance includes zero contributions from primary misses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PartialPixel {
    Unknown,
    Sampled { linear_premultiplied: [f64; 3], covered: u32, count: u32 },
}

pub(crate) struct PrimaryFilm {
    pixels: Vec<Pixel>,
    count: usize,
    sample_limit: u32,
    samples: u32,
    next_pixel: usize,
}

impl PrimaryFilm {
    pub(crate) fn required_bytes(width: u32, height: u32, options: CpuProgressiveOptions, budget: CpuBudget) -> Result<u64> {
        options.validate_image_with_budget(width, height, budget)?;
        (u64::from(width) * u64::from(height))
            .checked_mul(std::mem::size_of::<Pixel>() as u64)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .ok_or_else(|| limit("primary film storage overflow"))
    }

    pub(crate) fn new(
        width: u32,
        height: u32,
        options: CpuProgressiveOptions,
        admitted_bytes: u64,
        cancel: &CancelToken,
        budget: CpuBudget,
    ) -> Result<Self> {
        cancel.check()?;
        let required = Self::required_bytes(width, height, options, budget)?;
        if admitted_bytes < required {
            return Err(limit("primary film is not covered by image admission"));
        }
        let count = usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| limit("primary film address space exceeded"))?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(count).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "could not reserve primary film"))?;
        if pixels.capacity() != count {
            return Err(limit("actual primary film capacity exceeds admission"));
        }
        cancel.check()?;
        Ok(Self { pixels, count, sample_limit: options.samples_per_pixel, samples: 0, next_pixel: 0 })
    }

    pub(crate) fn initialized(&self) -> usize {
        self.pixels.len()
    }

    pub(crate) fn ready(&self) -> bool {
        self.pixels.len() == self.count
    }

    pub(crate) fn initialize_one(&mut self, cancel: &CancelToken, charge: &mut impl FnMut() -> Result<()>) -> Result<()> {
        cancel.check()?;
        if self.ready() {
            return Err(invalid("primary film is already initialized"));
        }
        charge()?;
        cancel.check()?;
        self.pixels.push(Pixel::default());
        Ok(())
    }

    pub(crate) fn samples(&self) -> u32 {
        self.samples
    }

    /// Validate everything before the one bounded mutation. Failed/cancelled
    /// updates never consume a sample ordinal or partially alter a pixel.
    pub(crate) fn add(
        &mut self,
        index: usize,
        ordinal: u32,
        radiance: [f64; 3],
        covered: bool,
        cancel: &CancelToken,
        charge: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        cancel.check()?;
        if !self.ready() || index != self.next_pixel || ordinal != self.samples || ordinal >= self.sample_limit {
            return Err(invalid("primary film requires the next pixel and absolute sample ordinal"));
        }
        if radiance.into_iter().any(|v| !v.is_finite() || !(0.0..=1e12).contains(&v)) || (!covered && radiance != [0.0; 3]) {
            return Err(invalid("invalid primary linear radiance or coverage"));
        }
        let pixel = self.pixels.get(index).copied().ok_or_else(|| invalid("primary film pixel out of range"))?;
        let sum = std::array::from_fn(|i| pixel.sum[i] + radiance[i]);
        if sum.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > f64::from(self.sample_limit) * 1e12) {
            return Err(invalid("primary film radiance sum exceeds finite range"));
        }
        let hits = pixel.covered.checked_add(u32::from(covered)).filter(|n| *n <= ordinal + 1).ok_or_else(|| invalid("primary coverage overflow"))?;
        charge()?;
        cancel.check()?;
        *self.pixels.get_mut(index).ok_or_else(|| invalid("primary film pixel disappeared"))? = Pixel { sum, covered: hits };
        self.next_pixel += 1;
        if self.next_pixel == self.count {
            self.next_pixel = 0;
            self.samples += 1;
        }
        Ok(())
    }

    /// A display consumer can only read a fully completed, immutable pass. The
    /// returned radiance is premultiplied by coverage; background is kept separate.
    pub(crate) fn pixel(&self, index: usize) -> Result<([f64; 3], u32, u32)> {
        if !self.ready() || self.next_pixel != 0 || self.samples == 0 {
            return Err(invalid("primary film has no uniform complete pass"));
        }
        let pixel = self.pixels.get(index).ok_or_else(|| invalid("primary film resolve index out of range"))?;
        Ok((pixel.sum.map(|sum| sum / f64::from(self.samples)), pixel.covered, self.samples))
    }
    /// Inspect only the real committed state. This value does not freeze the film
    /// or authorize a snapshot. A caller resolving several pixels must keep the
    /// film immutably borrowed at one completed-pixel frontier, and must charge
    /// reads/copies and check cancellation against its existing job budget.
    pub(crate) fn partial_frontier(&self) -> Result<PartialFrontier> {
        if !self.ready()
            || self.next_pixel >= self.count
            || self.samples > self.sample_limit
            || (self.samples == self.sample_limit && self.next_pixel != 0)
        {
            return Err(invalid("primary film has no initialized partial frontier"));
        }
        Ok(PartialFrontier { completed_passes: self.samples, updated_prefix: self.next_pixel, pixels: self.count })
    }

    /// Bounded allocation-free read, separate from the strict complete-pass pixel().
    /// Counts come from this film, never from a supplied frontier or sample count.
    /// Unknown means count0/covered0; a sampled miss has count>0 and covered0.
    /// No exposure, tone mapping, background, work lease or publication occurs here.
    /// The caller owes cancellation checks and work charges for each read/copy;
    /// this accessor creates no independent budget and never advances an ordinal.
    pub(crate) fn partial_pixel(&self, index: usize) -> Result<PartialPixel> {
        let frontier = self.partial_frontier()?;
        let pixel = self.pixels.get(index).ok_or_else(|| invalid("primary film partial index out of range"))?;
        let count = frontier
            .completed_passes
            .checked_add(u32::from(index < frontier.updated_prefix))
            .filter(|count| *count <= self.sample_limit)
            .ok_or_else(|| limit("primary partial sample count exceeded"))?;
        if pixel.covered > count {
            return Err(invalid("primary partial coverage exceeds its committed count"));
        }
        if count == 0 {
            if pixel.covered != 0 || pixel.sum != [0.0; 3] {
                return Err(invalid("unobserved primary pixel has accumulated data"));
            }
            return Ok(PartialPixel::Unknown);
        }
        Ok(PartialPixel::Sampled { linear_premultiplied: pixel.sum.map(|sum| sum / f64::from(count)), covered: pixel.covered, count })
    }
}
