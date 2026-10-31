//! Original single-part, uncompressed OpenEXR scanline writer from the public
//! OpenEXR layout documentation. No third-party implementation is incorporated.
//! RGB is scene-linear sRGB/D65, coverage-premultiplied, before presentation.
use super::{CancelToken, ErrorCode, JobError, Result};

fn invalid(message: &str) -> JobError {
    JobError::new(ErrorCode::InvalidRequest, message)
}

/// Conservative encoded bound, including header, offsets and row metadata.
/// The writer checks exact dimensions and payload before reserving any output.
pub fn encoded_limit(width: u32, height: u32) -> Result<u64> {
    let (w, h) = (u64::from(width), u64::from(height));
    if w == 0 || h == 0 || w > 8192 || h > 8192 || w * h > 16_777_216 {
        return Err(invalid("linear EXR requires 1..8192 sides and at most 16777216 pixels"));
    }
    encoded_limit_with_limits(width, height, super::GPU_PATH_LIMITS)
}

/// Internal bounded policy, not proof of a host reservation. Future callers must
/// retain their request's lease and charge work before advancing output stages.
pub(crate) fn output_pixels_with_limits(width: u32, height: u32, limits: crate::RenderLimits) -> Result<u64> {
    let maximum = super::CpuBudget::Workstation.image_limits(false);
    if limits.max_dimension > maximum.max_dimension || limits.max_pixels > maximum.max_pixels || limits.max_scratch_bytes > maximum.max_scratch_bytes
    {
        return Err(invalid("output policy exceeds the bounded implementation envelope"));
    }
    let (w, h) = (u64::from(width), u64::from(height));
    if limits.fit(w, h, None) != Some((w, h)) {
        return Err(invalid("output dimensions exceed the explicit policy"));
    }
    let pixels = w.checked_mul(h).ok_or_else(|| invalid("output pixel count overflow"))?;
    checked_output_bytes(pixels, 1, 0, limits)?;
    Ok(pixels)
}

/// Checked combined live-byte estimate; this is not an individual Vec layout or a lease.
pub(crate) fn checked_stage_bytes(count: u64, element_bytes: u64, extra: u64, limits: crate::RenderLimits) -> Result<u64> {
    let maximum = super::CpuBudget::Workstation.image_limits(false);
    if limits.max_dimension > maximum.max_dimension || limits.max_pixels > maximum.max_pixels || limits.max_scratch_bytes > maximum.max_scratch_bytes
    {
        return Err(invalid("output policy exceeds the bounded implementation envelope"));
    }
    let bytes = count.checked_mul(element_bytes).and_then(|n| n.checked_add(extra)).ok_or_else(|| invalid("output byte layout overflow"))?;
    if bytes > limits.max_scratch_bytes {
        return Err(invalid("output byte layout exceeds its explicit policy"));
    }
    Ok(bytes)
}

/// Check an individual addressable byte layout separately from combined estimates.
pub(crate) fn checked_output_bytes(count: u64, element_bytes: u64, extra: u64, limits: crate::RenderLimits) -> Result<usize> {
    let bytes = checked_stage_bytes(count, element_bytes, extra, limits)?;
    let addressable = usize::try_from(bytes).map_err(|_| invalid("output byte layout exceeds address space"))?;
    if std::alloc::Layout::array::<u8>(addressable).is_err() {
        return Err(invalid("output byte layout exceeds address space"));
    }
    Ok(addressable)
}

pub(crate) fn encoded_limit_with_limits(width: u32, height: u32, limits: crate::RenderLimits) -> Result<u64> {
    let pixels = output_pixels_with_limits(width, height, limits)?;
    let row_bytes = u64::from(width).checked_mul(16).ok_or_else(|| invalid("EXR row overflow"))?;
    i32::try_from(row_bytes).map_err(|_| invalid("EXR row exceeds scanline representation"))?;
    i32::try_from(width).and_then(|_| i32::try_from(height)).map_err(|_| invalid("EXR window exceeds integer representation"))?;
    let extra = u64::from(height).checked_mul(16).and_then(|n| n.checked_add(4096)).ok_or_else(|| invalid("EXR metadata overflow"))?;
    Ok(checked_output_bytes(pixels, 16, extra, limits)? as u64)
}

