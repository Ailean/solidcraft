use super::*;
use std::f64::consts::PI;

fn lambert() -> OpaqueSurface {
    // At normal incidence IOR 1 gives f = base_colour / pi exactly.
    OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap()
}

#[test]
fn analytic_lambert_separates_linear_colour_intensity_and_cosine() {
    let light = DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 0.5, 0.25), 2.0).unwrap();
    let lights = Lighting::new(vec![light]).unwrap();
    let actual = lights.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap();
    let expected = Vec3::new(0.8, 0.25, 0.0625) * (2.0 / PI);
    assert!((actual - expected).len() < 1e-14);
    let twice = Lighting::new(vec![DirectionalLight::new(light.direction(), light.color(), 4.0).unwrap()]).unwrap();
    assert!((twice.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap() - actual * 2.0).len() < 1e-14);
    // White, fully rough conductor has f = 1 / (2*pi*(1+cos(theta)))
    // at a normal view. This independently checks the incident cosine.
    let conductor = OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 1.0, 1.0, 1.5).unwrap();
    let angled = Lighting::new(vec![DirectionalLight::new(Vec3::new(0.6, 0.0, 0.8), light.color(), 2.0).unwrap()]).unwrap();
    let actual = angled.shade_linear(conductor, Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap();
    assert!((actual - light.color() * (0.8 / (1.8 * PI))).len() < 1e-14);
}

#[test]
fn shadow_visibility_is_independent_for_each_coloured_light() {
    let lights = Lighting::new(vec![
        DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 0.0, 0.0), 2.0).unwrap(),
        DirectionalLight::new(Vec3::Z, Vec3::new(0.0, 0.0, 1.0), 3.0).unwrap(),
    ])
    .unwrap();
    let mut visited = Vec::new();
    let blue = lights
        .shade_linear(lambert(), Vec3::Z, Vec3::Z, |i, direction| {
            visited.push((i, direction));
            Ok(i == 1)
        })
        .unwrap();
    assert_eq!(visited, vec![(0, Vec3::Z), (1, Vec3::Z)]);
    assert!((blue - Vec3::new(0.0, 0.0, 0.75 / PI)).len() < 1e-14);
    assert_eq!(lights.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(false)).unwrap(), Vec3::ZERO);
    for error in [TraceError::Cancelled, TraceError::WorkLimit, TraceError::MemoryLimit] {
        assert_eq!(lights.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Err(error)), Err(error));
    }
}

#[test]
fn neutral_default_preserves_existing_studio_linear_response() {
    let lighting = Lighting::studio_default();
    assert_eq!(lighting.lights().len(), 3);
    for (light, (direction, intensity)) in lighting.lights().iter().zip(crate::studio::LIGHTS) {
        assert_eq!(light.direction(), direction);
        assert_eq!(light.intensity(), intensity);
        assert_eq!(light.color(), Vec3::new(1.0, 1.0, 1.0));
    }
    for metallic in [0.0, 0.5, 1.0] {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.3, 0.2), metallic, 0.5, 1.5).unwrap();
        let expected = crate::studio::shade(surface, Vec3::Z, Vec3::Z).unwrap();
        let actual = lighting.shade_linear(surface, Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap();
        assert!((expected - actual).len() < 1e-12);
    }
}

#[test]
fn exposure_is_applied_after_the_sum_before_one_tone_map() {
    let linear = Vec3::new(0.25, 0.5, 1.0);
    let mapped = map_to_display(linear, 1.0).unwrap();
    assert!((mapped - Vec3::new(1.0 / 3.0, 0.5, 2.0 / 3.0)).len() < 1e-14);
    assert_eq!(map_to_display(Vec3::ZERO, -16.0).unwrap(), Vec3::ZERO);
    assert_eq!(map_to_display(linear, 0.0).unwrap(), crate::studio::tone_map(linear).unwrap());
    let high = map_to_display(Vec3::new(MAX_LINEAR_RADIANCE, MAX_LINEAR_RADIANCE, MAX_LINEAR_RADIANCE), 16.0).unwrap();
    assert!(high.is_finite() && high.x <= 1.0);
}

