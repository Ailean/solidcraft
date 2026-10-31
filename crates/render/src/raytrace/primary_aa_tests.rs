//! Spatial AA regressions: WRITTEN NOT RUN. No historical gate is transferred.
use super::*;
use crate::job::{ReflectionBody, ReflectionMode, ReflectionOptions};
use crate::{ProductMesh, StandardView};
use solidcraft_geom::{Aabb3, Mesh};
use std::sync::Arc;

fn progressive_fixture(samples: u32, seed: u32, left: Option<bool>, exposure: f64) -> ImageTask {
    // Reuse the analytic half-plane camera/geometry, not fixed-AA output.
    let fixed = fixture(1, 1, left);
    let mut options = fixed.visibility_owner.options().clone();
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: samples, seed });
    options.exposure_ev = exposure;
    let mut request = fixed.request.clone();
    request.raytrace = Some(options.clone());
    ImageTask::new(fixed.visibility_owner.source().clone(), request, CancelToken::default(), options).unwrap()
}

type ProgressiveOutput = (Vec<(u32, Vec<u8>)>, Vec<u8>, u64);
fn progressive_complete(mut task: ImageTask, quantum: usize) -> ProgressiveOutput {
    let mut previews = Vec::new();
    for _ in 0..200_000 {
        let result = task.step_with_previews(quantum).unwrap();
        let progress = task.progress();
        assert!(progress.done <= progress.total);
        match result {
            ImagePoll::Pending(_) => {}
            ImagePoll::Preview { png, samples_per_pixel } => previews.push((samples_per_pixel, png)),
            ImagePoll::Complete(png) => return (previews, png, task.work.used()),
        }
    }
    panic!("bounded progressive fixture did not finish");
}

#[test]
fn progressive_real_png_has_stable_prefixes_quanta_and_fresh_primary_samples() {
    let (previews, final_png, work) = progressive_complete(progressive_fixture(8, 123, Some(true), 0.0), 1);
    assert_eq!(previews.iter().map(|(samples, _)| *samples).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7]);
    let (other, other_final, other_work) = progressive_complete(progressive_fixture(8, 123, Some(true), 0.0), 16_384);
    assert_eq!(other, previews);
    assert_eq!(other_final, final_png);
    assert_eq!(other_work, work, "quanta cannot reset or change the cumulative budget");
    for (samples, checkpoint) in &previews {
        let (_, prefix, _) = progressive_complete(progressive_fixture(*samples, 123, Some(true), 0.0), 3);
        assert_eq!(&prefix, checkpoint, "target SPP must not alter an earlier prefix");
    }
    let mut task = progressive_fixture(8, 123, Some(true), 0.0);
    let mut rays = Vec::new();
    for _ in 0..200_000 {
        let absent = task.pending.is_none();
        let result = task.step_with_previews(1).unwrap();
        if absent && let Some(pending) = task.pending.as_ref() {
            assert!(!pending.repeatable_primary, "jittered primary samples cannot use fixed-centre reuse");
            rays.push([pending.ray.origin().x.to_bits(), pending.ray.origin().y.to_bits()]);
        }
        if matches!(result, ImagePoll::Complete(_)) {
            break;
        }
    }
    assert_eq!(rays.len(), 8);
    rays.sort();
    rays.dedup();
    assert_eq!(rays.len(), 8, "each absolute ordinal traces a new camera sample");
}

#[test]
fn progressive_coverage_keeps_backplate_outside_exposure_and_tone_mapping() {
    for exposure in [-16.0, 0.0, 16.0] {
        let (_, background, _) = progressive_complete(progressive_fixture(5, 9, None, exposure), 4);
        assert_eq!(::image::load_from_memory(&background).unwrap().to_rgba8().get_pixel(0, 0).0, [255; 4]);
        let (_, half, _) = progressive_complete(progressive_fixture(64, 9, Some(true), exposure), 4);
        let covered = (0..64).filter(|ordinal| progressive::variates(9, 0, *ordinal, 0)[0] < 0.5).count();
        assert!(covered > 0 && covered < 64);
        let expected = encode8(1.0 - covered as f64 / 64.0);
        assert_eq!(::image::load_from_memory(&half).unwrap().to_rgba8().get_pixel(0, 0).0, [expected, expected, expected, 255]);
    }
    let (previews, _, _) = progressive_complete(progressive_fixture(5, 9, Some(true), 0.0), 1);
    assert_eq!(
        previews.iter().map(|(samples, _)| *samples).collect::<Vec<_>>(),
        [1, 2, 3, 4],
        "non-power-of-two target still has an exact final pass"
    );
}

#[test]
fn progressive_resolves_the_mean_radiance_once_instead_of_averaging_display_pixels() {
    let left = fixture(1, 1, Some(true));
    let right = fixture(1, 1, Some(false));
    let mut source = left.visibility_owner.source().clone();
    let mut other = right.visibility_owner.source().meshes[0].clone();
    other.name = "Bright half".into();
    source.bounds = source.bounds.union(&other.mesh.bounds());
    source.meshes.push(other);
    let mut options = left.visibility_owner.options().clone();
    options.reflections = None;
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 64, seed: 9 });
    options.exposure_ev = 1.0;
    options.emissions = Some(vec![
        job::SurfaceEmission { body_index: 0, body: "Black half".into(), color: [1.0, 0.5, 0.25], strength: 1.0, sides: job::EmissionSides::Both },
        job::SurfaceEmission { body_index: 1, body: "Bright half".into(), color: [1.0, 0.5, 0.25], strength: 9.0, sides: job::EmissionSides::Both },
    ]);
    let request = ImageRequest { raytrace: Some(options.clone()), ..left.request.clone() };
    let task = ImageTask::new(source, request, CancelToken::default(), options).unwrap();
    let (_, png, _) = progressive_complete(task, 4);
    let left_count = (0..64).filter(|ordinal| progressive::variates(9, 0, *ordinal, 0)[0] < 0.5).count();
    let mean = (left_count as f64 + (64 - left_count) as f64 * 9.0) / 64.0;
    let expected = [1.0, 0.5, 0.25].map(|color| {
        let exposed = mean * color * 2.0;
        encode8(exposed / (1.0 + exposed))
    });
    let actual = ::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0;
    assert_eq!(actual, [expected[0], expected[1], expected[2], 255]);
    let fraction = left_count as f64 / 64.0;
    let prematurely_mapped = encode8(fraction * (2.0 / 3.0) + (1.0 - fraction) * (18.0 / 19.0));
    assert_ne!(actual[0], prematurely_mapped, "each sample must remain linear until the whole pass resolves");
}

#[test]
fn progressive_film_and_resolve_share_cancellation_and_one_cumulative_work_budget() {
    let mut task = progressive_fixture(4, 0, Some(true), 0.0);
    task.work = WorkBudget::new(1).unwrap();
    assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
    assert_eq!(task.step_with_previews(1).err().unwrap().code, ErrorCode::LimitExceeded);
    for during_resolve in [false, true] {
        let mut task = progressive_fixture(4, 0, Some(true), 0.0);
        if during_resolve {
            for _ in 0..200_000 {
                if task.progressive.as_ref().is_some_and(|p| p.resolving) {
                    break;
                }
                assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            }
            assert!(task.progressive.as_ref().unwrap().resolving);
        }
        task.cancel.cancel();
        assert_eq!(task.step_with_previews(4).err().unwrap().code, ErrorCode::Cancelled);
        assert!(!task.done);
    }
}

fn fixture(samples: u8, r: u8, left: Option<bool>) -> ImageTask {
    let mut camera = crate::Camera { distance: 10.0, fov: 0.0, ..Default::default() };
    camera.set_view(StandardView::Top);
    let meshes = if let Some(left) = left {
        let (x0, x1) = if left { (0.0, 0.5) } else { (0.5, 1.0) };
        let point = |x, y| {
            let (o, _) = camera.ray(x, y, 1.0, 1.0);
            Vec3::new(o.x, o.y, 0.0)
        };
        vec![ProductMesh {
            transmission: None,
            name: "Black half".into(),
            mesh: Arc::new(Mesh {
                positions: vec![point(x0, 0.0), point(x1, 0.0), point(x1, 1.0), point(x0, 1.0)],
                normals: vec![Vec3::Z; 4],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                tri_face: vec![0, 0],
                ..Default::default()
            }),
            color: [0; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::ZERO, 1.0, 1.0, 1.5).unwrap()),
            faces: Vec::new(),
        }]
    } else {
        Vec::new()
    };
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let options = RaytraceOptions {
        primary_samples: samples,
        lights: Some(Vec::new()),
        shadows: false,
        reflections: left.map(|_| ReflectionOptions {
            bodies: vec![ReflectionBody { body: "Black half".into(), mode: ReflectionMode::RoughSpecular }],
            samples: r,
            max_bounces: 2,
        }),
        ..Default::default()
    };
    let request = ImageRequest {
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        environment: None,
        textures: None,
        integrator: job::Integrator::Raytrace,
        raytrace: Some(options.clone()),
        camera,
        width: 1,
        height: 1,
        background: [255; 3],
        transparent_background: false,
    };
    ImageTask::new(ProductScene { meshes, bounds }, request, CancelToken::default(), options).unwrap()
}
fn until_emit(task: &mut ImageTask) -> usize {
    let mut primaries = 0;
    let mut previous_primary = None;
    for _ in 0..200_000 {
        let absent = task.pending.is_none();
        let before = task.work.used();
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        assert!(task.work.used() - before <= 4);
        if absent && task.pending.as_ref().is_some_and(|p| p.bounce == 0) {
            primaries += 1;
            let pending = task.pending.as_ref().unwrap();
            assert!(pending.repeatable_primary, "fixed generator must certify only intra-spatial R invariance");
            let o = pending.ray.origin();
            let d = pending.ray.direction();
            let ray = [o.x.to_bits(), o.y.to_bits(), o.z.to_bits(), d.x.to_bits(), d.y.to_bits(), d.z.to_bits()];
            if let Some((pixel, spatial, previous_ray)) = previous_primary {
                if (pixel, spatial) == (task.pixel, task.spatial) {
                    assert_eq!(ray, previous_ray, "fixed primary ray changed between R attempts");
                } else {
                    assert_ne!(ray, previous_ray, "distinct spatial samples must not reuse their primary ray");
                }
            }
            previous_primary = Some((task.pixel, task.spatial, ray));
        }
        if let Some(p) = task.pending.as_ref() {
            if p.bounce != 0 {
                assert!(!p.repeatable_primary, "secondary paths must revoke primary reuse");
            } else if let Some(hit) = p.hit {
                let shared = PreparedScene { bvh: task.bvh.as_ref().unwrap(), primitives: &task.looks, source: task.visibility_owner.source() }
                    .resolve_hit(p.ray, hit)
                    .unwrap();
                if task.visibility_owner.options().reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|b| b.body == shared.body_name)) {
                    assert!(!p.repeatable_primary, "selected primary hit must revoke reuse even when its reflection is null");
                }
            }
        }
        assert!(task.pixels.is_empty(), "early primary hit/miss emitted pixel");
        if task.progress().phase == Phase::Running {
            assert_eq!(task.progress().done, 0, "subsamples counted as completed pixels");
        }
        if task.emit_spatial_pixel {
            return primaries;
        }
    }
    panic!("bounded AA fixture did not reach emission");
}
fn complete(mut task: ImageTask, quantum: usize) -> (Vec<u8>, u64) {
    for _ in 0..200_000 {
        if let TaskPoll::Complete(png) = task.step(quantum).unwrap() {
            return (png, task.work.used());
        }
    }
    panic!("bounded AA fixture did not finish");
}

