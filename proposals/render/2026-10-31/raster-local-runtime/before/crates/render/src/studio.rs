//! Original fixed direct-light studio used by opaque product preview consumers.
//! Relative linear radiance, not a calibrated illuminance or an HDRI environment.

use solidcraft_geom::Vec3;

use crate::surface::{OpaqueSurface, SurfaceError, unit_direction};

/// Fixed, neutral directional lights in the project's world frame (Z up).
/// Directions point towards each light and are analytically unit length.
pub const LIGHTS: [(Vec3, f64); 3] = [(Vec3::new(0.6, 0.0, 0.8), 3.0), (Vec3::new(-0.8, 0.0, 0.6), 1.0), (Vec3::new(0.0, -0.6, 0.8), 1.5)];

/// Sum BRDF * incident radiance * light cosine, without display transfer.
/// There are no cast shadows, indirect bounces, ambient term or emission.
pub fn shade(surface: OpaqueSurface, normal: Vec3, view: Vec3) -> Result<Vec3, SurfaceError> {
    shade_weighted(surface, normal, view, [1.0; 3])
}

pub fn shade_weighted(surface: OpaqueSurface, normal: Vec3, view: Vec3, weights: [f64; 3]) -> Result<Vec3, SurfaceError> {
    if weights.iter().any(|value| !value.is_finite() || !(0.0..=1.0).contains(value)) {
        return Err(SurfaceError::Radiance);
    }
    let n = unit_direction(normal)?;
    let mut radiance = Vec3::ZERO;
    for ((light, intensity), weight) in LIGHTS.into_iter().zip(weights) {
        radiance += surface.brdf(n, view, light)? * (intensity * weight.clamp(0.0, 1.0) * n.dot(light).max(0.0));
    }
    Ok(radiance)
}

/// Same eight-source bound as the existing explicit CPU directional-light DTO.
/// This core does not install a Raster request consumer or change its defaults.
pub const MAX_SCHEMATIC_DIRECTIONAL_LIGHTS: usize = 8;

/// Bounded direct-light response to explicit, linear RGB directional sources.
/// Directions point towards the source; intensities are relative, not lux.
/// No shadows, environment, exposure, tone mapping or display transfer occurs.
///
/// Uses the existing opaque BRDF, including its authored coat when present.
/// It does not convert opacity into transmission or approximate a dielectric
/// medium. The caller must retain that distinction and admit the actual scene.
pub fn shade_directional(
    surface: OpaqueSurface,
    normal: Vec3,
    view: Vec3,
    lights: &[crate::job::DirectionalLightOptions],
) -> Result<Vec3, SurfaceError> {
    if lights.len() > MAX_SCHEMATIC_DIRECTIONAL_LIGHTS {
        return Err(SurfaceError::Radiance);
    }
    let n = unit_direction(normal)?;
    let v = unit_direction(view)?;
    // Validate every authored source, even if it is dark or behind the surface.
    // Bounds match RaytraceOptions' directional inputs; no clamping/remapping.
    for light in lights {
        if !light.intensity.is_finite()
            || !(0.0..=64.0).contains(&light.intensity)
            || !light.color.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c))
        {
            return Err(SurfaceError::Radiance);
        }
        if !light.direction.iter().all(|c| c.is_finite() && c.abs() <= 1e9) || !light.direction.iter().any(|c| *c != 0.0) {
            return Err(SurfaceError::Direction);
        }
    }
    let mut radiance = Vec3::ZERO;
    for light in lights {
        let [x, y, z] = light.direction;
        let direction = unit_direction(Vec3::new(x, y, z))?;
        let brdf = surface.brdf(n, v, direction)?;
        let [red, green, blue] = light.color;
        radiance += Vec3::new(brdf.x * red, brdf.y * green, brdf.z * blue) * (light.intensity * n.dot(direction).max(0.0));
    }
    if !radiance.is_finite() || radiance.x < 0.0 || radiance.y < 0.0 || radiance.z < 0.0 {
        return Err(SurfaceError::Radiance);
    }
    Ok(radiance)
}

