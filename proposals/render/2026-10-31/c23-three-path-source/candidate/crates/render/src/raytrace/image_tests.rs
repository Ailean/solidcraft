use super::*;
use solidcraft_geom::Aabb3;
fn emission_fixture(reverse: bool) -> (ProductScene, ImageRequest, RaytraceOptions) {
    use std::sync::Arc;
    let mut meshes = Vec::new();
    for (name, lo, hi) in [("emitter", -20.0, 0.0), ("receiver", 0.0, 20.0)] {
        let mesh = solidcraft_geom::Mesh {
            positions: vec![Vec3::new(lo, -10.0, 0.0), Vec3::new(hi, -10.0, 0.0), Vec3::new(hi, 10.0, 0.0), Vec3::new(lo, 10.0, 0.0)],
            normals: vec![Vec3::Z; 4],
            triangles: if reverse { vec![[0, 2, 1], [0, 3, 2]] } else { vec![[0, 1, 2], [0, 2, 3]] },
            tri_face: vec![0; 2],
            ..Default::default()
        };
        meshes.push(crate::ProductMesh {
            transmission: None,
            name: name.into(),
            mesh: Arc::new(mesh),
            color: [255; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        });
    }
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let mut request = request();
    request.width = 2;
    request.height = 1;
    request.camera.distance = 25.0;
    request.camera.fov = 0.0;
    request.camera.set_view(crate::StandardView::Top);
    let options = RaytraceOptions {
        lights: Some(Vec::new()),
        emissions: Some(vec![job::SurfaceEmission {
            body_index: 0,
            body: "emitter".into(),
            color: [0.25, 0.5, 1.0],
            strength: 1.0,
            sides: job::EmissionSides::Front,
        }]),
        ..Default::default()
    };
    (ProductScene { meshes, bounds }, request, options)
}
fn emission_png(scene: ProductScene, mut request: ImageRequest, options: RaytraceOptions) -> Vec<u8> {
    request.raytrace = Some(options.clone());
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    for _ in 0..10_000 {
        if let TaskPoll::Complete(png) = task.step(16384).unwrap() {
            return png;
        }
    }
    panic!("emission fixture did not complete");
}
#[test]
fn emission_actual_png_adds_linear_radiance_once_and_never_lights_neighbor() {
    let (scene, request, mut options) = emission_fixture(false);
    for ev in [0.0, 1.0] {
        options.exposure_ev = ev;
        let image = ::image::load_from_memory(&emission_png(scene.clone(), request.clone(), options.clone())).unwrap().to_rgba8();
        let expected = [0.25, 0.5, 1.0].map(|c| {
            let v = c * 2.0_f64.powf(ev);
            encode8(v / (1.0 + v))
        });
        assert_eq!(image.get_pixel(0, 0).0, [expected[0], expected[1], expected[2], 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 255]);
    }
}
#[test]
fn emission_zero_default_png_equivalence_and_geometric_sidedness() {
    let (scene, request, mut options) = emission_fixture(false);
    options.emissions.as_mut().unwrap()[0].strength = 0.0;
    let zero = emission_png(scene.clone(), request.clone(), options.clone());
    options.emissions = None;
    assert_eq!(zero, emission_png(scene, request, options));
    let (scene, request, mut options) = emission_fixture(true);
    let image = ::image::load_from_memory(&emission_png(scene.clone(), request.clone(), options.clone())).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255], "shading normals cannot turn a back face into an emitter");
    options.emissions.as_mut().unwrap()[0].sides = job::EmissionSides::Both;
    let image = ::image::load_from_memory(&emission_png(scene, request, options)).unwrap().to_rgba8();
    assert!(image.get_pixel(0, 0).0[0] > 0);
}
#[test]
fn emission_stale_duplicate_and_capacity_targets_refuse_before_work() {
    let (scene, mut request, mut options) = emission_fixture(false);
    options.emissions.as_mut().unwrap()[0].body = "renamed".into();
    request.raytrace = Some(options.clone());
    assert_eq!(admit(&scene, &request, &options).unwrap_err().code, ErrorCode::InvalidScene);
    let (_, _, mut options) = emission_fixture(false);
    let first = options.emissions.as_ref().unwrap()[0].clone();
    options.emissions.as_mut().unwrap().push(first);
    assert!(options.validate().is_err());
    options.emissions = Some(Vec::with_capacity(64));
    assert!(options.validate().is_err());
}
#[test]
fn emission_survives_face_reflectance_override_without_changing_captured_geometry() {
    let (mut scene, request, options) = emission_fixture(false);
    let original = scene.meshes[0].mesh.clone();
    let baseline = emission_png(scene.clone(), request.clone(), options.clone());
    scene.meshes[0].faces.push(crate::FaceAppearance { transmission: None, face: 0, color: [0; 3], opacity: 1.0, surface: None });
    assert_eq!(baseline, emission_png(scene.clone(), request, options));
    assert!(std::sync::Arc::ptr_eq(&original, &scene.meshes[0].mesh));
    assert_eq!(scene.meshes[0].faces[0].color, [0; 3]);
}
#[test]
fn emission_scan_observes_cancel_and_exhausts_shared_work_without_png() {
    let (scene, mut request, options) = emission_fixture(false);
    request.raytrace = Some(options.clone());
    let cancel = CancelToken::default();
    let mut task = ImageTask::new(scene, request, cancel.clone(), options).unwrap();
    for _ in 0..10_000 {
        if task.pending.as_ref().is_some_and(|p| p.hit.is_some() && p.light == task.lights.lights().len() && p.emission_cursor == 0) {
            task.work.limit = task.work.used() + 1;
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
            assert_eq!(task.step(1).err().unwrap().code, ErrorCode::LimitExceeded);
            assert!(!task.done);
            assert!(task.pixels.is_empty());
            cancel.cancel();
            assert_eq!(task.step(1).err().unwrap().code, ErrorCode::Cancelled);
            return;
        }
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
    }
    panic!("fixture never reached shading");
}
#[test]
fn thirty_two_emission_bindings_resume_with_four_work_units_and_cancel_without_publication() {
    for cancel_midway in [false, true] {
        let (mut scene, mut request, mut options) = emission_fixture(false);
        while scene.meshes.len() < 32 {
            let mut mesh = scene.meshes[1].clone();
            mesh.name = format!("receiver {}", scene.meshes.len());
            scene.meshes.push(mesh);
        }
        let base = options.emissions.as_ref().unwrap()[0].clone();
        let mut values = Vec::with_capacity(32);
        for index in (1..32).chain(std::iter::once(0)) {
            values.push(job::SurfaceEmission { body_index: index, body: scene.meshes[index].name.clone(), ..base.clone() });
        }
        options.emissions = Some(values);
        request.raytrace = Some(options.clone());
        let cancel = CancelToken::default();
        let mut task = ImageTask::new(scene, request, cancel.clone(), options).unwrap();
        for _ in 0..100_000 {
            if task.pending.as_ref().is_some_and(|p| p.hit.is_some() && p.light == task.lights.lights().len() && p.emission_cursor == 0) {
                break;
            }
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        }
        assert_eq!(task.pending.as_ref().unwrap().emission_cursor, 0);
        assert_eq!(task.pending.as_ref().unwrap().radiance, Vec3::ZERO);
        for expected_cursor in (4..=32).step_by(4) {
            let before = task.work.used();
            assert!(matches!(task.step(16384).unwrap(), TaskPoll::Pending(_)));
            assert_eq!(task.work.used() - before, 4);
            let pending = task.pending.as_ref().unwrap();
            assert_eq!(pending.emission_cursor, expected_cursor);
            assert_eq!(pending.radiance, if expected_cursor == 32 { Vec3::new(0.25, 0.5, 1.0) } else { Vec3::ZERO });
            assert!(task.pixels.is_empty());
            if cancel_midway && expected_cursor == 16 {
                cancel.cancel();
                assert_eq!(task.step(4).err().unwrap().code, ErrorCode::Cancelled);
                assert_eq!(task.work.used() - before, 4);
                assert!(task.pixels.is_empty());
                break;
            }
        }
        if !cancel_midway {
            let before = task.work.used();
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
            assert_eq!(task.work.used() - before, 1);
            assert_eq!(task.pixel, 1);
            assert_eq!(task.pixels, [encode8(0.25 / 1.25), encode8(0.5 / 1.5), encode8(0.5), 255]);
        }
    }
}
fn request() -> ImageRequest {
    ImageRequest {
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        environment: None,
        textures: None,
        integrator: job::Integrator::Raytrace,
        raytrace: None,
        camera: Default::default(),
        width: 8,
        height: 8,
        background: [10, 20, 30],
        transparent_background: false,
    }
}
#[test]
fn image_work_exhaustion_is_typed_and_never_publishes_partial_png() {
    let mut task =
        ImageTask::new(ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY }, request(), CancelToken::default(), Default::default()).unwrap();
    task.work = WorkBudget::new(2).unwrap();
    let mut previous = 0;
    for _ in 0..20 {
        match task.step(1) {
            Ok(TaskPoll::Pending(_)) => {
                assert!(task.work.used() >= previous);
                previous = task.work.used();
            }
            Ok(TaskPoll::Complete(_)) => panic!("exhausted task published a frame"),
            Err(e) => {
                assert_eq!(e.code, ErrorCode::LimitExceeded);
                assert_eq!(task.work.used(), 2);
                return;
            }
        }
    }
    panic!("tiny budget did not stop image");
}
#[test]
fn thin_shadow_occluder_survives_large_unrelated_translation() {
    for x in [0.0, 1.0e12 - 100.0] {
        let vertices = [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 20.0, 0.0, 0.0), Vec3::new(x, 20.0, 0.0)];
        let receiver = Triangle::new(vertices).unwrap();
        let primary = Ray::new(Vec3::new(x + 5.0, 5.0, 10.0), -Vec3::Z, 0.0, 20.0).unwrap();
        let geometry = receiver.intersect(&primary).unwrap();
        let shadow = shadow_ray(primary, super::super::Hit { primitive_index: 0, geometry }, Vec3::Z, vertices).unwrap();
        assert!(shadow.interval()[0] > 0.0, "secondary rays exclude signed-zero distance after underflow");
        assert!(receiver.intersect(&shadow).is_none(), "self-shadow at translation {x}");
        let occluder = Triangle::new(vertices.map(|v| v + Vec3::Z * 0.005)).unwrap();
        assert!(occluder.intersect(&shadow).is_some(), "thin blocker was skipped at translation {x}");
    }
}
#[test]
fn primary_intervals_follow_camera_clip_planes_for_both_projections_and_up_axes() {
    for fov in [0.0, 0.7] {
        for up in [crate::UpAxis::Z, crate::UpAxis::Y] {
            let mut request = request();
            request.camera.fov = fov;
            request.camera.up = up;
            let clip = request.camera.clip(20.0);
            let ray = primary_ray(&request, 4.0, 4.0, clip).unwrap();
            let forward = -request.camera.back();
            let near_depth = (ray.origin() - request.camera.eye()).dot(forward);
            let far_depth = (ray.at(ray.interval()[1]).unwrap() - request.camera.eye()).dot(forward);
            assert!((near_depth - clip.0).abs() < 1e-9);
            assert!((far_depth - clip.1).abs() < 1e-9);
        }
    }
}
#[test]
fn resumable_cursor_matches_original_visibility_and_charges_each_unit() {
    let cancel = CancelToken::default();
    let triangle = Triangle::new([Vec3::ZERO, Vec3::X, Vec3::Y]).unwrap();
    let mut builder = BvhBuilder::new(vec![triangle], limits(job::CpuBudget::Compact), &cancel).unwrap();
    while !builder.step(1, &cancel).unwrap().complete {}
    let bvh = builder.finish().unwrap();
    let ray = Ray::new(Vec3::new(0.2, 0.2, 1.0), -Vec3::Z, 0.0, 2.0).unwrap();
    let expected = bvh.closest_hit(&ray, &mut WorkBudget::new(100).unwrap(), &cancel).unwrap();
    let mut cursor = TraceCursor::new(ray, false);
    let mut work = WorkBudget::new(100).unwrap();
    for _ in 0..100 {
        let before = work.used();
        let result = cursor.step(&bvh, &mut work, &cancel).unwrap();
        assert_eq!(work.used(), before + 1);
        if let Some(actual) = result {
            assert_eq!(actual, expected);
            return;
        }
    }
    panic!("cursor failed to finish");
}
#[test]
fn actual_png_has_analytic_lit_and_cast_shadow_pixels() {
    use crate::ProductMesh;
    use solidcraft_geom::Mesh;
    use std::sync::Arc;
    fn plane(z: f64, x0: f64, x1: f64) -> ProductMesh {
        let mesh = Mesh {
            positions: vec![Vec3::new(x0, -10.0, z), Vec3::new(x1, -10.0, z), Vec3::new(x1, 10.0, z), Vec3::new(x0, 10.0, z)],
            normals: vec![Vec3::Z; 4],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            tri_face: vec![0, 0],
            ..Default::default()
        };
        ProductMesh {
            transmission: None,
            name: format!("plane {z}"),
            mesh: Arc::new(mesh),
            color: [255; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 1.0, 1.0, 1.5).unwrap()),
            faces: Vec::new(),
        }
    }
    let meshes = vec![plane(0.0, -20.0, 20.0), plane(5.0, -5.0, 5.0)];
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let scene = ProductScene { meshes, bounds };
    let mut request = request();
    request.width = 40;
    request.height = 40;
    request.camera.distance = 50.0;
    request.camera.set_view(crate::StandardView::Top);
    let options = RaytraceOptions {
        lights: Some(vec![job::DirectionalLightOptions { direction: [-1.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 }]),
        ..Default::default()
    };
    fn render(scene: ProductScene, mut request: ImageRequest, options: RaytraceOptions) -> ::image::RgbaImage {
        request.raytrace = Some(options.clone());
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        for _ in 0..100_000 {
            if let TaskPoll::Complete(png) = task.step(16384).unwrap() {
                return ::image::load_from_memory(&png).unwrap().to_rgba8();
            }
        }
        panic!("small analytic render did not complete");
    }
    let shadowed = render(scene.clone(), request.clone(), options.clone());
    let lit = render(scene, request, RaytraceOptions { shadows: false, ..options });
    // Top view: screen x maps world x, width40 spans[-20,20]. Blocker[-5,5]
    // at z5 casts a footprint[0,10] for the light toward(-1,0,+1).
    let shadow = shadowed.get_pixel(27, 20).0;
    let unshadowed = lit.get_pixel(27, 20).0;
    assert_eq!(shadow, [0, 0, 0, 255]);
    // White fully rough conductor: F=1, D=1/pi, Smith visibility
    // at normal view yields BRDF=1/[2*pi*(1+cosL)].
    let cosine = (0.5_f64).sqrt();
    let radiance = cosine / (2.0 * std::f64::consts::PI * (1.0 + cosine));
    let expected = encode8(radiance / (1.0 + radiance));
    assert_eq!(unshadowed, [expected, expected, expected, 255]);
    assert_eq!(shadowed.get_pixel(7, 20), lit.get_pixel(7, 20), "unoccluded receiver changes when toggling shadows");
}
fn triangle_task(cancel: CancelToken) -> ImageTask {
    use std::sync::Arc;
    let mesh = solidcraft_geom::Mesh {
        positions: vec![Vec3::new(-20.0, -20.0, 0.0), Vec3::new(20.0, -20.0, 0.0), Vec3::new(0.0, 20.0, 0.0)],
        normals: vec![Vec3::Z; 3],
        triangles: vec![[0, 1, 2]],
        tri_face: vec![0],
        ..Default::default()
    };
    let bounds = mesh.bounds();
    let scene = ProductScene {
        meshes: vec![crate::ProductMesh {
            transmission: None,
            name: "work fixture".into(),
            mesh: Arc::new(mesh),
            color: [220; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        }],
        bounds,
    };
    let mut request = request();
    request.camera.distance = 50.0;
    request.camera.set_view(crate::StandardView::Top);
    ImageTask::new(scene, request, cancel, Default::default()).unwrap()
}
#[test]
fn build_primary_and_shadows_share_cumulative_work_and_exhaust_atomically() {
    let mut task = triangle_task(CancelToken::default());
    let mut previous = 0;
    let mut built = false;
    for _ in 0..10_000 {
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        assert!(task.work.used() >= previous);
        previous = task.work.used();
        built |= task.builder.is_some();
        if task.pending.as_ref().is_some_and(|p| p.hit.is_some() && p.light < task.lights.lights().len()) {
            assert!(built);
            assert!(task.build_used > 0);
            assert!(task.work.used() > task.build_used);
            task.work.limit = task.work.used() + 1;
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
            assert_eq!(task.step(1).err().unwrap().code, ErrorCode::LimitExceeded);
            return;
        }
    }
    panic!("triangle fixture never reached shadow traversal");
}
#[test]
fn cancellation_is_observed_in_every_ray_image_phase() {
    for phase in 0..5 {
        let cancel = CancelToken::default();
        let mut task = triangle_task(cancel.clone());
        let mut reached = false;
        for _ in 0..100_000 {
            let current = match phase {
                0 => task.builder.is_none() && task.bvh.is_none(),
                1 => task.builder.is_some(),
                2 => task.pending.as_ref().is_some_and(|p| p.hit.is_none()),
                3 => task.pending.as_ref().is_some_and(|p| p.hit.is_some()),
                _ => task.encoder.is_some(),
            };
            if current {
                reached = true;
                break;
            }
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        }
        assert!(reached, "phase {phase}");
        cancel.cancel();
        assert_eq!(task.step(4).err().unwrap().code, ErrorCode::Cancelled);
    }
}
#[test]
fn runtime_presets_reach_the_lighting_consumer() {
    for (runtime, expected) in [
        (job::LightingPreset::Neutral, StudioPreset::Neutral),
        (job::LightingPreset::WarmCool, StudioPreset::WarmCool),
        (job::LightingPreset::KeyRim, StudioPreset::KeyRim),
    ] {
        let options = RaytraceOptions { preset: Some(runtime), ..Default::default() };
        assert_eq!(lighting(&options).unwrap(), Lighting::preset(expected).unwrap());
    }
    let conflict = RaytraceOptions { preset: Some(job::LightingPreset::Neutral), lights: Some(Vec::new()), ..Default::default() };
    assert_eq!(lighting(&conflict).err().unwrap().code, ErrorCode::InvalidRequest);
}
#[test]
fn warm_cool_preset_changes_actual_triangle_png() {
    fn render(preset: job::LightingPreset) -> Vec<u8> {
        let initial = triangle_task(CancelToken::default());
        let options = RaytraceOptions { preset: Some(preset), ..Default::default() };
        let mut request = initial.request.clone();
        request.raytrace = Some(options.clone());
        let mut task = ImageTask::new(initial.visibility_owner.source().clone(), request, CancelToken::default(), options).unwrap();
        for _ in 0..100_000 {
            if let TaskPoll::Complete(bytes) = task.step(16384).unwrap() {
                return bytes;
            }
        }
        panic!("preset render did not complete");
    }
    let neutral = ::image::load_from_memory(&render(job::LightingPreset::Neutral)).unwrap().to_rgba8();
    let warm = ::image::load_from_memory(&render(job::LightingPreset::WarmCool)).unwrap().to_rgba8();
    assert_ne!(neutral.get_pixel(4, 4), warm.get_pixel(4, 4));
    assert_eq!(neutral.get_pixel(0, 0), warm.get_pixel(0, 0), "background must remain independent of lighting preset");
}

#[test]
fn emission_primary_aa_and_reflection_attempts_preserve_single_linear_value() {
    for s in [1, 4, 16] {
        for r in [1, 4, 16] {
            let (mut scene, request, mut options) = emission_fixture(false);
            for body in &mut scene.meshes {
                let mesh = std::sync::Arc::make_mut(&mut body.mesh);
                for p in &mut mesh.positions {
                    p.x *= 100.0;
                    p.y *= 100.0;
                }
            }
            scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
            options.primary_samples = s;
            options.reflections = Some(job::ReflectionOptions {
                bodies: vec![job::ReflectionBody { body: "receiver".into(), mode: job::ReflectionMode::Mirror }],
                max_bounces: 1,
                samples: r,
            });
            let image = ::image::load_from_memory(&emission_png(scene, request, options)).unwrap().to_rgba8();
            assert_eq!(image.get_pixel(0, 0).0, [encode8(0.25 / 1.25), encode8(0.5 / 1.5), encode8(0.5), 255], "S{s}/R{r}");
            assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 255], "emission does not illuminate the neighboring reflector");
        }
    }
}