#[test]
fn transient_options_are_strict_default_one_and_do_not_serialize_a_new_default_field() {
    let omitted: RaytraceOptions = serde_json::from_str("{}").unwrap();
    let explicit: RaytraceOptions = serde_json::from_str(r#"{"primary_samples":1}"#).unwrap();
    assert_eq!(omitted, explicit);
    assert_eq!(omitted.primary_samples, 1);
    assert!(!serde_json::to_value(&omitted).unwrap().as_object().unwrap().contains_key("primary_samples"));
    for n in [0, 2, 8, 17, 255] {
        assert!(RaytraceOptions { primary_samples: n, ..Default::default() }.validate().is_err());
    }
    for raw in [
        r#"{"primary_samples":-1}"#,
        r#"{"primary_samples":1.5}"#,
        r#"{"primary_samples":null}"#,
        r#"{"primary_samples":"4"}"#,
        r#"{"primary_samples":4,"unknown":0}"#,
    ] {
        assert!(serde_json::from_str::<RaytraceOptions>(raw).is_err());
    }
}

#[test]
fn fixed_grids_have_independent_analytic_halfplane_and_triangle_coverage() {
    assert_eq!(primary_offset(1, 0).unwrap(), [0.5, 0.5]);
    assert_eq!(primary_offset(4, 0).unwrap(), [0.25, 0.25]);
    assert_eq!(primary_offset(4, 3).unwrap(), [0.75, 0.75]);
    for (s, half, triangle) in [(4, 2, 1), (16, 8, 3)] {
        let points: Vec<_> = (0..s).map(|i| primary_offset(s, i).unwrap()).collect();
        assert!(points.iter().flatten().all(|v| *v > 0.0 && *v < 1.0));
        assert_eq!(points.iter().filter(|p| p[0] < 0.5).count(), half);
        assert_eq!(points.iter().filter(|p| p[0] + p[1] < 5.0 / 8.0).count(), triangle);
        assert!(primary_offset(s, s).is_err());
    }
    assert!(primary_offset(8, 0).is_err());
    assert!(spatial_key(u64::MAX, 0, 16).is_err());
    assert_ne!(spatial_key(7, 0, 4).unwrap(), spatial_key(7, 1, 4).unwrap());
}

#[test]
fn compositor_keeps_conditional_scene_mean_then_display_linear_coverage() {
    // White L=1, Reinhard=1/2; half coverage over black gives1/4 display-linear.
    assert_eq!(composite_spatial(Vec3::new(0.5, 0.5, 0.5), 2, 4, [0; 3], 0.0).unwrap(), [encode8(0.25), encode8(0.25), encode8(0.25), 255]);
    assert_ne!(encode8(0.25), encode8(1.0 / 3.0));
    // Full coverage of L=0 and L=2 usesT(mean)=1/2, not mean(T)=1/3.
    assert_eq!(composite_spatial(Vec3::new(1.0, 1.0, 1.0), 4, 4, [255; 3], 0.0).unwrap(), [encode8(0.5), encode8(0.5), encode8(0.5), 255]);
    for ev in [-16.0, 0.0, 16.0] {
        assert_eq!(composite_spatial(Vec3::ZERO, 0, 16, [13, 27, 41], ev).unwrap(), [13, 27, 41, 255]);
    }
    let gray = srgb8_to_linear(128);
    assert_eq!(composite_spatial(Vec3::ZERO, 2, 4, [128; 3], 0.0).unwrap(), [encode8(gray * 0.5), encode8(gray * 0.5), encode8(gray * 0.5), 255]);
    assert!(composite_spatial(Vec3::ZERO, 5, 4, [0; 3], 0.0).is_err());
    assert!(composite_spatial(Vec3::X, 0, 4, [0; 3], 0.0).is_err());
}

#[test]
fn real_hit_first_and_miss_first_silhouettes_keep_all_attempts_and_null_coverage() {
    for s in [4, 16] {
        for r in [1, 4, 16, 64] {
            for left in [false, true] {
                let mut task = fixture(s, r, Some(left));
                let primaries = until_emit(&mut task);
                assert_eq!(primaries, usize::from(s) * usize::from(r));
                assert_eq!(task.primary_coverage, u16::from(s) * u16::from(r) / 2);
                let before = task.work.used();
                task.step(1).unwrap();
                assert_eq!(task.work.used(), before + 1);
                assert_eq!(task.pixels, [encode8(0.5), encode8(0.5), encode8(0.5), 255]);
                assert_eq!(task.pixel, 1);
            }
        }
    }
}

#[test]
fn miss_only_aa_retains_exact_background_and_quantum_independent_png() {
    for s in [4, 16] {
        let mut task = fixture(s, 1, None);
        assert_eq!(until_emit(&mut task), usize::from(s));
        assert_eq!(task.primary_coverage, 0);
        let small = complete(task, 1);
        let large = complete(fixture(s, 1, None), 16384);
        assert_eq!(small, large);
        let image = ::image::load_from_memory(&small.0).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [255; 4]);
    }
}

#[test]
fn spatial_finalization_and_emission_observe_shared_work_and_cancellation() {
    for emit in [false, true] {
        for cancel in [false, true] {
            let mut task = fixture(16, 4, Some(false));
            for _ in 0..200_000 {
                if (emit && task.emit_spatial_pixel) || (!emit && task.finish_spatial) {
                    break;
                }
                task.step(1).unwrap();
            }
            assert!(if emit { task.emit_spatial_pixel } else { task.finish_spatial });
            let used = task.work.used();
            if cancel {
                task.cancel.cancel();
            } else {
                task.work.limit = used;
            }
            let code = task.step(1).err().unwrap().code;
            assert_eq!(code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
            assert_eq!(task.work.used(), used);
            assert!(!task.done && task.pixels.is_empty());
        }
    }
}

#[test]
fn legacy_single_primary_ignores_aa_state_and_preserves_default_bytes_work_and_order() {
    for left in [None, Some(true), Some(false)] {
        for r in [1, 4, 16, 64] {
            let control = fixture(1, r, left);
            let mut isolated = fixture(1, r, left);
            isolated.spatial = 16;
            isolated.primary_coverage = 1024;
            isolated.spatial_radiance = Vec3::new(f64::NAN, 0.0, 0.0);
            isolated.finish_spatial = true;
            isolated.emit_spatial_pixel = true;
            assert_eq!(complete(control, 1), complete(isolated, 16384));
        }
    }
    assert!(std::mem::size_of::<ImageTask>() <= 4096);
    assert_eq!(crate::raytrace::MAX_WORK, 1_000_000_000);
    assert_eq!(job::image::MAX_JOB_BYTES, 384 * 1024 * 1024);
    let legacy = fixture(1, 4, Some(false));
    let aa = fixture(16, 4, Some(false));
    assert_eq!(legacy.admission.estimated_total_bytes, aa.admission.estimated_total_bytes);
}

#[test]
fn full_coverage_black_plane_never_leaks_display_background() {
    for samples in [4, 16] {
        let initial = fixture(samples, 16, Some(true));
        let top = initial.request.camera.ray(1.0, 0.0, 1.0, 1.0).0;
        let bottom = initial.request.camera.ray(1.0, 1.0, 1.0, 1.0).0;
        let (mut fixture_scene, fixture_options) = initial.visibility_owner.into_capture();
        let mesh = Arc::get_mut(&mut fixture_scene.meshes[0].mesh).unwrap();
        mesh.positions[1] = Vec3::new(top.x, top.y, 0.0);
        mesh.positions[2] = Vec3::new(bottom.x, bottom.y, 0.0);
        fixture_scene.bounds = mesh.bounds();
        let mut task = ImageTask::new(fixture_scene, initial.request, initial.cancel, fixture_options).unwrap();
        assert_eq!(until_emit(&mut task), usize::from(samples) * 16);
        assert_eq!(task.primary_coverage, u16::from(samples) * 16);
        task.step(1).unwrap();
        assert_eq!(task.pixels, [0, 0, 0, 255]);
    }
}

#[test]
fn later_spatial_attempts_do_not_reset_work_and_never_publish_when_interrupted() {
    for cancel in [false, true] {
        let mut task = fixture(16, 64, Some(true));
        for _ in 0..200_000 {
            if task.spatial >= 1 && task.sample >= 1 {
                break;
            }
            task.step(1).unwrap();
        }
        assert!(task.spatial >= 1 && task.sample >= 1);
        let used = task.work.used();
        assert!(used > 64, "no cumulative work across spatial samples");
        if cancel {
            task.cancel.cancel();
        } else {
            task.work.limit = used;
        }
        assert_eq!(task.step(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
        assert_eq!(task.work.used(), used);
        assert!(!task.done && task.pixels.is_empty());
    }
}

#[test]
fn actual_colored_lit_partial_coverage_wires_positive_radiance_exposure_and_background() {
    for samples in [4, 16] {
        for left in [false, true] {
            let mut initial = fixture(samples, 1, Some(left));
            let (mut fixture_scene, mut fixture_options) = initial.visibility_owner.into_capture();
            fixture_scene.meshes[0].surface = Some(OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 1.0, 1.0, 1.5).unwrap());
            fixture_options.reflections = None;
            fixture_options.lights = Some(vec![job::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [0.25, 0.5, 1.0], intensity: 2.0 }]);
            fixture_options.exposure_ev = 1.0;
            initial.request.background = [13, 73, 139];
            initial.request.raytrace = Some(fixture_options.clone());
            let mut task = ImageTask::new(fixture_scene, initial.request, initial.cancel, fixture_options).unwrap();
            assert_eq!(until_emit(&mut task), usize::from(samples));
            assert_eq!(task.primary_coverage, u16::from(samples) / 2);
            // At normal incidence, unit-alpha white conductor: D=1/pi,
            // G1=F=1, f=1/(4pi), incoming radiance=2*colored light.
            let linear = [0.5, 1.0, 2.0].map(|v| v / (4.0 * std::f64::consts::PI));
            let expected_p = Vec3::new(linear[0], linear[1], linear[2]) * 0.5;
            assert!((task.spatial_radiance - expected_p).len() < 1e-12);
            let expected: [u8; 3] = std::array::from_fn(|i| {
                let exposed = linear[i] * 2.0;
                let display = 0.5 * exposed / (1.0 + exposed) + 0.5 * srgb8_to_linear([13, 73, 139][i]);
                encode8(display)
            });
            task.step(1).unwrap();
            assert_eq!(task.pixels, [expected[0], expected[1], expected[2], 255]);
        }
    }
}

#[test]
fn workstation_progressive_streams_every_historical_2048_4096_checkpoint_and_checks_the_new_cap() {
    let fixed = fixture(1, 1, Some(true));
    let cap = job::CpuBudget::Workstation.limits().max_samples_per_pixel;
    assert_eq!(cap, 1_048_576);
    for samples in [2048, 4096] {
        let mut options = fixed.visibility_owner.options().clone();
        options.budget = job::CpuBudget::Workstation;
        options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: samples, seed: 123 });
        let request = ImageRequest { raytrace: Some(options.clone()), ..fixed.request.clone() };
        let mut task = ImageTask::new(fixed.visibility_owner.source().clone(), request, CancelToken::default(), options).unwrap();
        let mut observed = Vec::new();
        let mut complete = false;
        for _ in 0..200_000 {
            match task.step_with_previews(16_384).unwrap() {
                ImagePoll::Pending(_) => {}
                ImagePoll::Preview { png, samples_per_pixel } => {
                    observed.push(samples_per_pixel);
                    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().dimensions(), (1, 1));
                }
                ImagePoll::Complete(png) => {
                    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().dimensions(), (1, 1));
                    complete = true;
                    break;
                }
            }
        }
        assert!(complete);
        assert_eq!(observed, (1..samples).collect::<Vec<_>>());
    }
    assert_eq!(composite_spatial(Vec3::ZERO, 0, 4097, [0; 3], 0.0).unwrap(), [0, 0, 0, 255]);
    assert!(composite_spatial(Vec3::ZERO, 0, 1_048_577, [0; 3], 0.0).is_err());
}

