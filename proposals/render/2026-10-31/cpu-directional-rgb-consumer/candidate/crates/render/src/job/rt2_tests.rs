//! Runtime-option and admission regressions; no render timing assumptions.

use super::*;
use crate::ProductScene;
use solidcraft_geom::Aabb3;

fn request(integrator: Integrator) -> ImageRequest {
    ImageRequest {
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        environment: None,
        textures: None,
        integrator,
        raytrace: None,
        camera: Default::default(),
        width: 16,
        height: 16,
        background: [0; 3],
        transparent_background: false,
    }
}

#[test]
fn options_reject_retained_capacity_even_when_no_lights_are_active() {
    let options = RaytraceOptions { lights: Some(Vec::with_capacity(64)), ..Default::default() };
    assert_eq!(options.validate().unwrap_err().code, ErrorCode::InvalidRequest);
    let serialized = serde_json::to_value(&options).unwrap();
    let decoded: RaytraceOptions = serde_json::from_value(serialized).unwrap();
    assert!(decoded.validate().is_ok(), "wire input retains no oversized source allocation");
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn raytrace_admission_validates_retained_options_before_cloning() {
    let scene = ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY };
    let mut request = request(Integrator::Raytrace);
    request.raytrace = Some(RaytraceOptions { lights: Some(Vec::with_capacity(64)), ..Default::default() });
    let original = request.raytrace.as_ref().unwrap();
    assert_eq!(original.validate().unwrap_err().code, ErrorCode::InvalidRequest);
    assert!(original.clone().validate().is_ok(), "cloning an empty Vec discards its retained capacity");
    let error = admit(&scene, &request).err().unwrap();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(request.raytrace.as_ref().unwrap().lights.as_ref().unwrap().capacity(), 64);
}

#[test]
fn options_preserve_the_eight_light_boundary_and_refuse_the_ninth() {
    let light = serde_json::json!({"direction": [0.0, 0.0, -1.0], "color": [1.0, 1.0, 1.0], "intensity": 1.0});
    let accepted: RaytraceOptions = serde_json::from_value(serde_json::json!({"lights": vec![light.clone(); 8]})).unwrap();
    assert!(accepted.validate().is_ok());
    assert!(serde_json::from_value::<RaytraceOptions>(serde_json::json!({"lights": vec![light; 9]})).is_err());
}

#[test]
fn exposure_refuses_nonfinite_and_out_of_range_values() {
    for exposure_ev in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -33.0, 33.0, -32.01, 32.01] {
        let options = RaytraceOptions { exposure_ev, ..Default::default() };
        assert_eq!(options.validate().unwrap_err().code, ErrorCode::InvalidRequest);
    }
    // CPU exposure now shares the saved/UI range; the old +/-16 refusal
    // is superseded, not a reason to change any transport oracle.
    assert_eq!(RaytraceOptions::default().exposure_ev, 0.0);
    for exposure_ev in [-32.0, -17.0, -16.0, 0.0, 16.0, 17.0, 32.0] {
        assert!(RaytraceOptions { exposure_ev, ..Default::default() }.validate().is_ok());
    }
}

