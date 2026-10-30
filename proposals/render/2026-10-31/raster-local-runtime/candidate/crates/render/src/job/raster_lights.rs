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

/// New local families share the real DTO validators with CPU transport.
pub fn validate_punctual(lights: &Option<Vec<super::PunctualLightOptions>>) -> Result<()> {
    if let Some(lights) = lights {
        if lights.len() > 8 || lights.capacity() > 8 {
            return Err(invalid("Raster point/spot length and capacity must be at most eight"));
        }
        for light in lights {
            light.validate()?;
        }
    }
    Ok(())
}

pub fn validate_areas(lights: &Option<Vec<super::RectangleLightOptions>>) -> Result<()> {
    if let Some(lights) = lights {
        if lights.len() > 4 || lights.capacity() > 4 {
            return Err(invalid("Raster rectangular length and capacity must be at most four"));
        }
        for light in lights {
            light.validate()?;
        }
    }
    Ok(())
}

pub fn deserialize_punctual<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<super::PunctualLightOptions>>, D::Error> {
    let lights = super::punctual::bounded_punctual(deserializer)?;
    validate_punctual(&lights).map_err(serde::de::Error::custom)?;
    Ok(lights)
}

pub fn deserialize_areas<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<Vec<super::RectangleLightOptions>>, D::Error> {
    let lights = super::bounded_area_lights(deserializer)?;
    validate_areas(&lights).map_err(serde::de::Error::custom)?;
    Ok(lights)
}

pub(super) fn try_copy_punctual(lights: &Option<Vec<super::PunctualLightOptions>>) -> Result<Option<Vec<super::PunctualLightOptions>>> {
    validate_punctual(lights)?;
    let Some(source) = lights else { return Ok(None) };
    if source.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut copy = Vec::new();
    copy.try_reserve_exact(8).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "could not reserve Raster point/spot lights"))?;
    if copy.capacity() > 8 {
        return Err(invalid("Raster point/spot allocation exceeded its bounded capacity"));
    }
    // Fixed-size DTO: the admitted reserve covers every push, without hidden allocation.
    copy.extend(source.iter().copied());
    Ok(Some(copy))
}

pub(super) fn try_copy_areas(lights: &Option<Vec<super::RectangleLightOptions>>) -> Result<Option<Vec<super::RectangleLightOptions>>> {
    validate_areas(lights)?;
    let Some(source) = lights else { return Ok(None) };
    if source.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut copy = Vec::new();
    copy.try_reserve_exact(4).map_err(|_| JobError::new(ErrorCode::AllocationFailed, "could not reserve Raster rectangular lights"))?;
    if copy.capacity() > 4 {
        return Err(invalid("Raster rectangular allocation exceeded its bounded capacity"));
    }
    // RectangleLightOptions contains only fixed arrays and a scalar.
    copy.extend(source.iter().cloned());
    Ok(Some(copy))
}

pub(super) fn local_retained_bytes(punctual: &Option<Vec<super::PunctualLightOptions>>, areas: &Option<Vec<super::RectangleLightOptions>>) -> Result<u64> {
    validate_punctual(punctual)?;
    validate_areas(areas)?;
    let p = punctual.as_ref().map_or(0, Vec::capacity);
    let a = areas.as_ref().map_or(0, Vec::capacity);
    p.checked_mul(std::mem::size_of::<super::PunctualLightOptions>())
        .and_then(|n| a.checked_mul(std::mem::size_of::<super::RectangleLightOptions>()).and_then(|m| n.checked_add(m)))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster local retained size overflow"))
}

pub(super) fn local_task_bytes(punctual: &Option<Vec<super::PunctualLightOptions>>, areas: &Option<Vec<super::RectangleLightOptions>>) -> Result<u64> {
    validate_punctual(punctual)?;
    validate_areas(areas)?;
    // Covers the original public-task Vec capacity even for Some(empty), and
    // the bounded fallible host-copy capacity. No allocation occurs here on valid input.
    let p = if punctual.is_some() { 8usize } else { 0 };
    let a = if areas.is_some() { 4usize } else { 0 };
    p.checked_mul(std::mem::size_of::<super::PunctualLightOptions>())
        .and_then(|n| a.checked_mul(std::mem::size_of::<super::RectangleLightOptions>()).and_then(|m| n.checked_add(m)))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| JobError::new(ErrorCode::LimitExceeded, "Raster local task size overflow"))
}