#[test]
fn area_light_points_include_spatial_and_film_sample_identity() {
    let light = super::super::area::RectangleLight::new(Vec3::Z * 10.0, Vec3::X * 4.0, -Vec3::Y * 4.0, Vec3::new(1.0, 1.0, 1.0), 1.0).unwrap();
    let mut keys = std::collections::BTreeSet::new();
    let mut points = std::collections::BTreeSet::new();
    for film in [0, 1, 1023, 2047, 4095] {
        for spatial in 0..16 {
            let key = area_sample_key(41, spatial, 16, film).unwrap();
            assert!(keys.insert(key));
            let point = light.sample(Vec3::ZERO, key, 2, 0, 4, 73).unwrap().unwrap().position();
            assert!(points.insert((point.x.to_bits(), point.y.to_bits())));
            assert_eq!(light.sample(Vec3::ZERO, key, 2, 0, 4, 73).unwrap().unwrap().position(), point);
        }
    }
    assert_eq!(area_sample_key(41, 0, 1, 0).unwrap(), 41);
    assert_eq!(area_sample_key(41, 0, 1, 4096).unwrap(), (1_u64 << 63) | (41_u64 << 36) | (4096_u64 << 16));
    assert!(area_sample_key(41, 0, 1, 1_048_576).is_err());
    assert!(area_sample_key(41, 4, 4, 0).is_err());
    assert!(area_sample_key(u64::MAX, 0, 4, 0).is_err());
}

// Original receiver and rectangular source; no borrowed model or recorded image.
fn progressive_area_fixture(area_samples: u32, samples: u32, seed: u32, shadows: bool) -> ImageTask {
    let original = fixture(1, 1, Some(true));
    let mut scene = original.visibility_owner.source().clone();
    scene.meshes[0].mesh = Arc::new(Mesh {
        positions: vec![Vec3::new(-100.0, -100.0, 0.0), Vec3::new(100.0, -100.0, 0.0), Vec3::new(100.0, 100.0, 0.0), Vec3::new(-100.0, 100.0, 0.0)],
        normals: vec![Vec3::Z; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0; 2],
        ..Default::default()
    });
    scene.meshes[0].surface = Some(OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 0.0, 1.0, 1.0).unwrap());
    scene.bounds = scene.meshes[0].mesh.bounds();
    let options = RaytraceOptions {
        progressive: Some(job::CpuProgressiveOptions { samples_per_pixel: samples, seed: 97 }),
        lights: Some(Vec::new()),
        area_lights: Some(vec![job::RectangleLightOptions {
            center_mm: [0.0, 0.0, 10.0],
            edge_u_mm: [2.0, 0.0, 0.0],
            edge_v_mm: [0.0, -2.0, 0.0],
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
        }]),
        area_samples: Some(area_samples),
        seed: Some(seed),
        shadows,
        ..Default::default()
    };
    let request = ImageRequest { raytrace: Some(options.clone()), ..original.request.clone() };
    ImageTask::new(scene, request, CancelToken::default(), options).unwrap()
}

fn progressive_area_rebuild(source: &ImageTask, options: RaytraceOptions) -> ImageTask {
    let request = ImageRequest { raytrace: Some(options.clone()), ..source.request.clone() };
    ImageTask::new(source.visibility_owner.source().clone(), request, CancelToken::default(), options).unwrap()
}

#[test]
fn progressive_area_transport_uses_absolute_film_ordinal_and_seed_for_all_sample_counts() {
    use super::super::area::RectangleLight;
    for count in [1, 4, 16, 64] {
        for seed in [7, 91] {
            let mut task = progressive_area_fixture(count, 2, seed, false);
            let light = RectangleLight::new(Vec3::Z * 10.0, Vec3::X * 2.0, -Vec3::Y * 2.0, Vec3::new(1.0, 0.5, 0.25), 2.0).unwrap();
            let mut inspected = 0;
            let mut first_positions = Vec::new();
            let mut complete = false;
            for _ in 0..200_000 {
                let ordinal = task.progressive.as_ref().unwrap().film.samples();
                let before =
                    task.pending.as_ref().filter(|p| p.hit.is_some() && p.area_light == 0 && p.area_index < count).map(|p| (p.area_index, p.ray));
                let poll = task.step_with_previews(1).unwrap();
                if let Some((index, ray)) = before
                    && let Some(pending) = task.pending.as_ref()
                    && pending.area_light == 0
                    && pending.area_index == index + 1
                {
                    let point = ray.origin() + ray.direction() * (-ray.origin().z / ray.direction().z);
                    let key = area_sample_key(0, 0, 1, ordinal).unwrap();
                    let sampled = light.sample(point, key, 0, index, count, seed).unwrap().unwrap();
                    // Index-matched nonmetallic white receiver is exactly Lambertian.
                    // Check the actual consumer, independently of its BRDF and area weight.
                    let delta = sampled.position() - point;
                    let distance_squared = delta.dot(delta);
                    let factor = 4.0 * delta.z * delta.z / (distance_squared * distance_squared * f64::from(count) * std::f64::consts::PI);
                    let expected = Vec3::new(2.0, 1.0, 0.5) * factor;
                    assert!((pending.area_value - expected).len() < 1e-12, "N={count}, seed={seed}, film={ordinal}, index={index}");
                    if index == 0 {
                        first_positions.push(sampled.position());
                    }
                    inspected += 1;
                }
                if matches!(poll, ImagePoll::Complete(_)) {
                    complete = true;
                    break;
                }
            }
            assert!(complete);
            assert_eq!(inspected, 2 * count);
            assert_eq!(first_positions.len(), 2);
            if count == 1 {
                assert_eq!(first_positions[0], first_positions[1], "one sample intentionally stays at the emitter centre");
            } else {
                assert_ne!(first_positions[0], first_positions[1], "later film passes must not reuse the first emitter point");
            }
        }
    }
}

#[test]
fn progressive_area_center_sample_resolves_the_analytic_linear_mean_once() {
    let task = progressive_area_fixture(1, 3, 7, false);
    let camera = task.request.camera;
    let mut sum = Vec3::ZERO;
    for ordinal in 0..3 {
        let [u, v] = progressive::variates(97, 0, ordinal, 0);
        let (origin, direction) = camera.ray(u, v, 1.0, 1.0);
        let point = origin + direction * (-origin.z / direction.z);
        let delta = Vec3::Z * 10.0 - point;
        let r2 = delta.dot(delta);
        sum += Vec3::new(2.0, 1.0, 0.5) * (4.0 * delta.z * delta.z / (r2 * r2 * std::f64::consts::PI));
    }
    let mean = sum / 3.0;
    let expected = [mean.x, mean.y, mean.z].map(|value| encode8(value / (1.0 + value)));
    let (_, png, _) = progressive_complete(task, 1);
    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, [expected[0], expected[1], expected[2], 255]);
}

#[test]
fn progressive_area_checkpoints_and_work_are_independent_of_target_and_quantum() {
    for count in [1, 4, 16, 64] {
        let small = progressive_complete(progressive_area_fixture(count, 5, 23, true), 1);
        let large = progressive_complete(progressive_area_fixture(count, 5, 23, true), 16_384);
        assert_eq!(small, large, "area continuation must share the same cumulative work across quanta");
        assert_eq!(small.0.iter().map(|(samples, _)| *samples).collect::<Vec<_>>(), [1, 2, 3, 4]);
        for (samples, checkpoint) in &small.0 {
            let (_, final_png, _) = progressive_complete(progressive_area_fixture(count, *samples, 23, true), 3);
            assert_eq!(&final_png, checkpoint);
        }
    }
}

#[test]
fn progressive_led_lowering_keeps_rectangle_power_sampling_and_final_bytes() {
    let strip = job::led_strip::LedStripOptions {
        center_mm: [0.0, 0.0, 10.0],
        length_mm: 2.0,
        width_mm: 2.0,
        length_axis: [1.0, 0.0, 0.0],
        width_axis: [0.0, -1.0, 0.0],
        color: [1.0, 0.5, 0.25],
        intensity: 2.0,
    };
    for count in [1, 4, 16, 64] {
        let direct = progressive_area_fixture(count, 3, 7, true);
        let mut options = direct.visibility_owner.options().clone();
        options.area_lights = None;
        options.area_samples = None;
        options.seed = None;
        strip.append_to(&mut options, count).unwrap();
        options.seed = direct.visibility_owner.options().seed;
        assert_eq!(options, *direct.visibility_owner.options());
        let lowered = progressive_area_rebuild(&direct, options);
        assert_eq!(progressive_complete(direct, 1), progressive_complete(lowered, 7));
    }
}

#[test]
fn progressive_area_admission_bills_source_capacity_and_reuses_bounded_scratch() {
    let source = progressive_area_fixture(1, 3, 7, true);
    let mut empty = source.visibility_owner.options().clone();
    empty.area_lights = None;
    empty.area_samples = None;
    empty.seed = None;
    let baseline = progressive_area_rebuild(&source, empty);
    let slots = source.visibility_owner.options().area_lights.as_ref().unwrap().capacity();
    let bytes = (slots * std::mem::size_of::<job::RectangleLightOptions>()) as u64;
    assert_eq!(source.visibility_owner.options().retained_bytes().unwrap(), bytes);
    assert_eq!(source.admission.estimated_total_bytes - baseline.admission.estimated_total_bytes, bytes);
    assert_eq!(source.admission.preparation_bytes - baseline.admission.preparation_bytes, bytes);
    for count in [4, 16, 64] {
        let more = progressive_area_fixture(count, 3, 7, true);
        assert_eq!(
            more.admission.estimated_total_bytes, source.admission.estimated_total_bytes,
            "area samples reuse the same continuation, not an N-sized buffer"
        );
    }
    let mut over = source.request.clone();
    over.width = 1025;
    over.height = 1024;
    assert_eq!(admit(source.visibility_owner.source(), &over, source.visibility_owner.options()).err().unwrap().code, ErrorCode::LimitExceeded);
}

#[test]
fn progressive_area_shadow_cancellation_and_checkpoint_do_not_reset_work() {
    let mut task = progressive_area_fixture(16, 3, 7, true);
    let mut pending_shadow = false;
    for _ in 0..200_000 {
        if task.pending.as_ref().is_some_and(|p| p.area_shadow) {
            pending_shadow = true;
            break;
        }
        assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
    }
    assert!(pending_shadow);
    let used = task.work.used();
    task.cancel.cancel();
    assert_eq!(task.step_with_previews(1).err().unwrap().code, ErrorCode::Cancelled);
    assert_eq!(task.work.used(), used);
    assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
    assert!(!task.done);

    let mut reference = progressive_area_fixture(16, 3, 7, true);
    let mut checkpoint_work = None;
    for _ in 0..200_000 {
        if let ImagePoll::Preview { samples_per_pixel, .. } = reference.step_with_previews(1).unwrap() {
            assert_eq!(samples_per_pixel, 1);
            checkpoint_work = Some(reference.work.used());
            break;
        }
    }
    let limit = checkpoint_work.unwrap();
    let mut bounded = progressive_area_fixture(16, 3, 7, true);
    bounded.work = WorkBudget::new(limit).unwrap();
    let mut observed_checkpoint = false;
    for _ in 0..200_000 {
        match bounded.step_with_previews(1) {
            Ok(ImagePoll::Pending(_)) => {}
            Ok(ImagePoll::Preview { samples_per_pixel, .. }) => {
                assert!(!observed_checkpoint);
                assert_eq!(samples_per_pixel, 1);
                observed_checkpoint = true;
                assert_eq!(bounded.work.used(), limit);
            }
            Ok(ImagePoll::Complete(_)) => panic!("work exhausted after the checkpoint must not publish a final"),
            Err(error) => {
                assert_eq!(error.code, ErrorCode::LimitExceeded);
                assert!(observed_checkpoint);
                assert_eq!(bounded.work.used(), limit);
                assert!(!bounded.done);
                return;
            }
        }
    }
    panic!("area work ceiling was reset or the fixture did not finish");
}

