// PRIVATE UNAPPLIED ORIGINAL ANALYTIC TEST PROPOSALS.
// Append inside existing crates/render/src/raytrace/punctual_tests.rs.
// Reuse existing imports from super and solidcraft_geom::Vec3.
// These qualify the existing CPU numerical contract ONLY.
// No GPU payload, shader, pipeline, or new API is invoked.
#[test]
fn gpu_contract_reference_spot_colour_distance_and_cosine_feather_compose() {
    let axis = Vec3::new(0.0, 0.0, 7.0);
    let cone = SpotCone::new(axis, 0.9_f64.acos(), 0.7_f64.acos()).unwrap();
    let colour = Vec3::new(1.0, 0.25, 0.0625);
    let light = PunctualLight::new(Vec3::ZERO, colour, 4.0, 10.0, 0.001, Some(cone)).unwrap();
    // A 3-4-5 triangle has emitted-axis cosine4/5=0.8.
    // Halfway between cone cosines0.9 and0.7 gives smoothstep=1/2.
    // I=4, reference10, distance5 => incident=(8,2,0.5).
    let a = light.sample(Vec3::new(3.0, 0.0, 4.0)).unwrap();
    assert!(a.omitted.is_none());
    assert!((a.incident_linear_srgb - Vec3::new(8.0, 2.0, 0.5)).len() < 1e-11);
    // Same angle at twice the distance must quarter every colour channel.
    let b = light.sample(Vec3::new(6.0, 0.0, 8.0)).unwrap();
    assert!(b.omitted.is_none());
    assert!((a.incident_linear_srgb - b.incident_linear_srgb * 4.0).len() < 1e-11);
    // Reversing only the emitted axis places both samples behind the cone.
    let reversed = SpotCone::new(-axis, 0.9_f64.acos(), 0.7_f64.acos()).unwrap();
    let dark = PunctualLight::new(Vec3::ZERO, colour, 4.0, 10.0, 0.001, Some(reversed)).unwrap();
    assert_eq!(dark.sample(Vec3::new(3.0, 0.0, 4.0)).unwrap().incident_linear_srgb, Vec3::ZERO);
}

#[test]
fn gpu_contract_reference_translation_and_explicit_length_units_preserve_point_response() {
    let colour = Vec3::new(0.125, 0.5, 1.0);
    let source = Vec3::new(37.0, -19.0, 11.0);
    let shift = Vec3::new(1024.0, -2048.0, 4096.0);
    let a = PunctualLight::new(source, colour, 3.0, 50.0, 0.001, None).unwrap();
    let b = PunctualLight::new(source + shift, colour, 3.0, 50.0, 0.001, None).unwrap();
    let c = PunctualLight::new(source, colour, 3.0, 100.0, 0.002, None).unwrap();
    let first = a.sample(source + Vec3::Z * 25.0).unwrap();
    let translated = b.sample(source + shift + Vec3::Z * 25.0).unwrap();
    let scaled = c.sample(source + Vec3::Z * 50.0).unwrap();
    assert_eq!(first.incident_linear_srgb, Vec3::new(1.5, 6.0, 12.0));
    assert_eq!(first.incident_linear_srgb, translated.incident_linear_srgb);
    assert_eq!(first.incident_linear_srgb, scaled.incident_linear_srgb);
    // Doubling only the reference length instead quadruples response.
    let brighter = c.sample(source + Vec3::Z * 25.0).unwrap();
    assert_eq!(brighter.incident_linear_srgb, first.incident_linear_srgb * 4.0);
}