#[test]
fn rejected_raytrace_submission_leaves_existing_raster_reservation_intact() {
    let host = CpuHost::cooperative();
    let scene = || ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY };
    let raster = host.submit(JobId { session: 1, serial: 1, generation: 1 }, 1, scene(), request(Integrator::Raster)).unwrap();
    let before = host.admitted_bytes();
    assert!(before > 0);
    let error = host.submit(JobId { session: 1, serial: 2, generation: 2 }, 1, scene(), request(Integrator::Raytrace)).err().unwrap();
    assert_eq!(error.code, ErrorCode::HostUnavailable);
    assert_eq!(host.admitted_bytes(), before);
    assert_eq!(raster.status().unwrap().progress.phase, Phase::Queued);
    assert!(raster.frame().unwrap().is_none());
    assert_eq!(raster.cancel().unwrap().progress.phase, Phase::Cancelled);
    // Pump only to discard cancelled queued input; it must not produce a frame.
    assert!(!host.pump(1).unwrap());
    assert_eq!(host.admitted_bytes(), 0);
    assert!(raster.frame().unwrap().is_none());
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
#[test]
fn native_integrators_share_one_host_and_retained_frame_budget() {
    let mut host = CpuHost::for_platform().unwrap();
    let scene = || ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY };
    let raster_id = JobId { session: 10, serial: 1, generation: 1 };
    let raytrace_id = JobId { session: 20, serial: 1, generation: 1 };
    let raster = host.submit(raster_id, 100, scene(), request(Integrator::Raster)).unwrap();
    let raytrace = host.submit(raytrace_id, 200, scene(), request(Integrator::Raytrace)).unwrap();
    // Deadlock watchdog only; there is no rendering-speed assertion.
    for job in [&raster, &raytrace] {
        assert_eq!(job.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase, Phase::Completed);
    }
    host.join_worker_for_test().unwrap();
    let raster_frame = raster.frame().unwrap().unwrap();
    let raytrace_frame = raytrace.frame().unwrap().unwrap();
    assert_eq!(raster_frame.info.job, raster_id);
    assert_eq!(raytrace_frame.info.job, raytrace_id);
    assert_eq!(raster_frame.info.scene_generation, 100);
    assert_eq!(raytrace_frame.info.scene_generation, 200);
    assert_eq!(raster_frame.info.executor.as_str(), "cpu-product");
    assert_eq!(raytrace_frame.info.executor.as_str(), "cpu-raytrace");
    let both = host.admitted_bytes();
    assert!(both > 0);
    drop(raster);
    drop(raytrace);
    assert_eq!(host.admitted_bytes(), both, "external frames retain both leases");
    drop(raster_frame);
    assert!(host.admitted_bytes() > 0 && host.admitted_bytes() < both);
    drop(raytrace_frame);
    assert_eq!(host.admitted_bytes(), 0);
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn cancelled_raytrace_task_never_returns_a_frame() {
    let cancel = CancelToken::default();
    let scene = ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY };
    let mut task = crate::raytrace::image::ImageTask::new(scene, request(Integrator::Raytrace), cancel.clone(), RaytraceOptions::default()).unwrap();
    cancel.cancel();
    assert_eq!(task.step(4).err().unwrap().code, ErrorCode::Cancelled);
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn completed_raytrace_task_releases_captured_mesh_before_returning_png() {
    let mesh = Arc::new(solidcraft_geom::Mesh::default());
    let captured = Arc::downgrade(&mesh);
    let scene = ProductScene {
        meshes: vec![crate::ProductMesh {
            transmission: None,
            name: "Lease probe".into(),
            mesh,
            color: [0; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        }],
        bounds: Aabb3::EMPTY,
    };
    let mut task =
        crate::raytrace::image::ImageTask::new(scene, request(Integrator::Raytrace), CancelToken::default(), RaytraceOptions::default()).unwrap();
    assert!(captured.upgrade().is_some());
    let mut completed = false;
    for _ in 0..10_000 {
        if let TaskPoll::Complete(png) = task.step(4).unwrap() {
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            completed = true;
            break;
        }
    }
    assert!(completed, "bounded empty-scene fixture must complete");
    assert!(captured.upgrade().is_none(), "the host may shrink the lease as soon as Complete is returned, even while the task remains alive");
    assert_eq!(task.progress().phase, Phase::Completed);
}
#[test]
fn emission_runtime_json_is_bounded_explicit_and_roundtrips() {
    use super::{EmissionSides, RaytraceOptions, SurfaceEmission};
    let emitter = SurfaceEmission { body_index: 0, body: "fixture".into(), color: [1.0, 0.5, 0.0], strength: 2.0, sides: EmissionSides::Front };
    let options = RaytraceOptions { emissions: Some(vec![emitter.clone()]), ..Default::default() };
    let encoded = serde_json::to_value(&options).unwrap();
    assert_eq!(serde_json::from_value::<RaytraceOptions>(encoded).unwrap(), options);
    assert!(serde_json::from_str::<RaytraceOptions>("{\"emissions\":null}").is_err());
    assert!(serde_json::from_value::<RaytraceOptions>(serde_json::json!({"emissions": vec![emitter; 33]})).is_err());
    assert!(
        serde_json::from_value::<RaytraceOptions>(
            serde_json::json!({"emissions":[{"body_index":0,"body":"x".repeat(257),"color":[1,1,1],"strength":1}]})
        )
        .is_err()
    );
    assert_eq!(serde_json::from_str::<RaytraceOptions>("{}").unwrap().emissions, None);
    assert_eq!(serde_json::from_str::<RaytraceOptions>("{\"emissions\":[]}").unwrap().emissions, Some(Vec::new()));
}

fn directional_options() -> RaytraceOptions {
    RaytraceOptions {
        directional_shadow_transmission: true,
        dielectrics: vec![DielectricOptions {
            body_index: 0, body_name: "directional glass".into(), boundary: DielectricBoundary::ThinSheet,
            ior: 1.0, absorption_per_mm: [0.0; 3], thickness_mm: 1.0,
        }],
        ..Default::default()
    }
}

#[test]
fn directional_mode_is_explicit_false_omitted_and_true_roundtrips() {
    let old_default = serde_json::json!({"reflections":null,"anisotropic_faces":[null,null],"preset":null,"exposure_ev":0.0,"shadows":true});
    let decoded: RaytraceOptions = serde_json::from_value(old_default.clone()).unwrap();
    assert!(!decoded.directional_shadow_transmission);
    assert_eq!(serde_json::to_value(&decoded).unwrap(), old_default);
    let mut explicit = old_default.clone();
    explicit["directional_shadow_transmission"] = serde_json::json!(false);
    assert_eq!(serde_json::to_value(serde_json::from_value::<RaytraceOptions>(explicit).unwrap()).unwrap(), old_default);
    let enabled = directional_options();
    enabled.validate().unwrap();
    let wire = serde_json::to_value(&enabled).unwrap();
    assert_eq!(wire["directional_shadow_transmission"], serde_json::json!(true));
    assert_eq!(serde_json::from_value::<RaytraceOptions>(wire).unwrap(), enabled);
    for bad in [serde_json::json!(null), serde_json::json!(1), serde_json::json!("true")] {
        let mut wire = old_default.clone();
        wire["directional_shadow_transmission"] = bad;
        assert!(serde_json::from_value::<RaytraceOptions>(wire).is_err());
    }
}

#[test]
fn directional_mode_refuses_incompatible_inputs_without_rewriting_legacy() {
    for case in 0..7 {
        let mut options = directional_options();
        match case {
            0 => options.shadows = false,
            1 => options.dielectrics = Vec::new(),
            2 => options.reflections = Some(ReflectionOptions { bodies: Vec::new(), max_bounces: 1, samples: 1 }),
            3 => options.anisotropic_faces[0] = Some(AnisotropicFaceOptions {
                body_index: 0, body_name: "directional glass".into(), face: 0, tangent: [1.0, 0.0, 0.0],
                orientation: AnisotropicOrientation::QuarterTurn, roughness_x: 0.2, roughness_y: 0.5,
            }),
            4 => options.environment = Some(RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
            5 => options.punctual_lights = Some(vec![PunctualLightOptions {
                position_mm: [0.0, 0.0, 10.0], color: [1.0; 3], intensity: 1.0,
                reference_distance_mm: 10.0, minimum_distance_mm: 0.001, cone: None,
            }]),
            _ => options.area_lights = Some(vec![RectangleLightOptions {
                center_mm: [0.0, 0.0, 10.0], edge_u_mm: [1.0, 0.0, 0.0], edge_v_mm: [0.0, -1.0, 0.0],
                color: [1.0; 3], intensity: 1.0,
            }]),
        }
        assert_eq!(options.validate().unwrap_err().code, ErrorCode::InvalidRequest);
        options.directional_shadow_transmission = false;
        options.validate().unwrap();
    }
    let mut empty_locals = directional_options();
    empty_locals.punctual_lights = Some(Vec::new());
    empty_locals.area_lights = Some(Vec::new());
    empty_locals.validate().unwrap();
}