#[test]
fn emission_and_directional_point_area_terms_share_one_linear_output_transform() {
    let (mut scene, mut request, mut options) = emission_fixture(false);
    scene.meshes.truncate(1);
    let body = &mut scene.meshes[0];
    for p in &mut std::sync::Arc::make_mut(&mut body.mesh).positions {
        p.x += 10.0;
    }
    body.surface = Some(OpaqueSurface::new(Vec3::new(1.0, 1.0, 1.0), 1.0, 1.0, 1.5).unwrap());
    scene.bounds = body.mesh.bounds();
    request.width = 1;
    request.height = 1;
    options.shadows = false;
    options.lights = Some(vec![job::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
    options.punctual_lights = Some(vec![job::PunctualLightOptions {
        position_mm: [0.0, 0.0, 10.0],
        color: [1.0; 3],
        intensity: 3.0,
        reference_distance_mm: 10.0,
        minimum_distance_mm: 0.001,
        cone: None,
    }]);
    options.area_lights = Some(vec![job::RectangleLightOptions {
        center_mm: [0.0, 0.0, 10.0],
        edge_u_mm: [2.0, 0.0, 0.0],
        edge_v_mm: [0.0, -2.0, 0.0],
        color: [1.0; 3],
        intensity: 5.0,
    }]);
    options.area_samples = Some(1);
    let image = ::image::load_from_memory(&emission_png(scene, request, options)).unwrap().to_rgba8();
    // At normal incidence a white fully rough conductor has BRDF 1/(4*pi).
    // Point reference distance is 10 mm; the 4 mm² rectangle contributes 5*4/100.
    let direct = (2.0 + 3.0 + 5.0 * 4.0 / 100.0) / (4.0 * std::f64::consts::PI);
    let expected = [0.25, 0.5, 1.0].map(|e| encode8((e + direct) / (1.0 + e + direct)));
    assert_eq!(image.get_pixel(0, 0).0, [expected[0], expected[1], expected[2], 255]);
}

#[test]
fn reflected_hit_adds_surface_emission_with_the_current_path_throughput() {
    use std::sync::Arc;
    let normal = Vec3::new(1.0, 0.0, 1.0) / 2.0_f64.sqrt();
    let mirror_mesh = solidcraft_geom::Mesh {
        positions: vec![Vec3::new(-20.0, -20.0, 20.0), Vec3::new(20.0, -20.0, -20.0), Vec3::new(20.0, 20.0, -20.0), Vec3::new(-20.0, 20.0, 20.0)],
        normals: vec![normal; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0; 2],
        ..Default::default()
    };
    let emitter_mesh = solidcraft_geom::Mesh {
        positions: vec![Vec3::new(10.0, -4.0, -2.0), Vec3::new(10.0, -4.0, 2.0), Vec3::new(10.0, 4.0, 2.0), Vec3::new(10.0, 4.0, -2.0)],
        normals: vec![-Vec3::X; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0; 2],
        ..Default::default()
    };
    let base_color = Vec3::new(0.2, 0.5, 0.75);
    let meshes = vec![
        crate::ProductMesh {
            transmission: None,
            name: "mirror".into(),
            mesh: Arc::new(mirror_mesh),
            color: [255; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(base_color, 1.0, 0.1, 1.5).unwrap()),
            faces: Vec::new(),
        },
        crate::ProductMesh {
            transmission: None,
            name: "emitter".into(),
            mesh: Arc::new(emitter_mesh),
            color: [0; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        },
    ];
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let mut request = request();
    request.width = 1;
    request.height = 1;
    request.camera.distance = 50.0;
    request.camera.fov = 0.0;
    request.camera.set_view(crate::StandardView::Top);
    let options = RaytraceOptions {
        lights: Some(Vec::new()),
        shadows: false,
        emissions: Some(vec![job::SurfaceEmission {
            body_index: 1,
            body: "emitter".into(),
            color: [0.25, 0.5, 1.0],
            strength: 2.0,
            sides: job::EmissionSides::Front,
        }]),
        reflections: Some(job::ReflectionOptions {
            bodies: vec![job::ReflectionBody { body: "mirror".into(), mode: job::ReflectionMode::Mirror }],
            max_bounces: 1,
            samples: 1,
        }),
        ..Default::default()
    };
    let image = ::image::load_from_memory(&emission_png(ProductScene { meshes, bounds }, request, options)).unwrap().to_rgba8();
    let schlick = (1.0 - 0.5_f64.sqrt()).powi(5);
    let expected = [(0.2, 0.5), (0.5, 1.0), (0.75, 2.0)].map(|(f0, emitted)| {
        let radiance = (f0 + (1.0 - f0) * schlick) * emitted;
        encode8(radiance / (1.0 + radiance))
    });
    assert_eq!(image.get_pixel(0, 0).0, [expected[0], expected[1], expected[2], 255]);
}

#[test]
fn procedural_effects_change_completed_pngs_independently_and_zero_is_byte_identical() {
    use crate::{ProductMesh, surface_patterns::SurfaceEffect};
    use solidcraft_geom::Mesh;
    use std::sync::Arc;
    let mesh = Arc::new(Mesh {
        positions: vec![Vec3::new(-10.0, -10.0, 0.0), Vec3::new(10.0, -10.0, 0.0), Vec3::new(10.0, 10.0, 0.0), Vec3::new(-10.0, 10.0, 0.0)],
        normals: vec![Vec3::Z; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0, 0],
        ..Default::default()
    });
    let original = mesh.positions.clone();
    let scene = ProductScene {
        bounds: mesh.bounds(),
        meshes: vec![ProductMesh {
            transmission: None,
            name: "Coupon".into(),
            mesh: mesh.clone(),
            color: [180; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::new(0.7, 0.5, 0.3), 1.0, 0.4, 1.5).unwrap()),
            faces: Vec::new(),
        }],
    };
    let effect = SurfaceEffect {
        body: "Coupon".into(),
        frame: Default::default(),
        repeat_mm: [12.0, 17.0, 20.0],
        roughness_amplitude: 0.0,
        bump_amplitude_mm: 0.0,
    };
    let render = |effect: Option<SurfaceEffect>| {
        let mut request = request();
        request.width = 32;
        request.height = 32;
        request.camera.distance = 40.0;
        request.camera.set_view(crate::StandardView::Top);
        let options = RaytraceOptions { surface_effects: effect.map(|e| vec![e]), shadows: false, ..Default::default() };
        request.raytrace = Some(options.clone());
        let mut task = ImageTask::new(scene.clone(), request, CancelToken::default(), options).unwrap();
        for _ in 0..100_000 {
            if let TaskPoll::Complete(png) = task.step(16384).unwrap() {
                return png;
            }
        }
        panic!("small surface fixture did not complete");
    };
    let baseline = render(None);
    assert_eq!(baseline, render(Some(effect.clone())));
    let rough = render(Some(SurfaceEffect { roughness_amplitude: 0.3, ..effect.clone() }));
    let bump = render(Some(SurfaceEffect { bump_amplitude_mm: 0.3, ..effect }));
    let base = ::image::load_from_memory(&baseline).unwrap().to_rgba8();
    for changed in [&rough, &bump] {
        let decoded = ::image::load_from_memory(changed).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), base.dimensions());
        assert_ne!(decoded, base);
        for (a, b) in decoded.pixels().zip(base.pixels()) {
            assert_eq!(a[3], b[3]);
        }
    }
    assert_ne!(rough, bump);
    assert_eq!(mesh.positions, original);
}

#[test]
fn surface_target_refusals_are_atomic_before_pixels_or_bvh_allocation() {
    use crate::surface_patterns::SurfaceEffect;
    let effect =
        SurfaceEffect { body: "missing".into(), frame: Default::default(), repeat_mm: [20.0; 3], roughness_amplitude: 0.2, bump_amplitude_mm: 0.0 };
    let scene = ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY };
    for effects in [vec![effect.clone()], vec![effect.clone(), effect]] {
        let options = RaytraceOptions { surface_effects: Some(effects), ..Default::default() };
        let mut request = request();
        request.raytrace = Some(options.clone());
        assert_eq!(admit(&scene, &request, &options).unwrap_err().code, ErrorCode::InvalidRequest);
    }
}

fn surface_consumer_fixture() -> (ProductScene, ImageRequest, RaytraceOptions) {
    use crate::{ProductMesh, surface_patterns::SurfaceEffect};
    use solidcraft_geom::Mesh;
    use std::sync::Arc;
    let mesh = Arc::new(Mesh {
        positions: vec![Vec3::new(-20.0, -20.0, 0.0), Vec3::new(20.0, -20.0, 0.0), Vec3::new(20.0, 20.0, 0.0), Vec3::new(-20.0, 20.0, 0.0)],
        normals: vec![Vec3::Z; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0, 0],
        ..Default::default()
    });
    let scene = ProductScene {
        bounds: mesh.bounds(),
        meshes: vec![ProductMesh {
            transmission: None,
            name: "Coupon".into(),
            mesh,
            color: [180; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::new(0.7, 0.5, 0.3), 1.0, 0.4, 1.5).unwrap()),
            faces: Vec::new(),
        }],
    };
    let options = RaytraceOptions {
        surface_effects: Some(vec![SurfaceEffect {
            body: "Coupon".into(),
            frame: Default::default(),
            repeat_mm: [12.0, 17.0, 20.0],
            roughness_amplitude: 0.2,
            bump_amplitude_mm: 0.3,
        }]),
        lights: Some(Vec::new()),
        shadows: false,
        ..Default::default()
    };
    let mut request = request();
    request.width = 1;
    request.height = 1;
    request.camera.distance = 40.0;
    request.camera.set_view(crate::StandardView::Top);
    request.raytrace = Some(options.clone());
    (scene, request, options)
}

fn surface_consumer_png(mut task: ImageTask, quantum: usize) -> Vec<u8> {
    for _ in 0..200_000 {
        let used = task.work.used();
        let poll = task.step(quantum).unwrap();
        assert!(task.work.used() - used <= 4, "surface consumer exceeded public work quantum");
        if let TaskPoll::Complete(png) = poll {
            return png;
        }
    }
    panic!("bounded surface consumer did not complete");
}

fn surface_constant_environment() -> crate::lighting::environment_input::EnvironmentInput {
    use crate::lighting::{HdrSource, environment_input::EnvironmentInput};
    let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nPRIMARIES=0.64 0.33 0.30 0.60 0.15 0.06 0.3127 0.3290\n\n-Y 2 +X 4\n".to_vec();
    for _ in 0..8 {
        bytes.extend_from_slice(&[128, 64, 32, 129]);
    }
    EnvironmentInput::new(HdrSource::new(bytes).unwrap(), 0.0, 1.0, 4).unwrap()
}

#[test]
fn surface_effect_shared_hit_reaches_directional_point_area_and_hdr_consumers() {
    use crate::job::{DirectionalLightOptions, PunctualLightOptions, RectangleLightOptions};
    use std::sync::Arc;
    for mask in 1..16 {
        for shadows in [false, true] {
            let (scene, mut request, mut options) = surface_consumer_fixture();
            options.shadows = shadows;
            if mask & 1 != 0 {
                options.lights = Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
            }
            if mask & 2 != 0 {
                options.punctual_lights = Some(vec![PunctualLightOptions {
                    position_mm: [0.0, 0.0, 10.0],
                    color: [1.0; 3],
                    intensity: 3.0,
                    reference_distance_mm: 10.0,
                    minimum_distance_mm: 0.001,
                    cone: None,
                }]);
            }
            if mask & 4 != 0 {
                options.area_lights = Some(vec![RectangleLightOptions {
                    center_mm: [0.0, 0.0, 10.0],
                    edge_u_mm: [2.0, 0.0, 0.0],
                    edge_v_mm: [0.0, -2.0, 0.0],
                    color: [1.0; 3],
                    intensity: 5.0,
                }]);
                options.area_samples = Some(1);
            }
            let effect = options.surface_effects.as_ref().unwrap()[0].prepare().unwrap();
            let changed = effect.apply(scene.meshes[0].surface.unwrap(), Vec3::Z, Vec3::Z, Vec3::ZERO).unwrap();
            let mut reference_scene = scene.clone();
            reference_scene.meshes[0].surface = Some(changed.surface);
            Arc::make_mut(&mut reference_scene.meshes[0].mesh).normals.fill(changed.shading_normal);
            let mut reference_options = options.clone();
            reference_options.surface_effects = None;
            let reference_request = ImageRequest { raytrace: Some(reference_options.clone()), ..request.clone() };
            request.raytrace = Some(options.clone());
            let environment = || if mask & 8 != 0 { Some(surface_constant_environment()) } else { None };
            let expected = surface_consumer_png(
                ImageTask::new_with_environment(reference_scene, reference_request, CancelToken::default(), reference_options, environment())
                    .unwrap(),
                1,
            );
            for quantum in [1, 4, 16384] {
                let actual = surface_consumer_png(
                    ImageTask::new_with_environment(scene.clone(), request.clone(), CancelToken::default(), options.clone(), environment()).unwrap(),
                    quantum,
                );
                assert_eq!(actual, expected, "mask={mask}, shadows={shadows}, quantum={quantum}");
            }
        }
    }
}

