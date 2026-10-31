//! Nondestructive display intent shared by transport output and retained-master
//! regrading. Raw scene-linear samples are immutable; this is not denoising.
use super::finishing::{Backdrop, Finishing};
use super::{CancelToken, ErrorCode, JobError, JobId, Result, ToneMap};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Presentation {
    pub exposure_ev: f64,
    pub tone_map: ToneMap,
    pub finishing: Finishing,
    pub backdrop: Option<Backdrop>,
    pub background: [u8; 3],
    pub transparent_background: bool,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            tone_map: ToneMap::default(),
            finishing: Finishing::default(),
            backdrop: None,
            background: [240, 243, 248],
            transparent_background: false,
        }
    }
}
impl Presentation {
    pub fn validate(self) -> Result<()> {
        self.finishing.validate()?;
        if let Some(backdrop) = self.backdrop {
            backdrop.validate()?;
        }
        if !self.exposure_ev.is_finite() || !(-32.0..=32.0).contains(&self.exposure_ev) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "presentation exposure must be -32..32 EV"));
        }
        if self.transparent_background
            && (self.backdrop.is_some() || self.finishing.bloom.is_some_and(|b| b.active()) || self.finishing.photographic.active())
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "choose opaque presentation for bloom or a camera backdrop"));
        }
        Ok(())
    }
    pub fn display(self, width: u32, height: u32) -> Result<Display> {
        self.validate()?;
        super::linear_exr::encoded_limit(width, height)?;
        self.display_with_limits(width, height, super::GPU_PATH_LIMITS)
    }
    pub(crate) fn display_with_limits(self, width: u32, height: u32, limits: crate::RenderLimits) -> Result<Display> {
        self.validate()?;
        super::linear_exr::output_pixels_with_limits(width, height, limits)?;
        Ok(Display {
            width,
            height,
            exposure: 2.0f64.powf(self.exposure_ev),
            tone_map: self.tone_map,
            finishing: self.finishing,
            backdrop: self.backdrop,
            background: self.background.map(crate::surface::srgb8_to_linear),
            transparent: self.transparent_background,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MasterPresentation {
    pub source_job: JobId,
    pub master_job: JobId,
    pub master_sha256: String,
    pub presentation: Presentation,
    pub denoise_applied: bool,
}

#[derive(Clone, Copy)]
pub struct Display {
    pub finishing: super::finishing::Finishing,
    pub backdrop: Option<super::finishing::Backdrop>,
    pub width: u32,
    pub height: u32,
    pub exposure: f64,
    pub tone_map: ToneMap,
    pub background: [f64; 3],
    pub transparent: bool,
}
impl Display {
    pub fn encode(self, input: &[[f32; 4]], output: &mut [u8], cancel: &CancelToken) -> Result<()> {
        self.encode_at(input, output, cancel, 0)
    }
    pub fn encode_at(self, input: &[[f32; 4]], output: &mut [u8], cancel: &CancelToken, offset: usize) -> Result<()> {
        self.encode_bloom_at(input, output, cancel, offset, None)
    }
    pub fn encode_bloom_at(
        self,
        input: &[[f32; 4]],
        output: &mut [u8],
        cancel: &CancelToken,
        offset: usize,
        bloom: Option<&[[f32; 3]]>,
    ) -> Result<()> {
        if bloom.is_some_and(|pixels| pixels.len() != input.len()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "bloom/display length mismatch"));
        }
        super::linear_exr::encoded_limit(self.width, self.height)?;
        self.encode_bloom_at_with_limits(input, output, cancel, offset, bloom, super::GPU_PATH_LIMITS)
    }
    pub(crate) fn encode_bloom_at_with_limits(
        self,
        input: &[[f32; 4]],
        output: &mut [u8],
        cancel: &CancelToken,
        offset: usize,
        bloom: Option<&[[f32; 3]]>,
        limits: crate::RenderLimits,
    ) -> Result<()> {
        if bloom.is_some_and(|pixels| pixels.len() != input.len()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "bloom/display length mismatch"));
        }
        let count = super::linear_exr::output_pixels_with_limits(self.width, self.height, limits)?;
        super::linear_exr::checked_output_bytes(count, 16, 0, limits)?;
        super::linear_exr::checked_output_bytes(count, 4, 0, limits)?;
        if bloom.is_some() {
            super::linear_exr::checked_output_bytes(count, 12, 0, limits)?;
        }
        self.finishing.validate()?;
        if let Some(backdrop) = self.backdrop {
            backdrop.validate()?;
        }
        let count = self.width as usize * self.height as usize;
        if input.len().checked_mul(4) != Some(output.len())
            || offset.checked_add(input.len()).is_none_or(|n| n > count)
            || !self.exposure.is_finite()
            || !(2.0_f64.powi(-32)..=2.0_f64.powi(32)).contains(&self.exposure)
            || self.background.iter().any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
            || bloom.is_some_and(|g| g.len() != input.len())
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid presentation dimensions, exposure or storage"));
        }
        if self.transparent && (self.backdrop.is_some() || self.finishing.bloom.is_some_and(|b| b.active()) || self.finishing.photographic.active()) {
            return Err(JobError::new(ErrorCode::InvalidRequest, "bloom and camera backdrops require opaque presentation"));
        }
        for (index, (pixel, rgba)) in input.iter().zip(output.as_chunks_mut::<4>().0).enumerate() {
            if index % 4096 == 0 {
                cancel.check()?;
            }
            let alpha = f64::from(pixel[3]).clamp(0.0, 1.0);
            let absolute = offset + index;
            let position = [
                ((absolute % self.width as usize) as f64 + 0.5) / f64::from(self.width),
                ((absolute / self.width as usize) as f64 + 0.5) / f64::from(self.height),
            ];
            let background = self.backdrop.map_or(self.background, |backdrop| backdrop.color(position));
            let mapped = std::array::from_fn(|channel| {
                let c = if alpha > 0. { f64::from(pixel[channel]) * self.exposure / alpha * self.finishing.rgb_gain[channel] } else { 0. };
                self.tone_map.map(c)
            });
            let mapped = self.finishing.display(mapped, position);
            for (channel, ((c, b), out)) in mapped.into_iter().zip(background).zip(rgba.iter_mut()).enumerate() {
                let mut value = if self.transparent { c } else { c * alpha + b * (1. - alpha) };
                if let Some(glow) = bloom.and_then(|pixels| pixels.get(index)) {
                    let strength = self.finishing.bloom.map_or(0.0, |b| b.strength);
                    value += strength * self.tone_map.map(f64::from(glow[channel]) * self.exposure * self.finishing.rgb_gain[channel]);
                }
                *out = solidcraft_geom::srgb::encode8(value);
            }
            rgba[3] = if self.transparent { (alpha * 255.0).round() as u8 } else { 255 };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_radiance_ev_and_coverage_have_independent_analytic_display_oracles() {
        let raw = [[4.0, 1.0, 0.25, 1.0], [2.0, 0.5, 0.125, 0.5]];
        let before = raw.map(|pixel| pixel.map(f32::to_bits));
        let display = Presentation { exposure_ev: 1.0, background: [0; 3], ..Default::default() }.display(2, 1).unwrap();
        let mut output = [0; 8];
        display.encode(&raw, &mut output, &CancelToken::default()).unwrap();
        let opaque = [8.0 / 9.0, 2.0 / 3.0, 1.0 / 3.0].map(solidcraft_geom::srgb::encode8);
        let covered = [4.0 / 9.0, 1.0 / 3.0, 1.0 / 6.0].map(solidcraft_geom::srgb::encode8);
        assert_eq!(&output[..4], &[opaque[0], opaque[1], opaque[2], 255]);
        assert_eq!(&output[4..], &[covered[0], covered[1], covered[2], 255]);
        assert_eq!(raw.map(|pixel| pixel.map(f32::to_bits)), before);
    }

    #[test]
    fn wide_display_requires_explicit_policy_on_both_calls_without_allocating_a_frame() {
        let wide = super::super::CpuBudget::Workstation.image_limits(false);
        assert!(Presentation::default().display(8193, 1).is_err());
        let display = Presentation::default().display_with_limits(8193, 1, wide).unwrap();
        let mut output = [0; 4];
        let raw = [[1.0, 0.0, 0.0, 1.0]];
        assert!(display.encode_at(&raw, &mut output, &CancelToken::default(), 8192).is_err());
        display.encode_bloom_at_with_limits(&raw, &mut output, &CancelToken::default(), 8192, None, wide).unwrap();
        assert_eq!(output, [solidcraft_geom::srgb::encode8(0.5), 0, 0, 255]);
        assert!(display.encode_bloom_at_with_limits(&raw, &mut output, &CancelToken::default(), 8193, None, wide).is_err());
    }

    #[test]
    fn explicit_gpu_display_matches_legacy_and_preserves_validation_before_cancel() {
        let display = Presentation { exposure_ev: -1.0, ..Default::default() }.display(3, 2).unwrap();
        let input = [[0.125, 2.0, 0.5, 1.0]; 6];
        let mut legacy = [0; 24];
        let mut explicit = [0; 24];
        display.encode(&input, &mut legacy, &CancelToken::default()).unwrap();
        display.encode_bloom_at_with_limits(&input, &mut explicit, &CancelToken::default(), 0, None, super::super::GPU_PATH_LIMITS).unwrap();
        assert_eq!(explicit, legacy);
        let cancel = CancelToken::default();
        cancel.cancel();
        assert_eq!(display.encode(&[], &mut explicit, &cancel).unwrap_err().code, ErrorCode::InvalidRequest);
        assert_eq!(display.encode(&input, &mut explicit, &cancel).unwrap_err().code, ErrorCode::Cancelled);
    }
}