pub(crate) fn scanline_layout(width: u32, height: u32, header_bytes: usize, limits: crate::RenderLimits) -> Result<(usize, usize)> {
    let limit = encoded_limit_with_limits(width, height, limits)?;
    let rows = usize::try_from(height).map_err(|_| invalid("EXR row count exceeds address space"))?;
    let row = usize::try_from(width).ok().and_then(|n| n.checked_mul(16)).ok_or_else(|| invalid("EXR row layout overflow"))?;
    let first = rows.checked_mul(8).and_then(|n| n.checked_add(header_bytes)).ok_or_else(|| invalid("EXR offset table overflow"))?;
    let total = row
        .checked_add(8)
        .and_then(|n| n.checked_mul(rows))
        .and_then(|n| n.checked_add(first))
        .ok_or_else(|| invalid("EXR scanline layout overflow"))?;
    if total as u64 > limit {
        return Err(invalid("EXR output exceeds its admission"));
    }
    Ok((first, total))
}

fn attribute(out: &mut Vec<u8>, name: &[u8], kind: &[u8], value: &[u8]) -> Result<()> {
    if name.len() > 31 || kind.len() > 31 || value.len() > 2048 {
        return Err(invalid("EXR internal attribute exceeds its fixed header bound"));
    }
    out.extend_from_slice(name);
    out.push(0);
    out.extend_from_slice(kind);
    out.push(0);
    let length = i32::try_from(value.len()).map_err(|_| invalid("EXR attribute length overflow"))?;
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(value);
    Ok(())
}

pub(super) fn header(width: u32, height: u32) -> Result<Vec<u8>> {
    encoded_limit(width, height)?;
    header_with_limits(width, height, super::GPU_PATH_LIMITS)
}

pub(crate) fn header_with_limits(width: u32, height: u32, limits: crate::RenderLimits) -> Result<Vec<u8>> {
    encoded_limit_with_limits(width, height, limits)?;
    let mut out = Vec::new();
    out.try_reserve_exact(4096).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "EXR header allocation failed"))?;
    out.extend_from_slice(&20_000_630_u32.to_le_bytes());
    out.extend_from_slice(&2_u32.to_le_bytes());
    let mut channels = Vec::new();
    channels.try_reserve_exact(73).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "EXR channel header allocation failed"))?;
    for name in *b"ABGR" {
        channels.extend_from_slice(&[name, 0]);
        channels.extend_from_slice(&2_i32.to_le_bytes());
        channels.extend_from_slice(&[0; 4]); // pLinear + three reserved bytes
        channels.extend_from_slice(&1_i32.to_le_bytes());
        channels.extend_from_slice(&1_i32.to_le_bytes());
    }
    channels.push(0);
    attribute(&mut out, b"channels", b"chlist", &channels)?;
    attribute(&mut out, b"compression", b"compression", &[0])?;
    let mut window = Vec::new();
    window.try_reserve_exact(16).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "EXR window header allocation failed"))?;
    for value in [0, 0, (width - 1) as i32, (height - 1) as i32] {
        window.extend_from_slice(&value.to_le_bytes());
    }
    attribute(&mut out, b"dataWindow", b"box2i", &window)?;
    attribute(&mut out, b"displayWindow", b"box2i", &window)?;
    attribute(&mut out, b"lineOrder", b"lineOrder", &[0])?;
    attribute(&mut out, b"pixelAspectRatio", b"float", &1_f32.to_le_bytes())?;
    attribute(&mut out, b"screenWindowCenter", b"v2f", &[0; 8])?;
    attribute(&mut out, b"screenWindowWidth", b"float", &1_f32.to_le_bytes())?;
    let mut chromaticities = Vec::new();
    chromaticities.try_reserve_exact(32).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "EXR chromaticities header allocation failed"))?;
    for value in [0.64_f32, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290] {
        chromaticities.extend_from_slice(&value.to_le_bytes());
    }
    attribute(&mut out, b"chromaticities", b"chromaticities", &chromaticities)?;
    attribute(&mut out, b"software", b"string", b"SolidCraft native GPU path")?;
    attribute(&mut out,b"comments",b"string",b"Raw scene-linear sRGB/D65; RGB coverage-premultiplied; before exposure, denoising, finishing, backdrop and tone mapping. Relative radiance, not calibrated SI units.")?;
    out.push(0);
    Ok(out)
}

