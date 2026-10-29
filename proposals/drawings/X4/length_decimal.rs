//! PRIVATE production helper proposal for #5321; no application hook.
//! NOT COMPILED / NOT RUN. Lengths only, in mm; never use for dimensionless
//! angles, ratios, bulges, weights, knots or parameters (retain 12 decimals).
//! Exact rounding of the stored binary64 value onto a 1e-9 mm integer grid.
//! Tie-away-from-zero and at least one decimal are explicit review proposals.
//! Constant work, 24 inline bytes, no heap allocation, no exponent or negative 0.

use std::fmt;

const MAX_LENGTH_MM: f64 = 9_000_000.0;
const MAX_TICKS: u128 = 9_000_000_000_000_000;
const TICKS_PER_MM: u64 = 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LengthFormatError {
    NonFinite,
    OutsideLengthRange,
    ArithmeticBounds,
    BufferInvariant,
}

impl LengthFormatError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NonFinite => "DXF length must be finite",
            Self::OutsideLengthRange => "DXF length magnitude exceeds 9000000 mm",
            Self::ArithmeticBounds => "DXF length integer-grid arithmetic bounds failed",
            Self::BufferInvariant => "DXF length decimal buffer bounds failed",
        }
    }
}

impl fmt::Display for LengthFormatError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.message())
    }
}

impl std::error::Error for LengthFormatError {}

/// Inline spelling, directly usable by the real Out::code Display argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LengthDecimal {
    bytes: [u8; 24],
    len: usize,
}

impl LengthDecimal {
    fn push(&mut self, byte: u8) -> Result<(), LengthFormatError> {
        let slot = self.bytes.get_mut(self.len).ok_or(LengthFormatError::BufferInvariant)?;
        *slot = byte;
        self.len += 1;
        Ok(())
    }

    pub(crate) fn as_str(&self) -> Result<&str, LengthFormatError> {
        let bytes = self.bytes.get(..self.len).ok_or(LengthFormatError::BufferInvariant)?;
        std::str::from_utf8(bytes).map_err(|_| LengthFormatError::BufferInvariant)
    }
}

impl fmt::Display for LengthDecimal {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.as_str().map_err(|_| fmt::Error)?)
    }
}

/// Exact grid magnitude for the binary64 input, without first rounding a
/// floating-point multiplication. Does not make upstream trig platform-stable.
fn grid_magnitude(value: f64) -> Result<u64, LengthFormatError> {
    if !value.is_finite() { return Err(LengthFormatError::NonFinite); }
    if value.abs() > MAX_LENGTH_MM { return Err(LengthFormatError::OutsideLengthRange); }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as u32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (significand, shift) = if exponent == 0 {
        (fraction, 1074)
    } else {
        // value = significand * 2^(exponent-1075). Accepted magnitudes
        // ensure exponent <=1046, hence 29<=shift<=1074.
        (fraction | (1u64 << 52), 1075u32.checked_sub(exponent).ok_or(LengthFormatError::ArithmeticBounds)?)
    };
    // At most 53+30 bits: exact in u128, including subnormal significands.
    let scaled = u128::from(significand).checked_mul(u128::from(TICKS_PER_MM))
        .ok_or(LengthFormatError::ArithmeticBounds)?;
    let rounded = if shift >= 128 {
        // scaled <2^83, so such a value is strictly below half a grid tick.
        0
    } else {
        if shift == 0 { return Err(LengthFormatError::ArithmeticBounds); }
        let quotient = scaled >> shift;
        let remainder = scaled & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        quotient.checked_add(u128::from(remainder >= half)).ok_or(LengthFormatError::ArithmeticBounds)?
    };
    if rounded > MAX_TICKS { return Err(LengthFormatError::ArithmeticBounds); }
    u64::try_from(rounded).map_err(|_| LengthFormatError::ArithmeticBounds)
}