fn partial_four_pixel_source(samples: u32, seed: u32) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let mut camera = crate::Camera { distance: 10.0, fov: 0.0, ..Default::default() };
    camera.set_view(StandardView::Top);
    let body = |name: &str, x0: f64, x1: f64| {
        let point = |x, y| {
            let (origin, _) = camera.ray(x, y, 4.0, 1.0);
            Vec3::new(origin.x, origin.y, 0.0)
        };
        ProductMesh {
            transmission: None,
            name: name.into(),
            mesh: Arc::new(Mesh {
                positions: vec![point(x0, 0.0), point(x1, 0.0), point(x1, 1.0), point(x0, 1.0)],
                normals: vec![Vec3::Z; 4],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                tri_face: vec![0; 2],
                ..Default::default()
            }),
            color: [0; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::ZERO, 0.0, 1.0, 1.5).unwrap()),
            faces: Vec::new(),
        }
    };
    let meshes = vec![body("Partial red", 0.0, 1.0), body("Partial black", 2.0, 3.0)];
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |bounds, body| bounds.union(&body.mesh.bounds()));
    let options = RaytraceOptions {
        progressive: Some(job::CpuProgressiveOptions { samples_per_pixel: samples, seed }),
        lights: Some(Vec::new()),
        shadows: false,
        emissions: Some(vec![job::SurfaceEmission {
            body_index: 0,
            body: "Partial red".into(),
            color: [1.0, 0.0, 0.0],
            strength: 2.0,
            sides: job::EmissionSides::Both,
        }]),
        ..Default::default()
    };
    let request = ImageRequest {
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        environment: None,
        textures: None,
        integrator: job::Integrator::Raytrace,
        raytrace: Some(options.clone()),
        camera,
        width: 4,
        height: 1,
        background: [10, 20, 30],
        transparent_background: false,
    };
    (ProductScene { meshes, bounds }, request, options)
}

fn partial_four_pixel_task(samples: u32, seed: u32, partial: bool) -> ImageTask {
    let (scene, request, options) = partial_four_pixel_source(samples, seed);
    if partial {
        let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
        ImageTask::new_with_environment_and_nested_and_partial(scene, request, CancelToken::default(), options, None, false, plan).unwrap()
    } else {
        ImageTask::new(scene, request, CancelToken::default(), options).unwrap()
    }
}

type PartialRayLog = Vec<(usize, u32, [u64; 6])>;
fn record_partial_primary(task: &ImageTask, was_empty: bool, log: &mut PartialRayLog) {
    if was_empty && let Some(pending) = task.pending.as_ref() {
        let origin = pending.ray.origin();
        let direction = pending.ray.direction();
        let ordinal = task.progressive.as_ref().unwrap().film.samples();
        log.push((
            task.pixel,
            ordinal,
            [origin.x.to_bits(), origin.y.to_bits(), origin.z.to_bits(), direction.x.to_bits(), direction.y.to_bits(), direction.z.to_bits()],
        ));
    }
}
fn reach_partial_frontier(task: &mut ImageTask, passes: u32, prefix: usize, rays: &mut PartialRayLog) {
    for _ in 0..200_000 {
        if let Some(progressive) = &task.progressive
            && progressive.film.ready()
            && progressive.film.partial_frontier().unwrap().completed_passes == passes
            && progressive.film.partial_frontier().unwrap().updated_prefix == prefix
            && task.pending.is_none()
        {
            return;
        }
        let was_empty = task.pending.is_none();
        assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
        record_partial_primary(task, was_empty, rays);
    }
    panic!("real task did not reach requested committed frontier");
}
fn finish_partial_snapshot(task: &mut ImageTask) -> (Vec<u8>, Vec<u32>, u32, u64) {
    let paid = task.work.used();
    for _ in 0..200_000 {
        let poll = task.step_with_partial_previews(1).unwrap();
        assert_eq!(task.work.used(), paid, "prepaid resolve/encode must not debit the job twice");
        match poll {
            PartialImagePoll::Pending(_) => {}
            PartialImagePoll::Preview { png, covered, completed_passes, updated_prefix } => return (png, covered, completed_passes, updated_prefix),
            PartialImagePoll::Complete(_) => panic!("partial snapshot must not become a final frame"),
        }
    }
    panic!("bounded partial snapshot did not finish");
}
fn finish_partial_final(task: &mut ImageTask, partial: bool, rays: &mut PartialRayLog) -> (Vec<u8>, Vec<[u64; 3]>) {
    let target = task.visibility_owner.options().progressive.unwrap().samples_per_pixel;
    let mut bits = None;
    for _ in 0..1_000_000 {
        if bits.is_none()
            && let Some(progressive) = &task.progressive
            && progressive.film.ready()
            && progressive.film.samples() == target
        {
            bits = Some(
                (0..task.request.width as usize * task.request.height as usize)
                    .map(|index| progressive.film.pixel(index).unwrap().0.map(f64::to_bits))
                    .collect(),
            );
        }
        let was_empty = task.pending.is_none();
        let complete = if partial {
            match task.step_with_partial_previews(1).unwrap() {
                PartialImagePoll::Pending(_) => None,
                PartialImagePoll::Complete(png) => Some(png),
                PartialImagePoll::Preview { .. } => panic!("producer created an unrequested partial snapshot"),
            }
        } else {
            match task.step_with_previews(1).unwrap() {
                ImagePoll::Pending(_) | ImagePoll::Preview { .. } => None,
                ImagePoll::Complete(png) => Some(png),
            }
        };
        record_partial_primary(task, was_empty, rays);
        if let Some(png) = complete {
            return (png, bits.expect("uniform target film must exist before final resolution"));
        }
    }
    panic!("partial/control final did not finish");
}

#[test]
fn partial_real_film_png_distinguishes_unknown_miss_hit_and_known_black() {
    let mut task = partial_four_pixel_task(5, 123, true);
    let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
    reach_partial_frontier(&mut task, 0, 2, &mut Vec::new());
    assert!(task.partial_preview_ready(plan).unwrap());
    let before = task.work.used();
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    assert_eq!(task.work.used(), before + plan.snapshot_work().unwrap());
    let (png, covered, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!((passes, prefix, task.pixel), (0, 2, 2));
    assert_eq!(covered, [1, 0, 0, 0]);
    let image = ::image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (4, 1));
    assert_eq!(image.get_pixel(0, 0).0, [213, 0, 0, 255], "linear red2 -> Reinhard2/3 -> sRGB213");
    assert_eq!(image.get_pixel(1, 0).0, [10, 20, 30, 255], "observed miss is the captured backplate");
    assert_eq!(image.get_pixel(2, 0).0, [0, 0, 0, 0]);
    assert_eq!(image.get_pixel(3, 0).0, [0, 0, 0, 0]);

    reach_partial_frontier(&mut task, 1, 0, &mut Vec::new());
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    let (png, covered, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!((passes, prefix, task.pixel), (1, 0, 4), "snapshot retains the sentinel until the normal charged transition");
    assert_eq!(covered, [1, 0, 1, 0]);
    let image = ::image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(2, 0).0, [0, 0, 0, 255], "known black is opaque, not Unknown");
    assert_eq!(image.get_pixel(3, 0).0, [10, 20, 30, 255]);
}

#[test]
fn partial_snapshots_preserve_interior_cursor_absolute_rays_and_final_bits() {
    for seed in [0, 123] {
        let mut task = partial_four_pixel_task(5, seed, true);
        let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
        let mut actual_rays = Vec::new();
        for (passes, prefix) in [(0, 1), (0, 3), (1, 0)] {
            reach_partial_frontier(&mut task, passes, prefix, &mut actual_rays);
            assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
            let _ = finish_partial_snapshot(&mut task);
            assert_eq!(task.pixel, if prefix == 0 { 4 } else { prefix });
        }
        let actual = finish_partial_final(&mut task, true, &mut actual_rays);
        let mut control = partial_four_pixel_task(5, seed, false);
        let mut control_rays = Vec::new();
        let expected = finish_partial_final(&mut control, false, &mut control_rays);
        assert_eq!(actual, expected, "final PNG and linear film bits must be exact");
        assert_eq!(actual_rays, control_rays, "snapshot timing cannot replay a camera pixel or ordinal");
        assert_eq!(actual_rays.len(), 20);
    }
}

#[test]
fn partial_work_preflight_is_optional_and_successful_debit_is_once() {
    let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
    for enough in [false, true] {
        let mut task = partial_four_pixel_task(5, 123, true);
        reach_partial_frontier(&mut task, 0, 2, &mut Vec::new());
        let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
        let before = task.work.used();
        let cost = plan.snapshot_work().unwrap();
        task.work.limit = before + cost - u64::from(!enough);
        if enough {
            assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
            assert_eq!(task.work.used(), before + cost);
            let _ = finish_partial_snapshot(&mut task);
            assert_eq!(
                task.step_with_partial_previews(1).err().unwrap().code,
                ErrorCode::LimitExceeded,
                "accepted presentation does not promise future trace headroom"
            );
        } else {
            assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Exhausted));
            assert_eq!(task.work.used(), before);
            assert_eq!(task.pixel, 2);
            assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
            assert!(task.progressive.as_ref().unwrap().partial.is_none());
            assert!(task.encoder.is_none() && task.pixels.is_empty());
            assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
        }
    }
}

#[test]
fn partial_ready_requires_actual_safe_commit_plan_and_new_represented_state() {
    let mut task = partial_four_pixel_task(5, 123, true);
    let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
    assert!(!task.partial_preview_ready(plan).unwrap());
    let mut observed_pending = false;
    for _ in 0..200_000 {
        let _ = task.step_with_partial_previews(1).unwrap();
        if !task.pending.is_none() {
            assert!(!task.partial_preview_ready(plan).unwrap());
            let used = task.work.used();
            assert!(task.begin_partial_preview(plan).is_err());
            assert_eq!(task.work.used(), used);
            observed_pending = true;
            break;
        }
    }
    assert!(observed_pending, "test must observe a real in-flight primary pixel");
    reach_partial_frontier(&mut task, 0, 1, &mut Vec::new());
    assert!(task.partial_preview_ready(job::PartialPreviewPlan::new(8, 1).unwrap()).is_err());
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut task);
    assert!(!task.partial_preview_ready(plan).unwrap(), "same frontier cannot be a fresh snapshot");

    use crate::film::primary::PartialFrontier;
    let reduced = job::PartialPreviewPlan::new(4096, 1).unwrap();
    let old = PartialFrontier { completed_passes: 0, updated_prefix: 1, pixels: 4096 };
    assert!(!progressive::represented_changed(reduced, Some(old), PartialFrontier { updated_prefix: 4, ..old }).unwrap());
    assert!(progressive::represented_changed(reduced, Some(old), PartialFrontier { updated_prefix: 5, ..old }).unwrap());
}