#[test]
fn dark_backfacing_and_unlit_surfaces_do_not_trace_shadow_rays() {
    let muted = DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 1.0, 1.0), 0.0).unwrap();
    let behind = DirectionalLight::new(-Vec3::Z, Vec3::new(1.0, 1.0, 1.0), 1.0).unwrap();
    let black = DirectionalLight::new(Vec3::Z, Vec3::ZERO, 1.0).unwrap();
    let lighting = Lighting::new(vec![muted, behind, black]).unwrap();
    assert_eq!(lighting.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| panic!("no contributing light")).unwrap(), Vec3::ZERO);
    assert_eq!(Lighting::default().shade_linear(lambert(), Vec3::Z, -Vec3::Z, |_, _| panic!("backfacing view")).unwrap(), Vec3::ZERO);
    assert_eq!(Lighting::new(Vec::new()).unwrap().shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| panic!("no lights")).unwrap(), Vec3::ZERO);
}

#[test]
fn invalid_inputs_are_refused_and_extreme_direction_scales_normalize() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 64.01] {
        assert!(DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 1.0, 1.0), bad).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(DirectionalLight::new(Vec3::Z, Vec3::new(bad, 1.0, 1.0), 1.0).is_err());
    }
    for bad in [Vec3::ZERO, Vec3::new(f64::NAN, 0.0, 1.0)] {
        assert!(DirectionalLight::new(bad, Vec3::ZERO, 1.0).is_err());
        assert!(Lighting::default().shade_linear(lambert(), bad, Vec3::Z, |_, _| Ok(true)).is_err());
        assert!(Lighting::default().shade_linear(lambert(), Vec3::Z, bad, |_, _| Ok(true)).is_err());
    }
    let white = Vec3::new(1.0, 1.0, 1.0);
    for magnitude in [1e-300, 1e308] {
        let direction = DirectionalLight::new(Vec3::new(magnitude, magnitude, magnitude), white, 1.0).unwrap().direction();
        assert!((direction.len() - 1.0).abs() < 1e-14);
    }
    let light = DirectionalLight::new(Vec3::Z, white, MAX_LIGHT_INTENSITY).unwrap();
    assert!(Lighting::new(vec![light; MAX_LIGHTS]).is_ok());
    assert!(Lighting::new(vec![light; MAX_LIGHTS + 1]).is_err());
    let excess_capacity = Vec::<DirectionalLight>::with_capacity(MAX_LIGHTS + 1);
    assert_eq!(excess_capacity.len(), 0);
    assert!(Lighting::new(excess_capacity).is_err(), "empty vectors cannot retain an unbounded light allocation");
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -33.0, 33.0, -32.01, 32.01] {
        assert!(map_to_display(Vec3::ZERO, bad).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, -1.0, MAX_LINEAR_RADIANCE * 2.0] {
        assert!(map_to_display(Vec3::new(bad, 0.0, 0.0), 0.0).is_err());
    }
}

#[test]
fn original_presets_have_distinct_bounded_real_direct_responses() {
    let white = Lighting::preset(StudioPreset::Neutral).unwrap();
    assert_eq!(white, Lighting::default());
    let warm = Lighting::preset(StudioPreset::WarmCool).unwrap();
    let rim = Lighting::preset(StudioPreset::KeyRim).unwrap();
    for lights in [&white, &warm, &rim] {
        assert!(lights.lights().len() <= MAX_LIGHTS);
        let response = lights.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap();
        assert!(response.is_finite() && response.x > 0.0 && response.y > 0.0 && response.z > 0.0);
    }
    assert_ne!(
        white.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap(),
        warm.shade_linear(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(true)).unwrap()
    );
}