/// Per-channel Reinhard preview tone map, still in linear sRGB.
/// Consumers encode exactly once after applying this to nonnegative finite radiance.
pub fn tone_map(radiance: Vec3) -> Result<Vec3, SurfaceError> {
    if !radiance.is_finite() || radiance.x < 0.0 || radiance.y < 0.0 || radiance.z < 0.0 {
        return Err(SurfaceError::Radiance);
    }
    let map = |c: f64| c / (1.0 + c);
    Ok(Vec3::new(map(radiance.x), map(radiance.y), map(radiance.z)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn render_lights_studio_visibility_preserves_default_and_zero_weights_remove_all_sources() {
        direct_white_conductor_matches_closed_form_and_is_tone_mapped_once();
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.5).unwrap();
        assert_eq!(shade(surface, Vec3::Z, Vec3::Z).unwrap(), shade_weighted(surface, Vec3::Z, Vec3::Z, [1.0; 3]).unwrap());
        assert_eq!(shade_weighted(surface, Vec3::Z, Vec3::Z, [0.0; 3]).unwrap(), Vec3::ZERO);
        assert!(shade_weighted(surface, Vec3::Z, Vec3::Z, [f64::NAN, 1.0, 1.0]).is_err());
        assert!(shade_weighted(surface, Vec3::Z, Vec3::Z, [2.0, 1.0, 1.0]).is_err());
    }

    #[test]
    fn direct_white_conductor_matches_closed_form_and_is_tone_mapped_once() {
        let s = OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 1.0, 1.0, 1.5).unwrap();
        let c = shade(s, Vec3::Z, Vec3::Z).unwrap();
        let expected = (4.5 * 0.8 / 1.8 + 0.6 / 1.6) / (2.0 * PI);
        assert!((c - Vec3::new(expected, expected, expected)).len() < 1e-14);
        let mapped = tone_map(c).unwrap();
        assert!((mapped.x - expected / (1.0 + expected)).abs() < 1e-14);
        assert_eq!(tone_map(Vec3::ZERO).unwrap(), Vec3::ZERO);
        assert!(tone_map(Vec3::new(f64::NAN, 0.0, 0.0)).is_err());
        assert!(tone_map(Vec3::new(-1.0, 0.0, 0.0)).is_err());
        assert!(shade(s, Vec3::ZERO, Vec3::Z).is_err());
    }

    fn directional(direction: [f64; 3], color: [f64; 3], intensity: f64) -> crate::job::DirectionalLightOptions {
        crate::job::DirectionalLightOptions { direction, color, intensity }
    }

    #[test]
    fn schematic_directional_linear_rgb_matches_independent_lambert_radiance() {
        // Index matching removes the BRDF's Fresnel term, leaving rho/pi.
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
        let light = directional([0.0, 0.0, 1.0], [0.25, 0.5, 1.0], 2.0);
        let value = shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).unwrap();
        let expected = Vec3::new(0.4 / PI, 0.5 / PI, 0.5 / PI);
        assert!((value - expected).len() < 1e-14);
        // Transfer is deliberately a later operation; the core has not encoded
        // an sRGB colour or applied a second Reinhard mapping.
        let mapped = tone_map(value).unwrap();
        assert!((mapped.x - (0.4 / PI) / (1.0 + 0.4 / PI)).abs() < 1e-14);
        assert_ne!(value, mapped);
    }

    #[test]
    fn schematic_directional_superposition_and_intensity_are_linear_before_display() {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
        let red = directional([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 2.0);
        let blue = directional([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 4.0);
        let both = shade_directional(surface, Vec3::Z, Vec3::Z, &[red.clone(), blue.clone()]).unwrap();
        let expected = Vec3::new(1.6 / PI, 0.0, 1.0 / PI);
        assert!((both - expected).len() < 1e-14);
        let separate = shade_directional(surface, Vec3::Z, Vec3::Z, &[red]).unwrap() + shade_directional(surface, Vec3::Z, Vec3::Z, &[blue]).unwrap();
        assert!((both - separate).len() < 1e-14);
    }

    #[test]
    fn schematic_directional_cosine_and_direction_scale_are_not_source_distance() {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
        let light = directional([0.0, 3.0, 4.0], [1.0; 3], 2.0);
        let value = shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).unwrap();
        // Normal dot the normalised3-4-5 direction is4/5, not4 or distance5.
        let expected = Vec3::new(1.28 / PI, 0.8 / PI, 0.4 / PI);
        assert!((value - expected).len() < 1e-14);
        for scale in [1e-200, 0.25, 1.0, 1e8] {
            let scaled = directional([0.0, 3.0 * scale, 4.0 * scale], [1.0; 3], 2.0);
            assert!((shade_directional(surface, Vec3::Z, Vec3::Z, &[scaled]).unwrap() - expected).len() < 1e-14);
        }
    }

    #[test]
    fn schematic_directional_empty_dark_and_back_facing_sources_produce_no_ambient() {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
        assert_eq!(shade_directional(surface, Vec3::Z, Vec3::Z, &[]).unwrap(), Vec3::ZERO);
        for light in
            [directional([0.0, 0.0, 1.0], [1.0; 3], 0.0), directional([0.0, 0.0, 1.0], [0.0; 3], 64.0), directional([0.0, 0.0, -1.0], [1.0; 3], 64.0)]
        {
            assert_eq!(shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).unwrap(), Vec3::ZERO);
        }
        let light = directional([0.0, 0.0, 1.0], [1.0; 3], 1.0);
        assert_eq!(shade_directional(surface, Vec3::Z, -Vec3::Z, &[light]).unwrap(), Vec3::ZERO);
        assert!(shade_directional(surface, Vec3::ZERO, Vec3::Z, &[]).is_err());
        assert!(shade_directional(surface, Vec3::Z, Vec3::ZERO, &[]).is_err());
    }

    #[test]
    fn schematic_directional_keeps_eight_source_and_existing_numerical_bounds() {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
        let valid = directional([0.0, 0.0, 1.0], [1.0; 3], 64.0);
        let eight: [_; 8] = std::array::from_fn(|_| valid.clone());
        let nine: [_; 9] = std::array::from_fn(|_| valid.clone());
        assert!(shade_directional(surface, Vec3::Z, Vec3::Z, &eight).is_ok());
        assert!(shade_directional(surface, Vec3::Z, Vec3::Z, &nine).is_err());
        for bad in [f64::NAN, f64::INFINITY, -0.01, 64.01] {
            let mut light = valid.clone();
            light.intensity = bad;
            assert!(shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).is_err());
        }
        for bad in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
            let mut light = valid.clone();
            light.color[1] = bad;
            assert!(shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).is_err());
        }
        for direction in [[0.0; 3], [f64::NAN, 0.0, 1.0], [0.0, f64::INFINITY, 1.0], [1e9 + 1.0, 0.0, 1.0]] {
            // Even a disabled or black source cannot hide malformed intent.
            let light = directional(direction, [0.0; 3], 0.0);
            assert!(shade_directional(surface, Vec3::Z, Vec3::Z, &[light]).is_err());
        }
    }

    #[test]
    fn schematic_directional_neutral_rig_matches_existing_opaque_studio_without_changing_it() {
        let explicit = LIGHTS.map(|(direction, intensity)| directional([direction.x, direction.y, direction.z], [1.0; 3], intensity));
        for metallic in [0.0, 1.0] {
            let surface = OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), metallic, 1.0, 1.5).unwrap();
            let old = shade(surface, Vec3::Z, Vec3::Z).unwrap();
            let new = shade_directional(surface, Vec3::Z, Vec3::Z, &explicit).unwrap();
            assert!((old - new).len() < 1e-14);
        }
    }
}