#[test]
fn partial_cancel_before_resolve_or_during_encoding_never_emits_payload() {
    let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
    for during_encoding in [false, true] {
        let mut task = partial_four_pixel_task(5, 123, true);
        reach_partial_frontier(&mut task, 0, 2, &mut Vec::new());
        assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
        let paid = task.work.used();
        if during_encoding {
            for _ in 0..20 {
                if task.encoder.is_some() {
                    break;
                }
                assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
            }
            assert!(task.encoder.is_some());
        }
        task.cancel.cancel();
        assert_eq!(task.step_with_partial_previews(1).err().unwrap().code, ErrorCode::Cancelled);
        assert_eq!(task.work.used(), paid, "there is no refund or second partial-stage debit");
        assert!(!task.done);
    }
}

#[test]
fn partial_reduced_resolve_is_bounded_by_display_cells_and_admission_is_not_legacy_prepay() {
    let fixed = fixture(1, 1, None);
    let mut options = fixed.visibility_owner.options().clone();
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 5, seed: 123 });
    let mut request = fixed.request.clone();
    request.width = 1500;
    request.raytrace = Some(options.clone());
    let plan = job::PartialPreviewPlan::new(1500, 1).unwrap();
    let first = admit_with_partial_preview(fixed.visibility_owner.source(), &request, &options, false, plan).unwrap();
    options.progressive.as_mut().unwrap().samples_per_pixel = 9;
    request.raytrace = Some(options.clone());
    let second = admit_with_partial_preview(fixed.visibility_owner.source(), &request, &options, false, plan).unwrap();
    assert_eq!(first.estimated_total_bytes, second.estimated_total_bytes, "streaming reserves two slots, not every legacy power2 image");
    let (fixture_scene, _) = fixed.visibility_owner.into_capture();
    let mut task =
        ImageTask::new_with_environment_and_nested_and_partial(fixture_scene, request, CancelToken::default(), options, None, false, plan).unwrap();
    reach_partial_frontier(&mut task, 0, 1, &mut Vec::new());
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    let paid = task.work.used();
    let mut output = None;
    let mut steps = 0_u64;
    for _ in 0..2000 {
        steps += 1;
        match task.step_with_partial_previews(1).unwrap() {
            PartialImagePoll::Pending(_) => {}
            PartialImagePoll::Preview { png, covered, .. } => {
                output = Some((png, covered));
                break;
            }
            PartialImagePoll::Complete(_) => panic!("display resolve cannot complete the final"),
        }
        assert_eq!(task.work.used(), paid);
    }
    let (png, covered) = output.expect("bounded reduced snapshot");
    assert_eq!(steps, plan.pixels() + u64::from(plan.height()) + 2);
    assert_eq!(covered.len() as u64, plan.pixels());
    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().dimensions(), (1024, 1));
    assert_eq!((task.request.width, task.request.height), (1500, 1));
    assert_eq!(task.pixel, 1);
}

#[test]
fn partial_area_first_pass_and_final_preserve_all_existing_emitter_sample_counts() {
    for area_samples in [1, 4, 16, 64] {
        let source = progressive_area_fixture(area_samples, 5, 23, true);
        let plan = job::PartialPreviewPlan::new(source.request.width, source.request.height).unwrap();
        let mut task = ImageTask::new_with_environment_and_nested_and_partial(
            source.visibility_owner.source().clone(),
            source.request.clone(),
            CancelToken::default(),
            source.visibility_owner.options().clone(),
            None,
            false,
            plan,
        )
        .unwrap();
        reach_partial_frontier(&mut task, 1, 0, &mut Vec::new());
        assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
        let (first, _, passes, prefix) = finish_partial_snapshot(&mut task);
        assert_eq!((passes, prefix), (1, 0));
        let (actual, _) = finish_partial_final(&mut task, true, &mut Vec::new());
        let (legacy_previews, expected, _) = progressive_complete(source, 1);
        assert_eq!(first, legacy_previews[0].1);
        assert_eq!(actual, expected, "area source normalization/sampler must remain unchanged");
    }
}

fn partial_original_hdr() -> crate::lighting::environment_input::EnvironmentInput {
    use crate::lighting::{HdrSource, environment_input::EnvironmentInput};
    let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nPRIMARIES=0.64 0.33 0.30 0.60 0.15 0.06 0.3127 0.3290\n\n-Y 2 +X 4\n".to_vec();
    for _ in 0..8 {
        bytes.extend_from_slice(&[128, 128, 128, 129]);
    }
    EnvironmentInput::new(HdrSource::new(bytes).unwrap(), 0.0, 1.0, 4).unwrap()
}

#[test]
fn partial_hdr_rough_reflection_first_pass_and_final_match_legacy_sampling() {
    let base = fixture(1, 1, Some(true));
    let mut scene = base.visibility_owner.source().clone();
    scene.meshes[0].surface = Some(OpaqueSurface::new(Vec3::new(0.8, 0.4, 0.2), 1.0, 0.35, 1.5).unwrap());
    let mut options = base.visibility_owner.options().clone();
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 5, seed: 23 });
    let request = ImageRequest { raytrace: Some(options.clone()), ..base.request.clone() };
    let plan = job::PartialPreviewPlan::new(request.width, request.height).unwrap();
    let mut task = ImageTask::new_with_environment_and_nested_and_partial(
        scene.clone(),
        request.clone(),
        CancelToken::default(),
        options.clone(),
        Some(partial_original_hdr()),
        false,
        plan,
    )
    .unwrap();
    let control = ImageTask::new_with_environment(scene, request, CancelToken::default(), options, Some(partial_original_hdr())).unwrap();
    reach_partial_frontier(&mut task, 1, 0, &mut Vec::new());
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    let (first, _, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!((passes, prefix), (1, 0));
    let (actual, _) = finish_partial_final(&mut task, true, &mut Vec::new());
    let (legacy_previews, expected, _) = progressive_complete(control, 1);
    assert_eq!(first, legacy_previews[0].1);
    assert_eq!(actual, expected, "HDR and rough-reflection sample identity cannot depend on presentation");
}

