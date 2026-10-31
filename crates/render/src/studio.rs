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

/// Shared slots for points and spots, independently of directional slots.
pub const MAX_SCHEMATIC_PUNCTUAL_LIGHTS: usize = 8;
pub const MAX_SCHEMATIC_RECTANGLE_LIGHTS: usize = 4;

/// Unshadowed numerical response for the bounded opaque Raster consumer.
///
/// Valid inputs use borrowed slices without new containers or retained allocations.
/// Invalid DTOs may allocate the existing validator's bounded error String.
/// The caller must separately admit the
/// retained capacities/copies and charge up to twenty BRDF evaluations per
/// sample; these slice bounds are not a memory or cancellation admission.
/// An empty rig is dark. No implicit studio, exposure or display transfer.
///
/// Points use reference-distance inverse-square falloff with the authored
/// minimum distance. Spots use a cubic smoothstep in cosine space (an interval
/// <= 1e-6 uses the outer-cosine step). Rectangles use ONLY the centre, projected
/// area and emitting U-cross-V normal; aspect ratio at fixed area has no effect.
/// No finite-footprint integration, soft shadows, sampling seed or environment.
/// Coincident source/sample contributes zero, as in the paired Model helper.
/// Source alignment is not executed f64/f32 image or physical-parity acceptance.
pub fn shade_schematic(
    surface: OpaqueSurface,
    normal: Vec3,
    view: Vec3,
    position_mm: Vec3,
    directional: &[crate::job::DirectionalLightOptions],
    punctual: &[crate::job::PunctualLightOptions],
    rectangles: &[crate::job::RectangleLightOptions],
) -> Result<Vec3, SurfaceError> {
    if directional.len() > MAX_SCHEMATIC_DIRECTIONAL_LIGHTS
        || punctual.len() > MAX_SCHEMATIC_PUNCTUAL_LIGHTS
        || rectangles.len() > MAX_SCHEMATIC_RECTANGLE_LIGHTS
        || !schematic_world_point(position_mm)
    {
        return Err(SurfaceError::Radiance);
    }
    // Reuse the real public DTO validator; no shadow parser or CPU feature gate.
    // Check all local inputs before visibility, colour or intensity can skip one.
    for light in punctual {
        light.validate().map_err(|_| SurfaceError::Radiance)?;
    }
    for light in rectangles {
        schematic_rectangle_geometry(light)?;
    }
    let n = unit_direction(normal)?;
    let v = unit_direction(view)?;
    // Keep the existing directional implementation and its validation intact.
    let mut radiance = shade_directional(surface, n, v, directional)?;
    for light in punctual {
        let [x, y, z] = light.position_mm;
        let delta = Vec3::new(x, y, z) - position_mm;
        if delta == Vec3::ZERO {
            continue;
        }
        let direction = unit_direction(delta)?;
        let distance_squared = delta.dot(delta).max(light.minimum_distance_mm * light.minimum_distance_mm).max(1e-6);
        let mut weight = light.reference_distance_mm * light.reference_distance_mm / distance_squared;
        if let Some(cone) = light.cone {
            let [x, y, z] = cone.emitted_axis;
            let cosine = unit_direction(Vec3::new(x, y, z))?.dot(-direction);
            let inner = cone.inner_half_angle_radians.cos();
            let outer = cone.outer_half_angle_radians.cos();
            let profile = if inner - outer > 1e-6 {
                let t = ((cosine - outer) / (inner - outer)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            } else if cosine >= outer {
                1.0
            } else {
                0.0
            };
            weight *= profile;
        }
        radiance += schematic_contribution(surface, n, v, direction, light.color, light.intensity * weight)?;
    }
    for light in rectangles {
        let (center, emitting_normal, area) = schematic_rectangle_geometry(light)?;
        let delta = center - position_mm;
        if delta == Vec3::ZERO {
            continue;
        }
        let direction = unit_direction(delta)?;
        let weight = area * emitting_normal.dot(-direction).max(0.0) / delta.dot(delta).max(1e-6);
        radiance += schematic_contribution(surface, n, v, direction, light.color, light.intensity * weight)?;
    }
    if !radiance.is_finite() || radiance.x < 0.0 || radiance.y < 0.0 || radiance.z < 0.0 {
        return Err(SurfaceError::Radiance);
    }
    Ok(radiance)
}

fn schematic_world_point(point: Vec3) -> bool {
    point.is_finite() && [point.x, point.y, point.z].into_iter().all(|c| c.abs() <= 1e9)
}

// The crate-local rectangle validator is shared with CPU transport and admission.
// This geometry projection does not parse or admit retained containers.
fn schematic_rectangle_geometry(light: &crate::job::RectangleLightOptions) -> Result<(Vec3, Vec3, f64), SurfaceError> {
    light.validate().map_err(|_| SurfaceError::Radiance)?;
    let [x, y, z] = light.center_mm;
    let center = Vec3::new(x, y, z);
    let [x, y, z] = light.edge_u_mm;
    let u = Vec3::new(x, y, z);
    let [x, y, z] = light.edge_v_mm;
    let v = Vec3::new(x, y, z);
    let cross = u.cross(v);
    Ok((center, unit_direction(cross)?, cross.len()))
}

fn schematic_contribution(
    surface: OpaqueSurface,
    normal: Vec3,
    view: Vec3,
    direction: Vec3,
    color: [f64; 3],
    incident: f64,
) -> Result<Vec3, SurfaceError> {
    let brdf = surface.brdf(normal, view, direction)?;
    let [r, g, b] = color;
    let value = Vec3::new(brdf.x * r, brdf.y * g, brdf.z * b) * (incident * normal.dot(direction).max(0.0));
    if !value.is_finite() || value.x < 0.0 || value.y < 0.0 || value.z < 0.0 {
        return Err(SurfaceError::Radiance);
    }
    Ok(value)
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

    fn local_surface() -> OpaqueSurface {
        OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap()
    }

    fn local_point() -> crate::job::PunctualLightOptions {
        crate::job::PunctualLightOptions {
            position_mm: [0.0, 0.0, 10.0],
            color: [1.0; 3],
            intensity: 2.0,
            reference_distance_mm: 10.0,
            minimum_distance_mm: 0.001,
            cone: None,
        }
    }

    fn local_rectangle() -> crate::job::RectangleLightOptions {
        crate::job::RectangleLightOptions {
            center_mm: [0.0, 0.0, 10.0],
            edge_u_mm: [2.0, 0.0, 0.0],
            edge_v_mm: [0.0, -2.0, 0.0],
            color: [1.0; 3],
            intensity: 2.0,
        }
    }

    fn assert_lambert(value: Vec3, incident_cosine: f64) {
        // Closed-form rho/pi, independent of the production light helper.
        let expected = Vec3::new(0.8, 0.5, 0.25) * (incident_cosine / PI);
        assert!((value - expected).len() < 1e-12 * expected.len().max(1.0), "{value:?} != {expected:?}");
    }

    #[test]
    fn schematic_local_point_reference_and_double_distance_match_closed_form() {
        let mut point = local_point();
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), 2.0);
        point.position_mm = [0.0, 0.0, 20.0];
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), 0.5);
    }

    #[test]
    fn schematic_local_point_colour_and_receiver_cosine_are_linear() {
        let mut point = local_point();
        point.position_mm = [0.0, 6.0, 8.0];
        point.color = [0.25, 0.5, 1.0];
        let value = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap();
        let expected = Vec3::new(0.32, 0.4, 0.4) * (1.0 / PI);
        assert!((value - expected).len() < 1e-13);
    }

    #[test]
    fn schematic_local_world_translation_preserves_point_spot_and_rectangle_response() {
        let mut point = local_point();
        point.cone =
            Some(crate::job::SpotConeOptions { emitted_axis: [0.0, 0.0, -1.0], inner_half_angle_radians: 0.2, outer_half_angle_radians: 0.5 });
        let mut rectangle = local_rectangle();
        let first = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[rectangle.clone()]).unwrap();
        point.position_mm = [123.0, -45.0, 77.0];
        rectangle.center_mm = point.position_mm;
        let moved = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::new(123.0, -45.0, 67.0), &[], &[point], &[rectangle]).unwrap();
        assert!((first - moved).len() < 1e-13);
    }

    #[test]
    fn schematic_local_minimum_distance_and_tiny_nonzero_delta_remain_finite() {
        let mut point = local_point();
        point.position_mm = [0.0, 0.0, 1.0];
        point.minimum_distance_mm = 2.0;
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), 50.0);
        point.position_mm = [0.0, 0.0, 1e-200];
        point.minimum_distance_mm = 0.001;
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), 2e8);
    }

    #[test]
    fn schematic_local_spot_inner_outer_and_cosine_midpoint_match_cubic_profile() {
        // Source at +Z: these axes have cosine 1, -1 or 1/2 with -Z.
        // cos(inner)=4/5 and cos(outer)=1/5, so cosine1/2 gives t=1/2.
        for (axis, expected) in [([0.0, 0.0, -1.0], 2.0), ([0.0, 0.0, 1.0], 0.0), ([3.0_f64.sqrt(), 0.0, -1.0], 1.0)] {
            let mut point = local_point();
            point.cone = Some(crate::job::SpotConeOptions {
                emitted_axis: axis,
                inner_half_angle_radians: 0.8_f64.acos(),
                outer_half_angle_radians: 0.2_f64.acos(),
            });
            assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), expected);
        }
    }

    #[test]
    fn schematic_local_spot_narrow_interval_uses_explicit_step_policy() {
        for (axis, expected) in [([0.0, 0.0, -1.0], 2.0), ([1.0, 0.0, 0.0], 0.0)] {
            let mut point = local_point();
            point.cone = Some(crate::job::SpotConeOptions { emitted_axis: axis, inner_half_angle_radians: 0.0, outer_half_angle_radians: 0.001 });
            assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), expected);
        }
    }

    #[test]
    fn schematic_local_area_centroid_matches_area_over_squared_distance() {
        let mut rectangle = local_rectangle();
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle.clone()]).unwrap(), 0.08);
        rectangle.edge_u_mm = [4.0, 0.0, 0.0];
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle.clone()]).unwrap(), 0.16);
        rectangle.center_mm = [0.0, 0.0, 20.0];
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).unwrap(), 0.04);
    }

    #[test]
    fn schematic_local_area_emitting_side_and_projected_cosine_are_explicit() {
        let mut rectangle = local_rectangle();
        rectangle.edge_v_mm = [0.0, -1.6, -1.2];
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle.clone()]).unwrap(), 0.064);
        rectangle.edge_v_mm = [0.0, 2.0, 0.0];
        assert_eq!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).unwrap(), Vec3::ZERO);
    }

    #[test]
    fn schematic_local_area_same_area_different_aspect_is_documented_approximation() {
        let rectangle = local_rectangle();
        let mut thin = rectangle.clone();
        thin.edge_u_mm = [4.0, 0.0, 0.0];
        thin.edge_v_mm = [0.0, -1.0, 0.0];
        let first = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).unwrap();
        let second = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[thin]).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn schematic_local_coincident_point_spot_and_area_are_zero_without_fallback_axis() {
        let mut point = local_point();
        point.position_mm = [0.0; 3];
        let mut rectangle = local_rectangle();
        rectangle.center_mm = [0.0; 3];
        for cone in
            [None, Some(crate::job::SpotConeOptions { emitted_axis: [0.0, 0.0, -1.0], inner_half_angle_radians: 0.2, outer_half_angle_radians: 0.5 })]
        {
            point.cone = cone;
            assert_eq!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[rectangle.clone()]).unwrap(), Vec3::ZERO);
        }
    }

    #[test]
    fn schematic_local_empty_back_facing_and_back_view_have_no_ambient() {
        assert_eq!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[]).unwrap(), Vec3::ZERO);
        let point = local_point();
        for (n, v) in [(-Vec3::Z, Vec3::Z), (Vec3::Z, -Vec3::Z)] {
            assert_eq!(shade_schematic(local_surface(), n, v, Vec3::ZERO, &[], &[point], &[]).unwrap(), Vec3::ZERO);
        }
    }

    #[test]
    fn schematic_local_three_families_superpose_without_display_transfer() {
        let d = directional([0.0, 0.0, 1.0], [1.0; 3], 1.0);
        let value = shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[d], &[local_point()], &[local_rectangle()]).unwrap();
        assert_lambert(value, 3.08);
        assert_ne!(value, tone_map(value).unwrap());
    }

    #[test]
    fn schematic_local_counts_eight_shared_punctual_and_four_rectangles_separately() {
        let d: [_; 8] = std::array::from_fn(|_| directional([0.0, 0.0, 1.0], [1.0; 3], 1.0));
        let mut p = [local_point(); 8];
        p[7].cone =
            Some(crate::job::SpotConeOptions { emitted_axis: [0.0, 0.0, -1.0], inner_half_angle_radians: 0.2, outer_half_angle_radians: 0.5 });
        let a: [_; 4] = std::array::from_fn(|_| local_rectangle());
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &d, &p, &a).unwrap(), 24.32);
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &d, &[local_point(); 9], &a).is_err());
        let five: [_; 5] = std::array::from_fn(|_| local_rectangle());
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &d, &p, &five).is_err());
        let nine: [_; 9] = std::array::from_fn(|_| directional([0.0, 0.0, 1.0], [1.0; 3], 1.0));
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &nine, &p, &a).is_err());
    }

    #[test]
    fn schematic_local_dark_coincident_inputs_still_validate_every_punctual_property() {
        let mut dark = local_point();
        dark.color = [0.0; 3];
        dark.intensity = 0.0;
        dark.position_mm = [0.0; 3];
        for bad in [f64::NAN, f64::INFINITY, 0.0, 1e6 + 1.0] {
            let mut point = dark;
            point.reference_distance_mm = bad;
            assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).is_err());
        }
        let mut point = dark;
        point.cone = Some(crate::job::SpotConeOptions { emitted_axis: [0.0; 3], inner_half_angle_radians: 0.2, outer_half_angle_radians: 0.5 });
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).is_err());
        point = dark;
        point.position_mm = [f64::NAN, 0.0, 0.0];
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).is_err());
    }

    #[test]
    fn schematic_local_dark_rectangles_refuse_bad_lengths_axes_corners_and_radiance() {
        let mut dark = local_rectangle();
        dark.color = [0.0; 3];
        dark.intensity = 0.0;
        for edge in [[0.0; 3], [f64::NAN, 0.0, 0.0], [f64::INFINITY, 0.0, 0.0], [1e-7, 0.0, 0.0], [1e6 + 1.0, 0.0, 0.0], [1.0, 1.0, 0.0]] {
            let mut rectangle = dark.clone();
            rectangle.edge_u_mm = edge;
            assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).is_err());
        }
        for bad in [f64::NAN, f64::INFINITY, -1.0, 64.01] {
            let mut rectangle = dark.clone();
            rectangle.intensity = bad;
            assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).is_err());
        }
        let mut rectangle = dark;
        rectangle.center_mm = [1e9, 0.0, 0.0];
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).is_err());
    }

    #[test]
    fn schematic_local_rejects_bad_sample_position_and_directions_even_when_empty() {
        for p in [Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(f64::INFINITY, 0.0, 0.0), Vec3::new(1e9 + 1.0, 0.0, 0.0)] {
            assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, p, &[], &[], &[]).is_err());
        }
        assert!(shade_schematic(local_surface(), Vec3::ZERO, Vec3::Z, Vec3::ZERO, &[], &[], &[]).is_err());
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::ZERO, Vec3::ZERO, &[], &[], &[]).is_err());
        let bad = directional([0.0; 3], [0.0; 3], 0.0);
        assert!(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[bad], &[], &[]).is_err());
    }

    #[test]
    fn schematic_local_finite_maximum_authored_scales_are_not_silently_clipped() {
        let mut point = local_point();
        point.position_mm = [0.0, 0.0, 0.001];
        point.reference_distance_mm = 1e6;
        point.intensity = 64.0;
        let mut rectangle = local_rectangle();
        rectangle.center_mm = point.position_mm;
        rectangle.edge_u_mm = [1e6, 0.0, 0.0];
        rectangle.edge_v_mm = [0.0, -1e6, 0.0];
        rectangle.intensity = 64.0;
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[rectangle]).unwrap(), 1.28e20);
    }

    #[test]
    fn schematic_local_area_floor_and_spot_cosine_endpoints_are_explicit() {
        let mut rectangle = local_rectangle();
        rectangle.center_mm = [0.0, 0.0, 0.0001];
        assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[], &[rectangle]).unwrap(), 8e6);
        for (cosine, expected) in [(0.8_f64, 2.0), (0.2_f64, 0.0)] {
            let mut point = local_point();
            point.cone = Some(crate::job::SpotConeOptions {
                emitted_axis: [(1.0 - cosine * cosine).sqrt(), 0.0, -cosine],
                inner_half_angle_radians: 0.8_f64.acos(),
                outer_half_angle_radians: 0.2_f64.acos(),
            });
            assert_lambert(shade_schematic(local_surface(), Vec3::Z, Vec3::Z, Vec3::ZERO, &[], &[point], &[]).unwrap(), expected);
        }
    }
}