fn surface_at_thirty_two_binding_scan() -> ImageTask {
    use crate::{ProductMesh, surface_patterns::SurfaceEffect};
    use solidcraft_geom::Mesh;
    use std::sync::Arc;
    let (mut scene, mut request, mut options) = surface_consumer_fixture();
    let visible = scene.meshes.pop().unwrap();
    let mut effects = Vec::with_capacity(32);
    for index in 0..31 {
        let name = format!("unused-{index}");
        scene.meshes.push(ProductMesh {
            transmission: None,
            name: name.clone(),
            mesh: Arc::new(Mesh::default()),
            color: [255; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        });
        effects.push(SurfaceEffect { body: name, frame: Default::default(), repeat_mm: [20.0; 3], roughness_amplitude: 0.1, bump_amplitude_mm: 0.0 });
    }
    scene.meshes.push(visible);
    effects.push(options.surface_effects.take().unwrap().remove(0));
    options.surface_effects = Some(effects);
    request.raytrace = Some(options.clone());
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    for _ in 0..20_000 {
        if task.pending.as_ref().is_some_and(|p| p.hit.is_some() && !p.surface_ready && p.surface_cursor == 0) {
            return task;
        }
        task.step(1).unwrap();
    }
    panic!("did not reach actual unresolved surface scan");
}

#[test]
fn thirty_two_surface_bindings_resume_and_refuse_cancel_or_work_exhaustion_without_png() {
    let mut task = surface_at_thirty_two_binding_scan();
    for expected_cursor in [4, 8, 12, 16, 20, 24, 28, 32] {
        let before = task.work.used();
        assert!(matches!(task.step(16384).unwrap(), TaskPoll::Pending(_)));
        assert_eq!(task.work.used() - before, 4);
        let pending = task.pending.as_ref().unwrap();
        assert_eq!(pending.surface_cursor, expected_cursor);
        assert!(!pending.surface_ready);
        assert_eq!(pending.surface_index, if expected_cursor == 32 { Some(31) } else { None });
        assert!(task.pixels.is_empty() && !task.done);
    }
    let png = surface_consumer_png(task, 4);
    assert_eq!(::image::load_from_memory(&png).unwrap().width(), 1);
    for cancel in [false, true] {
        let mut task = surface_at_thirty_two_binding_scan();
        let before = task.work.used();
        if cancel {
            task.cancel.cancel();
        } else {
            task.work.limit = before;
        }
        assert_eq!(task.step(16384).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
        assert_eq!(task.work.used(), before);
        assert_eq!(task.pending.as_ref().unwrap().surface_cursor, 0);
        assert!(task.pixels.is_empty() && !task.done);
    }
}

#[test]
fn surface_effects_reject_ambiguous_targets_and_active_anisotropic_overlap() {
    use crate::job::{AnisotropicFaceOptions, AnisotropicOrientation};
    let (scene, mut request, mut options) = surface_consumer_fixture();
    let mut ambiguous = scene.clone();
    ambiguous.meshes.push(scene.meshes[0].clone());
    assert_eq!(admit(&ambiguous, &request, &options).unwrap_err().code, ErrorCode::InvalidRequest);
    options.anisotropic_faces[0] = Some(AnisotropicFaceOptions {
        body_index: 0,
        body_name: "Coupon".into(),
        face: 0,
        tangent: [1.0, 0.0, 0.0],
        orientation: AnisotropicOrientation::Zero,
        roughness_x: 0.2,
        roughness_y: 0.7,
    });
    request.raytrace = Some(options.clone());
    assert_eq!(admit(&scene, &request, &options).unwrap_err().code, ErrorCode::InvalidRequest);
    let identity = options.surface_effects.as_mut().unwrap().first_mut().unwrap();
    identity.roughness_amplitude = 0.0;
    identity.bump_amplitude_mm = 0.0;
    request.raytrace = Some(options.clone());
    assert!(admit(&scene, &request, &options).is_ok());
}

#[test]
fn reflected_surface_effect_uses_changed_shading_and_resets_before_emitting_secondary_hit() {
    use crate::surface_patterns::SurfaceEffect;
    use std::sync::Arc;
    let normal = Vec3::new(1.0, 0.0, 1.0) / 2.0_f64.sqrt();
    let mirror_mesh = solidcraft_geom::Mesh {
        positions: vec![Vec3::new(-20.0, -20.0, 20.0), Vec3::new(20.0, -20.0, -20.0), Vec3::new(20.0, 20.0, -20.0), Vec3::new(-20.0, 20.0, 20.0)],
        normals: vec![normal; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0; 2],
        ..Default::default()
    };
    let emitter_mesh = solidcraft_geom::Mesh {
        positions: vec![Vec3::new(10.0, -20.0, -20.0), Vec3::new(10.0, -20.0, 20.0), Vec3::new(10.0, 20.0, 20.0), Vec3::new(10.0, 20.0, -20.0)],
        normals: vec![-Vec3::X; 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0; 2],
        ..Default::default()
    };
    let meshes = vec![
        crate::ProductMesh {
            transmission: None,
            name: "mirror".into(),
            mesh: Arc::new(mirror_mesh),
            color: [255; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::new(0.2, 0.5, 0.75), 1.0, 0.1, 1.5).unwrap()),
            faces: Vec::new(),
        },
        crate::ProductMesh {
            transmission: None,
            name: "emitter".into(),
            mesh: Arc::new(emitter_mesh),
            color: [0; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        },
    ];
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let scene = ProductScene { meshes, bounds };
    let effect = SurfaceEffect {
        body: "mirror".into(),
        frame: Default::default(),
        repeat_mm: [12.0, 17.0, 20.0],
        roughness_amplitude: 0.2,
        bump_amplitude_mm: 0.3,
    };
    let changed = effect.prepare().unwrap().apply(scene.meshes[0].surface.unwrap(), normal, normal, Vec3::ZERO).unwrap();
    let mut reference_scene = scene.clone();
    reference_scene.meshes[0].surface = Some(changed.surface);
    Arc::make_mut(&mut reference_scene.meshes[0].mesh).normals.fill(changed.shading_normal);
    let mut request = request();
    request.width = 1;
    request.height = 1;
    request.camera.distance = 50.0;
    request.camera.fov = 0.0;
    request.camera.set_view(crate::StandardView::Top);
    for mode in [job::ReflectionMode::Mirror, job::ReflectionMode::RoughSpecular] {
        let options = RaytraceOptions {
            surface_effects: Some(vec![effect.clone()]),
            lights: Some(Vec::new()),
            shadows: false,
            emissions: Some(vec![job::SurfaceEmission {
                body_index: 1,
                body: "emitter".into(),
                color: [0.25, 0.5, 1.0],
                strength: 2.0,
                sides: job::EmissionSides::Front,
            }]),
            reflections: Some(job::ReflectionOptions {
                bodies: vec![job::ReflectionBody { body: "mirror".into(), mode }],
                max_bounces: 2,
                samples: 16,
            }),
            ..Default::default()
        };
        let mut reference_options = options.clone();
        reference_options.surface_effects = None;
        let expected = emission_png(reference_scene.clone(), request.clone(), reference_options);
        assert_eq!(emission_png(scene.clone(), request.clone(), options), expected, "mode={mode:?}");
        let decoded = ::image::load_from_memory(&expected).unwrap().to_rgba8();
        assert!(decoded.get_pixel(0, 0).0[..3].iter().any(|value| *value > 0));
    }
}

fn dielectric_triangle_task(cancel: CancelToken) -> ImageTask {
    let task = triangle_task(cancel.clone());
    let mut request = task.request;
    let options = RaytraceOptions {
        dielectrics: vec![job::DielectricOptions {
            body_index: 0,
            body_name: "work fixture".into(),
            boundary: job::DielectricBoundary::ThinSheet,
            ior: 1.5,
            absorption_per_mm: [0.0; 3],
            thickness_mm: 0.0,
        }],
        shadows: false,
        ..Default::default()
    };
    request.raytrace = Some(options.clone());
    let (fixture_scene, _) = task.visibility_owner.into_capture();
    ImageTask::new(fixture_scene, request, cancel, options).unwrap()
}

#[test]
fn dielectric_branch_traversal_exhausts_the_same_job_work_budget() {
    let mut task = dielectric_triangle_task(CancelToken::default());
    for _ in 0..100_000 {
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        if task.optical.first().and_then(|optical| optical.pending.as_ref()).is_some_and(|state| state.paths > 1) {
            assert!(task.work.used() > task.build_used);
            task.work.limit = task.work.used();
            assert_eq!(task.step(1).err().unwrap().code, ErrorCode::LimitExceeded);
            assert!(!task.done);
            return;
        }
    }
    panic!("dielectric branch not reached");
}

#[test]
fn completed_dielectric_png_releases_capture_and_path_state() {
    let mut task = dielectric_triangle_task(CancelToken::default());
    let weak = std::sync::Arc::downgrade(&task.visibility_owner.source().meshes[0].mesh);
    for _ in 0..100_000 {
        if let TaskPoll::Complete(png) = task.step(16384).unwrap() {
            assert!(::image::load_from_memory(&png).is_ok());
            assert!(weak.upgrade().is_none());
            assert!(task.optical.is_empty());
            assert!(task.visibility_owner.options().dielectrics.is_empty());
            assert!(task.request.raytrace.is_none());
            assert!(task.bvh.is_none());
            return;
        }
    }
    panic!("dielectric fixture did not complete");
}

#[test]
fn dielectric_fixed_stack_and_path_caps_return_typed_failures() {
    let ray = Ray::new(Vec3::Z, -Vec3::Z, 0.0, 2.0).unwrap();
    let mut state = DielectricPixel::new(ray);
    state.paths = MAX_PATHS;
    assert_eq!(state.push(state.path).err().unwrap().code, ErrorCode::LimitExceeded);
    state.paths = 1;
    state.stack_len = state.stack.len();
    assert_eq!(state.push(state.path).err().unwrap().code, ErrorCode::LimitExceeded);
}

#[test]
fn dielectric_image_path_applies_radiance_eta_squared_at_each_interface() {
    let options = RaytraceOptions {
        dielectrics: vec![dielectric::tests::input(job::DielectricBoundary::ClosedBox)],
        lights: Some(Vec::new()),
        shadows: false,
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera.pitch = std::f64::consts::FRAC_PI_2;
    let mut task = ImageTask::new(dielectric::tests::scene(dielectric::tests::box_body(2.0)), request, CancelToken::default(), options).unwrap();
    let mut entry = None;
    for _ in 0..100_000 {
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        if let Some(state) = task.optical.first().and_then(|optical| optical.pending.as_ref()) {
            let path = state.path;
            if path.boundaries == 1 && path.medium == Some(0) {
                // Air -> glass at normal incidence: (1-F0)/n^2 = .96/2.25.
                let expected = 0.96 / 2.25;
                assert!((path.throughput.x - expected).abs() < 1e-13);
                assert!((path.throughput.y - expected).abs() < 1e-13);
                assert!((path.throughput.z - expected).abs() < 1e-13);
                entry = Some(path.throughput.x);
            }
            if path.boundaries == 2 && path.medium.is_none() {
                // Glass -> air independently multiplies by .96*n^2 = 2.16.
                let ratio = path.throughput.x / entry.expect("entry checked before exit");
                assert!((ratio - 2.16).abs() < 1e-13);
                return;
            }
        }
    }
    panic!("slab's first entry and exit were not observed");
}

fn optical_sheet(name: &str, x: f64, z: f64) -> crate::ProductMesh {
    use std::sync::Arc;
    crate::ProductMesh {
        transmission: None,
        name: name.into(),
        mesh: Arc::new(solidcraft_geom::Mesh {
            positions: vec![Vec3::new(x - 20.0, -20.0, z), Vec3::new(x + 20.0, -20.0, z), Vec3::new(x + 20.0, 20.0, z), Vec3::new(x - 20.0, 20.0, z)],
            normals: vec![Vec3::Z; 4],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..Default::default()
        }),
        color: [255; 3],
        opacity: 1.0,
        surface: None,
        faces: Vec::new(),
    }
}

fn optical_input(body_index: usize, body_name: &str, ior: f64) -> job::DielectricOptions {
    job::DielectricOptions {
        body_index,
        body_name: body_name.into(),
        boundary: job::DielectricBoundary::ThinSheet,
        ior,
        absorption_per_mm: [0.0; 3],
        thickness_mm: 0.0,
    }
}

fn optical_png(mut task: ImageTask, quantum: usize) -> Vec<u8> {
    for _ in 0..2_000_000 {
        let before = task.work.used();
        let poll = task.step(quantum).unwrap();
        assert!(task.work.used() - before <= 4, "an optical continuation exceeded the shared work quantum");
        if let TaskPoll::Complete(png) = poll {
            return png;
        }
    }
    panic!("bounded optical fixture did not finish");
}

#[test]
fn dielectric_branches_sum_inside_each_spatial_and_reflection_attempt() {
    let mut scene = dielectric::tests::scene(dielectric::tests::box_body(2.0));
    scene.meshes.push(optical_sheet("unused reflector", 100.0, 0.0));
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    let mut baseline = None;
    for spatial in [1, 4, 16] {
        for attempts in [1, 4, 16] {
            let options = RaytraceOptions {
                primary_samples: spatial,
                dielectrics: vec![dielectric::tests::input(job::DielectricBoundary::ClosedBox)],
                environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
                reflections: Some(job::ReflectionOptions {
                    bodies: vec![job::ReflectionBody { body: "unused reflector".into(), mode: job::ReflectionMode::Mirror }],
                    samples: attempts,
                    max_bounces: 1,
                }),
                lights: Some(Vec::new()),
                shadows: false,
                ..Default::default()
            };
            let mut request = dielectric::tests::request(options.clone());
            request.camera.distance = 10.0;
            request.camera.fov = 0.0;
            for quantum in [1, 4, 16384] {
                let mut task = ImageTask::new(scene.clone(), request.clone(), CancelToken::default(), options.clone()).unwrap();
                let mut saw_complete_coverage = false;
                let mut done = None;
                for _ in 0..2_000_000 {
                    let before = task.work.used();
                    let poll = task.step(quantum).unwrap();
                    assert!(task.work.used() - before <= 4);
                    assert!(task.primary_coverage <= u16::from(spatial) * u16::from(attempts));
                    if task.emit_spatial_pixel {
                        assert_eq!(task.primary_coverage, u16::from(spatial) * u16::from(attempts), "branches are not coverage samples");
                        saw_complete_coverage = true;
                    }
                    if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref())
                        && state.path.boundaries > 0
                    {
                        assert!(!task.pending.as_ref().unwrap().repeatable_primary, "selected optical paths have no opaque reuse proof");
                    }
                    if let TaskPoll::Complete(png) = poll {
                        done = Some(png);
                        break;
                    }
                }
                let png = done.expect("bounded optical AA fixture completes");
                // A four-unit call may commit/reset coverage before yielding. The
                // one-unit run observes that state; every quantum checks final bytes.
                if spatial != 1 && quantum == 1 {
                    assert!(saw_complete_coverage);
                }
                let rgba = ::image::load_from_memory(&png).unwrap().to_rgba8();
                let pixel = rgba.get_pixel(0, 0).0;
                assert!(pixel[0] > 150 && pixel[0] == pixel[1] && pixel[1] == pixel[2]);
                if let Some(expected) = &baseline {
                    assert_eq!(&png, expected, "S={spatial}, R={attempts}, quantum={quantum}; branches sum, S/R average");
                } else {
                    baseline = Some(png);
                }
            }
        }
    }
}

#[test]
fn unit_ior_sheet_rejoins_surface_emission_and_every_existing_light_consumer() {
    use job::{DirectionalLightOptions, PunctualLightOptions, RectangleLightOptions};
    for mask in 1..16 {
        let (scene, mut request, mut options) = surface_consumer_fixture();
        request.camera.fov = 0.0;
        request.camera.distance = 10.0;
        // Shadows are deliberately off: this preview's shadow rays conservatively
        // treat every dielectric boundary as an opaque blocker.
        options.shadows = false;
        if mask & 1 != 0 {
            options.lights = Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
        }
        if mask & 2 != 0 {
            options.punctual_lights = Some(vec![PunctualLightOptions {
                position_mm: [0.0, 0.0, 10.0],
                color: [1.0; 3],
                intensity: 3.0,
                reference_distance_mm: 10.0,
                minimum_distance_mm: 0.001,
                cone: None,
            }]);
        }
        if mask & 4 != 0 {
            options.area_lights = Some(vec![RectangleLightOptions {
                center_mm: [0.0, 0.0, 10.0],
                edge_u_mm: [2.0, 0.0, 0.0],
                edge_v_mm: [0.0, -2.0, 0.0],
                color: [1.0; 3],
                intensity: 5.0,
            }]);
            options.area_samples = Some(4);
        }
        options.emissions = Some(vec![job::SurfaceEmission {
            body_index: 0,
            body: "Coupon".into(),
            color: [0.1, 0.2, 0.3],
            strength: 0.5,
            sides: job::EmissionSides::Front,
        }]);
        request.raytrace = Some(options.clone());
        let environment = || if mask & 8 != 0 { Some(surface_constant_environment()) } else { None };
        let expected = optical_png(
            ImageTask::new_with_environment(scene.clone(), request.clone(), CancelToken::default(), options.clone(), environment()).unwrap(),
            1,
        );
        let mut transmitted_scene = scene;
        transmitted_scene.meshes.push(optical_sheet("sheet", 0.0, 5.0));
        transmitted_scene.bounds = transmitted_scene.meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
        options.dielectrics = vec![optical_input(1, "sheet", 1.0)];
        request.raytrace = Some(options.clone());
        for quantum in [1, 4, 16384] {
            let actual = optical_png(
                ImageTask::new_with_environment(transmitted_scene.clone(), request.clone(), CancelToken::default(), options.clone(), environment())
                    .unwrap(),
                quantum,
            );
            assert_eq!(actual, expected, "the same opaque light/surface/emission consumer after transmission; mask={mask}");
        }
    }
}

fn optical_at_eight_selector_scan() -> ImageTask {
    let (mut scene, mut request, mut options) = surface_consumer_fixture();
    for i in 0..8 {
        let name = format!("unused sheet {i}");
        let index = scene.meshes.len();
        scene.meshes.push(optical_sheet(&name, 100.0 + 50.0 * f64::from(i), 5.0));
        options.dielectrics.push(optical_input(index, &name, 1.5));
    }
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    request.raytrace = Some(options.clone());
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    for _ in 0..200_000 {
        if task.pending.as_ref().is_some_and(|p| p.hit.is_some())
            && task.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|p| !p.ready && p.selector == 0)
        {
            return task;
        }
        task.step(1).unwrap();
    }
    panic!("actual optical selector scan not reached");
}

#[test]
fn eight_optical_selectors_resume_with_cancel_and_work_checks_before_mutation() {
    let mut task = optical_at_eight_selector_scan();
    for expected in [4, 8] {
        let before = task.work.used();
        assert!(matches!(task.step(16384).unwrap(), TaskPoll::Pending(_)));
        assert_eq!(task.work.used() - before, 4);
        let state = task.optical[0].pending.as_ref().unwrap();
        assert_eq!(state.selector, expected);
        assert!(!state.ready);
        assert!(task.pixels.is_empty());
    }
    assert!(!optical_png(task, 4).is_empty());
    for cancelled in [false, true] {
        let mut task = optical_at_eight_selector_scan();
        let before = task.work.used();
        if cancelled {
            task.cancel.cancel();
        } else {
            task.work.limit = before;
        }
        assert_eq!(task.step(4).err().unwrap().code, if cancelled { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
        assert_eq!(task.work.used(), before);
        assert_eq!(task.optical[0].pending.as_ref().unwrap().selector, 0);
        assert!(task.pixels.is_empty() && task.encoder.is_none() && !task.done);
    }
}

#[test]
fn optical_reuse_rejects_an_undrained_branch_and_a_selected_primary() {
    fn reusable() -> ImageTask {
        let mut task = optical_at_eight_selector_scan();
        // Start a fresh coherent request with an unreachable ordinary reflector to
        // request R=4, and a spatial grid whose visible receiver remains opaque.
        let mut scene = task.visibility_owner.source().clone();
        let mut request = task.request.clone();
        let mut options = task.visibility_owner.options().clone();
        scene.meshes.push(optical_sheet("unused reflector", 1000.0, 0.0));
        scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
        options.primary_samples = 4;
        options.reflections = Some(job::ReflectionOptions {
            bodies: vec![job::ReflectionBody { body: "unused reflector".into(), mode: job::ReflectionMode::Mirror }],
            max_bounces: 1,
            samples: 4,
        });
        request.camera.distance = 10.0;
        request.camera.fov = 0.0;
        request.raytrace = Some(options.clone());
        task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        for _ in 0..200_000 {
            if task.pending.as_ref().is_some_and(|p| p.light == task.lights.lights().len() + 1) {
                assert!(task.optical[0].pending.is_none(), "only drained optics reach the completed opaque marker");
                return task;
            }
            task.step(1).unwrap();
        }
        panic!("completed nonselected primary marker not reached");
    }
    for pending_branch in [false, true] {
        let mut task = reusable();
        if pending_branch {
            task.optical[0].pending = Some(DielectricPixel::new(task.pending.as_ref().unwrap().ray));
        } else {
            let (fixture_scene, mut fixture_options) = task.visibility_owner.into_capture();
            fixture_options.dielectrics[0].body_index = 0;
            fixture_options.dielectrics[0].body_name = "Coupon".into();
            task.visibility_owner = dielectric::DirectVisibilityOwner::new(fixture_scene, fixture_options);
        }
        let before = task.primary_coverage;
        assert_eq!(task.step(1).err().unwrap().code, ErrorCode::InvalidScene);
        assert_eq!(task.primary_coverage, before);
        assert!(task.pixels.is_empty());
    }
}

#[test]
fn optical_admission_bills_capacity_and_refuses_ambiguous_effect_and_environment_contracts() {
    let scene = dielectric::tests::scene(dielectric::tests::box_body(2.0));
    let mut options = RaytraceOptions { dielectrics: vec![dielectric::tests::input(job::DielectricBoundary::ClosedBox)], ..Default::default() };
    let name = scene.meshes[0].name.clone();
    let bytes = options.retained_bytes().unwrap();
    assert_eq!(
        bytes,
        options.dielectrics.capacity() as u64 * std::mem::size_of::<job::DielectricOptions>() as u64
            + options.dielectrics[0].body_name.capacity() as u64
    );
    let mut request = dielectric::tests::request(options.clone());
    let with_optics = admit(&scene, &request, &options).unwrap();
    let plain = RaytraceOptions::default();
    let plain_request = ImageRequest { raytrace: Some(plain.clone()), ..request.clone() };
    let without_optics = admit(&scene, &plain_request, &plain).unwrap();
    assert_eq!(
        with_optics.estimated_total_bytes - without_optics.estimated_total_bytes,
        bytes + request.raytrace.as_ref().unwrap().retained_dielectric_bytes().unwrap() + std::mem::size_of::<OpticalTask>() as u64
    );
    options.surface_effects = Some(vec![crate::surface_patterns::SurfaceEffect {
        body: name,
        frame: Default::default(),
        repeat_mm: [10.0; 3],
        roughness_amplitude: 0.1,
        bump_amplitude_mm: 0.0,
    }]);
    request.raytrace = Some(options.clone());
    assert_eq!(admit(&scene, &request, &options).unwrap_err().code, ErrorCode::InvalidScene);
    options.surface_effects.as_mut().unwrap()[0].roughness_amplitude = 0.0;
    request.raytrace = Some(options.clone());
    assert!(admit(&scene, &request, &options).is_ok(), "an identity opaque effect does not change the geometric boundary");
    options.environment = Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 });
    request.raytrace = Some(options.clone());
    assert_eq!(
        options.validate_environment_reflections(&scene).unwrap_err().code,
        ErrorCode::InvalidRequest,
        "the same guard is called by engine preflight and host admission"
    );
    assert_eq!(
        ImageTask::new_with_environment(
            scene.clone(),
            request.clone(),
            CancelToken::default(),
            options.clone(),
            Some(surface_constant_environment())
        )
        .err()
        .unwrap()
        .code,
        ErrorCode::InvalidRequest
    );
    options.dielectrics = Vec::with_capacity(9);
    assert_eq!(options.validate().unwrap_err().code, ErrorCode::InvalidRequest);
}

#[test]
fn captured_clear_coat_keeps_direct_shading_but_refuses_unmatched_cpu_transport() {
    let (mut scene, mut request, mut options) = emission_fixture(false);
    let coat = crate::clear_coat::ClearCoat::new(0.8, 0.2, 1.5).unwrap();
    let surface = OpaqueSurface::new(Vec3::new(0.4, 0.3, 0.2), 0.0, 0.4, 1.5).unwrap().with_clear_coat(coat);
    scene.meshes[0].surface = Some(surface);
    request.raytrace = Some(options.clone());
    admit(&scene, &request, &options).unwrap();
    options.reflections = Some(job::ReflectionOptions {
        bodies: vec![job::ReflectionBody { body: "emitter".into(), mode: job::ReflectionMode::RoughSpecular }],
        max_bounces: 1,
        samples: 1,
    });
    request.raytrace = Some(options.clone());
    assert!(admit(&scene, &request, &options).unwrap_err().message.contains("clear-coat transport"));
    scene.meshes[0].surface = None;
    scene.meshes[0].faces.push(crate::FaceAppearance { face: 0, color: [255; 3], opacity: 1.0, surface: Some(surface), transmission: None });
    assert!(admit(&scene, &request, &options).unwrap_err().message.contains("clear-coat transport"));
}

#[test]
fn pending_pixel_heap_slot_stays_single_and_reuses_storage_until_completion() {
    let mut task = triangle_task(CancelToken::default());
    let initial = task.pending.0.as_ptr();
    assert_eq!(task.pending.0.capacity(), 1);
    assert!(task.pending.is_none());
    assert!(std::mem::size_of::<ImageTask>() <= 4096);
    let expected = admit(task.visibility_owner.source(), &task.request, task.visibility_owner.options()).unwrap();
    assert_eq!(task.admission.estimated_total_bytes, expected.estimated_total_bytes);
    assert!(expected.preparation_bytes >= PendingPixelSlot::required_bytes());
    let mut seen = false;
    let mut completed = false;
    for _ in 0..100_000 {
        match task.step(1).unwrap() {
            TaskPoll::Pending(_) => {
                assert_eq!(task.pending.0.capacity(), 1);
                assert_eq!(task.pending.0.as_ptr(), initial, "samples must reuse the one admitted continuation slot");
                assert!(task.pending.0.len() <= 1);
                seen |= task.pending.is_some();
            }
            TaskPoll::Complete(png) => {
                assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
                assert_eq!(task.pending.0.capacity(), 0, "completed task releases continuation storage before its lease is shrunk");
                assert!(task.pending.is_none());
                completed = true;
                break;
            }
        }
    }
    assert!(completed, "fixture must complete within its bounded test work");
    assert!(seen, "fixture must exercise a live resumable pixel");
}

#[test]
fn cancelled_pending_pixel_allocation_is_refused_before_any_continuation() {
    let cancel = CancelToken::default();
    cancel.cancel();
    assert_eq!(PendingPixelSlot::new(&cancel).err().unwrap().code, ErrorCode::Cancelled);
}

fn nested_optical_task() -> ImageTask {
    let (scene, mut options) = dielectric::tests::nested_fixture(false);
    for medium in &mut options.dielectrics {
        medium.ior = if medium.body_name == "Outer" { 1.5 } else { 2.0 };
        medium.absorption_per_mm = [0.0; 3];
    }
    ImageTask::new_with_environment_and_nested(scene, dielectric::tests::request(options.clone()), CancelToken::default(), options, None, true)
        .unwrap()
}

#[test]
fn nested_image_interfaces_use_actual_parent_ior_and_restore_it_on_exit() {
    let mut task = nested_optical_task();
    let outer_entry = 0.96 / 2.25;
    let inner_entry = outer_entry * (27.0 / 49.0);
    let inner_exit = inner_entry * (256.0 / 147.0);
    let expected = [(1, Some(0), outer_entry), (2, Some(1), inner_entry), (3, Some(0), inner_exit), (4, None, inner_exit * 2.16)];
    let mut next = 0;
    for _ in 0..100_000 {
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        if let Some(state) = task.optical.first().and_then(|optical| optical.pending.as_ref()) {
            let (depth, medium, throughput) = expected[next];
            if state.path.boundaries == depth && state.path.medium == medium {
                assert!((state.path.throughput.x - throughput).abs() < 1e-12);
                assert!((state.path.throughput.y - throughput).abs() < 1e-12);
                assert!((state.path.throughput.z - throughput).abs() < 1e-12);
                next += 1;
                if next == expected.len() {
                    return;
                }
            }
        }
    }
    panic!("nested first transmitted path did not cross all four interfaces");
}

#[test]
fn nested_paths_keep_one_unit_work_cancellation_and_total_budget_contracts() {
    for cancel in [false, true] {
        let mut task = nested_optical_task();
        let mut observed = false;
        for _ in 0..100_000 {
            let before = task.work.used();
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
            assert!(task.work.used() - before <= 1, "one-unit continuation overspent its quantum");
            if task.optical.first().and_then(|optical| optical.pending.as_ref()).is_some_and(|state| state.path.medium == Some(1)) {
                if cancel {
                    task.cancel.cancel();
                } else {
                    task.work.limit = task.work.used();
                }
                assert_eq!(task.step(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
                assert!(!task.done);
                observed = true;
                break;
            }
        }
        assert!(observed, "nested interior was never reached");
    }
}

#[test]
fn nested_admission_bills_parent_table_and_keeps_geometry_and_camera_refusals() {
    let (scene, options) = dielectric::tests::nested_fixture(false);
    let request = dielectric::tests::request(options.clone());
    let admitted = admit_with_nested(&scene, &request, &options, true).unwrap();
    let original = options.retained_dielectric_bytes().unwrap()
        + request.raytrace.as_ref().unwrap().retained_dielectric_bytes().unwrap()
        + std::mem::size_of::<OpticalTask>() as u64;
    assert!(admitted.preparation_bytes >= original);
    assert!(std::mem::size_of::<OpticalTask>() >= std::mem::size_of::<[Option<usize>; MAX_OVERRIDES]>());

    let mut invalid = scene.clone();
    std::sync::Arc::make_mut(&mut invalid.meshes[1].mesh).triangles.pop();
    assert!(admit_with_nested(&invalid, &request, &options, true).is_err());

    let mut inside = request.clone();
    inside.camera.distance = 0.1;
    inside.camera.target = Vec3::new(0.0, 0.0, 2.0);
    let mut task = ImageTask::new_with_environment_and_nested(scene, inside, CancelToken::default(), options, None, true).unwrap();
    for _ in 0..100_000 {
        match task.step(1) {
            Err(error) => {
                assert_eq!(error.code, ErrorCode::InvalidScene);
                assert!(error.message.contains("inside a dielectric"));
                return;
            }
            Ok(TaskPoll::Pending(_)) => {}
            Ok(TaskPoll::Complete(_)) => panic!("inside camera must not publish"),
        }
    }
    panic!("inside camera was not refused");
}

fn close_gap_nested_task(gap: f64, positive: bool) -> ImageTask {
    use std::sync::Arc;
    let mut outer = dielectric::tests::box_body(3.0);
    outer.name = "OuterGap".into();
    for p in &mut Arc::make_mut(&mut outer.mesh).positions {
        *p = Vec3::new(p.x * 0.2, p.y * 0.2, p.z + 1.0);
    }
    let mut inner = dielectric::tests::box_body(2.0);
    inner.name = "InnerGap".into();
    for p in &mut Arc::make_mut(&mut inner.mesh).positions {
        *p = Vec3::new(p.x * 0.1, p.y * 0.1, if p.z == 0.0 { 1.0 + gap } else { 3.0 });
    }
    let mut scene = dielectric::tests::scene(outer);
    scene.meshes.push(inner);
    let options = RaytraceOptions {
        dielectrics: scene
            .meshes
            .iter()
            .enumerate()
            .map(|(body_index, body)| job::DielectricOptions {
                body_index,
                body_name: body.name.clone(),
                boundary: job::DielectricBoundary::ClosedBox,
                ior: 1.0,
                absorption_per_mm: [0.0; 3],
                thickness_mm: 0.0,
            })
            .collect(),
        lights: Some(Vec::new()),
        shadows: false,
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera.set_view(if positive { crate::StandardView::Bottom } else { crate::StandardView::Top });
    request.camera.target = Vec3::new(0.0, 0.0, 2.5);
    request.camera.fov = 0.0;
    ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap()
}

#[test]
fn positive_one_ulp_nested_gap_and_separated_control_visit_every_real_interface() {
    for gap in [f64::EPSILON, 0.25] {
        for positive in [false, true] {
            let mut task = close_gap_nested_task(gap, positive);
            let points = if positive { [1.0, 1.0 + gap, 3.0, 4.0] } else { [4.0, 3.0, 1.0 + gap, 1.0] };
            let media = [Some(0), Some(1), Some(0), None];
            let mut next = 0;
            let mut completed = false;
            for _ in 0..100_000 {
                match task.step(1).unwrap() {
                    TaskPoll::Pending(_) => {
                        if next < points.len()
                            && let Some(state) = task.optical.first().and_then(|optical| optical.pending.as_ref())
                            && state.path.boundaries == next as u8 + 1
                        {
                            assert_eq!(state.path.medium, media[next]);
                            assert_eq!(state.path.segment_start.z, points[next]);
                            assert_eq!(state.path.ray.origin(), state.path.segment_start, "propagation and Beer start share one physical point");
                            assert_eq!(state.path.throughput, Vec3::new(1.0, 1.0, 1.0));
                            next += 1;
                        }
                    }
                    TaskPoll::Complete(png) => {
                        assert_eq!(next, 4, "an admitted interface was skipped");
                        let image = ::image::load_from_memory(&png).unwrap().to_rgba8();
                        assert_eq!(image.get_pixel(0, 0).0, [188, 188, 188, 255]);
                        completed = true;
                        break;
                    }
                }
            }
            assert!(completed, "close-gap image failed to complete");
        }
    }
}

#[test]
fn nested_reflected_route_keeps_child_beer_then_restores_parent_from_both_sides() {
    for positive in [false, true] {
        let (scene, mut options) = dielectric::tests::nested_fixture(false);
        for medium in &mut options.dielectrics {
            medium.ior = if medium.body_name == "Outer" { 1.5 } else { 2.0 };
        }
        let mut request = dielectric::tests::request(options.clone());
        request.camera.set_view(if positive { crate::StandardView::Bottom } else { crate::StandardView::Top });
        request.camera.target = Vec3::new(0.0, 0.0, 2.0);
        request.camera.fov = 0.0;
        let mut task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
        let (near, far) = if positive { (1.0, 3.0) } else { (3.0, 1.0) };
        let factor = (0.96 / 2.25) * (27.0 / 49.0) / 49.0;
        let reflected = Vec3::new(factor * (-0.1_f64 - 0.3 * 2.0).exp(), factor * (-0.2_f64 - 0.4 * 2.0).exp(), factor);
        let returned = dielectric::multiply(reflected, Vec3::new((-0.3_f64 * 2.0).exp(), (-0.4_f64 * 2.0).exp(), 1.0)) * (256.0 / 147.0);
        let mut saw_reflected = false;
        let mut restored = false;
        for _ in 0..1_000_000 {
            if let TaskPoll::Complete(_) = task.step(1).unwrap() {
                break;
            }
            let Some(state) = task.optical.first().and_then(|optical| optical.pending.as_ref()) else { continue };
            let path = state.path;
            if path.boundaries == 3 && path.medium == Some(1) && path.segment_start.z == far {
                assert!((path.throughput - reflected).len() < 1e-12);
                assert_eq!(path.ray.origin(), path.segment_start);
                assert!(path.ray.direction().z * if positive { -1.0 } else { 1.0 } > 0.0);
                saw_reflected = true;
            }
            if saw_reflected && path.boundaries == 4 && path.medium == Some(0) && path.segment_start.z == near {
                assert!((path.throughput - returned).len() < 1e-12);
                assert_eq!(path.ray.origin(), path.segment_start);
                restored = true;
                break;
            }
        }
        assert!(saw_reflected && restored, "actual reflected child route never restored its parent");
    }
}

#[test]
fn exact_box_launch_is_opt_in_and_legacy_default_origin_is_preserved() {
    for nested in [false, true] {
        let mut material = dielectric::tests::input(job::DielectricBoundary::ClosedBox);
        material.ior = 1.0;
        let options = RaytraceOptions { dielectrics: vec![material], lights: Some(Vec::new()), shadows: false, ..Default::default() };
        let request = dielectric::tests::request(options.clone());
        let mut task = ImageTask::new_with_environment_and_nested(
            dielectric::tests::scene(dielectric::tests::box_body(2.0)),
            request,
            CancelToken::default(),
            options,
            None,
            nested,
        )
        .unwrap();
        let mut observed = false;
        for _ in 0..100_000 {
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
            if let Some(state) = task.optical.first().and_then(|optical| optical.pending.as_ref())
                && state.path.boundaries == 1
                && state.path.medium == Some(0)
            {
                assert_eq!(state.path.segment_start.z, 2.0);
                if nested {
                    assert_eq!(state.path.ray.origin(), state.path.segment_start);
                } else {
                    assert!(state.path.ray.origin().z < state.path.segment_start.z, "default must retain its original inward offset");
                }
                observed = true;
                break;
            }
        }
        assert!(observed, "first box entry was not observed");
    }
}

// Progressive smooth-optics unit: original tests WRITTEN, NOT COMPILED OR RUN.
pub(super) fn progressive_optics_source(
    boundary: job::DielectricBoundary,
    thickness: f64,
    ior: f64,
    samples: u32,
) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let body = match boundary {
        job::DielectricBoundary::ThinSheet => optical_sheet("captured slab@occurrence-2", 0.0, 0.0),
        job::DielectricBoundary::ClosedBox => dielectric::tests::box_body(thickness),
    };
    let mut medium = dielectric::tests::input(boundary);
    medium.ior = ior;
    medium.thickness_mm = if boundary == job::DielectricBoundary::ThinSheet { thickness } else { 0.0 };
    let options = RaytraceOptions {
        progressive: Some(job::CpuProgressiveOptions { samples_per_pixel: samples, seed: 97 }),
        dielectrics: vec![medium],
        lights: Some(Vec::new()),
        shadows: false,
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera.distance = 10.0;
    request.camera.fov = 0.0;
    request.camera.pitch = std::f64::consts::FRAC_PI_2;
    (dielectric::tests::scene(body), request, options)
}

#[derive(Debug, PartialEq)]
struct OpticalProgressiveRun {
    checkpoints: Vec<(u32, Vec<u8>)>,
    png: Vec<u8>,
    film: Vec<[u64; 3]>,
    primaries: Vec<(usize, u32, [u64; 6])>,
    work: u64,
}

fn finish_optical_progressive(mut task: ImageTask, quantum: usize) -> OpticalProgressiveRun {
    let mut checkpoints = Vec::new();
    let mut film = None;
    let mut primaries = Vec::new();
    let mut previous_sample = None;
    let target = task.visibility_owner.options().progressive.unwrap().samples_per_pixel;
    for _ in 0..2_000_000 {
        if let Some(progressive) = &task.progressive
            && progressive.film.ready()
            && progressive.film.samples() == target
        {
            film.get_or_insert_with(|| {
                (0..task.request.width as usize * task.request.height as usize)
                    .map(|pixel| progressive.film.pixel(pixel).unwrap().0.map(f64::to_bits))
                    .collect()
            });
        }
        let poll = task.step_with_previews(quantum).unwrap();
        if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) {
            let identity = state.camera_sample.unwrap();
            assert_eq!(identity, (task.pixel, task.progressive.as_ref().unwrap().film.samples()));
            assert!(task.primary_coverage <= 1, "Fresnel branches cannot become coverage samples");
            if previous_sample != Some(identity) {
                // Each fixture hits certified geometry; even the four-unit call
                // yields before the first boundary can replace its camera ray.
                assert_eq!(state.path.boundaries, 0);
                let origin = state.path.ray.origin();
                let direction = state.path.ray.direction();
                primaries.push((
                    identity.0,
                    identity.1,
                    [origin.x.to_bits(), origin.y.to_bits(), origin.z.to_bits(), direction.x.to_bits(), direction.y.to_bits(), direction.z.to_bits()],
                ));
                previous_sample = Some(identity);
            }
        }
        match poll {
            ImagePoll::Pending(_) => {}
            ImagePoll::Preview { png, samples_per_pixel } => {
                assert!(task.pending.is_none());
                assert!(task.optical.first().unwrap().pending.is_none());
                let (_, covered, count) = task.progressive.as_ref().unwrap().film.pixel(0).unwrap();
                assert_eq!((u32::from(covered), u32::from(count)), (samples_per_pixel, samples_per_pixel));
                checkpoints.push((samples_per_pixel, png));
            }
            ImagePoll::Complete(png) => {
                assert!(task.optical.is_empty() && task.progressive.is_none());
                return OpticalProgressiveRun { checkpoints, png, film: film.unwrap(), primaries, work: task.work.used() };
            }
        }
    }
    panic!("original optical progressive fixture did not complete");
}

#[test]
fn progressive_optical_trees_commit_once_and_keep_absolute_rays_across_quanta() {
    for boundary in [job::DielectricBoundary::ThinSheet, job::DielectricBoundary::ClosedBox] {
        let (scene, request, options) = progressive_optics_source(boundary, 2.0, 1.5, 5);
        let one = finish_optical_progressive(ImageTask::new(scene.clone(), request.clone(), CancelToken::default(), options.clone()).unwrap(), 1);
        let four = finish_optical_progressive(ImageTask::new(scene, request, CancelToken::default(), options).unwrap(), 4);
        assert_eq!(one, four, "film bits, actual primary rays, checkpoints, final and total work agree");
        assert_eq!(one.primaries.len(), 5);
        assert_eq!(one.checkpoints.iter().map(|c| c.0).collect::<Vec<_>>(), [1, 2, 4]);
        assert_ne!(one.primaries[0].2, one.primaries[1].2, "fresh camera jitter, no repeated completed sample");
        let linear = f64::from_bits(one.film[0][0]);
        assert!(linear > 0.999 && linear <= 1.0 + 1e-12, "bounded eight-boundary lossless response, not alpha or branch averaging");
        assert_eq!(::image::load_from_memory(&one.png).unwrap().to_rgba8().get_pixel(0, 0).0, [188, 188, 188, 255]);
    }
}

#[test]
fn progressive_optical_checkpoint_is_the_exact_shorter_job_prefix() {
    let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 5);
    let run = finish_optical_progressive(ImageTask::new(scene.clone(), request.clone(), CancelToken::default(), options.clone()).unwrap(), 1);
    for (samples, png) in &run.checkpoints {
        let mut prefix_options = options.clone();
        prefix_options.progressive.as_mut().unwrap().samples_per_pixel = *samples;
        let prefix_request = ImageRequest { raytrace: Some(prefix_options.clone()), ..request.clone() };
        let prefix = finish_optical_progressive(ImageTask::new(scene.clone(), prefix_request, CancelToken::default(), prefix_options).unwrap(), 2);
        assert_eq!(&prefix.png, png);
        assert_eq!(prefix.primaries, run.primaries[..*samples as usize]);
    }
}

#[test]
fn progressive_optical_film_uses_real_beer_distance_for_thin_and_closed_controls() {
    for boundary in [job::DielectricBoundary::ThinSheet, job::DielectricBoundary::ClosedBox] {
        for thickness in [2.0, 8.0] {
            let (scene, mut request, mut options) = progressive_optics_source(boundary, thickness, 1.0, 3);
            options.dielectrics[0].absorption_per_mm = [0.1, 0.2, 0.0];
            request.raytrace = Some(options.clone());
            let run = finish_optical_progressive(ImageTask::new(scene, request, CancelToken::default(), options).unwrap(), 1);
            for (actual, sigma) in run.film[0].into_iter().zip([0.1_f64, 0.2, 0.0]) {
                assert!((f64::from_bits(actual) - (-sigma * thickness).exp()).abs() < 1e-10);
            }
            assert_eq!(run.primaries.len(), 3);
        }
    }
}

#[test]
fn progressive_closed_optics_use_four_percent_fresnel_and_snell_eta_transport() {
    let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    let mut saw_entry = false;
    for _ in 0..100_000 {
        assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) {
            if state.path.boundaries == 1 && state.path.medium == Some(0) {
                let entry = 0.96 / 2.25;
                assert!((state.path.throughput.x - entry).abs() < 1e-13);
                assert!((state.path.ray.direction() + Vec3::Z).len() < 1e-12);
                let reflected = state.stack.iter().flatten().find(|path| path.boundaries == 1 && path.medium.is_none()).unwrap();
                assert!((reflected.throughput.x - 0.04).abs() < 1e-13);
                saw_entry = true;
            }
            if saw_entry && state.path.boundaries == 2 && state.path.medium.is_none() {
                assert!((state.path.throughput.x - 0.96 * 0.96).abs() < 1e-12);
                assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0, "first transmitted leaf is not a complete film sample");
                return;
            }
        }
    }
    panic!("real progressive slab interfaces not reached");
}