/// Length in [-9000000,9000000] mm to shortest grid decimal with at least one
/// fractional digit. Sign is emitted only for a nonzero rounded magnitude.
pub(crate) fn length_decimal(value: f64) -> Result<LengthDecimal, LengthFormatError> {
    let ticks = grid_magnitude(value)?;
    let mut output = LengthDecimal { bytes: [0; 24], len: 0 };
    if ticks != 0 && value.is_sign_negative() { output.push(b'-')?; }
    let mut whole = ticks / TICKS_PER_MM;
    let fraction = ticks % TICKS_PER_MM;
    let mut reverse = [0u8; 7];
    let mut count = 0usize;
    loop {
        let digit = u8::try_from(whole % 10).map_err(|_| LengthFormatError::ArithmeticBounds)?;
        *reverse.get_mut(count).ok_or(LengthFormatError::BufferInvariant)? = b'0' + digit;
        count += 1;
        whole /= 10;
        if whole == 0 { break; }
    }
    for byte in reverse.iter().take(count).rev() { output.push(*byte)?; }
    output.push(b'.')?;
    let decimal_start = output.len;
    let mut divisor = 100_000_000u64;
    for _ in 0..9 {
        let digit = u8::try_from((fraction / divisor) % 10).map_err(|_| LengthFormatError::ArithmeticBounds)?;
        output.push(b'0' + digit)?;
        divisor /= 10;
    }
    while output.len > decimal_start + 1 && output.bytes.get(output.len - 1) == Some(&b'0') {
        output.len -= 1;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_subnormals_signs_and_integral_values_have_explicit_spellings() {
        for value in [0.0, -0.0, f64::from_bits(1), -f64::from_bits(1), f64::MIN_POSITIVE, -f64::MIN_POSITIVE] {
            assert_eq!(length_decimal(value).unwrap().as_str().unwrap(), "0.0");
        }
        for (value, expected) in [(1.0, "1.0"), (-1.0, "-1.0"), (12.5, "12.5"), (-12.5, "-12.5")] {
            assert_eq!(length_decimal(value).unwrap().as_str().unwrap(), expected);
        }
    }

    #[test]
    fn rational_binary_half_grid_ties_and_neighbors_have_independent_expectations() {
        // 1/1024 mm * 1e9 = 976562.5 ticks exactly, a binary rational tie.
        let half: f64 = 1.0 / 1024.0;
        for (value, expected) in [
            (half, "0.000976563"), (half.next_down(), "0.000976562"), (half.next_up(), "0.000976563"),
            (-half, "-0.000976563"), ((-half).next_up(), "-0.000976562"), ((-half).next_down(), "-0.000976563"),
        ] { assert_eq!(length_decimal(value).unwrap().as_str().unwrap(), expected); }
    }

    #[test]
    fn exact_grid_rounding_does_not_lose_large_half_tick_to_f64_multiplication() {
        // Exact binary value; its scaled half tick is not representable at
        // this magnitude (the floating product has spacing 1 tick).
        let value: f64 = 8_000_000.0 + 1.0 / 1024.0;
        // The literal multiply-then-round policy rounds the intermediate
        // half tick to the even lower integer, demonstrating different bytes.
        assert_eq!((value * 1e9).round() as u64, 8_000_000_000_976_562);
        assert_eq!(grid_magnitude(value).unwrap(), 8_000_000_000_976_563);
        assert_eq!(length_decimal(value).unwrap().as_str().unwrap(), "8000000.000976563");
        assert_eq!(length_decimal(-value).unwrap().as_str().unwrap(), "-8000000.000976563");
    }

    #[test]
    fn inclusive_length_bounds_refuse_outside_values_and_all_nonfinite_values() {
        assert_eq!(length_decimal(9_000_000.0).unwrap().as_str().unwrap(), "9000000.0");
        assert_eq!(length_decimal(-9_000_000.0).unwrap().as_str().unwrap(), "-9000000.0");
        for value in [9_000_000.0f64.next_up(), (-9_000_000.0f64).next_down(), f64::MAX, -f64::MAX] {
            assert_eq!(length_decimal(value), Err(LengthFormatError::OutsideLengthRange));
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(length_decimal(value), Err(LengthFormatError::NonFinite));
        }
    }

    #[test]
    fn grid_fraction_leading_and_trailing_zeroes_are_decimal_only() {
        for (value, expected) in [
            (1.0 / 512.0, "0.001953125"), (1.0 / 16.0, "0.0625"),
            (1e-9, "0.000000001"), (-1e-9, "-0.000000001"), (1e-12, "0.0"),
        ] { assert_eq!(length_decimal(value).unwrap().as_str().unwrap(), expected); }
        for value in [-8_999_999.0, -0.001, 0.0, 0.001, 8_999_999.0] {
            let decimal = length_decimal(value).unwrap();
            let text = decimal.as_str().unwrap();
            assert!(text.len() <= 18);
            assert!(!text.chars().any(|c| matches!(c, 'e' | 'E' | '+')));
            assert_ne!(text, "-0.0");
        }
        assert_eq!(format!("{}", length_decimal(12.5).unwrap()), "12.5");
    }
}