#[test]
fn exposure_full_saved_range_has_finite_binary_scaled_rational_outputs() {
    // Independent powers of two make the exposed channels exactly 1, 1/2, 0.
    // Do not obtain expected values from another renderer or the display mapper.
    for (ev, unit) in [(17.0, 1.0 / 131_072.0), (32.0, 1.0 / 4_294_967_296.0), (-17.0, 131_072.0), (-32.0, 4_294_967_296.0)] {
        let actual = map_to_display(Vec3::new(unit, unit * 0.5, 0.0), ev).unwrap();
        assert!((actual - Vec3::new(0.5, 1.0 / 3.0, 0.0)).len() < 1e-14);
    }
    for ev in [-32.0, -17.0, 0.0, 17.0, 32.0] {
        assert_eq!(map_to_display(Vec3::ZERO, ev).unwrap(), Vec3::ZERO);
        let high = map_to_display(Vec3::new(MAX_LINEAR_RADIANCE, MAX_LINEAR_RADIANCE, MAX_LINEAR_RADIANCE), ev).unwrap();
        assert!(high.is_finite());
        assert!([high.x, high.y, high.z].into_iter().all(|v| (0.0..=1.0).contains(&v)));
    }
    // Expanding exposure never expands the admitted LINEAR transport bound.
    assert!(map_to_display(Vec3::new(MAX_LINEAR_RADIANCE * 2.0, 0.0, 0.0), -32.0).is_err());
}

#[test]
fn transmitted_lambert_has_independent_analytic_colour_and_channel_weights() {
    let light = DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 0.5, 0.25), PI).unwrap();
    let lighting = Lighting::new(vec![light]).unwrap();
    let actual = lighting
        .shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |index, direction| {
            assert_eq!((index, direction), (0, Vec3::Z));
            Ok(Vec3::new(0.5, 0.25, 0.75))
        })
        .unwrap();
    // base(.8,.5,.25) * light(1,.5,.25) * PI/PI * T(.5,.25,.75).
    assert!((actual - Vec3::new(0.4, 0.0625, 0.046875)).len() < 1e-14);
}

#[test]
fn transmitted_white_black_and_binary_visibility_preserve_boolean_shading() {
    let lighting = Lighting::preset(StudioPreset::WarmCool).unwrap();
    for metallic in [0.0, 0.5, 1.0] {
        let surface = OpaqueSurface::new(Vec3::new(0.8, 0.3, 0.2), metallic, 0.5, 1.5).unwrap();
        for selection in 0..3 {
            let visible = |index: usize| match selection { 0 => false, 1 => true, _ => index % 2 == 0 };
            let old = lighting.shade_linear(surface, Vec3::Z, Vec3::Z, |i, _| Ok(visible(i))).unwrap();
            let rgb = lighting
                .shade_linear_transmitted(surface, Vec3::Z, Vec3::Z, |i, _| {
                    Ok(if visible(i) { Vec3::new(1.0, 1.0, 1.0) } else { Vec3::ZERO })
                })
                .unwrap();
            assert_eq!(rgb, old);
        }
    }
}

#[test]
fn transmitted_two_lights_use_independent_indices_and_sum_before_output() {
    let lighting = Lighting::new(vec![
        DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 0.5, 0.0), PI).unwrap(),
        DirectionalLight::new(Vec3::Z, Vec3::new(0.5, 0.0, 1.0), 2.0 * PI).unwrap(),
    ])
    .unwrap();
    let mut visited = Vec::new();
    let actual = lighting
        .shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |index, direction| {
            visited.push((index, direction));
            match index {
                0 => Ok(Vec3::new(0.25, 1.0, 0.5)),
                1 => Ok(Vec3::new(1.0, 0.5, 0.5)),
                _ => panic!("unexpected light index"),
            }
        })
        .unwrap();
    assert_eq!(visited, vec![(0, Vec3::Z), (1, Vec3::Z)]);
    // Light0=(.2,.25,0), light1=(.8,0,.25); no tone mapping or exposure.
    assert!((actual - Vec3::new(1.0, 0.25, 0.25)).len() < 1e-14);
}