#[test]
fn progressive_nested_optics_preserve_parent_beer_distance_and_require_opt_in() {
    for reverse in [false, true] {
        let (scene, mut options) = dielectric::tests::nested_fixture(reverse);
        options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 19 });
        let mut request = dielectric::tests::request(options.clone());
        request.camera.target = Vec3::new(0.0, 0.0, 2.0);
        request.camera.distance = 6.0;
        request.camera.fov = 0.0;
        assert!(admit(&scene, &request, &options).is_err());
        let run = finish_optical_progressive(
            ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap(),
            1,
        );
        for (actual, exponent) in run.film[0].into_iter().zip([-0.8_f64, -1.2, 0.0]) {
            assert!((f64::from_bits(actual) - exponent.exp()).abs() < 1e-10);
        }
        assert_eq!(::image::load_from_memory(&run.png).unwrap().to_rgba8().get_pixel(0, 0).0, [151, 132, 188, 255]);
    }
}

#[test]
fn progressive_optical_sample_identity_and_commit_barriers_refuse_partial_trees() {
    let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ThinSheet, 1.0, 1.5, 3);
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    for _ in 0..100_000 {
        assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        if task.optical.first().and_then(|o| o.pending.as_ref()).is_some() {
            let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
            assert!(task.finish_sample(Vec3::new(1.0, 1.0, 1.0)).is_err());
            assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
            task.optical.first_mut().unwrap().pending.as_mut().unwrap().camera_sample = Some((0, 1));
            let error = task.step_with_previews(1).err().unwrap();
            assert_eq!(error.code, ErrorCode::InvalidScene);
            assert!(error.message.contains("absolute camera sample"));
            assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
            return;
        }
    }
    panic!("optical camera sample not reached");
}

