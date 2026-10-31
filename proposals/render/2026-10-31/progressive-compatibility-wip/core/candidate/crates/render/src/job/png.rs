//! Bounded PNG encoding shared by native image executors.
use super::{CancelToken, ErrorCode, ImageRequest, ImageWork, JobError, Phase, Progress, Result, TaskPoll};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

struct Output {
    bytes: Arc<Mutex<Vec<u8>>>,
    limit: usize,
    cancel: CancelToken,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("render cancelled"));
        }
        let mut out = self.bytes.lock().map_err(|_| std::io::Error::other("PNG lock poisoned"))?;
        if out.len().checked_add(bytes.len()).is_none_or(|n| n > self.limit) {
            return Err(std::io::Error::other("PNG byte limit exceeded"));
        }
        out.try_reserve_exact(bytes.len()).map_err(|_| std::io::Error::other("PNG allocation failed"))?;
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Owns admitted RGBA bytes and yields after at most four scanlines.
pub struct PngTask {
    rgba: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
    bytes: Arc<Mutex<Vec<u8>>>,
    writer: Option<png::StreamWriter<'static, Output>>,
    cancel: CancelToken,
}
impl PngTask {
    pub fn new(request: &ImageRequest, rgba: Vec<u8>, cancel: CancelToken) -> Result<Self> {
        let pixels = u64::from(request.width) * u64::from(request.height);
        if request.width == 0
            || request.height == 0
            || u64::from(request.width) > super::GPU_PATH_LIMITS.max_dimension
            || u64::from(request.height) > super::GPU_PATH_LIMITS.max_dimension
            || pixels > super::GPU_PATH_LIMITS.max_pixels
            || rgba.len() as u64 != pixels * 4
        {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid PNG dimensions or RGBA length"));
        }
        cancel.check()?;
        Self::new_with_limits(request, rgba, cancel, super::GPU_PATH_LIMITS)
    }
    pub(crate) fn new_with_limits(request: &ImageRequest, rgba: Vec<u8>, cancel: CancelToken, limits: crate::RenderLimits) -> Result<Self> {
        let pixels = super::linear_exr::output_pixels_with_limits(request.width, request.height, limits)?;
        let rgba_bytes = super::linear_exr::checked_output_bytes(pixels, 4, 0, limits)?;
        if rgba.len() != rgba_bytes {
            return Err(JobError::new(ErrorCode::InvalidRequest, "invalid PNG dimensions or RGBA length"));
        }
        let limit = super::linear_exr::checked_output_bytes(pixels, 8, 65_536, limits)?;
        let retained = super::linear_exr::checked_output_bytes(rgba.capacity() as u64, 1, 0, limits)? as u64;
        super::linear_exr::checked_stage_bytes(1, retained, limit as u64, limits)?;
        cancel.check()?;
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = Output { bytes: bytes.clone(), limit, cancel: cancel.clone() };
        let mut encoder = png::Encoder::new(output, request.width, request.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let writer = encoder.write_header().and_then(|w| w.into_stream_writer()).map_err(encoding)?;
        Ok(Self { rgba, row: 0, width: request.width as usize, height: request.height as usize, bytes, writer: Some(writer), cancel })
    }
}
fn encoding(e: impl std::fmt::Display) -> JobError {
    JobError::new(ErrorCode::EncodingFailed, e.to_string())
}
impl ImageWork for PngTask {
    fn step(&mut self, _units: usize) -> Result<TaskPoll> {
        self.cancel.check()?;
        for _ in 0..4 {
            if self.row == self.height {
                break;
            }
            let start = self.row * self.width * 4;
            let row = self.rgba.get(start..start + self.width * 4).ok_or_else(|| encoding("missing PNG row"))?;
            self.writer.as_mut().ok_or_else(|| encoding("PNG already completed"))?.write_all(row).map_err(encoding)?;
            self.row += 1;
        }
        if self.row != self.height {
            return Ok(TaskPoll::Pending(Progress { phase: Phase::Encoding, done: self.row as u64, total: self.height as u64 }));
        }
        self.writer.take().ok_or_else(|| encoding("PNG already completed"))?.finish().map_err(encoding)?;
        self.cancel.check()?;
        let mut bytes = self.bytes.lock().map_err(|_| encoding("PNG lock poisoned"))?;
        Ok(TaskPoll::Complete(std::mem::take(&mut *bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(width: u32, height: u32) -> ImageRequest {
        ImageRequest {
            cpu_progressive_every_pass: true,
            raster_punctual_lights: None,
            raster_area_lights: None,
            raster_exposure_ev: 0.0,
            raster_lights: None,
            width,
            height,
            camera: Default::default(),
            integrator: Default::default(),
            raytrace: None,
            environment: None,
            textures: None,
            studio_weights: None,
            background: [0; 3],
            transparent_background: false,
        }
    }
    fn finish(mut task: PngTask) -> Vec<u8> {
        loop {
            if let TaskPoll::Complete(bytes) = task.step(1).unwrap() {
                return bytes;
            }
        }
    }

    #[test]
    fn explicit_png_keeps_default_bytes_and_roundtrips_a_narrow_wide_image() {
        let small = request(3, 2);
        let rgba = [20, 40, 60, 255].repeat(6);
        let legacy = finish(PngTask::new(&small, rgba.clone(), CancelToken::default()).unwrap());
        let explicit = finish(PngTask::new_with_limits(&small, rgba, CancelToken::default(), super::super::GPU_PATH_LIMITS).unwrap());
        assert_eq!(explicit, legacy);
        let wide = request(8193, 1);
        let rgba = [20, 40, 60, 255].repeat(8193);
        assert!(PngTask::new(&wide, vec![], CancelToken::default()).is_err());
        let png = finish(
            PngTask::new_with_limits(&wide, rgba.clone(), CancelToken::default(), super::super::CpuBudget::Workstation.image_limits(false)).unwrap(),
        );
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (8193, 1));
        assert_eq!(decoded.as_raw(), &rgba);
    }

    #[test]
    fn png_dimension_length_and_cancel_precedence_remains_legacy() {
        let cancel = CancelToken::default();
        cancel.cancel();
        assert_eq!(PngTask::new(&request(0, 1), vec![], cancel.clone()).err().unwrap().code, ErrorCode::InvalidRequest);
        assert_eq!(PngTask::new(&request(1, 1), vec![], cancel.clone()).err().unwrap().code, ErrorCode::InvalidRequest);
        assert_eq!(PngTask::new(&request(1, 1), vec![0; 4], cancel.clone()).err().unwrap().code, ErrorCode::Cancelled);
        let bad = crate::RenderLimits { max_scratch_bytes: u64::MAX, ..super::super::GPU_PATH_LIMITS };
        assert!(PngTask::new_with_limits(&request(1, 1), vec![0; 4], CancelToken::default(), bad).is_err());
        let cancel = CancelToken::default();
        let mut task = PngTask::new_with_limits(&request(1, 1), vec![0; 4], cancel.clone(), super::super::GPU_PATH_LIMITS).unwrap();
        cancel.cancel();
        assert_eq!(task.step(1).err().unwrap().code, ErrorCode::Cancelled);
    }
}