#[test]
fn transmitted_ineligible_lights_do_not_request_visibility_or_renumber_indices() {
    let white = Vec3::new(1.0, 1.0, 1.0);
    let lighting = Lighting::new(vec![
        DirectionalLight::new(Vec3::Z, white, 0.0).unwrap(),
        DirectionalLight::new(-Vec3::Z, white, 1.0).unwrap(),
        DirectionalLight::new(Vec3::Z, Vec3::ZERO, 1.0).unwrap(),
        DirectionalLight::new(Vec3::X, white, 1.0).unwrap(),
        DirectionalLight::new(Vec3::Z, white, PI).unwrap(),
    ])
    .unwrap();
    let mut visited = Vec::new();
    let actual = lighting
        .shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |index, direction| {
            assert_eq!((index, direction), (4, Vec3::Z));
            visited.push(index);
            Ok(white)
        })
        .unwrap();
    assert_eq!(visited, vec![4]);
    assert!((actual - Vec3::new(0.8, 0.5, 0.25)).len() < 1e-14);
    assert_eq!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, -Vec3::Z, |_, _| panic!("backfacing view")).unwrap(), Vec3::ZERO);
    assert_eq!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, Vec3::X, |_, _| panic!("tangent view")).unwrap(), Vec3::ZERO);
    assert_eq!(Lighting::new(Vec::new()).unwrap().shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| panic!("no lights")).unwrap(), Vec3::ZERO);
}

#[test]
fn transmitted_rejects_every_invalid_channel_and_propagates_callback_errors() {
    let lighting = Lighting::new(vec![DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 1.0, 1.0), PI).unwrap()]).unwrap();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        for channel in 0..3 {
            let transmittance = match channel {
                0 => Vec3::new(bad, 0.0, 0.0),
                1 => Vec3::new(0.0, bad, 0.0),
                _ => Vec3::new(0.0, 0.0, bad),
            };
            assert!(matches!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(transmittance)), Err(TraceError::InvalidInput(_))));
        }
    }
    for error in [TraceError::Cancelled, TraceError::WorkLimit, TraceError::MemoryLimit] {
        assert_eq!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| Err(error)), Err(error));
    }
}

#[test]
fn transmitted_keeps_shading_direction_validation_and_scale_normalization() {
    let lighting = Lighting::new(vec![DirectionalLight::new(Vec3::Z, Vec3::new(1.0, 1.0, 1.0), PI).unwrap()]).unwrap();
    for bad in [Vec3::ZERO, Vec3::new(f64::NAN, 0.0, 1.0), Vec3::new(0.0, 0.0, f64::INFINITY)] {
        assert!(lighting.shade_linear_transmitted(lambert(), bad, Vec3::Z, |_, _| panic!("invalid normal")).is_err());
        assert!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, bad, |_, _| panic!("invalid view")).is_err());
    }
    for magnitude in [1e-300, 1e308] {
        let actual = lighting
            .shade_linear_transmitted(lambert(), Vec3::new(0.0, 0.0, magnitude), Vec3::new(0.0, 0.0, magnitude), |_, _| Ok(Vec3::new(0.5, 0.25, 1.0)))
            .unwrap();
        assert!((actual - Vec3::new(0.4, 0.125, 0.25)).len() < 1e-14);
    }
}

#[test]
fn transmitted_checks_accumulated_radiance_after_transmission_without_clamping() {
    // Hostile INTERNAL fixture deliberately bypasses DirectionalLight::new's
    // intensity admission. This isolates the existing accumulated-radiance gate;
    // these intensities are not asserted to be accepted by the public API.
    // Each unattenuated term is (.6,.375,.1875)*MAX; two exceed MAX in red.
    let light = DirectionalLight { direction: Vec3::Z, color: Vec3::new(1.0, 1.0, 1.0), intensity: 0.75 * PI * MAX_LINEAR_RADIANCE };
    let lighting = Lighting::new(vec![light, light]).unwrap();
    let mut visits = 0;
    assert!(matches!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| {
        visits += 1;
        Ok(Vec3::new(1.0, 1.0, 1.0))
    }), Err(TraceError::InvalidInput(_))));
    assert_eq!(visits, 2);
    let attenuated = lighting
        .shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(Vec3::new(0.5, 0.5, 0.5)))
        .unwrap();
    let expected = Vec3::new(0.6, 0.375, 0.1875) * MAX_LINEAR_RADIANCE;
    assert!((attenuated - expected).len() <= MAX_LINEAR_RADIANCE * 1e-14);
    assert_eq!(lighting.shade_linear_transmitted(lambert(), Vec3::Z, Vec3::Z, |_, _| Ok(Vec3::ZERO)).unwrap(), Vec3::ZERO);
}