#[test]
fn progressive_optics_keep_cancellation_work_exhaustion_and_geometry_refusals() {
    for cancel in [false, true] {
        let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        let mut stopped = false;
        for _ in 0..100_000 {
            let before = task.work.used();
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            assert!(task.work.used() - before <= 1);
            if task.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|s| s.path.medium.is_some()) {
                let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
                if cancel {
                    task.cancel.cancel();
                } else {
                    task.work.limit = task.work.used();
                }
                assert_eq!(task.step_with_previews(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
                assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
                assert!(!task.done);
                stopped = true;
                break;
            }
        }
        assert!(stopped);
    }
    let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
    let mut open = scene.clone();
    std::sync::Arc::make_mut(&mut open.meshes[0].mesh).triangles.pop();
    assert!(admit(&open, &request, &options).is_err());
    let mut alpha = scene.clone();
    alpha.meshes[0].opacity = 0.5;
    assert!(admit(&alpha, &request, &options).is_err());
    let mut identity = options.clone();
    identity.dielectrics[0].body_name = "stale optical target".into();
    let wrong = ImageRequest { raytrace: Some(identity.clone()), ..request };
    assert!(admit(&scene, &wrong, &identity).is_err());
}

#[test]
fn progressive_closed_optics_tir_keeps_medium_and_generates_no_transmitted_branch() {
    let (scene, request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    // Observe a real certified transmitted interior, then exercise its actual
    // consumer with an analytic 3-4-5 internal direction at the top face.
    // Camera-inside admission is not bypassed for any submitted request.
    for _ in 0..100_000 {
        assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        if task.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|s| s.path.boundaries == 1 && s.path.medium == Some(0)) {
            let ray = Ray::new(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.8, 0.0, 0.6), 0.0, 100.0).unwrap();
            let state = task.optical.first_mut().unwrap().pending.as_mut().unwrap();
            state.stack.fill(None);
            state.stack_len = 0;
            state.path.ray = ray;
            state.path.segment_start = ray.origin();
            state.ready = false;
            resume_optical_path(task.pending.as_mut().unwrap(), state.path, task.lights.lights().len()).unwrap();
            for _ in 0..100_000 {
                assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
                let state = task.optical.first().unwrap().pending.as_ref().unwrap();
                if state.path.boundaries == 2 {
                    assert_eq!(state.path.medium, Some(0));
                    assert!(state.path.ray.direction().z < 0.0);
                    assert_eq!(state.stack_len, 0, "TIR cannot enqueue a transmitted path");
                    assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
                    return;
                }
            }
            panic!("TIR boundary not consumed");
        }
    }
    panic!("certified interior not reached");
}

#[test]
fn progressive_oblique_box_uses_snell_direction_and_geometric_path_length() {
    let (scene, mut request, mut options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
    options.dielectrics[0].absorption_per_mm = [0.1, 0.0, 0.0];
    request.raytrace = Some(options.clone());
    request.camera.pitch = std::f64::consts::FRAC_PI_3;
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    let mut incident = None;
    let mut entry = None;
    for _ in 0..100_000 {
        assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) else { continue };
        if state.path.boundaries == 0 {
            incident = Some(state.path.ray.direction());
        }
        if state.path.boundaries == 1 && state.path.medium == Some(0) {
            let d = incident.unwrap();
            let tx = d.x / 1.5;
            let ty = d.y / 1.5;
            let tz = -(1.0 - tx * tx - ty * ty).sqrt();
            assert!((state.path.ray.direction() - Vec3::new(tx, ty, tz)).len() < 1e-12);
            entry = Some((state.path.segment_start, state.path.throughput, tz));
        }
        if state.path.boundaries == 2 && state.path.medium.is_none() {
            let (point, throughput, tz) = entry.unwrap();
            let distance = 2.0 / -tz;
            assert!((state.path.segment_start.dist(point) - distance).abs() < 1e-10);
            assert!((state.path.ray.direction() - incident.unwrap()).len() < 1e-12);
            // Exit Fresnel and eta factors are achromatic. Their ratio cancels
            // between red and green, leaving only exp(-sigma * REAL distance).
            let attenuation = (state.path.throughput.x / state.path.throughput.y) / (throughput.x / throughput.y);
            assert!((attenuation - (-0.1 * distance).exp()).abs() < 1e-10);
            return;
        }
    }
    panic!("oblique progressive entry/exit not reached");
}

#[test]
fn progressive_nested_optics_still_refuse_contacts_and_interior_cameras() {
    let (mut scene, mut options) = dielectric::tests::nested_fixture(false);
    options.progressive = Some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 19 });
    let request = dielectric::tests::request(options.clone());
    for point in &mut std::sync::Arc::make_mut(&mut scene.meshes[1].mesh).positions {
        point.z += 1.0;
    }
    assert!(admit_with_nested(&scene, &request, &options, true).is_err(), "inner top touches outer top");
    let (scene, mut request, options) = progressive_optics_source(job::DielectricBoundary::ClosedBox, 2.0, 1.5, 3);
    request.camera.target = Vec3::new(0.0, 0.0, 1.0);
    request.camera.distance = 0.1;
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    for _ in 0..100_000 {
        match task.step_with_previews(1) {
            Err(error) => {
                assert_eq!(error.code, ErrorCode::InvalidScene);
                assert!(error.message.contains("inside a dielectric"));
                assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
                assert!(!task.done);
                return;
            }
            Ok(ImagePoll::Pending(_)) => {}
            Ok(_) => panic!("interior camera cannot publish a checkpoint or final"),
        }
    }
    panic!("interior progressive camera not refused");
}

// Original regression implementation guided by Kernel Specialist's analytic
// identity-shell counterexample (FINDING5282bf86 / proposal dae7f679). SOURCE
// ONLY, WRITTEN NOT RUN; expectations are physical, never recorded output.
pub(super) fn straight_identity_shells_source(count: usize, progressive: bool, absorbing: bool) -> (ProductScene, ImageRequest, RaytraceOptions) {
    assert!((1..=8).contains(&count));
    let mut meshes = Vec::with_capacity(8);
    let mut dielectrics = Vec::with_capacity(8);
    for index in 0..count {
        let h = (count - index) as f64;
        let mut body = dielectric::tests::box_body(2.0 * h);
        body.name = format!("Straight identity shell {index}");
        for point in &mut std::sync::Arc::make_mut(&mut body.mesh).positions {
            *point = Vec3::new(if point.x < 0.0 { -(16.0 + h) } else { 16.0 + h }, if point.y < 0.0 { -(16.0 + h) } else { 16.0 + h }, point.z - h);
        }
        let sigma = if absorbing { 0.01 * (index + 1) as f64 } else { 0.0 };
        dielectrics.push(job::DielectricOptions {
            body_index: index,
            body_name: body.name.clone(),
            boundary: job::DielectricBoundary::ClosedBox,
            ior: 1.0,
            absorption_per_mm: [sigma, 2.0 * sigma, 0.0],
            thickness_mm: 0.0,
        });
        meshes.push(body);
    }
    assert!(dielectrics.capacity() <= 8);
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |a, body| a.union(&body.mesh.bounds()));
    let options = RaytraceOptions {
        dielectrics,
        progressive: progressive.then_some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 71 }),
        lights: Some(Vec::new()),
        shadows: false,
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.background = [188; 3];
    request.camera.target = Vec3::ZERO;
    request.camera.distance = 20.0;
    request.camera.yaw = 0.0;
    request.camera.pitch = std::f64::consts::FRAC_PI_2;
    request.camera.fov = 0.0;
    (ProductScene { meshes, bounds }, request, options)
}

pub(super) fn straight_shells_expected_linear(count: usize, absorbing: bool) -> [f64; 3] {
    // Each shell's active parent interval totals2mm, including the innermost
    // 2mm slab. Sum 2*.01*(1+...+N)=.01*N*(N+1); green doubles sigma.
    let tau = if absorbing { 0.01 * (count * (count + 1)) as f64 } else { 0.0 };
    [(-tau).exp(), (-2.0 * tau).exp(), 1.0]
}

pub(super) fn straight_expected_pixel(linear: [f64; 3]) -> [u8; 4] {
    // Independent documented zero-EV Reinhard and IEC sRGB equations, not the
    // renderer's compositor/map_to_display/encode8 or a prior output capture.
    let rgb = linear.map(|v| {
        let mapped = v / (1.0 + v);
        let encoded = if mapped <= 0.0031308 { 12.92 * mapped } else { 1.055 * mapped.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0 + 0.5).floor() as u8
    });
    [rgb[0], rgb[1], rgb[2], 255]
}

fn complete_straight_shells(mut task: ImageTask, expected_crossings: u8, expected_linear: [f64; 3]) -> Vec<u8> {
    let progressive = task.visibility_owner.options().progressive;
    let mut final_linear = None;
    let mut maximum = 0;
    let mut final_png = None;
    assert_eq!(task.optical.first().unwrap().straight_limit, Some(expected_crossings));
    for _ in 0..1_000_000 {
        if let Some(p) = &task.progressive
            && p.film.ready()
            && p.film.samples() == progressive.unwrap().samples_per_pixel
        {
            let (rgb, covered, samples) = p.film.pixel(0).unwrap();
            assert_eq!((covered, samples), (3, 3));
            final_linear = Some(rgb);
        }
        let before = task.work.used();
        let poll = task.step_with_previews(1).unwrap();
        // PNG row/hash charges are the unchanged resolve contract. During
        // optical traversal/interface stages one unit cannot overspend.
        if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) {
            assert!(task.work.used() - before <= 1);
            assert_eq!(state.stack_len, 0, "zero Fresnel reflection creates no pending branch");
            assert_eq!(state.paths, u16::from(state.path.boundaries) + 1);
            maximum = maximum.max(state.path.boundaries);
        }
        match poll {
            ImagePoll::Pending(_) => {}
            ImagePoll::Preview { png, .. } => {
                assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, straight_expected_pixel(expected_linear));
                assert!(task.optical.first().unwrap().pending.is_none());
            }
            ImagePoll::Complete(png) => {
                final_png = Some(png);
                break;
            }
        }
    }
    assert_eq!(maximum, expected_crossings, "every real geometric interface was traversed");
    if progressive.is_some() {
        for (actual, expected) in final_linear.unwrap().into_iter().zip(expected_linear) {
            assert!((actual - expected).abs() < 1e-11, "physical parent-only Beer path length");
        }
    }
    let png = final_png.expect("certified finite straight path must complete");
    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, straight_expected_pixel(expected_linear));
    png
}

#[test]
fn straight_identity_shells_four_control_five_minimum_and_eight_maximum_reach_environment() {
    for progressive in [false, true] {
        for count in [4, 5, 8] {
            let (scene, request, options) = straight_identity_shells_source(count, progressive, false);
            options.validate().unwrap();
            assert!(admit(&scene, &request, &options).is_err(), "nesting still requires explicit opt-in");
            admit_with_nested(&scene, &request, &options, true).unwrap();
            let task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
            let png = complete_straight_shells(task, (2 * count) as u8, [1.0; 3]);
            assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, [188, 188, 188, 255]);
        }
    }
}

#[test]
fn straight_identity_shells_keep_actual_beer_parent_intervals_past_eight_crossings() {
    for progressive in [false, true] {
        for count in [4, 5, 8] {
            let (scene, request, options) = straight_identity_shells_source(count, progressive, true);
            let task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
            let _ = complete_straight_shells(task, (2 * count) as u8, straight_shells_expected_linear(count, true));
        }
    }
}

#[test]
fn straight_mixed_certificates_derive_two_per_box_and_one_per_sheet_without_extra_stack() {
    let (mut scene, mut request, mut options) = straight_identity_shells_source(5, true, false);
    for index in 0..3 {
        let name = format!("Straight external sheet {index}");
        let body_index = scene.meshes.len();
        scene.meshes.push(optical_sheet(&name, 0.0, 6.0 + index as f64));
        options.dielectrics.push(optical_input(body_index, &name, 1.0));
    }
    assert_eq!(options.dielectrics.len(), 8);
    assert!(options.dielectrics.capacity() <= 8);
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |a, body| a.union(&body.mesh.bounds()));
    request.raytrace = Some(options.clone());
    let task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
    let _ = complete_straight_shells(task, 13, [1.0; 3]);
}

#[test]
fn straight_policy_requires_exact_unit_indices_and_no_selected_opaque_reflector() {
    assert_eq!(MAX_BOUNDARIES, 8);
    assert_eq!(MAX_PATHS, 511);
    let (scene, request, options) = straight_identity_shells_source(5, false, false);
    for ior in [1.0_f64.next_up(), 1.5] {
        let mut changed = options.clone();
        changed.dielectrics[2].ior = ior;
        let request = ImageRequest { raytrace: Some(changed.clone()), ..request.clone() };
        let task = ImageTask::new_with_environment_and_nested(scene.clone(), request, CancelToken::default(), changed, None, true).unwrap();
        assert_eq!(task.optical.first().unwrap().straight_limit, None, "no epsilon-index shortcut or new rejection of legacy refracting input");
    }
    let mut scene = scene;
    let mut changed = options;
    scene.meshes.push(optical_sheet("Separate opaque reflector", 100.0, 0.0));
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |a, body| a.union(&body.mesh.bounds()));
    changed.reflections = Some(job::ReflectionOptions {
        bodies: vec![job::ReflectionBody { body: "Separate opaque reflector".into(), mode: job::ReflectionMode::Mirror }],
        max_bounces: 1,
        samples: 1,
    });
    let request = ImageRequest { raytrace: Some(changed.clone()), ..request };
    let task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), changed, None, true).unwrap();
    assert_eq!(task.optical.first().unwrap().straight_limit, None);
    assert!(task.optical.first().unwrap().pending.is_none());
}

#[test]
fn straight_identity_transport_still_charges_and_can_cancel_or_exhaust_inside_deep_chain() {
    for cancel in [false, true] {
        let (scene, request, options) = straight_identity_shells_source(8, true, true);
        let mut task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
        let mut observed = false;
        for _ in 0..200_000 {
            let before = task.work.used();
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            assert!(task.work.used() - before <= 1);
            if task.optical.first().and_then(|o| o.pending.as_ref()).is_some_and(|s| s.path.boundaries == 9) {
                let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
                if cancel {
                    task.cancel.cancel();
                } else {
                    task.work.limit = task.work.used();
                }
                assert_eq!(task.step_with_previews(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
                assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
                assert!(!task.done);
                observed = true;
                break;
            }
        }
        assert!(observed, "real ninth crossing must be reached before the stop");
    }
}

#[test]
fn straight_crossing_bound_corruption_is_typed_error_not_a_successful_black_tail() {
    let (scene, request, options) = straight_identity_shells_source(5, true, false);
    let mut task = ImageTask::new_with_environment_and_nested(scene, request, CancelToken::default(), options, None, true).unwrap();
    // Controlled invariant unit, distinct from the unchanged valid E2E fixtures:
    // an impossible internal policy must fail at a real subsequent interface.
    task.optical.first_mut().unwrap().straight_limit = Some(1);
    for _ in 0..200_000 {
        match task.step_with_previews(1) {
            Err(error) => {
                assert_eq!(error.code, ErrorCode::InvalidScene);
                assert!(error.message.contains("straight optical crossing bound exceeded"));
                assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
                assert!(!task.done);
                return;
            }
            Ok(ImagePoll::Pending(_)) => {}
            Ok(_) => panic!("bound inconsistency cannot publish success"),
        }
    }
    panic!("unexpected extra crossing was not refused");
}

// Original camera-to-final TIR supplement, AFTER frozen progressive optics.
// WRITTEN NOT COMPILED/RUN. Observations below never mutate the renderer.
fn camera_tir_source(samples: Option<u32>, seed: u32, intensity: f64) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let mut body = dielectric::tests::box_body(2.0);
    for point in &mut std::sync::Arc::make_mut(&mut body.mesh).positions {
        point.x = if point.x < 0.0 { 0.0 } else { 1.0 };
    }
    let options = RaytraceOptions {
        progressive: samples.map(|samples_per_pixel| job::CpuProgressiveOptions { samples_per_pixel, seed }),
        dielectrics: vec![dielectric::tests::input(job::DielectricBoundary::ClosedBox)],
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity }),
        lights: Some(Vec::new()),
        shadows: false,
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera = crate::Camera {
        target: Vec3::new(0.9, 0.37, 2.0),
        yaw: std::f64::consts::FRAC_PI_2,
        pitch: 0.8_f64.asin(),
        roll: 0.0,
        distance: 10.0,
        fov: 1e-6,
        ..Default::default()
    };
    (dielectric::tests::scene(body), request, options)
}