#[test]
fn partial_full_pass_snapshot_preserves_regular_transition_work_and_final_ledger() {
    let mut task = partial_four_pixel_task(5, 123, true);
    let mut control = partial_four_pixel_task(5, 123, true);
    let plan = job::PartialPreviewPlan::new(4, 1).unwrap();
    let mut actual_rays = Vec::new();
    let mut control_rays = Vec::new();
    reach_partial_frontier(&mut task, 1, 0, &mut actual_rays);
    reach_partial_frontier(&mut control, 1, 0, &mut control_rays);
    let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
    assert_eq!(frontier, control.progressive.as_ref().unwrap().film.partial_frontier().unwrap());
    assert_eq!((task.pixel, control.pixel), (4, 4));
    let before = control.work.used();
    assert_eq!(task.work.used(), before);
    let cost = plan.snapshot_work().unwrap();
    assert!(matches!(task.begin_partial_preview(plan).unwrap(), PartialBegin::Started));
    let (_, _, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!((passes, prefix, task.pixel), (1, 0, 4), "snapshot may not consume the ordinary pass transition");
    assert_eq!(task.work.used(), before + cost);
    assert_eq!(control.work.used(), before);
    assert!(!task.partial_preview_ready(plan).unwrap(), "unchanged frontier remains coalesced");

    // The host hashes without stepping this task. On the next producer step,
    // BOTH tasks owe the same ordinary pass-transition debit before a new ray.
    assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
    assert!(matches!(control.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
    assert_eq!((task.pixel, control.pixel), (0, 0));
    assert!(task.pending.is_none() && control.pending.is_none());
    assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
    assert_eq!(control.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
    assert_eq!(control.work.used(), before + 1, "the no-snapshot control owes the regular transition tick");
    assert_eq!(task.work.used(), before + cost + 1, "the snapshot must preserve that same tick in addition to W");

    let actual = finish_partial_final(&mut task, true, &mut actual_rays);
    let expected = finish_partial_final(&mut control, true, &mut control_rays);
    assert_eq!(actual, expected, "same partial policy: final PNG and linear film bits are exact");
    assert_eq!(actual_rays, control_rays, "presentation cannot replay or skip a primary sample");
    assert_eq!(actual_rays.len(), 20);
    assert_eq!(task.work.used(), control.work.used() + cost, "the complete-job optional cost is exactly W, not W-1");
}

fn protected_plan(task: &ImageTask) -> job::ProtectedPreviewPlan {
    job::ProtectedPreviewPlan::new(
        task.request.width,
        task.request.height,
        task.visibility_owner.options().progressive.unwrap().samples_per_pixel,
        task.visibility_owner.options().budget,
    )
    .unwrap()
}
fn protected_rebuild(source: &ImageTask, input: Option<crate::lighting::environment_input::EnvironmentInput>) -> ImageTask {
    ImageTask::new_with_environment_and_nested_and_protected_partial(
        source.visibility_owner.source().clone(),
        source.request.clone(),
        CancelToken::default(),
        source.visibility_owner.options().clone(),
        input,
        false,
        protected_plan(source),
    )
    .unwrap()
}
fn shared_partial_rebuild(source: &ImageTask, input: Option<crate::lighting::environment_input::EnvironmentInput>) -> ImageTask {
    ImageTask::new_with_environment_and_nested_and_partial(
        source.visibility_owner.source().clone(),
        source.request.clone(),
        CancelToken::default(),
        source.visibility_owner.options().clone(),
        input,
        false,
        protected_plan(source).preview_plan(),
    )
    .unwrap()
}
fn empty_progressive_source() -> ImageTask {
    let fixed = fixture(1, 1, None);
    let options = RaytraceOptions {
        progressive: Some(job::CpuProgressiveOptions { samples_per_pixel: 64, seed: 123 }),
        lights: Some(Vec::new()),
        shadows: false,
        ..Default::default()
    };
    let request = ImageRequest { width: 32, height: 32, background: [10, 20, 30], raytrace: Some(options.clone()), ..fixed.request };
    let (fixture_scene, _) = fixed.visibility_owner.into_capture();
    ImageTask::new(fixture_scene, request, CancelToken::default(), options).unwrap()
}
fn initial_test_work(task: &mut ImageTask, ordinary: u64, protected: bool) {
    assert_eq!(task.work.used(), 0, "test boundary is fixed BEFORE any real work, never mid-render");
    task.work =
        if protected { WorkBudget::new_protected_for_test(protected_plan(task), ordinary).unwrap() } else { WorkBudget::new(ordinary).unwrap() };
}
fn require_empty_final(png: &[u8], bits: &[[u64; 3]]) {
    let image = ::image::load_from_memory(png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (32, 32));
    assert!(image.pixels().all(|p| p.0 == [10, 20, 30, 255]), "independent all-miss backplate oracle");
    assert_eq!(bits.len(), 1024);
    assert!(bits.iter().all(|p| *p == [0.0_f64.to_bits(); 3]), "no emitted or reflected radiance in the empty fixture");
}

#[test]
fn protected_actual_empty_counterexample_preserves_final_work_while_shared_cap_can_starve() {
    let source = empty_progressive_source();
    let plan = protected_plan(&source);
    assert_eq!((plan.total_commits(), plan.quota_limit(), plan.snapshot_work().unwrap()), (65536, 127, 79874));
    let mut reference = shared_partial_rebuild(&source, None);
    let mut reference_rays = Vec::new();
    let expected = finish_partial_final(&mut reference, true, &mut reference_rays);
    require_empty_final(&expected.0, &expected.1);
    let ordinary = reference.work.used();
    assert!(ordinary > plan.snapshot_work().unwrap());

    let mut control = shared_partial_rebuild(&source, None);
    initial_test_work(&mut control, ordinary, false);
    let actual = finish_partial_final(&mut control, true, &mut Vec::new());
    assert_eq!(actual, expected);
    assert_eq!(control.work.used(), ordinary);

    let mut starved = shared_partial_rebuild(&source, None);
    initial_test_work(&mut starved, ordinary, false);
    reach_partial_frontier(&mut starved, 0, 1, &mut Vec::new());
    assert!(matches!(starved.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut starved);
    let mut failure = None;
    for _ in 0..1_000_000 {
        match starved.step_with_partial_previews(1) {
            Ok(PartialImagePoll::Pending(_)) => {}
            Ok(PartialImagePoll::Complete(_)) => panic!("shared cap T cannot pay the same T ordinary work plus W"),
            Ok(PartialImagePoll::Preview { .. }) => panic!("no unrequested snapshot"),
            Err(error) => {
                failure = Some(error.code);
                break;
            }
        }
    }
    assert_eq!(failure, Some(ErrorCode::LimitExceeded));
    assert!(!starved.done);
    assert!(starved.work.used() <= ordinary);

    for request_snapshot in [false, true] {
        let mut task = protected_rebuild(&source, None);
        initial_test_work(&mut task, ordinary, true);
        let mut rays = Vec::new();
        if request_snapshot {
            reach_partial_frontier(&mut task, 0, 1, &mut rays);
            let before = task.protected_preview_report().unwrap();
            assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
            let _ = finish_partial_snapshot(&mut task);
            let paid = task.protected_preview_report().unwrap();
            assert_eq!(paid.ordinary_used, before.ordinary_used);
            assert_eq!((paid.presentation_used, paid.accepted_snapshots), (79874, 1));
        }
        let actual = finish_partial_final(&mut task, true, &mut rays);
        assert_eq!(actual, expected, "protected feedback cannot change PNG or linear film");
        assert_eq!(rays, reference_rays, "absolute pixel/ordinal/ray sequence is identical");
        assert_eq!(rays.len(), 65536);
        let report = task.protected_preview_report().expect("actual report survives completed-film destruction");
        assert_eq!(report.ordinary_used, ordinary);
        let optional = if request_snapshot { 79874 } else { 0 };
        assert_eq!(report.presentation_used, optional);
        assert_eq!(task.work.used(), ordinary + optional);
        assert_eq!(report.aggregate_limit, ordinary + plan.presentation_limit().unwrap());
        assert!(task.done && task.progressive.is_none());
    }
}

#[test]
fn protected_real_milestone_wait_is_not_exhaustion_or_a_debit() {
    let source = empty_progressive_source();
    let mut task = protected_rebuild(&source, None);
    let plan = protected_plan(&task);
    let preview = plan.preview_plan();
    reach_partial_frontier(&mut task, 0, 1, &mut Vec::new());
    assert!(matches!(task.begin_partial_preview(preview).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut task);
    reach_partial_frontier(&mut task, 0, 2, &mut Vec::new());
    let before = task.protected_preview_report().unwrap();
    let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
    let cursor = task.pixel;
    assert!(plan.milestone(before.next_milestone_index).unwrap() > 2);
    for _ in 0..4 {
        assert!(!task.partial_preview_ready(preview).unwrap());
        assert_eq!(task.begin_partial_preview(preview).err().unwrap().code, ErrorCode::InvalidRequest);
        assert_eq!(task.protected_preview_report().unwrap(), before);
        assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
        assert_eq!(task.pixel, cursor);
        assert!(task.encoder.is_none() && task.pixels.is_empty());
        assert!(task.progressive.as_ref().unwrap().partial.is_none());
    }
    assert!(!before.quota_exhausted && !before.milestones_complete);
    let milestone = plan.milestone(before.next_milestone_index).unwrap();
    reach_partial_frontier(&mut task, (milestone / 1024) as u32, (milestone % 1024) as usize, &mut Vec::new());
    assert!(task.partial_preview_ready(preview).unwrap(), "later real progress makes a temporary wait eligible");
}

#[test]
fn protected_real_coalesced_last_milestone_preserves_unspent_quota_and_final_priority() {
    let source = partial_four_pixel_task(5, 123, true);
    let mut task = protected_rebuild(&source, None);
    let mut control = shared_partial_rebuild(&source, None);
    let plan = protected_plan(&task);
    let mut rays = Vec::new();
    reach_partial_frontier(&mut task, 4, 3, &mut rays);
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut task);
    let coalesced = task.protected_preview_report().unwrap();
    assert_eq!((coalesced.accepted_snapshots, coalesced.next_milestone_index, coalesced.quota_limit), (1, 7, 7));
    assert!(coalesced.milestones_complete && !coalesced.quota_exhausted);
    assert_eq!(coalesced.presentation_used, plan.snapshot_work().unwrap());
    assert!(coalesced.presentation_used < coalesced.presentation_limit);
    assert!(!task.partial_preview_ready(plan.preview_plan()).unwrap());
    let actual = finish_partial_final(&mut task, true, &mut rays);
    let mut control_rays = Vec::new();
    let expected = finish_partial_final(&mut control, true, &mut control_rays);
    assert_eq!(actual, expected);
    assert_eq!(rays, control_rays);
    let report = task.protected_preview_report().unwrap();
    assert_eq!(report.ordinary_used, control.work.used());
    assert_eq!(task.work.used(), control.work.used() + plan.snapshot_work().unwrap());
    assert!(task.done, "final never waits to spend unreachable/unused quota");
}

#[test]
fn protected_actual_every_pass_and_early_snapshots_exhaust_quota_without_failing_final() {
    let source = empty_progressive_source();
    let mut control = shared_partial_rebuild(&source, None);
    let expected = finish_partial_final(&mut control, true, &mut Vec::new());
    let ordinary = control.work.used();
    let mut task = protected_rebuild(&source, None);
    initial_test_work(&mut task, ordinary, true);
    let plan = protected_plan(&task);
    let mut rays = Vec::new();
    for index in 0..127 {
        let milestone = plan.milestone(index).unwrap();
        reach_partial_frontier(&mut task, (milestone / 1024) as u32, (milestone % 1024) as usize, &mut rays);
        assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
        let _ = finish_partial_snapshot(&mut task);
        let report = task.protected_preview_report().unwrap();
        assert_eq!(report.accepted_snapshots, index + 1);
        assert_eq!(report.next_milestone_index, index + 1);
        assert_eq!(report.presentation_used, u64::from(index + 1) * 79874);
    }
    let exhausted = task.protected_preview_report().unwrap();
    assert!(exhausted.quota_exhausted && exhausted.milestones_complete);
    assert_eq!(exhausted.presentation_used, exhausted.presentation_limit);
    let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
    let cursor = task.pixel;
    for _ in 0..4 {
        assert!(!task.partial_preview_ready(plan.preview_plan()).unwrap());
        assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Exhausted));
        assert_eq!(task.protected_preview_report().unwrap(), exhausted);
        assert_eq!(task.pixel, cursor);
        assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
    }
    let actual = finish_partial_final(&mut task, true, &mut rays);
    assert_eq!(actual, expected);
    require_empty_final(&actual.0, &actual.1);
    assert_eq!(rays.len(), 65536);
    let complete = task.protected_preview_report().unwrap();
    assert_eq!(complete.ordinary_used, ordinary);
    assert_eq!(task.work.used(), complete.aggregate_limit, "O reaches its full original T and V exactly Q");
    assert_eq!(complete.presentation_used, 127 * 79874);
}

#[test]
fn protected_full_pass_snapshot_keeps_the_ordinary_transition_tick_and_report_after_complete() {
    let source = partial_four_pixel_task(5, 123, true);
    let mut task = protected_rebuild(&source, None);
    let mut control = shared_partial_rebuild(&source, None);
    let plan = protected_plan(&task);
    let mut rays = Vec::new();
    let mut control_rays = Vec::new();
    reach_partial_frontier(&mut task, 1, 0, &mut rays);
    reach_partial_frontier(&mut control, 1, 0, &mut control_rays);
    let before = task.protected_preview_report().unwrap();
    assert_eq!(before.ordinary_used, control.work.used());
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut task);
    assert_eq!((task.pixel, control.pixel), (4, 4));
    assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
    assert!(matches!(control.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
    assert_eq!((task.pixel, control.pixel), (0, 0));
    let after = task.protected_preview_report().unwrap();
    assert_eq!(after.ordinary_used, before.ordinary_used + 1);
    assert_eq!(after.ordinary_used, control.work.used());
    assert_eq!(after.presentation_used, plan.snapshot_work().unwrap());
    let actual = finish_partial_final(&mut task, true, &mut rays);
    let expected = finish_partial_final(&mut control, true, &mut control_rays);
    assert_eq!(actual, expected);
    assert_eq!(rays, control_rays);
    assert_eq!(task.protected_preview_report().unwrap().ordinary_used, control.work.used());
    assert!(control.protected_preview_report().is_none(), "old IF1 has no protected report");
}

#[test]
fn protected_cancelled_begin_and_encoder_keep_the_actual_paid_report_without_refund() {
    let source = partial_four_pixel_task(5, 123, true);
    for debit_first in [false, true] {
        let mut task = protected_rebuild(&source, None);
        let plan = protected_plan(&task);
        reach_partial_frontier(&mut task, 0, 1, &mut Vec::new());
        let before = task.protected_preview_report().unwrap();
        if debit_first {
            assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
            for _ in 0..20 {
                if task.encoder.is_some() {
                    break;
                }
                assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
            }
            assert!(task.encoder.is_some());
        }
        let paid = task.protected_preview_report().unwrap();
        task.cancel.cancel();
        let code = if debit_first {
            task.step_with_partial_previews(1).err().unwrap().code
        } else {
            task.begin_partial_preview(plan.preview_plan()).err().unwrap().code
        };
        assert_eq!(code, ErrorCode::Cancelled);
        assert_eq!(task.protected_preview_report().unwrap(), paid);
        assert_eq!(paid.ordinary_used, before.ordinary_used);
        assert_eq!(paid.presentation_used, if debit_first { plan.snapshot_work().unwrap() } else { 0 });
        assert_eq!(paid.accepted_snapshots, u32::from(debit_first));
        assert_eq!(paid.next_milestone_index, u32::from(debit_first));
    }
}

#[test]
fn protected_admission_matches_actual_source_sampling_budget_before_constructing_the_task() {
    let source = partial_four_pixel_task(5, 123, true);
    let plan = protected_plan(&source);
    let protected =
        admit_with_protected_partial_preview(source.visibility_owner.source(), &source.request, source.visibility_owner.options(), false, plan)
            .unwrap();
    let shared =
        admit_with_partial_preview(source.visibility_owner.source(), &source.request, source.visibility_owner.options(), false, plan.preview_plan())
            .unwrap();
    assert_eq!(
        protected.estimated_total_bytes, shared.estimated_total_bytes,
        "same two slots and source/codec buffers; fixed task layout guard stays required"
    );
    for wrong in [
        job::ProtectedPreviewPlan::new(8, 1, 5, source.visibility_owner.options().budget).unwrap(),
        job::ProtectedPreviewPlan::new(4, 1, 3, source.visibility_owner.options().budget).unwrap(),
        job::ProtectedPreviewPlan::new(4, 1, 5, job::CpuBudget::Studio).unwrap(),
    ] {
        assert_eq!(
            admit_with_protected_partial_preview(source.visibility_owner.source(), &source.request, source.visibility_owner.options(), false, wrong)
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            ImageTask::new_with_environment_and_nested_and_protected_partial(
                source.visibility_owner.source().clone(),
                source.request.clone(),
                CancelToken::default(),
                source.visibility_owner.options().clone(),
                None,
                false,
                wrong
            )
            .err()
            .unwrap()
            .code,
            ErrorCode::InvalidRequest
        );
    }
    assert_eq!(source.visibility_owner.options().emissions.as_ref().unwrap().len(), 1, "original fixture authors one red surface emission");
    let task = protected_rebuild(&source, None);
    let report = task.protected_preview_report().unwrap();
    assert_eq!(
        (report.ordinary_used, report.presentation_used, report.accepted_snapshots, report.next_milestone_index),
        (1, 0, 0, 0),
        "one authored emission owes one ordinary construction tick before the first camera sample"
    );
    assert_eq!(report.aggregate_limit, plan.aggregate_limit().unwrap());
}

#[test]
fn protected_area_and_hdr_reflection_preserve_ordinary_work_absolute_rays_and_final_pixels() {
    for area_samples in [1, 4, 16, 64] {
        let source = progressive_area_fixture(area_samples, 5, 23, true);
        let mut task = protected_rebuild(&source, None);
        let mut control = shared_partial_rebuild(&source, None);
        let plan = protected_plan(&task);
        let mut rays = Vec::new();
        let mut control_rays = Vec::new();
        reach_partial_frontier(&mut task, 1, 0, &mut rays);
        assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
        let _ = finish_partial_snapshot(&mut task);
        assert_eq!(finish_partial_final(&mut task, true, &mut rays), finish_partial_final(&mut control, true, &mut control_rays));
        assert_eq!(rays, control_rays);
        assert_eq!(task.protected_preview_report().unwrap().ordinary_used, control.work.used());
        assert_eq!(task.work.used(), control.work.used() + plan.snapshot_work().unwrap());
    }
    let mut source = progressive_fixture(5, 23, Some(true), 0.0);
    let (mut fixture_scene, fixture_options) = source.visibility_owner.into_capture();
    fixture_scene.meshes[0].surface = Some(OpaqueSurface::new(Vec3::new(0.8, 0.4, 0.2), 1.0, 0.35, 1.5).unwrap());
    source.visibility_owner = dielectric::DirectVisibilityOwner::new(fixture_scene, fixture_options);
    let mut task = protected_rebuild(&source, Some(partial_original_hdr()));
    let mut control = shared_partial_rebuild(&source, Some(partial_original_hdr()));
    let plan = protected_plan(&task);
    let mut rays = Vec::new();
    let mut control_rays = Vec::new();
    reach_partial_frontier(&mut task, 1, 0, &mut rays);
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let _ = finish_partial_snapshot(&mut task);
    assert_eq!(finish_partial_final(&mut task, true, &mut rays), finish_partial_final(&mut control, true, &mut control_rays));
    assert_eq!(rays, control_rays);
    assert_eq!(task.protected_preview_report().unwrap().ordinary_used, control.work.used());
    assert_eq!(task.work.used(), control.work.used() + plan.snapshot_work().unwrap());
}

#[test]
fn protected_real_sampling_is_independent_of_quantum_and_feedback_schedule() {
    for seed in [0, 123] {
        let source = partial_four_pixel_task(5, seed, true);
        let mut control = shared_partial_rebuild(&source, None);
        let expected = finish_partial_final(&mut control, true, &mut Vec::new());
        let ordinary = control.work.used();
        for quantum in [1, 2, 4] {
            let mut task = protected_rebuild(&source, None);
            let plan = protected_plan(&task);
            let mut snapshots = 0_u32;
            let mut output = None;
            let mut bits = None;
            for _ in 0..200_000 {
                if bits.is_none()
                    && let Some(progressive) = &task.progressive
                    && progressive.film.ready()
                    && progressive.film.samples() == 5
                {
                    bits = Some((0..4).map(|i| progressive.film.pixel(i).unwrap().0.map(f64::to_bits)).collect::<Vec<_>>());
                }
                if snapshots < 3 && task.partial_preview_ready(plan.preview_plan()).unwrap() {
                    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
                    let _ = finish_partial_snapshot(&mut task);
                    snapshots += 1;
                }
                match task.step_with_partial_previews(quantum).unwrap() {
                    PartialImagePoll::Pending(_) => {}
                    PartialImagePoll::Complete(png) => {
                        output = Some(png);
                        break;
                    }
                    PartialImagePoll::Preview { .. } => panic!("no snapshot was requested in this continuation"),
                }
            }
            assert!(snapshots > 0, "actual safe frontiers must be observed in this original fixture");
            assert_eq!(output.unwrap(), expected.0);
            assert_eq!(bits.unwrap(), expected.1);
            let report = task.protected_preview_report().unwrap();
            assert_eq!(report.ordinary_used, ordinary);
            assert_eq!(report.accepted_snapshots, snapshots);
            assert_eq!(report.presentation_used, u64::from(snapshots) * plan.snapshot_work().unwrap());
            assert_eq!(task.work.used(), ordinary + report.presentation_used);
        }
    }
}

#[test]
fn protected_real_one_and_two_camera_commit_jobs_finish_with_exact_zero_or_one_snapshot() {
    for samples in [1, 2] {
        let source = progressive_fixture(samples, 123, None, 0.0);
        let mut task = protected_rebuild(&source, None);
        let mut control = shared_partial_rebuild(&source, None);
        let plan = protected_plan(&task);
        let mut rays = Vec::new();
        let mut control_rays = Vec::new();
        assert_eq!(plan.quota_limit(), samples - 1);
        if samples == 1 {
            let initial = task.protected_preview_report().unwrap();
            assert!(initial.quota_exhausted && initial.milestones_complete);
            assert_eq!(initial.presentation_limit, 0);
            assert!(!task.partial_preview_ready(plan.preview_plan()).unwrap());
        } else {
            reach_partial_frontier(&mut task, 1, 0, &mut rays);
            assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
            let _ = finish_partial_snapshot(&mut task);
            assert_eq!(task.pixel, 1, "normal complete-pass transition remains pending");
        }
        assert_eq!(finish_partial_final(&mut task, true, &mut rays), finish_partial_final(&mut control, true, &mut control_rays));
        assert_eq!(rays, control_rays);
        let report = task.protected_preview_report().unwrap();
        assert_eq!(report.accepted_snapshots, samples - 1);
        assert_eq!(report.presentation_used, u64::from(samples - 1) * plan.snapshot_work().unwrap());
        assert_eq!(report.ordinary_used, control.work.used());
        assert!(report.quota_exhausted && report.milestones_complete);
    }
}

// Original progressive optics / presentation combinations. WRITTEN, NOT RUN.
fn optical_partial_source(samples: u32) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let (scene, mut request, options) = super::tests::progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, samples);
    request.width = 2;
    (scene, request, options)
}

#[test]
fn protected_optical_partial_contains_only_whole_drained_camera_trees() {
    let (scene, request, options) = optical_partial_source(3);
    let plan = job::ProtectedPreviewPlan::new(2, 1, 3, options.budget).unwrap();
    let mut task = ImageTask::new_with_environment_and_nested_and_protected_partial(
        scene.clone(),
        request.clone(),
        CancelToken::default(),
        options.clone(),
        None,
        false,
        plan,
    )
    .unwrap();
    let mut control =
        ImageTask::new_with_environment_and_nested_and_partial(scene, request, CancelToken::default(), options, None, false, plan.preview_plan())
            .unwrap();
    let mut rays = Vec::new();
    let mut control_rays = Vec::new();
    let mut saw_interior = false;
    for _ in 0..100_000 {
        let was_empty = task.pending.is_none();
        assert!(matches!(task.step_with_partial_previews(1).unwrap(), PartialImagePoll::Pending(_)));
        record_partial_primary(&task, was_empty, &mut rays);
        if task.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|s| s.path.medium.is_some()) {
            assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap().updated_prefix, 0);
            assert!(!task.partial_preview_ready(plan.preview_plan()).unwrap());
            saw_interior = true;
            break;
        }
    }
    assert!(saw_interior);
    reach_partial_frontier(&mut task, 0, 1, &mut rays);
    let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
    assert!(task.optical.first().unwrap().pending.is_none());
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let ordinary = task.protected_preview_report().unwrap().ordinary_used;
    let (_, covered, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!((passes, prefix), (0, 1));
    assert_eq!(covered, [1, 0], "second pixel is Unknown, never a fabricated optical sample");
    assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
    assert_eq!(task.protected_preview_report().unwrap().ordinary_used, ordinary);
    let actual = finish_partial_final(&mut task, true, &mut rays);
    let expected = finish_partial_final(&mut control, true, &mut control_rays);
    assert_eq!(actual, expected, "exact final PNG and linear film bits with observer on/off");
    assert_eq!(rays, control_rays);
    assert_eq!(rays.len(), 6);
    let report = task.protected_preview_report().unwrap();
    assert_eq!(report.ordinary_used, control.work.used());
    assert_eq!(report.presentation_used, plan.snapshot_work().unwrap());
}

#[test]
fn protected_optical_snapshot_cancellation_cannot_emit_or_commit_another_sample() {
    let (scene, request, options) = optical_partial_source(3);
    let plan = job::ProtectedPreviewPlan::new(2, 1, 3, options.budget).unwrap();
    let mut task =
        ImageTask::new_with_environment_and_nested_and_protected_partial(scene, request, CancelToken::default(), options, None, false, plan).unwrap();
    reach_partial_frontier(&mut task, 0, 1, &mut Vec::new());
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
    let report = task.protected_preview_report().unwrap();
    task.cancel.cancel();
    assert_eq!(task.step_with_partial_previews(1).err().unwrap().code, ErrorCode::Cancelled);
    assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
    assert_eq!(task.protected_preview_report().unwrap(), report);
    assert!(!task.done);
}

fn with_unit_optical_sheet(source: &ImageTask) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let mut scene = source.visibility_owner.source().clone();
    let mut options = source.visibility_owner.options().clone();
    // Original two-triangle rectangle between camera and receiver. It is
    // index-matched and lossless: the independent control has no sheet at all.
    scene.meshes.push(ProductMesh {
        transmission: None,
        name: "Progressive optical sheet".into(),
        mesh: Arc::new(Mesh {
            positions: vec![Vec3::new(-20.0, -20.0, 2.0), Vec3::new(20.0, -20.0, 2.0), Vec3::new(20.0, 20.0, 2.0), Vec3::new(-20.0, 20.0, 2.0)],
            normals: vec![Vec3::Z; 4],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..Default::default()
        }),
        color: [255; 3],
        opacity: 1.0,
        surface: None,
        faces: Vec::new(),
    });
    options.dielectrics.push(job::DielectricOptions {
        body_index: scene.meshes.len() - 1,
        body_name: "Progressive optical sheet".into(),
        boundary: job::DielectricBoundary::ThinSheet,
        ior: 1.0,
        absorption_per_mm: [0.0; 3],
        thickness_mm: 0.0,
    });
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |bounds, body| bounds.union(&body.mesh.bounds()));
    let request = ImageRequest { raytrace: Some(options.clone()), ..source.request.clone() };
    (scene, request, options)
}

