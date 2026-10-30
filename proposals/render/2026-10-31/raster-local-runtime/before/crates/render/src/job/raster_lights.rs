//! Bounded transient Raster input. No design persistence or implicit light conversion.

use std::fmt;

use super::{DirectionalLightOptions, ErrorCode, JobError, Result};

const MAX_LIGHTS: usize = crate::studio::MAX_SCHEMATIC_DIRECTIONAL_LIGHTS;

fn invalid(message: &str) -> JobError {
    JobError::new(ErrorCode::InvalidRequest, message)
}

fn validate_light(light: &DirectionalLightOptions) -> Result<()> {
    if !light.direction.iter().all(|v| v.is_finite() && v.abs() <= 1e9) || !light.direction.iter().any(|v| *v != 0.0) {
        return Err(invalid("Raster light direction must be finite, nonzero and bounded by 1e9"));
    }
    if !light.color.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        || !light.intensity.is_finite()
        || !(0.0..=64.0).contains(&light.intensity)
    {
        return Err(invalid("Raster light requires linear RGB in 0..1 and intensity in 0..64"));
    }
    Ok(())
}

pub fn validate(lights: &Option<Vec<DirectionalLightOptions>>) -> Result<()> {
    if let Some(lights) = lights {
        if lights.len() > MAX_LIGHTS || lights.capacity() > MAX_LIGHTS {
            return Err(invalid("Raster light length and retained capacity must be at most eight"));
        }
        for light in lights {
            validate_light(light)?;
        }
    }
    Ok(())
}

fn reserve() -> Result<Vec<DirectionalLightOptions>> {
    let mut lights = Vec::new();
    lights.try_reserve_exact(MAX_LIGHTS).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "could not reserve Raster lights"))?;
    if lights.capacity() > MAX_LIGHTS {
        return Err(invalid("Raster light allocation exceeded its bounded capacity"));
    }
    Ok(lights)
}

/// Copies only the bounded new payload; never whole-request clone-then-clear.
pub(super) fn try_copy(lights: &Option<Vec<DirectionalLightOptions>>) -> Result<Option<Vec<DirectionalLightOptions>>> {
    validate(lights)?;
    let Some(source) = lights else { return Ok(None) };
    if source.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut copy = reserve()?;
    // Each element consists solely of fixed arrays and a scalar; no hidden allocation.
    for light in source {
        copy.push(DirectionalLightOptions { direction: light.direction, color: light.color, intensity: light.intensity });
    }
    Ok(Some(copy))
}

pub(super) fn retained_bytes(lights: &Option<Vec<DirectionalLightOptions>>) -> Result<u64> {
    validate(lights)?;
    let capacity = lights.as_ref().map_or(0, Vec::capacity);
    capacity
        .checked_mul(std::mem::size_of::<DirectionalLightOptions>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster light retained size overflow"))
}

/// Covers host clones and public ImageTask callers retaining an original Vec,
/// including an empty Vec with nonzero capacity. None keeps the legacy estimate.
pub(super) fn task_bytes(lights: &Option<Vec<DirectionalLightOptions>>) -> Result<u64> {
    validate(lights)?;
    if lights.is_none() {
        return Ok(0);
    }
    MAX_LIGHTS
        .checked_mul(std::mem::size_of::<DirectionalLightOptions>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster task light size overflow"))
}

/// Missing fields use serde(default); explicit null is the unchanged legacy path.
pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<DirectionalLightOptions>>, D::Error> {
    struct Nullable;
    impl<'de> serde::de::Visitor<'de> for Nullable {
        type Value = Option<Vec<DirectionalLightOptions>>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("null or at most eight Raster directional lights")
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<Self::Value, D::Error> {
            struct Sequence;
            impl<'de> serde::de::Visitor<'de> for Sequence {
                type Value = Vec<DirectionalLightOptions>;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("at most eight Raster directional lights")
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error> {
                    let mut lights = Vec::new();
                    while let Some(light) = sequence.next_element::<DirectionalLightOptions>()? {
                        if lights.len() == MAX_LIGHTS {
                            return Err(serde::de::Error::custom("at most eight Raster directional lights"));
                        }
                        validate_light(&light).map_err(serde::de::Error::custom)?;
                        if lights.is_empty() {
                            lights = reserve().map_err(serde::de::Error::custom)?;
                        }
                        lights.push(light);
                    }
                    Ok(lights)
                }
            }
            d.deserialize_seq(Sequence).map(Some)
        }
    }
    deserializer.deserialize_option(Nullable)
}