#[derive(Clone, Default)]
struct CameraTirEvidence {
    started: bool,
    first_entry: Option<(Vec3, Vec3, u16)>,
    tir: bool,
    transmitted_escape: bool,
    truncated_top_hit: bool,
    finite_scalar: f64,
    physical_tail_bound: f64,
}

/// Independent public-equation oracle, conditional on the REAL camera direction.
/// No dielectric::interface/attenuation/thin_sheet call or renderer state mutation.
fn camera_tir_oracle(ray: Ray) -> (f64, f64) {
    let d = ray.direction();
    assert!((d.x - 0.6).abs() < 2e-6 && (d.z + 0.8).abs() < 2e-6 && d.y.abs() < 2e-6);
    let enter = ray.origin() + d * ((2.0 - ray.origin().z) / d.z);
    assert!((enter.x - 0.9).abs() < 2e-5 && (enter.y - 0.37).abs() < 2e-5);
    let cosine = -d.z;
    let tx = d.x / 1.5;
    let ct = (1.0 - (1.0 - cosine * cosine) / 2.25).sqrt();
    assert!(2.25 * (1.0 - tx * tx) > 1.0, "glass-to-air SIDE incidence is beyond critical");
    // All possible jittered rays in this narrow public-camera cone retain one
    // lateral TIR per horizontal crossing, with three escapes before depth8.
    let q = 2.0 * tx / ct;
    assert!((1.0..2.0).contains(&(enter.x + q)));
    let bottom1 = 2.0 - enter.x - q;
    assert!((-1.0..0.0).contains(&(bottom1 - q)));
    let top2 = q - bottom1;
    assert!((1.0..2.0).contains(&(top2 + q)));
    let bottom3 = 2.0 - top2 - q;
    assert!((-1.0..0.0).contains(&(bottom3 - q)));
    // s/p Fresnel *power* from first principles, then the declared scalar
    // unpolarized model's three-term geometric return series. Eta² cancels on
    // each complete air/glass/air escape. Lossless unit exterior radiance has
    // infinite-depth physical answer1; finite omission <=max(Rs,Rp)^3.
    let rs = ((cosine - 1.5 * ct) / (cosine + 1.5 * ct)).powi(2);
    let rp = ((1.5 * cosine - ct) / (1.5 * cosine + ct)).powi(2);
    let f = (rs + rp) * 0.5;
    let finite = f + (1.0 - f).powi(2) * (1.0 + f + f * f);
    assert!((finite - (1.0 - (1.0 - f) * f.powi(3))).abs() < 16.0 * f64::EPSILON);
    (finite, rs.max(rp).powi(3))
}

fn complete_camera_tir(scene: ProductScene, request: ImageRequest, options: RaytraceOptions) -> Vec<u8> {
    assert_eq!(MAX_BOUNDARIES, 8, "this oracle is derived for the actual eight-event model");
    let count = options.progressive.map_or(1, |p| p.samples_per_pixel) as usize;
    let intensity = options.environment.unwrap().intensity;
    let progressive = options.progressive.is_some();
    let mut evidence = vec![CameraTirEvidence::default(); count];
    let mut final_linear = None;
    let mut checkpoints = 0;
    let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    // Deliberately use the public task poll to completion. The only writes to
    // task state are its own API calls, never injected rays/paths/media/cursors.
    for _ in 0..2_000_000 {
        if let Some(p) = &task.progressive
            && p.film.ready()
            && p.film.samples() as usize == count
        {
            let (rgb, covered, samples) = p.film.pixel(0).unwrap();
            assert_eq!((covered as usize, samples as usize), (count, count));
            final_linear = Some(rgb);
        }
        let poll = if progressive {
            task.step_with_previews(1).unwrap()
        } else {
            match task.step(1).unwrap() {
                TaskPoll::Pending(p) => ImagePoll::Pending(p),
                TaskPoll::Complete(png) => ImagePoll::Complete(png),
            }
        };
        if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) {
            let ordinal = state.camera_sample.map_or(0, |identity| {
                assert_eq!(identity.0, 0);
                identity.1 as usize
            });
            let seen = &mut evidence[ordinal];
            assert!(state.path.boundaries <= 8);
            if !seen.started {
                assert_eq!(state.path.boundaries, 0);
                let (finite, tail) = camera_tir_oracle(state.path.ray);
                seen.finite_scalar = finite;
                seen.physical_tail_bound = tail;
                seen.started = true;
            }
            if state.path.boundaries == 1 && state.path.medium == Some(0) && seen.first_entry.is_none() {
                let d = state.path.ray.direction();
                assert!((d.x - 0.4).abs() < 2e-6 && (d.z + (0.84_f64).sqrt()).abs() < 2e-6);
                assert!((state.path.segment_start.z - 2.0).abs() < 1e-12);
                seen.first_entry = Some((d, state.path.throughput, state.paths));
            }
            if state.path.boundaries == 2 && state.path.medium == Some(0) {
                let (before, throughput, paths) = seen.first_entry.unwrap();
                let after = state.path.ray.direction();
                assert!((state.path.segment_start.x - 1.0).abs() < 1e-12, "real ray hit lateral face");
                assert!(2.25 * (1.0 - before.x * before.x) > 1.0);
                assert!((after - Vec3::new(-before.x, before.y, before.z)).len() < 1e-12);
                assert!((state.path.throughput - throughput).len() < 1e-12, "TIR retains all current branch power");
                assert_eq!(state.paths, paths + 1, "only reflected path was created at TIR");
                // The primary reflection is still waiting; TIR itself leaves no
                // transmitted child in addition to this resumed internal path.
                assert_eq!(state.stack_len, 1);
                seen.tir = true;
            }
            if state.path.boundaries == 3 && state.path.medium.is_none() {
                assert!(seen.tir);
                assert!(state.path.segment_start.z.abs() < 1e-12, "TIR branch really escaped through bottom");
                assert!(state.path.ray.direction().z < 0.0 && state.path.ray.direction().x < 0.0);
                seen.transmitted_escape = true;
            }
            if state.path.boundaries == 8
                && state.path.medium == Some(0)
                && let Some(hit) = task.pending.as_ref().and_then(|p| p.hit)
            {
                let vertices = task.looks[hit.primitive_index].vertices;
                if vertices.iter().all(|v| v.z == 2.0) {
                    seen.truncated_top_hit = true;
                }
            }
        }
        match poll {
            ImagePoll::Pending(_) => {}
            ImagePoll::Preview { .. } => {
                assert!(task.optical.first().unwrap().pending.is_none());
                checkpoints += 1;
            }
            ImagePoll::Complete(png) => {
                assert!(evidence.iter().all(|s| s.started && s.tir && s.transmitted_escape && s.truncated_top_hit));
                if progressive {
                    assert!(checkpoints > 0);
                    let expected = intensity * evidence.iter().map(|s| s.finite_scalar).sum::<f64>() / count as f64;
                    let omitted_bound = intensity * evidence.iter().map(|s| s.physical_tail_bound).sum::<f64>() / count as f64;
                    // Preselected absolute roundoff allowance, independent of
                    // product output. This is no numerical-certification claim.
                    let roundoff = 4096.0 * f64::EPSILON;
                    for actual in final_linear.unwrap() {
                        assert!((actual - expected).abs() <= roundoff, "independent finite scalar Fresnel series");
                        assert!(actual <= intensity + roundoff);
                        assert!(actual >= intensity - omitted_bound - roundoff, "physical lossless energy with explicit finite-depth omission bound");
                    }
                    if intensity == 1.0 {
                        assert!(expected < 1.0, "eight-event truncation is NOT perfect conservation");
                        assert!(omitted_bound > 1e-5 && omitted_bound < 1e-3);
                    }
                }
                let image = ::image::load_from_memory(&png).unwrap().to_rgba8();
                let expected = if intensity == 0.0 { [0, 0, 0, 255] } else { [188, 188, 188, 255] };
                assert_eq!(image.get_pixel(0, 0).0, expected, "black/white secondary environment controls, covered pixel");
                return png;
            }
        }
    }
    panic!("public-camera TIR fixture failed to produce complete image");
}

#[test]
fn camera_originated_tir_survives_fixed_render_to_final_png_with_environment_controls() {
    let mut outputs = Vec::new();
    for intensity in [0.0, 1.0] {
        let (scene, request, options) = camera_tir_source(None, 0, intensity);
        outputs.push(complete_camera_tir(scene, request, options));
    }
    assert_ne!(outputs[0], outputs[1]);
}

#[test]
fn camera_originated_tir_progressive_final_obeys_physical_energy_and_explicit_depth_tail() {
    for seed in [0, 97] {
        for intensity in [0.0, 1.0] {
            let (scene, request, options) = camera_tir_source(Some(5), seed, intensity);
            let _ = complete_camera_tir(scene, request, options);
        }
    }
}

#[test]
fn exposure_full_range_fixed_spatial_and_progressive_keep_linear_film_and_one_display_gain() {
    // For +17/+32, reciprocal binary source radiance exposes to [1,1/2,1/4].
    // Reinhard gives [1/2,1/3,1/5]; IEC sRGB8 rounds to [188,156,124].
    // These are preselected mathematical values, not captures from the renderer.
    for (ev, unit, expected) in [
        (-32.0, 1.0, [0, 0, 0, 255]),
        (-17.0, 1.0, [0, 0, 0, 255]),
        (0.0, 1.0, [188, 156, 124, 255]),
        (17.0, 1.0 / 131_072.0, [188, 156, 124, 255]),
        (32.0, 1.0 / 4_294_967_296.0, [188, 156, 124, 255]),
    ] {
        let color = [unit, unit * 0.5, unit * 0.25];
        for (progressive, primary_samples) in [(false, 1), (false, 4), (true, 1)] {
            let (scene, mut request, mut options) = emission_fixture(false);
            // Orthographic footprint x[-16,16],y[-8,8] fits inside both patches
            // x[-20,0]/[0,20],y[-10,10] for every real pixel jitter.
            request.camera.distance = 20.0;
            options.exposure_ev = ev;
            options.primary_samples = primary_samples;
            options.progressive = progressive.then_some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 919 });
            options.emissions.as_mut().unwrap()[0].color = color;
            request.raytrace = Some(options.clone());
            let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
            let mut observed_linear = false;
            let mut completed = false;
            for _ in 0..200_000 {
                if let Some(state) = &task.progressive
                    && state.film.ready()
                    && state.film.samples() == 3
                {
                    let (rgb, covered, count) = state.film.pixel(0).unwrap();
                    assert_eq!((covered, count), (3, 3));
                    assert_eq!(rgb, color, "EV must not enter the accumulated linear film");
                    let (dark, covered, count) = state.film.pixel(1).unwrap();
                    assert_eq!((covered, count), (3, 3));
                    assert_eq!(dark, [0.0; 3]);
                    observed_linear = true;
                }
                let poll = if progressive {
                    task.step_with_previews(1).unwrap()
                } else {
                    match task.step(1).unwrap() {
                        TaskPoll::Pending(progress) => ImagePoll::Pending(progress),
                        TaskPoll::Complete(png) => ImagePoll::Complete(png),
                    }
                };
                let (png, final_frame) = match poll {
                    ImagePoll::Pending(_) => continue,
                    ImagePoll::Preview { png, .. } => {
                        assert!(progressive);
                        (png, false)
                    }
                    ImagePoll::Complete(png) => (png, true),
                };
                let image = ::image::load_from_memory(&png).unwrap().to_rgba8();
                assert_eq!(image.get_pixel(0, 0).0, expected, "EV={ev}, progressive={progressive}, spatial={primary_samples}");
                assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 255]);
                if final_frame {
                    completed = true;
                    break;
                }
            }
            assert!(completed, "real public producer must finish its bounded fixture");
            assert!(!progressive || observed_linear);
        }
    }
}

// Experimental strict core successor. Original public-camera fixtures; no
// injected ray, direction, medium, hit or stack creates the TIR sequences.
fn long_tir_source(width: f64, absorber: bool, progressive: bool, sigma: [f64; 3]) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let mut body = dielectric::tests::box_body(2.0);
    for p in &mut std::sync::Arc::make_mut(&mut body.mesh).positions {
        p.x = if p.x < 0.0 { 0.0 } else { width };
    }
    let mut scene = dielectric::tests::scene(body);
    if absorber {
        assert_eq!(width, 0.05);
        let xb = 0.875 - 4.0 / 21.0_f64.sqrt();
        let xr = xb - 0.001 * 0.4 / 0.84_f64.sqrt();
        assert!(xr > 0.0001 && xr + 0.0001 < width);
        scene.meshes.push(crate::ProductMesh {
            transmission: None,
            name: "Original return absorber".into(),
            mesh: std::sync::Arc::new(solidcraft_geom::Mesh {
                positions: vec![
                    Vec3::new(xr - 0.0001, -0.5, 0.001),
                    Vec3::new(xr + 0.0001, -0.5, 0.001),
                    Vec3::new(xr + 0.0001, 1.5, 0.001),
                    Vec3::new(xr - 0.0001, 1.5, 0.001),
                ],
                normals: vec![Vec3::Z; 4],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                ..Default::default()
            }),
            color: [0; 3],
            opacity: 1.0,
            surface: Some(OpaqueSurface::new(Vec3::ZERO, 0.0, 1.0, 1.5).unwrap()),
            faces: Vec::new(),
        });
    }
    scene.bounds = scene.meshes.iter().fold(Aabb3::EMPTY, |a, m| a.union(&m.mesh.bounds()));
    let mut medium = dielectric::tests::input(job::DielectricBoundary::ClosedBox);
    medium.absorption_per_mm = sigma;
    let options = RaytraceOptions {
        dielectrics: vec![medium],
        lights: Some(Vec::new()),
        shadows: false,
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
        progressive: progressive.then_some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 71 }),
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera = crate::Camera {
        target: Vec3::new(width * 0.5, 0.37, 2.0),
        yaw: std::f64::consts::FRAC_PI_2,
        pitch: 0.8_f64.asin(),
        roll: 0.0,
        distance: 10.0,
        fov: 1e-9,
        ..Default::default()
    };
    (scene, request, options)
}

fn long_tir_expected(ray: Ray, sigma: [f64; 3], strict: bool) -> [f64; 3] {
    let c = -ray.direction().z;
    assert!((c - 0.8).abs() < 2e-9);
    let ct = (1.0 - (1.0 - c * c) / 2.25).sqrt();
    let rs = ((c - 1.5 * ct) / (c + 1.5 * ct)).powi(2);
    let rp = ((1.5 * c - ct) / (1.5 * c + ct)).powi(2);
    let f = (rs + rp) * 0.5;
    if strict { sigma.map(|s| f + (1.0 - f).powi(2) * (-s * 2.0 / ct).exp()) } else { [f; 3] }
}

fn complete_long_tir(mut task: ImageTask, sigma: [f64; 3], strict: bool) -> Vec<u8> {
    let count = task.visibility_owner.options().progressive.map_or(1, |p| p.samples_per_pixel) as usize;
    let progressive = task.visibility_owner.options().progressive.is_some();
    let mut oracle = vec![None; count];
    let mut previous_tir = 0;
    let mut previous_direction = vec![None; count];
    let mut actual_tir_rays = 0;
    let mut escaped = vec![false; count];
    let mut absorbed = vec![false; count];
    let mut maximum = 0;
    let mut linear = None;
    for _ in 0..2_000_000 {
        if let Some(p) = &task.progressive
            && p.film.ready()
            && p.film.samples() as usize == count
        {
            let (rgb, covered, samples) = p.film.pixel(0).unwrap();
            assert_eq!((covered as usize, samples as usize), (count, count));
            linear = Some(rgb);
        }
        let poll = task.step_with_previews(1).unwrap();
        if let Some(optical) = task.optical.first()
            && let Some(state) = &optical.pending
        {
            let ordinal = state.camera_sample.map_or(0, |identity| identity.1 as usize);
            maximum = maximum.max(state.path.boundaries);
            if oracle[ordinal].is_none() {
                assert_eq!(state.path.boundaries, 0);
                oracle[ordinal] = Some(long_tir_expected(state.path.ray, sigma, strict));
            }
            if state.path.boundaries == 1 && state.path.medium == Some(0) && previous_direction[ordinal].is_none() {
                previous_direction[ordinal] = Some(state.path.ray.direction());
            }
            if optical.tir_continuations > previous_tir {
                assert!(strict);
                assert_eq!(optical.tir_continuations, previous_tir + 1);
                assert_eq!(state.path.boundaries, 1, "TIR is not another split");
                assert_eq!(state.paths, 3, "initial path and two first-interface children only");
                assert_eq!(state.stack_len, 1, "waiting primary reflection is preserved");
                let before = previous_direction[ordinal].unwrap();
                let after = state.path.ray.direction();
                assert!((after - Vec3::new(-before.x, before.y, before.z)).len() < 1e-12);
                assert!(task.pending.as_ref().unwrap().hit.is_none(), "real new TraceCursor has no borrowed old hit");
                previous_direction[ordinal] = Some(after);
                previous_tir = optical.tir_continuations;
                actual_tir_rays += 1;
            }
            if strict && state.path.boundaries == 2 && state.path.medium.is_none() && state.path.segment_start.z.abs() < 1e-12 {
                assert_eq!(actual_tir_rays, 17 * (ordinal + 1));
                escaped[ordinal] = true;
            }
            if strict
                && let Some(hit) = task.pending.as_ref().and_then(|p| p.hit)
                && task.looks[hit.primitive_index].body_index == 1
            {
                assert!(escaped[ordinal]);
                absorbed[ordinal] = true;
            }
        }
        if let ImagePoll::Complete(png) = poll {
            let mut expected = [0.0; 3];
            for value in oracle {
                for (sum, v) in expected.iter_mut().zip(value.unwrap()) {
                    *sum += v / count as f64;
                }
            }
            if progressive {
                for (actual, expected) in linear.unwrap().into_iter().zip(expected) {
                    assert!((actual - expected).abs() < 1e-10);
                }
            }
            assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, straight_expected_pixel(expected));
            if strict {
                assert!(escaped.into_iter().all(|v| v) && absorbed.into_iter().all(|v| v));
                assert_eq!(actual_tir_rays, 17 * count);
                assert_eq!(maximum, 2);
            } else {
                assert!(escaped.into_iter().all(|v| !v));
                assert_eq!(maximum, 8, "pre-fix witness: entry and seven TIRs exhaust legacy events");
                assert_eq!(actual_tir_rays, 0, "legacy policy never enters the new core branch");
            }
            return png;
        }
    }
    panic!("long TIR original fixture did not complete");
}

#[test]
fn legacy_long_tir_counterexample_loses_the_first_real_transmitted_exit() {
    // This explicitly witnesses the retained C15 limitation, not a new physical
    // success oracle. Its derived F-only result is contrasted with the strict
    // terminating fixture below. No old L8 assertion is edited.
    let (scene, request, options) = long_tir_source(0.05, true, true, [0.0; 3]);
    let task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
    assert!(!task.optical.first().unwrap().deterministic_tir);
    let _ = complete_long_tir(task, [0.0; 3], false);
}