/// Owns raw pixels and encodes a bounded number of rows per worker advance.
/// No user path or file is involved. The job host later publishes immutable bytes.
pub struct ExrTask {
    pixels: Vec<[f32; 4]>,
    bytes: Vec<u8>,
    width: usize,
    height: usize,
    row: usize,
    cancel: CancelToken,
}
impl ExrTask {
    pub fn new(width: u32, height: u32, pixels: Vec<[f32; 4]>, cancel: CancelToken) -> Result<Self> {
        encoded_limit(width, height)?;
        Self::new_with_limits(width, height, pixels, cancel, super::GPU_PATH_LIMITS)
    }

    pub(crate) fn new_with_limits(width: u32, height: u32, pixels: Vec<[f32; 4]>, cancel: CancelToken, limits: crate::RenderLimits) -> Result<Self> {
        let limit = encoded_limit_with_limits(width, height, limits)?;
        if pixels.len() as u64 != u64::from(width) * u64::from(height) {
            return Err(invalid("EXR pixel count differs from dimensions"));
        }
        cancel.check()?;
        checked_output_bytes(pixels.capacity() as u64, 16, 0, limits)?;
        checked_stage_bytes(pixels.capacity() as u64, 16, limit, limits)?;
        let mut bytes = header_with_limits(width, height, limits)?;
        let row_bytes = u64::from(width) * 16;
        let (first, total) = scanline_layout(width, height, bytes.len(), limits)?;
        let first = first as u64;
        let additional = total.checked_sub(bytes.len()).ok_or_else(|| invalid("EXR address space exceeded"))?;
        bytes.try_reserve_exact(additional).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "EXR output allocation failed"))?;
        if bytes.capacity() as u64 > limit {
            return Err(invalid("EXR output exceeds its admission"));
        }
        for y in 0..height {
            bytes.extend_from_slice(&(first + u64::from(y) * (row_bytes + 8)).to_le_bytes());
        }
        Ok(Self { pixels, bytes, width: width as usize, height: height as usize, row: 0, cancel })
    }
    /// Returns false until all rows are encoded; repeated completion is harmless.
    pub fn step(&mut self, rows: usize) -> Result<bool> {
        self.cancel.check()?;
        for _ in 0..rows.clamp(1, 16) {
            if self.row == self.height {
                break;
            }
            let start = self.row * self.width;
            let scanline = self.pixels.get(start..start + self.width).ok_or_else(|| invalid("missing EXR scanline"))?;
            // Refuse invalid radiance before copying any bytes from this row.
            if scanline.iter().any(|p| p.iter().any(|v| !v.is_finite() || *v < 0.0) || p[3] > 1.0) {
                return Err(invalid("EXR requires finite nonnegative radiance and coverage in 0..1"));
            }
            self.bytes.extend_from_slice(&(self.row as i32).to_le_bytes());
            self.bytes.extend_from_slice(&((self.width * 16) as i32).to_le_bytes());
            // Canonical alphabetical channels, each complete scanline contiguous.
            for channel in [3, 2, 1, 0] {
                for pixel in scanline {
                    self.bytes.extend_from_slice(&pixel[channel].to_le_bytes());
                }
            }
            self.row += 1;
            self.cancel.check()?;
        }
        Ok(self.row == self.height)
    }
    pub fn finish(self) -> Result<(Vec<u8>, Vec<[f32; 4]>)> {
        self.cancel.check()?;
        if self.row != self.height {
            return Err(invalid("EXR encoder is not complete"));
        }
        Ok((self.bytes, self.pixels))
    }
    pub fn completed_rows(&self) -> u64 {
        self.row as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn encode(pixels: Vec<[f32; 4]>, w: u32, h: u32) -> Vec<u8> {
        let mut task = ExrTask::new(w, h, pixels, CancelToken::default()).unwrap();
        while !task.step(1).unwrap() {}
        task.finish().unwrap().0
    }
    #[test]
    fn scanline_layout_retains_hdr_values_alpha_and_signed_zero_bit_patterns() {
        let pixels = vec![[8.0, 0.25, 1234.5, 0.5], [0.0, -0.0, 0.0000001, 0.0], [1.0, 2.0, 3.0, 1.0], [4.0, 5.0, 6.0, 1.0]];
        let bytes = encode(pixels.clone(), 2, 2);
        assert_eq!(&bytes[..8], &[0x76, 0x2f, 0x31, 0x01, 2, 0, 0, 0]);
        // Independent attribute traversal and absolute row offsets, not writer helpers.
        let mut cursor = 8;
        while bytes[cursor] != 0 {
            while bytes[cursor] != 0 {
                cursor += 1;
            }
            cursor += 1;
            while bytes[cursor] != 0 {
                cursor += 1;
            }
            cursor += 1;
            let n = i32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4 + n;
        }
        cursor += 1;
        for y in 0..2 {
            let offset = u64::from_le_bytes(bytes[cursor + y * 8..cursor + y * 8 + 8].try_into().unwrap()) as usize;
            assert_eq!(i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()), y as i32);
            assert_eq!(i32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()), 32);
            for (plane, channel) in [3, 2, 1, 0].into_iter().enumerate() {
                for x in 0..2 {
                    let p = offset + 8 + plane * 8 + x * 4;
                    assert_eq!(u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()), pixels[y * 2 + x][channel].to_bits());
                }
            }
        }
        assert!((bytes.len() as u64) <= encoded_limit(2, 2).unwrap());
    }
    #[test]
    fn malformed_dimensions_radiance_partial_output_and_cancel_fail_closed() {
        for (w, h) in [(0, 1), (1, 0), (8193, 1), (8192, 8192), (u32::MAX, u32::MAX)] {
            assert!(encoded_limit(w, h).is_err());
        }
        assert!(ExrTask::new(2, 2, vec![], CancelToken::default()).is_err());
        assert!(ExrTask::new(1, 1, vec![[1.0; 4]], CancelToken::default()).unwrap().finish().is_err());
        for p in [[f32::NAN, 0., 0., 1.], [f32::INFINITY, 0., 0., 1.], [-1., 0., 0., 1.], [0., 0., 0., 1.01]] {
            let mut task = ExrTask::new(1, 1, vec![p], CancelToken::default()).unwrap();
            assert!(task.step(1).is_err());
        }
        let cancel = CancelToken::default();
        let mut task = ExrTask::new(1, 1, vec![[1.; 4]], cancel.clone()).unwrap();
        cancel.cancel();
        assert_eq!(task.step(1).unwrap_err().code, ErrorCode::Cancelled);
    }

    #[test]
    fn explicit_output_policy_is_exact_bounded_and_checked_before_allocation() {
        let wide = super::super::CpuBudget::Workstation.image_limits(false);
        assert_eq!(output_pixels_with_limits(8193, 1, wide).unwrap(), 8193);
        assert_eq!(encoded_limit_with_limits(8193, 1, wide).unwrap(), 135200);
        assert_eq!(checked_output_bytes(8193, 8, 65536, wide).unwrap(), 131080);
        for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX)] {
            assert!(output_pixels_with_limits(w, h, wide).is_err());
        }
        assert!(output_pixels_with_limits(8193, 1, super::super::GPU_PATH_LIMITS).is_err(), "a reduced fit is not acceptance");
        for hostile in [
            crate::RenderLimits { max_dimension: u64::MAX, ..wide },
            crate::RenderLimits { max_pixels: u64::MAX, ..wide },
            crate::RenderLimits { max_scratch_bytes: u64::MAX, ..wide },
        ] {
            assert!(output_pixels_with_limits(1, 1, hostile).is_err());
        }
        assert!(checked_output_bytes(u64::MAX, 16, 0, wide).is_err());
        assert!(checked_output_bytes(1, 1, u64::MAX, wide).is_err());
        assert!(scanline_layout(1, 1, usize::MAX, wide).is_err());
        assert!(checked_output_bytes(1, 16, 0, crate::RenderLimits { max_scratch_bytes: 15, ..wide }).is_err());
        assert!(checked_output_bytes(isize::MAX as u64 + 1, 1, 0, wide).is_err());
    }

    #[test]
    fn explicit_gpu_exr_keeps_legacy_bytes_and_dimension_before_cancel_order() {
        let pixels = vec![[4.0, 1.0, 0.25, 1.0]; 6];
        let expected = encode(pixels.clone(), 3, 2);
        let mut task = ExrTask::new_with_limits(3, 2, pixels, CancelToken::default(), super::super::GPU_PATH_LIMITS).unwrap();
        assert!(task.step(2).unwrap());
        assert_eq!(task.finish().unwrap().0, expected);
        let cancel = CancelToken::default();
        cancel.cancel();
        let error = ExrTask::new(0, 1, vec![], cancel.clone()).err().unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(error.to_string().contains("linear EXR requires 1..8192"));
        assert_eq!(ExrTask::new(1, 1, vec![], cancel).err().unwrap().code, ErrorCode::InvalidRequest);
    }
}