#[test]
fn progressive_unit_optics_rejoin_real_area_consumers_at_every_admitted_sample_count() {
    for count in [1, 4, 16, 64] {
        for seed in [0, 23] {
            // Conservative opaque shadow visibility is deliberately off. This
            // feature does not invent transmitted illumination or caustics.
            let source = progressive_area_fixture(count, 3, seed, false);
            let (scene, request, options) = with_unit_optical_sheet(&source);
            let optical = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
            let expected = progressive_complete(source, 1);
            let actual = progressive_complete(optical, 4);
            assert_eq!(actual.0, expected.0);
            assert_eq!(actual.1, expected.1, "unit transmission preserves independently lit receiver; count={count}, seed={seed}");
            assert!(actual.2 > expected.2, "real optical traversal is charged, never bypassed");
        }
    }
}

#[test]
fn progressive_unit_optics_rejoin_hdr_receiver_and_preserve_feedback_film_and_seed() {
    let mut source = progressive_area_fixture(4, 3, 23, false);
    let (fixture_scene, mut fixture_options) = source.visibility_owner.into_capture();
    fixture_options.area_lights = None;
    fixture_options.area_samples = None;
    fixture_options.seed = None;
    source.request.raytrace = Some(fixture_options.clone());
    source.visibility_owner = dielectric::DirectVisibilityOwner::new(fixture_scene, fixture_options);
    let (scene, request, options) = with_unit_optical_sheet(&source);
    let plan = job::ProtectedPreviewPlan::new(1, 1, 3, options.budget).unwrap();
    let mut observed = ImageTask::new_with_environment_and_nested_and_protected_partial(
        scene.clone(),
        request.clone(),
        CancelToken::default(),
        options.clone(),
        Some(partial_original_hdr()),
        false,
        plan,
    )
    .unwrap();
    let mut control = ImageTask::new_with_environment_and_nested_and_partial(
        scene,
        request,
        CancelToken::default(),
        options,
        Some(partial_original_hdr()),
        false,
        plan.preview_plan(),
    )
    .unwrap();
    let mut rays = Vec::new();
    let mut control_rays = Vec::new();
    reach_partial_frontier(&mut observed, 1, 0, &mut rays);
    assert!(matches!(observed.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let (snapshot, covered, passes, prefix) = finish_partial_snapshot(&mut observed);
    assert_eq!((passes, prefix), (1, 0));
    assert_eq!(covered, [1]);
    let actual = finish_partial_final(&mut observed, true, &mut rays);
    assert_eq!(actual, finish_partial_final(&mut control, true, &mut control_rays));
    assert_eq!(rays, control_rays);
    assert_eq!(observed.protected_preview_report().unwrap().ordinary_used, control.work.used());
    let without_sheet = ImageTask::new_with_environment(
        source.visibility_owner.source().clone(),
        source.request.clone(),
        CancelToken::default(),
        source.visibility_owner.options().clone(),
        Some(partial_original_hdr()),
    )
    .unwrap();
    let expected = progressive_complete(without_sheet, 1);
    assert_eq!(snapshot, expected.0[0].1);
    assert_eq!(actual.0, expected.1);
}

#[test]
fn protected_deep_identity_shells_preserve_partial_final_rays_film_and_real_work() {
    for absorbing in [false, true] {
        let (scene, mut request, options) = super::tests::straight_identity_shells_source(8, true, absorbing);
        request.width = 2; // orthographic X range[-16,16] remains inside innermost[-17,17].
        let plan = job::ProtectedPreviewPlan::new(2, 1, 3, options.budget).unwrap();
        let mut task = ImageTask::new_with_environment_and_nested_and_protected_partial(
            scene.clone(),
            request.clone(),
            CancelToken::default(),
            options.clone(),
            None,
            true,
            plan,
        )
        .unwrap();
        let mut control =
            ImageTask::new_with_environment_and_nested_and_partial(scene, request, CancelToken::default(), options, None, true, plan.preview_plan())
                .unwrap();
        let mut rays = Vec::new();
        let mut control_rays = Vec::new();
        reach_partial_frontier(&mut task, 0, 1, &mut rays);
        assert!(task.optical.first().unwrap().pending.is_none());
        assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
        let ordinary = task.protected_preview_report().unwrap().ordinary_used;
        let (_, covered, passes, prefix) = finish_partial_snapshot(&mut task);
        assert_eq!((passes, prefix), (0, 1));
        assert_eq!(covered, [1, 0]);
        assert_eq!(task.protected_preview_report().unwrap().ordinary_used, ordinary);
        let actual = finish_partial_final(&mut task, true, &mut rays);
        let expected = finish_partial_final(&mut control, true, &mut control_rays);
        assert_eq!(actual, expected, "exact final PNG and film bits with optional observation");
        assert_eq!(rays, control_rays);
        assert_eq!(rays.len(), 6);
        let linear = super::tests::straight_shells_expected_linear(8, absorbing);
        let expected_pixel = super::tests::straight_expected_pixel(linear);
        for rgb in &actual.1 {
            for (value, physical) in rgb.iter().copied().map(f64::from_bits).zip(linear) {
                assert!((value - physical).abs() < 1e-11);
            }
        }
        let image = ::image::load_from_memory(&actual.0).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, expected_pixel);
        assert_eq!(image.get_pixel(1, 0).0, expected_pixel);
        let report = task.protected_preview_report().unwrap();
        assert_eq!(report.ordinary_used, control.work.used());
        assert_eq!(report.presentation_used, plan.snapshot_work().unwrap());
    }
}

#[test]
fn every_cpu_pass_seven_has_exact_short_job_pngs_across_worker_quanta() {
    let run = progressive_complete(progressive_fixture(7, 123, Some(true), 0.0), 1);
    assert_eq!(run.0.iter().map(|(samples, _)| *samples).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
    assert_eq!(run, progressive_complete(progressive_fixture(7, 123, Some(true), 0.0), 16_384));
    for (samples, png) in &run.0 {
        assert_eq!(png, &progressive_complete(progressive_fixture(*samples, 123, Some(true), 0.0), 3).1);
    }
}

#[test]
fn every_cpu_pass_area_sample_keys_preserve_history_and_separate_high_ordinal_blocks() {
    for (pixel, ordinal, spatial, primary, expected) in [
        (0_u64, 4095_u32, 0_u8, 1_u8, 0x4152_4541_5341_b2a0_u64),
        (0, 4096, 0, 1, 0x8000_0000_1000_0000),
        (1, 4096, 0, 1, 0x8000_0010_1000_0000),
        (0, 4096, 0, 4, 0x8000_0000_1000_4000),
        (0, 4096, 3, 4, 0x8000_0000_1000_4c00),
        (0, 1_048_575, 15, 16, 0x8000_000f_ffff_bc00),
        (67_108_863, 1_048_575, 15, 16, 0xbfff_ffff_ffff_bc00),
    ] {
        assert_eq!(area_sample_key(pixel, spatial, primary, ordinal).unwrap(), expected);
    }
    for pixel in [0_u64, 1, 67_108_863] {
        for primary in [1_u8, 4, 16] {
            for spatial in 0..primary {
                for ordinal in [0_u32, 1, 63, 4095] {
                    let expected = if primary == 1 && ordinal == 0 {
                        pixel
                    } else {
                        (pixel * 65_536 + u64::from(ordinal) * 16 + u64::from(spatial)) ^ 0x4152_4541_5341_4d50
                    };
                    assert_eq!(area_sample_key(pixel, spatial, primary, ordinal).unwrap(), expected);
                }
                for ordinal in [4096_u32, 65_535, 65_536, 1_048_575] {
                    let mode = match primary {
                        1 => 0_u64,
                        4 => 1,
                        16 => 2,
                        _ => unreachable!(),
                    };
                    let expected = (1_u64 << 63) | (pixel << 36) | (u64::from(ordinal) << 16) | (mode << 14) | (u64::from(spatial) << 10);
                    assert_eq!(area_sample_key(pixel, spatial, primary, ordinal).unwrap(), expected);
                    assert_eq!(expected & 1023, 0);
                    assert_ne!(
                        area_sample_key(pixel, spatial, primary, ordinal).unwrap(),
                        area_sample_key(pixel, spatial, primary, ordinal % 4096).unwrap()
                    );
                }
            }
        }
    }
    assert!(area_sample_key(u64::MAX, 0, 1, 1).is_err());
    assert!(area_sample_key(0, 0, 1, 1_048_576).is_err());
    assert!(area_sample_key(1_u64 << 26, 0, 1, 4096).is_err());
    let mut states = std::collections::BTreeSet::new();
    for pixel in [0_u64, 67_108_863] {
        for ordinal in [4096_u32, 65_535, 1_048_575] {
            for light in 0..4_u64 {
                for sample in 0..64_u64 {
                    let key = area_sample_key(pixel, 0, 1, ordinal).unwrap();
                    let state = key.checked_add(u64::from(u32::MAX) * 65_537 + light * 104_729 + sample * 8191).unwrap();
                    assert_ne!(state & (1_u64 << 63), 0);
                    assert!(states.insert(state), "initial RNG states, not generated UVs, stay distinct");
                }
            }
        }
    }
}

#[test]
fn every_cpu_pass_display_uses_full_u32_coverage_without_saturation() {
    assert_eq!(composite_spatial(Vec3::ZERO, 65_536, 65_536, [13, 27, 41], 0.0).unwrap(), [0, 0, 0, 255]);
    assert_eq!(composite_spatial(Vec3::ZERO, 0, 65_536, [13, 27, 41], 0.0).unwrap(), [13, 27, 41, 255]);
    assert!(composite_spatial(Vec3::ZERO, 65_537, 65_536, [13, 27, 41], 0.0).is_err());
}

#[test]
#[cfg(target_pointer_width = "64")]
fn every_cpu_pass_actual_65536_film_resolves_one_u32_snapshot_and_unique_final() {
    let source = fixture(1, 1, None);
    let mut options = source.visibility_owner.options().clone();
    options.budget = job::CpuBudget::Workstation;
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 65_537, seed: 123 });
    let plan = job::ProtectedPreviewPlan::new(1, 1, 65_537, job::CpuBudget::Workstation).unwrap();
    let request = ImageRequest { raytrace: Some(options.clone()), ..source.request.clone() };
    let mut task = ImageTask::new_with_environment_and_nested_and_protected_partial(
        source.visibility_owner.source().clone(),
        request,
        CancelToken::default(),
        options,
        None,
        false,
        plan,
    )
    .unwrap();
    let mut reached = false;
    for _ in 0..2_000_000 {
        if task.progressive.as_ref().is_some_and(|state| state.film.ready() && state.film.samples() == 65_536)
            && task.partial_preview_ready(plan.preview_plan()).unwrap()
        {
            reached = true;
            break;
        }
        assert!(matches!(task.step_with_partial_previews(16_384).unwrap(), PartialImagePoll::Pending(_)));
    }
    assert!(reached);
    let (linear, covered, count) = task.progressive.as_ref().unwrap().film.pixel(0).unwrap();
    assert_eq!(linear, [0.0; 3]);
    assert_eq!((covered, count), (0, 65_536));
    assert!(matches!(task.begin_partial_preview(plan.preview_plan()).unwrap(), PartialBegin::Started));
    let (png, covered, passes, prefix) = finish_partial_snapshot(&mut task);
    assert_eq!(covered, [0]);
    assert_eq!((passes, prefix), (65_536, 0));
    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, [255; 4]);
    let report = task.protected_preview_report().unwrap();
    assert_eq!((report.accepted_snapshots, report.next_milestone_index), (1, 65_536));
    assert_eq!(report.presentation_used, 65_552);
    let (final_png, bits) = finish_partial_final(&mut task, true, &mut Vec::new());
    assert_eq!(::image::load_from_memory(&final_png).unwrap().to_rgba8().get_pixel(0, 0).0, [255; 4]);
    assert_eq!(bits, [[0.0_f64.to_bits(); 3]]);
    assert!(task.done && task.progressive.is_none());
}