#[test]
fn strict_core_long_tir_fixed_and_progressive_reach_real_exit_and_real_absorber() {
    for progressive in [false, true] {
        let (scene, request, options) = long_tir_source(0.05, true, progressive, [0.0; 3]);
        let task = ImageTask::new_with_deterministic_tir(scene, request, CancelToken::default(), options, false).unwrap();
        let _ = complete_long_tir(task, [0.0; 3], true);
    }
}

#[test]
fn strict_core_long_tir_keeps_beer_on_all_real_reflected_segments() {
    for progressive in [false, true] {
        let sigma = [0.1, 0.2, 0.0];
        let (scene, request, options) = long_tir_source(0.05, true, progressive, sigma);
        let task = ImageTask::new_with_deterministic_tir(scene, request, CancelToken::default(), options, false).unwrap();
        let _ = complete_long_tir(task, sigma, true);
    }
}

#[test]
fn strict_core_tir_more_than511_real_continuations_still_use_bvh_work_and_cancellation() {
    for cancel in [false, true] {
        let (scene, request, options) = long_tir_source(0.0005, false, true, [0.0; 3]);
        let mut task = ImageTask::new_with_deterministic_tir(scene, request, CancelToken::default(), options, false).unwrap();
        let mut observed = false;
        for _ in 0..2_000_000 {
            let before = task.work.used();
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            assert!(task.work.used() - before <= 1);
            if task.optical.first().unwrap().tir_continuations == 512 {
                let state = task.optical.first().unwrap().pending.as_ref().unwrap();
                assert_eq!((state.path.boundaries, state.paths, state.stack_len), (1, 3, 1));
                assert!(task.work.used() > 1024, "all previous traversals AND interfaces were real work");
                let start = task.work.used();
                assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
                assert_eq!(task.work.used(), start + 1, "newest real cursor also starts traversal before the stop");
                assert_eq!(task.optical.first().unwrap().tir_continuations, 512);
                let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
                if cancel {
                    task.cancel.cancel();
                } else {
                    task.work.limit = task.work.used();
                }
                assert_eq!(task.step_with_previews(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
                assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
                assert_eq!(frontier.completed_passes, 0);
                assert!(!task.done);
                observed = true;
                break;
            }
        }
        assert!(observed, "511 branch-state allowance must not truncate a deterministic ray sequence");
    }
}

#[test]
fn strict_core_positive_non_tir_depth_limit_is_explicit_instead_of_black_completion() {
    let (scene, request, options) = long_tir_source(0.05, false, true, [0.0; 3]);
    let mut task = ImageTask::new_with_deterministic_tir(scene, request, CancelToken::default(), options, false).unwrap();
    for _ in 0..2_000_000 {
        match task.step_with_previews(1) {
            Err(error) => {
                assert_eq!(error.code, ErrorCode::LimitExceeded);
                assert!(error.message.contains("non-TIR branch-depth budget exhausted"));
                let optical = task.optical.first().unwrap();
                assert!(optical.tir_continuations >= 17);
                assert!(optical.pending.as_ref().unwrap().paths <= 511);
                assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
                assert!(!task.done);
                return;
            }
            Ok(ImagePoll::Pending(_)) => {}
            Ok(_) => panic!("positive unresolved split cannot be published as a successful black tail"),
        }
    }
    panic!("strict unresolved split was not reported");
}

// ORIGINAL UNAPPLIED PROPOSAL for the existing raytrace/image_tests.rs module.
// SOURCE ONLY: UNFORMATTED / UNCOMPILED / UNRUN. No new production policy API.
// This witnesses LEGACY bias; it is NOT a strict-quality success expectation.

fn rational_depth_source(progressive: bool) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let scene = dielectric::tests::scene(dielectric::tests::box_body(2.0));
    let mut medium = dielectric::tests::input(job::DielectricBoundary::ClosedBox);
    medium.ior = 3.0;
    medium.absorption_per_mm = [0.0; 3];
    let options = RaytraceOptions {
        dielectrics: vec![medium],
        lights: Some(Vec::new()),
        shadows: false,
        environment: Some(job::RaytraceEnvironmentOptions { color: [1.0; 3], intensity: 1.0 }),
        progressive: progressive.then_some(job::CpuProgressiveOptions { samples_per_pixel: 3, seed: 71 }),
        ..Default::default()
    };
    let mut request = dielectric::tests::request(options.clone());
    request.camera.target = Vec3::new(0.23, 0.37, 1.0);
    request.camera.distance = 10.0;
    request.camera.fov = 0.0;
    (scene, request, options)
}

fn rational_depth_real_cut(task: &ImageTask) -> bool {
    let Some(optical) = task.optical.first() else { return false };
    let Some(state) = &optical.pending else { return false };
    if state.path.boundaries == 0 {
        assert!((state.path.ray.direction() + Vec3::Z).len() < 1e-14);
    }
    if state.path.boundaries != 8 || state.path.medium != Some(0) || task.pending.as_ref().and_then(|p| p.hit).is_none() {
        return false;
    }
    // All observed states came from the actual public camera/consumer.
    // Entry plus seven true Fresnel splits, with the primary reflection waiting.
    assert_eq!(state.paths, 17);
    assert_eq!(state.stack_len, 1);
    let expected = 1.0 / 196_608.0;
    for value in [state.path.throughput.x, state.path.throughput.y, state.path.throughput.z] {
        assert!(value > 0.0 && (value - expected).abs() < 1e-14);
    }
    true
}

#[test]
fn legacy_public_normal_ior_three_has_a_positive_rational_depth_tail_hidden_by_png() {
    const LEGACY_LINEAR: f64 = 65_533.0 / 65_536.0;
    const OMITTED: f64 = 3.0 / 65_536.0;
    assert_eq!(1.0 - LEGACY_LINEAR, OMITTED);
    assert!(OMITTED > 1e-10);
    for progressive in [false, true] {
        let (scene, request, options) = rational_depth_source(progressive);
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        let mut cut_observed = false;
        let mut linear = None;
        let mut completed = false;
        for _ in 0..200_000 {
            cut_observed |= rational_depth_real_cut(&task);
            if let Some(p) = &task.progressive
                && p.film.ready()
                && p.film.samples() == 3
            {
                let (rgb, covered, samples) = p.film.pixel(0).unwrap();
                assert_eq!((covered, samples), (3, 3));
                linear = Some(rgb);
            }
            let before_work = task.work.used();
            let poll = if progressive {
                task.step_with_previews(1).unwrap()
            } else {
                match task.step(1).unwrap() {
                    TaskPoll::Pending(p) => ImagePoll::Pending(p),
                    TaskPoll::Complete(png) => ImagePoll::Complete(png),
                }
            };
            assert!(task.work.used() >= before_work, "no ordinary-work reset across paths/passes");
            match poll {
                ImagePoll::Pending(_) => {}
                ImagePoll::Preview { .. } => {
                    assert!(progressive);
                    assert!(task.optical.first().unwrap().pending.is_none());
                }
                ImagePoll::Complete(png) => {
                    assert!(cut_observed);
                    if progressive {
                        for value in linear.unwrap() {
                            assert!((value - LEGACY_LINEAR).abs() < 1e-10);
                            assert!((value - 1.0).abs() > OMITTED * 0.5);
                        }
                    }
                    // Both physical1 and legacy65533/65536 round to188 under
                    // the documented Reinhard/IEC transfer: NOT a quality proof.
                    assert_eq!(::image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, [188, 188, 188, 255]);
                    completed = true;
                    break;
                }
            }
        }
        assert!(completed, "bounded legacy witness did not complete");
    }
}

#[test]
fn public_normal_ior_three_real_positive_tail_still_obeys_cancel_and_work_limit() {
    for cancel in [false, true] {
        let (scene, request, options) = rational_depth_source(true);
        let token = CancelToken::default();
        let mut task = ImageTask::new(scene, request, token.clone(), options).unwrap();
        let mut reached = false;
        for _ in 0..200_000 {
            if rational_depth_real_cut(&task) {
                reached = true;
                break;
            }
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        }
        assert!(reached, "real camera traversal must reach the unresolved positive tail");
        let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
        assert_eq!(frontier.completed_passes, 0);
        let used = task.work.used();
        assert!(used > 0);
        if cancel {
            token.cancel(); // Actual caller-held public cancellation token.
        } else {
            // Existing unit-test control, not a new public budget API and not
            // an injected ray, medium, hit or optical path.
            task.work.limit = used;
        }
        let error = task.step_with_previews(1).err().unwrap();
        assert_eq!(error.code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
        assert_eq!(task.work.used(), used, "failed/cancelled debit cannot advance or refund work");
        assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
        assert!(!task.done);
    }
}

// ORIGINAL UNAPPLIED acceptance for Worker2 policy index ee17a4bb.
// Intended for the EXISTING raytrace/image_tests.rs, after the original
// rational_depth_source / rational_depth_real_cut helpers from index1685daca.
// UNFORMATTED / UNCOMPILED / UNRUN. No cfg(test)-only policy selector is used.

fn rational_public_strict_source(progressive: bool) -> (ProductScene, ImageRequest, RaytraceOptions) {
    let (scene, mut request, mut options) = rational_depth_source(progressive);
    options.dielectric_transport = job::CpuDielectricTransport::StrictTir;
    request.raytrace = Some(options.clone());
    (scene, request, options)
}

#[test]
fn public_strict_ior_three_positive_non_tir_tail_is_a_real_error_before_final_or_sample() {
    for progressive in [false, true] {
        let (scene, request, options) = rational_public_strict_source(progressive);
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        let mut failed = false;
        for _ in 0..200_000 {
            let before = task.work.used();
            let poll = if progressive {
                task.step_with_previews(1)
            } else {
                task.step(1).map(|poll| match poll {
                    TaskPoll::Pending(p) => ImagePoll::Pending(p),
                    TaskPoll::Complete(png) => ImagePoll::Complete(png),
                })
            };
            match poll {
                Ok(ImagePoll::Pending(_)) => assert!(task.work.used() >= before),
                Ok(_) => panic!("strict positive rational tail cannot become preview/final quality"),
                Err(error) => {
                    assert_eq!(error.code, ErrorCode::LimitExceeded);
                    assert!(error.message.contains("strict optical non-TIR branch-depth budget exhausted"));
                    assert!(rational_depth_real_cut(&task), "actual depth8/paths17/positive internal state must exist");
                    assert_eq!(task.work.used(), before + 1, "the real failing interface consumes its original tick");
                    if progressive {
                        assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
                        assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap().completed_passes, 0);
                    }
                    assert!(!task.done);
                    failed = true;
                    break;
                }
            }
        }
        assert!(failed, "the actual selected strict policy must report its optical limit");
    }
}

#[test]
fn public_strict_rational_tail_preserves_real_cancel_and_ordinary_work_accounting() {
    for cancel in [false, true] {
        let (scene, request, options) = rational_public_strict_source(true);
        let token = CancelToken::default();
        let mut task = ImageTask::new(scene, request, token.clone(), options).unwrap();
        let mut reached = false;
        for _ in 0..200_000 {
            if rational_depth_real_cut(&task) {
                reached = true;
                break;
            }
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        }
        assert!(reached);
        let before = task.work.used();
        let frontier = task.progressive.as_ref().unwrap().film.partial_frontier().unwrap();
        assert_eq!(frontier.completed_passes, 0);
        if cancel {
            token.cancel();
        } else {
            // Explicit established unit-test stop control, not a public option
            // or a manufactured ray/hit/medium/branch.
            task.work.limit = before;
        }
        let error = task.step_with_previews(1).err().unwrap();
        assert_eq!(error.code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
        assert_eq!(task.work.used(), before);
        assert_eq!(task.progressive.as_ref().unwrap().film.partial_frontier().unwrap(), frontier);
        assert!(!task.done);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_public_final_strict_rational_tail_fails_without_a_frame_or_policy_fallback() {
    use std::time::Duration;
    let host = job::CpuHost::for_platform().unwrap();
    assert_eq!(host.kind(), job::HostKind::NativeWorker);
    host.validate_integrator(job::Integrator::Raytrace).unwrap();
    // Exactly ONE required native branch marker; an unsupported configuration
    // must fail rather than return early or switch to cooperative Raster.
    eprintln!("STRICT_RATIONAL_BRANCH cpu_supported=true");
    for (index, progressive) in [false, true].into_iter().enumerate() {
        let (scene, request, options) = rational_public_strict_source(progressive);
        host.admit(&scene, &request, job::JobOptions::default()).unwrap();
        let id = job::JobId { session: 17361, serial: index as u64 + 1, generation: 1 };
        let handle = host.submit(id, 1, scene, request).unwrap();
        let status = handle.wait_timeout(Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase, job::Phase::Failed);
        assert_eq!(status.request.raytrace.as_ref(), Some(&options));
        let error = status.error.as_ref().unwrap();
        assert_eq!(error.code, ErrorCode::LimitExceeded);
        assert!(error.message.contains("strict optical non-TIR branch-depth budget exhausted"));
        assert!(handle.frame().unwrap().is_none());
        assert!(handle.preview().unwrap().is_none());
    }
    // No immediate zero-pool claim: native terminal publication alone does not
    // prove concurrent task/lease destruction has finished.
}

mod original_directional_acceptance {
    // Adopted W2 acceptance helpers; SOURCE ONLY / UNCOMPILED / UNRUN.
    // Intended as a cfg(test) child module of raytrace::image, NOT as a crate or framework.
    // Readonly OWNER getters and the real runtime flag are adapted explicitly below.
    // Original donor8527 and oracle4ab12 stay sealed; registered cases follow these helpers.
    use super::*;
    use solidcraft_geom::{Aabb3, Mesh};
    use std::sync::Arc;

    const ORIGINAL_LINEAR: [[f64; 3]; 3] = [[0.8, 0.5, 0.25], [0.4, 0.125, 0.25], [0.2, 0.03125, 0.25]];
    const ORIGINAL_ABSOLUTE_TOLERANCE: f64 = 1e-12;

    fn original_receiver() -> crate::ProductMesh {
        crate::ProductMesh {
            transmission: None,
            name: "Original directional receiver".into(),
            mesh: Arc::new(Mesh {
                positions: vec![Vec3::new(-4.0, -4.0, 0.0), Vec3::new(4.0, -4.0, 0.0), Vec3::new(4.0, 4.0, 0.0), Vec3::new(-4.0, 4.0, 0.0)],
                normals: vec![Vec3::Z; 4],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                tri_face: vec![0; 2],
                ..Default::default()
            }),
            color: [255; 3],
            opacity: 1.0,
            // IOR1 removes the normal-incidence interface term. Rough Lambert albedo
            // and all original expected values were fixed before any consumer exists.
            surface: Some(OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap()),
            faces: Vec::new(),
        }
    }

    fn original_box(ordinal: usize) -> crate::ProductMesh {
        assert!(ordinal < 2);
        let lo = if ordinal == 0 { 1.0 } else { 3.0 };
        let hi = lo + 1.0;
        crate::ProductMesh {
            transmission: None,
            name: format!("Original directional glass {ordinal}"),
            mesh: Arc::new(Mesh {
                positions: (0..8)
                    .map(|i| Vec3::new(if i & 1 == 0 { -2.0 } else { 2.0 }, if i & 2 == 0 { -2.0 } else { 2.0 }, if i & 4 == 0 { lo } else { hi }))
                    .collect(),
                // Outward winding of six closed rectangular faces, two triangles each.
                triangles: vec![
                    [0, 2, 3],
                    [0, 3, 1],
                    [4, 5, 7],
                    [4, 7, 6],
                    [0, 4, 6],
                    [0, 6, 2],
                    [1, 3, 7],
                    [1, 7, 5],
                    [0, 1, 5],
                    [0, 5, 4],
                    [2, 6, 7],
                    [2, 7, 3],
                ],
                ..Default::default()
            }),
            color: [255; 3],
            opacity: 1.0,
            surface: None,
            faces: Vec::new(),
        }
    }

    /// Creates real scene/request/options with original admission. No flag or shadow mock.
    /// Identity effect is a SEPARATE one-box case, not a replacement for the plain case.
    pub(super) fn original_directional_fixture(boxes: usize, identity_effect: bool) -> (ProductScene, ImageRequest, RaytraceOptions) {
        assert!(boxes <= 2);
        assert!(!identity_effect || boxes == 1);
        let mut meshes = vec![original_receiver()];
        let mut dielectrics = Vec::with_capacity(boxes);
        for ordinal in 0..boxes {
            let body = original_box(ordinal);
            dielectrics.push(job::DielectricOptions {
                body_index: meshes.len(),
                body_name: body.name.clone(),
                boundary: job::DielectricBoundary::ClosedBox,
                ior: 1.0,
                absorption_per_mm: [std::f64::consts::LN_2, 2.0 * std::f64::consts::LN_2, 0.0],
                thickness_mm: 0.0,
            });
            meshes.push(body);
        }
        let bounds = meshes.iter().fold(Aabb3::EMPTY, |a, b| a.union(&b.mesh.bounds()));
        let options = RaytraceOptions {
            budget: job::CpuBudget::Compact,
            primary_samples: 1,
            progressive: Some(job::CpuProgressiveOptions { samples_per_pixel: 1, seed: 81 }),
            lights: Some(vec![job::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: std::f64::consts::PI }]),
            shadows: true,
            exposure_ev: 0.0,
            dielectrics,
            surface_effects: identity_effect.then(|| {
                vec![crate::surface_patterns::SurfaceEffect {
                    body: "Original directional receiver".into(),
                    frame: Default::default(),
                    repeat_mm: [10.0; 3],
                    roughness_amplitude: 0.0,
                    bump_amplitude_mm: 0.0,
                }]
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
            camera: crate::Camera {
                target: Vec3::new(0.25, 0.37, 0.0),
                yaw: 0.0,
                pitch: std::f64::consts::FRAC_PI_2,
                roll: 0.0,
                distance: 0.5,
                // Perspective avoids the orthographic 3*distance origin translation.
                // Tiny positive FOV keeps every real jittered ray essentially normal.
                fov: 1e-9,
                ..Default::default()
            },
            width: 1,
            height: 1,
            background: [0; 3],
            transparent_background: false,
        };
        (ProductScene { meshes, bounds }, request, options)
    }

    /// Real constructor/certifier and generated ray, with no skipped admission.
    /// Selects the real mode for N1/N2; N0 deliberately retains the legacy/default route.
    pub(super) fn original_directional_task(boxes: usize, identity_effect: bool) -> ImageTask {
        let (scene, mut request, mut options) = original_directional_fixture(boxes, identity_effect);
        options.directional_shadow_transmission = boxes != 0;
        request.raytrace = Some(options.clone());
        options.validate().unwrap();
        for input in &options.dielectrics {
            assert_ne!(input.body_index, 0, "receiver must remain optically unselected");
            assert_eq!(input.body_name, scene.meshes[input.body_index].name);
            assert!(matches!(dielectric::certify(&scene, input).unwrap(), Certificate::ClosedBox(_)));
        }
        if identity_effect {
            assert!(options.surface_effects.as_ref().unwrap()[0].prepare().unwrap().is_identity());
        }
        admit(&scene, &request, &options).unwrap();
        let task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        assert_eq!(task.optical.len(), usize::from(boxes != 0));
        if identity_effect {
            assert!(task.visibility_owner.options().surface_effects.is_some());
            assert!(task.effects.is_empty(), "identity is validated but not retained");
        }
        let [dx, dy] = progressive::variates(81, 0, 0, 0);
        let ray = primary_ray(&task.request, dx, dy, task.clip).unwrap();
        assert_original_primary(ray);
        // Check the actual clipped generator at every pixel-corner bound as well.
        for (x, y) in [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)] {
            assert_original_primary(primary_ray(&task.request, x, y, task.clip).unwrap());
        }
        task
    }

    fn assert_original_primary(ray: Ray) {
        assert!(ray.origin().z > 0.0 && ray.origin().z < 1.0, "REAL clipped primary origin must be between receiver and first box");
        assert!(ray.direction().z < 0.0);
        assert!((ray.direction() + Vec3::Z).len() < 1e-8);
        let t = -ray.origin().z / ray.direction().z;
        assert!(t > ray.interval()[0] && t < ray.interval()[1]);
        let on_receiver = ray.at(t).unwrap();
        assert!(on_receiver.x.abs() < 1.0 && on_receiver.y.abs() < 1.0);
    }

    /// Observe a REAL ImageTask supplied by the future, separately reviewed consumer.
    /// No fake mode, private state injection, visibility override or expected-from-output.
    /// The registered tests select the real mode and use progressive polling below.
    /// The legacy one-box test separately retains the original opaque-shadow contract.
    pub(super) fn assert_original_directional_linear(mut task: ImageTask, boxes: usize, identity_effect: bool, retained_effect: bool) -> [f64; 3] {
        assert!(boxes <= 2 && (!identity_effect || boxes == 1));
        assert!(!retained_effect || (boxes == 1 && !identity_effect));
        if identity_effect {
            assert!(task.visibility_owner.options().surface_effects.as_ref().unwrap()[0].prepare().unwrap().is_identity());
            assert!(task.effects.is_empty(), "identity is validated and deliberately pruned");
        }
        if retained_effect {
            assert!(!task.visibility_owner.options().surface_effects.as_ref().unwrap()[0].prepare().unwrap().is_identity());
            assert_eq!(task.effects.len(), 1, "nonidentity must reach the actual scanner");
        }
        assert_eq!(task.visibility_owner.options().dielectrics.len(), boxes);
        assert_eq!(task.visibility_owner.options().budget, job::CpuBudget::Compact);
        assert_eq!(task.visibility_owner.options().primary_samples, 1);
        assert_eq!(task.visibility_owner.options().progressive.unwrap().seed, 81);
        assert!(task.visibility_owner.options().environment.is_none());
        assert!(task.visibility_owner.options().emissions.is_none());
        assert!(task.visibility_owner.options().reflections.is_none());
        assert!(task.visibility_owner.options().punctual_lights.is_none());
        assert!(task.visibility_owner.options().area_lights.is_none());
        let lights = task.visibility_owner.options().lights.as_ref().unwrap();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].direction, [0.0, 0.0, 1.0]);
        assert_eq!(lights[0].color, [1.0; 3]);
        assert_eq!(lights[0].intensity, std::f64::consts::PI);
        assert_eq!(task.visibility_owner.source().meshes.len(), boxes + 1);

        assert_eq!(task.visibility_owner.options().progressive.unwrap().samples_per_pixel, 1);
        // Exact observer contract: Legacy constructor, one SPP, no optional feedback.
        // Legacy with more SPP CAN produce checkpoints; do not generalize this guard.
        assert!(task.progressive.as_ref().unwrap().partial_plan.is_none());
        assert!(task.progressive.as_ref().unwrap().partial.is_none());
        assert!(task.work.protected.is_none());
        assert_eq!(task.visibility_owner.options().exposure_ev, 0.0);
        assert!(task.visibility_owner.options().shadows);
        assert_eq!((task.request.width, task.request.height), (1, 1));
        let mut saw_receiver = false;
        let mut saw_unselected_optical = false;
        let mut saw_retained_effect_scan = false;
        let mut previous_work = task.work.used();
        for _ in 0..100_000 {
            assert!(task.work.used() >= previous_work, "one cumulative ledger, no resets");
            previous_work = task.work.used();
            if let Some(pending) = task.pending.as_ref() {
                assert_original_primary(pending.ray);
                if let Some(hit) = pending.hit {
                    let scene = PreparedScene { bvh: task.bvh.as_ref().unwrap(), primitives: &task.looks, source: task.visibility_owner.source() };
                    let shared = scene.resolve_hit(pending.ray, hit).unwrap();
                    assert_eq!(shared.body_index, 0);
                    assert_eq!(shared.body_name, "Original directional receiver");
                    assert!(shared.front_face);
                    assert!((shared.geometric_normal - Vec3::Z).len() < 1e-12);
                    // Exercise the actual geometric spawn helper, not a hand-offset ray.
                    let shadow = shared.spawn(Vec3::Z).unwrap();
                    assert_eq!(shadow.direction(), Vec3::Z);
                    assert!(shadow.origin().z > 0.0 && shadow.origin().z < 1.0);
                    let receiver = &task.visibility_owner.source().meshes[0].mesh;
                    for indices in &receiver.triangles {
                        let vertices = indices.map(|i| receiver.positions[i as usize]);
                        assert!(Triangle::new(vertices).unwrap().intersect(&shadow).is_none(), "self-hit must not hide the Beer shadow oracle");
                    }
                    saw_receiver = true;
                    if let Some(state) = task.optical.first().and_then(|o| o.pending.as_ref()) {
                        if state.ready {
                            assert!(state.selected.is_none(), "nonempty optical selectors must miss receiver");
                            assert_eq!(state.selector, task.visibility_owner.options().dielectrics.len());
                            assert_eq!(state.path.boundaries, 0, "camera must not transmit through glass");
                            assert!(state.path.medium.is_none());
                            saw_unselected_optical = true;
                        }
                    }
                    if retained_effect && pending.surface_ready && pending.surface_index.is_some() {
                        assert_eq!(pending.surface_cursor, task.effects.len());
                        let affected = resolved_surface_hit(&scene, pending, &task.effects).unwrap();
                        assert_eq!(affected.surface.base_color(), Vec3::new(0.8, 0.5, 0.25));
                        assert_eq!(affected.surface.dielectric_ior(), 1.0);
                        assert_eq!(affected.surface.metallic(), 0.0);
                        assert!(affected.surface.clear_coat().is_none());
                        assert!(affected.surface.roughness() < 1.0);
                        assert_eq!(affected.shading_normal, Vec3::Z);
                        saw_retained_effect_scan = true;
                    }
                }
            }
            if let Some(progressive) = &task.progressive {
                if progressive.film.ready() && progressive.film.samples() == 1 {
                    assert!(task.encoder.is_none() && task.pixels.is_empty());
                    assert_eq!(progressive.resolve_pixel, 0);
                    assert!(task.pending.is_none());
                    assert!(task.optical.first().is_none_or(|o| o.pending.is_none()));
                    let (linear, covered, samples) = progressive.film.pixel(0).unwrap();
                    assert_eq!((covered, samples), (1, 1));
                    assert!(saw_receiver);
                    assert!(boxes == 0 || saw_unselected_optical);
                    assert!(!retained_effect || saw_retained_effect_scan);
                    for (actual, expected) in linear.into_iter().zip(ORIGINAL_LINEAR[boxes]) {
                        assert!(actual.is_finite());
                        assert!(
                            (actual - expected).abs() <= ORIGINAL_ABSOLUTE_TOLERANCE,
                            "linear film {actual} differs from original oracle {expected}"
                        );
                    }
                    // Return before any PNG resolve/encoding. No display inversion.
                    return linear;
                }
            }
            match task.step_with_previews(1).unwrap() {
                ImagePoll::Pending(_) => {}
                ImagePoll::Preview { samples_per_pixel, .. } => {
                    panic!("unexpected checkpoint at {samples_per_pixel} SPP before observing original linear film");
                }
                ImagePoll::Complete(_) => panic!("completed before observing original linear film"),
            }
        }
        panic!("original tiny fixture exceeded its fixed observation bound");
    }

    /// Separate negative validation witness for the identity case. No shortcut injection.
    pub(super) fn assert_original_identity_target_is_still_validated() {
        let (scene, mut request, mut options) = original_directional_fixture(1, true);
        options.surface_effects.as_mut().unwrap()[0].body = "Missing identity target".into();
        request.raytrace = Some(options.clone());
        assert!(admit(&scene, &request, &options).is_err());
        assert!(ImageTask::new(scene, request, CancelToken::default(), options).is_err());
    }

    /// Additional REAL retained-effect variant for the second shadow-start route.
    /// It preserves the already fixed one-box oracle, because IOR1/metallic0 is exact
    /// Lambert for EVERY admitted roughness in the current surface contract.
    pub(super) fn original_directional_retained_effect_fixture() -> (ProductScene, ImageRequest, RaytraceOptions) {
        let (scene, mut request, mut options) = original_directional_fixture(1, false);
        let effect = crate::surface_patterns::SurfaceEffect {
            body: "Original directional receiver".into(),
            frame: crate::surface_patterns::MappingFrame {
                // At the receiver hit(.25,.37,0), all three phases are -PI.
                // Nearby tiny-FOV jitter keeps the cosine field negative; roughness
                // changes below1. No tint API, normal change or geometry change.
                origin_mm: [0.75, 0.87, 0.5],
                ..Default::default()
            },
            repeat_mm: [1.0; 3],
            roughness_amplitude: 0.1,
            bump_amplitude_mm: 0.0,
        };
        let prepared = effect.prepare().unwrap();
        assert!(!prepared.is_identity());
        let material = scene.meshes[0].surface.unwrap();
        let applied = prepared.apply(material, Vec3::Z, Vec3::Z, Vec3::new(0.25, 0.37, 0.0)).unwrap();
        assert!(applied.surface.roughness() < material.roughness());
        assert_eq!(applied.surface.base_color(), material.base_color());
        assert_eq!(applied.surface.metallic(), 0.0);
        assert_eq!(applied.surface.dielectric_ior(), 1.0);
        assert_eq!(applied.shading_normal, Vec3::Z);
        options.surface_effects = Some(vec![effect]);
        request.raytrace = Some(options.clone());
        admit(&scene, &request, &options).unwrap();
        (scene, request, options)
    }

    /// Retained-effect construction selects the real directional runtime mode.
    pub(super) fn original_directional_retained_effect_task() -> ImageTask {
        let (scene, mut request, mut options) = original_directional_retained_effect_fixture();
        options.directional_shadow_transmission = true;
        request.raytrace = Some(options.clone());
        for input in &options.dielectrics {
            assert!(matches!(dielectric::certify(&scene, input).unwrap(), Certificate::ClosedBox(_)));
        }
        let task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        assert_eq!(task.effects.len(), 1);
        assert_eq!(task.effects[0].0, 0);
        assert_eq!(task.optical.len(), 1);
        let [dx, dy] = progressive::variates(81, 0, 0, 0);
        assert_original_primary(primary_ray(&task.request, dx, dy, task.clip).unwrap());
        task
    }

    #[test]
    fn directional_real_film_matches_original_zero_one_two_box_oracles() {
        for boxes in 0..=2 {
            assert_original_directional_linear(original_directional_task(boxes, false), boxes, false, false);
        }
    }

    #[test]
    fn directional_identity_is_pruned_and_retained_roughness_exercises_scanner() {
        assert_original_directional_linear(original_directional_task(1, true), 1, true, false);
        assert_original_identity_target_is_still_validated();
        assert_original_directional_linear(original_directional_retained_effect_task(), 1, false, true);
    }

    #[test]
    fn directional_default_preserves_opaque_glass_shadow_in_linear_film() {
        let (scene, request, options) = original_directional_fixture(1, false);
        assert!(!options.directional_shadow_transmission);
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        for _ in 0..100_000 {
            if let Some(progressive) = &task.progressive {
                if progressive.film.ready() && progressive.film.samples() == 1 {
                    assert_eq!(progressive.film.pixel(0).unwrap(), ([0.0; 3], 1, 1));
                    return;
                }
            }
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        }
        panic!("legacy opaque-shadow film was not observed");
    }

    #[test]
    fn directional_scan_uses_one_tick_per_active_poll_and_keeps_fixed_storage() {
        let mut task = original_directional_task(2, false);
        let slot = task.pending.0.as_ptr();
        let admission = admit(task.visibility_owner.source(), &task.request, task.visibility_owner.options()).unwrap();
        assert_eq!(admission.estimated_total_bytes, task.admission.estimated_total_bytes);
        assert!(admission.preparation_bytes >= PendingPixelSlot::required_bytes());
        assert!(std::mem::size_of::<ImageTask>() <= 4096);
        let mut active_polls = 0;
        for _ in 0..100_000 {
            if task.progressive.as_ref().is_some_and(|p| p.film.ready() && p.film.samples() == 1) {
                assert!(active_polls > 0);
                assert!(task.pending.is_none());
                return;
            }
            let active = task.pending.as_ref().is_some_and(|p| p.directional_active);
            let used = task.work.used();
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            if active {
                active_polls += 1;
                assert_eq!(task.work.used(), used + 1);
            }
            assert_eq!(task.pending.0.capacity(), 1);
            assert_eq!(task.pending.0.as_ptr(), slot);
        }
        panic!("bounded directional scan never reached committed film");
    }

    #[test]
    fn directional_cancellation_or_work_limit_cannot_commit_a_pending_connection() {
        for cancel in [false, true] {
            let mut task = original_directional_task(1, false);
            let mut reached = false;
            for _ in 0..100_000 {
                if task.pending.as_ref().is_some_and(|p| p.directional_active) {
                    reached = true;
                    break;
                }
                assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
            }
            assert!(reached);
            let used = task.work.used();
            assert!(task.finish_sample(Vec3::ZERO).is_err());
            assert_eq!(task.work.used(), used);
            if cancel {
                task.cancel.cancel();
            } else {
                task.work.limit = used;
            }
            assert_eq!(task.step_with_previews(1).err().unwrap().code, if cancel { ErrorCode::Cancelled } else { ErrorCode::LimitExceeded });
            assert_eq!(task.work.used(), used);
            assert_eq!(task.progressive.as_ref().unwrap().film.samples(), 0);
            assert!(task.pixels.is_empty() && task.encoder.is_none() && !task.done);
        }
    }

    #[test]
    fn directional_context_refusals_and_completed_release_preserve_contracts() {
        let (scene, mut request, mut options) = original_directional_fixture(1, false);
        options.directional_shadow_transmission = true;
        request.raytrace = Some(options.clone());
        assert!(admit_with_nested(&scene, &request, &options, true).is_err());
        let mut mismatch = request.clone();
        mismatch.raytrace.as_mut().unwrap().directional_shadow_transmission = false;
        assert!(admit(&scene, &mismatch, &options).is_err());
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        let weak = Arc::downgrade(&task.visibility_owner.source().meshes[0].mesh);
        let budget = task.visibility_owner.options().budget;
        for _ in 0..100_000 {
            if let ImagePoll::Complete(png) = task.step_with_previews(16384).unwrap() {
                assert!(::image::load_from_memory(&png).is_ok());
                assert!(weak.upgrade().is_none());
                assert!(task.visibility_owner.source().meshes.is_empty());
                assert!(task.visibility_owner.options().dielectrics.is_empty());
                assert_eq!(task.visibility_owner.options().budget, budget);
                assert!(task.visibility_owner.options().directional_shadow_transmission);
                assert!(task.request.raytrace.is_none() && task.optical.is_empty());
                assert!(task.step_with_previews(1).is_err());
                return;
            }
        }
        panic!("directional completion did not release capture");
    }

    #[test]
    fn directional_direct_constructor_refuses_separate_hdr_input() {
        use crate::lighting::{HdrSource, environment_input::EnvironmentInput};
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nPRIMARIES=0.64 0.33 0.30 0.60 0.15 0.06 0.3127 0.3290\n\n-Y 2 +X 4\n".to_vec();
        for _ in 0..8 {
            bytes.extend_from_slice(&[128, 128, 128, 129]);
        }
        let hdr = EnvironmentInput::new(HdrSource::new(bytes).unwrap(), 0.0, 1.0, 4).unwrap();
        let (scene, mut request, mut options) = original_directional_fixture(1, false);
        options.directional_shadow_transmission = true;
        request.raytrace = Some(options.clone());
        let error = ImageTask::new_with_environment(scene, request, CancelToken::default(), options, Some(hdr)).err().unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(error.message, "directional shadow transmission does not support HDR environments");
    }

    // Added during static coverage review after the initial twelve tests/body.
    // Expectations are authored from geometry/Lambert/Beer, never candidate output.
    #[test]
    fn directional_second_light_has_its_own_oblique_beer_connection() {
        let (scene, mut request, mut options) = original_directional_fixture(1, false);
        options.directional_shadow_transmission = true;
        options.lights.as_mut().unwrap().push(job::DirectionalLightOptions {
            direction: [0.6, 0.0, 0.8],
            color: [1.0; 3],
            intensity: 1.25 * std::f64::consts::PI,
        });
        request.raytrace = Some(options.clone());
        let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
        // Both box faces are hit in their interiors: x=1 at z1, x=1.75 at z2.
        // Lz=4/5 gives chord5/4; intensity5*PI/4 cancels the Lambert cosine.
        let oblique = [2.0_f64.powf(-1.25), 4.0_f64.powf(-1.25), 1.0];
        let expected = [0.4 + 0.8 * oblique[0], 0.125 + 0.5 * oblique[1], 0.5];
        let mut saw_both = false;
        for _ in 0..100_000 {
            if let Some(pending) = task.pending.as_ref() {
                if pending.directional_complete == 3 {
                    assert!(!pending.directional_active);
                    let first = pending.directional_transmittance[0];
                    let second = pending.directional_transmittance[1];
                    assert!((first - Vec3::new(0.5, 0.25, 1.0)).len() < 1e-12);
                    assert!((second - Vec3::new(oblique[0], oblique[1], oblique[2])).len() < 1e-12);
                    saw_both = true;
                }
            }
            if let Some(progressive) = &task.progressive {
                if progressive.film.ready() && progressive.film.samples() == 1 {
                    assert!(saw_both);
                    let (linear, covered, samples) = progressive.film.pixel(0).unwrap();
                    assert_eq!((covered, samples), (1, 1));
                    for (actual, expected) in linear.into_iter().zip(expected) {
                        assert!((actual - expected).abs() <= 1e-12);
                    }
                    return;
                }
            }
            assert!(matches!(task.step_with_previews(1).unwrap(), ImagePoll::Pending(_)));
        }
        panic!("independent directional connections never reached film");
    }

    #[test]
    fn directional_fixed_spatial_samples_encode_original_rational_radiance_once() {
        for samples in [1, 4, 16] {
            let (scene, mut request, mut options) = original_directional_fixture(1, false);
            options.directional_shadow_transmission = true;
            options.progressive = None;
            options.primary_samples = samples;
            request.raytrace = Some(options.clone());
            let mut task = ImageTask::new(scene, request, CancelToken::default(), options).unwrap();
            let mut completed = false;
            for _ in 0..100_000 {
                if let TaskPoll::Complete(png) = task.step(1).unwrap() {
                    // Original linear(.4,.125,.25) -> Reinhard(2/7,1/9,1/5).
                    // encode8 is the unchanged encoding function, not a transport oracle.
                    let pixel = ::image::load_from_memory(&png).unwrap().to_rgba8();
                    assert_eq!(pixel.get_pixel(0, 0).0, [encode8(2.0 / 7.0), encode8(1.0 / 9.0), encode8(1.0 / 5.0), 255]);
                    completed = true;
                    break;
                }
            }
            assert!(completed);
        }
    }
}
