use super::*;
use crate::{Camera, ProductMesh, StandardView};
use solidcraft_geom::{Aabb3, Mesh};
use std::sync::Arc;

#[test]
fn raster_ev_scales_lambert_radiance_before_the_existing_single_tone_map() {
    let mut part = square("matte", [0; 3], 1.0, Vec3::ZERO);
    part.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap());
    let mut alpha = None;
    for (ev, expected) in [(-1.0, [69, 77, 77, 255]), (0.0, [94, 104, 104, 255]), (1.0, [124, 135, 135, 255])] {
        let mut input = request();
        input.raster_exposure_ev = ev;
        input.raster_lights =
            Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [0.25, 0.5, 1.0], intensity: 2.0 }]);
        let png = finish(ImageTask::new(scene(vec![part.clone()]), input, CancelToken::default()).unwrap());
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        // Original analytic oracle: [.4,.5,.5]/pi *2^EV -> Reinhard -> sRGB.
        assert_eq!(image.get_pixel(32, 32).0, expected);
        assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
        let coverage: Vec<u8> = image.pixels().map(|pixel| pixel[3]).collect();
        if let Some(expected_alpha) = &alpha { assert_eq!(&coverage, expected_alpha) } else { alpha = Some(coverage) }
    }
}

#[test]
fn raster_ev_covers_implicit_pbr_and_the_weighted_scalar_surrogate_with_original_oracles() {
    for (ev, implicit_rgb, weighted_gray) in [(-1.0, [90, 73, 52], 6), (0.0, [120, 99, 73], 12), (1.0, [152, 130, 99], 20)] {
        let mut input = request();
        input.raster_exposure_ev = ev;
        let mut part = square("matte", [0; 3], 1.0, Vec3::ZERO);
        part.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap());
        let png = finish(ImageTask::new(scene(vec![part]), input.clone(), CancelToken::default()).unwrap());
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        // Front normal sees only studio Rim:1.5*.6=.9, so rho*.9/pi.
        assert_eq!(image.get_pixel(32, 32).0, [implicit_rgb[0], implicit_rgb[1], implicit_rgb[2], 255]);
        input.studio_weights = Some([0.0, 0.0, 1.0]);
        let png = finish(ImageTask::new(scene(vec![square("scalar", [0; 3], 1.0, Vec3::ZERO)]), input, CancelToken::default()).unwrap());
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        // Black roughness1/IOR1.5 surrogate: .28125*F/pi, where the authored
        // Schlick approximation F=.04+.96*(1-sqrt(.8))^5. Then2^EV/Reinhard/sRGB.
        assert_eq!(image.get_pixel(32, 32).0, [weighted_gray, weighted_gray, weighted_gray, 255]);
    }
}

#[test]
fn raster_ev_legacy_linear_surrogate_preserves_straight_alpha_and_background() {
    for (ev, gray) in [(-1.0, 11), (0.0, 19), (1.0, 30)] {
        let mut input = request();
        input.raster_exposure_ev = ev;
        let part = square("legacy", [0; 3], 0.5, Vec3::ZERO);
        let png = finish(ImageTask::new(scene(vec![part.clone()]), input.clone(), CancelToken::default()).unwrap());
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        // Front-view scalar shade: spec=.25*(5/6)^20 on already decoded black;
        // multiply by2^EV then piecewise sRGB. No Reinhard on this old branch.
        assert_eq!(image.get_pixel(32, 32).0, [gray, gray, gray, 128]);
        assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
        input.transparent_background = false;
        let png = finish(ImageTask::new(scene(vec![part]), input, CancelToken::default()).unwrap());
        assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(0, 0).0, [20, 40, 60, 255]);
    }
}

#[test]
fn raster_ev_zero_wire_is_exact_and_nonfinite_bounds_or_wrong_integrator_refuse() {
    let part = scene(vec![square("legacy", [160, 120, 80], 0.5, Vec3::ZERO)]);
    let mut input = request();
    let wire = serde_json::to_value(&input).unwrap();
    assert!(wire.get("raster_exposure_ev").is_none());
    let decoded: ImageRequest = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.raster_exposure_ev, 0.0);
    let old = finish(ImageTask::new(part.clone(), input.clone(), CancelToken::default()).unwrap());
    assert_eq!(finish(ImageTask::new(part.clone(), decoded, CancelToken::default()).unwrap()), old);
    for zero in [0.0, -0.0] {
        input.raster_exposure_ev = zero;
        assert_eq!(finish(ImageTask::new(part.clone(), input.clone(), CancelToken::default()).unwrap()), old);
    }
    for value in [-32.0, 32.0] {
        input.raster_exposure_ev = value;
        assert!(admit(&part, &input).is_ok());
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -33.0, 33.0] {
        input.raster_exposure_ev = value;
        assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
    }
    input.raster_exposure_ev = 1.0;
    input.integrator = crate::job::Integrator::Raytrace;
    assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
    assert!(crate::job::interactive::cpu_request(&input).is_err());
    #[cfg(feature = "raytrace-cpu")]
    {
        input.integrator = crate::job::Integrator::Raster;
        assert!(crate::raytrace::image::admit(&part, &input, &crate::job::RaytraceOptions::default()).is_err());
    }
}

fn request() -> ImageRequest {
    let mut camera = Camera { distance: 50.0, ..Default::default() };
    camera.set_view(StandardView::Front);
    ImageRequest {
        cpu_progressive_every_pass: true,
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        environment: None,
        textures: None,
        integrator: Default::default(),
        raytrace: None,
        camera,
        width: 64,
        height: 64,
        background: [20, 40, 60],
        transparent_background: true,
    }
}

#[test]
fn render_lights_raster_mute_is_captured_in_request_and_darkens_only_the_product() {
    let part = scene(vec![square("part", [200, 180, 160], 1.0, Vec3::ZERO)]);
    let mut input = request();
    input.transparent_background = false;
    let normal = finish(ImageTask::new(part.clone(), input.clone(), super::CancelToken::default()).unwrap());
    input.studio_weights = Some([0.0; 3]);
    let dark = finish(ImageTask::new(part, input, super::CancelToken::default()).unwrap());
    assert_ne!(normal, dark);
    let image = image::load_from_memory(&dark).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(32, 32).0, [0, 0, 0, 255]);
    assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
}

fn square(name: &str, colour: [u8; 3], opacity: f64, offset: Vec3) -> ProductMesh {
    let mesh = Mesh {
        positions: vec![
            Vec3::new(-10.0, 0.0, -10.0) + offset,
            Vec3::new(10.0, 0.0, -10.0) + offset,
            Vec3::new(10.0, 0.0, 10.0) + offset,
            Vec3::new(-10.0, 0.0, 10.0) + offset,
        ],
        normals: vec![Vec3::new(0.0, -1.0, 0.0); 4],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        tri_face: vec![0, 1],
        ..Default::default()
    };
    ProductMesh { name: name.into(), mesh: Arc::new(mesh), color: colour, opacity, surface: None, transmission: None, faces: Vec::new() }
}

fn scene(meshes: Vec<ProductMesh>) -> ProductScene {
    let bounds = meshes.iter().fold(Aabb3::EMPTY, |b, m| b.union(&m.mesh.bounds()));
    ProductScene { meshes, bounds }
}

fn finish(mut task: ImageTask) -> Vec<u8> {
    for _ in 0..100_000 {
        match task.step(4096).unwrap() {
            TaskPoll::Complete(png) => return png,
            TaskPoll::Pending(p) => assert!(p.done <= p.total, "{p:?}"),
        }
    }
    panic!("bounded fixture did not finish");
}

#[test]
fn transparent_png_has_straight_srgb_colour_without_a_shared_edge_seam() {
    let scene = scene(vec![square("Glass", [220, 40, 20], 0.5, Vec3::ZERO)]);
    let png = finish(ImageTask::new(scene, request(), CancelToken::default()).unwrap());
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
    for (x, y) in [(32, 32), (31, 31), (30, 32), (32, 30)] {
        let p = image.get_pixel(x, y).0;
        assert!((i16::from(p[3]) - 128).abs() <= 1, "a shared diagonal must not double-blend: {p:?}");
        assert!(p[0] > 180 && p[0] > p[1] + 60, "RGB is straight, not multiplied by 0.5: {p:?}");
    }
    let reader = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
    assert_eq!(reader.info().srgb, Some(png::SrgbRenderingIntent::Perceptual));
}

#[test]
fn glass_is_sorted_back_to_front_independently_of_body_order() {
    let request = request();
    let far = square("Far", [240, 20, 20], 0.5, request.camera.back() * -5.0);
    let near = square("Near", [20, 20, 240], 0.5, request.camera.back() * 5.0);
    let a = finish(ImageTask::new(scene(vec![far.clone(), near.clone()]), request.clone(), CancelToken::default()).unwrap());
    let b = finish(ImageTask::new(scene(vec![near, far]), request, CancelToken::default()).unwrap());
    assert_eq!(a, b);
    let image = image::load_from_memory(&a).unwrap().to_rgba8();
    let p = image.get_pixel(32, 32).0;
    assert!((i16::from(p[3]) - 192).abs() <= 1, "two half-transparent layers: {p:?}");
    assert!(p[2] > p[0] + 30, "the near blue layer contributes more: {p:?}");
}

#[test]
fn physical_transmission_keeps_the_exact_scalar_cpu_preview() {
    let original = square("Glass", [80, 180, 140], 0.25, Vec3::ZERO);
    let mut glass = original.clone();
    glass.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(0.2, 0.4, 0.3), 0.0, 0.1, 1.5).unwrap());
    glass.transmission = Some(crate::transmission::Transmission::new(Vec3::new(-0.7_f64.ln(), -0.9_f64.ln(), 0.0) / 10.0, None).unwrap());
    let expected = finish(ImageTask::new(scene(vec![original]), request(), CancelToken::default()).unwrap());
    let actual = finish(ImageTask::new(scene(vec![glass]), request(), CancelToken::default()).unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn cpu_raytrace_refuses_physical_transmission_before_preparation() {
    let mut glass = square("Glass", [200; 3], 1.0, Vec3::ZERO);
    glass.transmission = Some(crate::transmission::Transmission::new(Vec3::ZERO, Some(1.0)).unwrap());
    let mut input = request();
    input.integrator = super::super::Integrator::Raytrace;
    input.transparent_background = false;
    let error = admit(&scene(vec![glass]), &input).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(error.message.contains("physical transmission"));
}

#[test]
fn cancellation_is_observed_during_preparation_sort_pixels_and_encoding() {
    for stage in [Stage::Looks, Stage::Prepare, Stage::Sort, Stage::Clear, Stage::Glass, Stage::Downsample, Stage::Encode] {
        let cancel = CancelToken::default();
        let mut task = ImageTask::new(scene(vec![square("Glass", [40, 180, 100], 0.4, Vec3::ZERO)]), request(), cancel.clone()).unwrap();
        let mut reached = false;
        for _ in 0..100_000 {
            if task.stage == stage {
                reached = true;
                break;
            }
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        }
        assert!(reached);
        cancel.cancel();
        assert_eq!(task.step(4096).err().unwrap().code, ErrorCode::Cancelled);
    }
}

#[test]
fn encoding_yields_between_rows_and_completed_result_is_consumed_once() {
    let mut task = ImageTask::new(scene(Vec::new()), request(), CancelToken::default()).unwrap();
    while task.stage != Stage::Encode {
        assert!(matches!(task.step(64).unwrap(), TaskPoll::Pending(_)));
    }
    let before = task.encoder.as_ref().unwrap().row();
    assert!(matches!(task.step(usize::MAX).unwrap(), TaskPoll::Pending(_)));
    assert!(task.encoder.as_ref().unwrap().row() - before <= 4);
    loop {
        if let TaskPoll::Complete(bytes) = task.step(4096).unwrap() {
            assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 64);
            break;
        }
    }
    assert_eq!(task.step(1).err().unwrap().code, ErrorCode::NotCompleted);
}

#[test]
fn invalid_sizes_indices_opacity_and_large_work_lists_fail_without_a_frame() {
    let body = square("Product", [100, 120, 140], 1.0, Vec3::ZERO);
    let mut bad_request = request();
    bad_request.width = u32::MAX;
    assert!(ImageTask::new(scene(vec![body.clone()]), bad_request, CancelToken::default()).is_err());
    for bad in [0, 1] {
        let mut body = body.clone();
        if bad == 0 {
            Arc::make_mut(&mut body.mesh).triangles[0][0] = u32::MAX;
        } else {
            body.faces.push(FaceAppearance { face: 0, color: [0; 3], opacity: f64::NAN, surface: None, transmission: None });
        }
        let mut task = ImageTask::new(scene(vec![body]), request(), CancelToken::default()).unwrap();
        let error = loop {
            if let Err(error) = task.step(64) {
                break error;
            }
        };
        assert_eq!(error.code, ErrorCode::InvalidScene);
    }
    let mut body = body;
    Arc::make_mut(&mut body.mesh).triangles.resize(MAX_TRIANGLES as usize + 1, [0, 1, 2]);
    assert_eq!(ImageTask::new(scene(vec![body]), request(), CancelToken::default()).err().unwrap().code, ErrorCode::LimitExceeded);
    assert_eq!(std::mem::size_of::<Draw>(), 16, "admission accounts for each of the three lists");
}

#[test]
fn registered_executor_geometry_admission_excludes_the_cpu_raster_estimate() {
    // A library-bounded source: about 100 MiB of face appearances on a small mesh at the
    // largest image. Its CPU raster estimate is above 384 MiB at a registered executor's cap.
    let count = (100 * 1024 * 1024 / std::mem::size_of::<FaceAppearance>()).min(1_000_000);
    let mut body = square("appearances", [200; 3], 1.0, Vec3::ZERO);
    body.faces = vec![FaceAppearance { transmission: None, face: 0, color: [200; 3], opacity: 1.0, surface: None }; count];
    let source = scene(vec![body]);
    let mut r = request();
    r.width = 2048;
    r.height = 2048;
    let cpu = admit_raster(admit_source(&source, &r, 1_048_576).unwrap(), &r).unwrap_err();
    assert_eq!(cpu.code, ErrorCode::LimitExceeded);
    assert!(cpu.message.contains("384 MiB"), "{}", cpu.message);
    let geometry = admit_geometry(&source, &r, 1_048_576).unwrap();
    assert_eq!((geometry.pixel_scratch_bytes, geometry.preparation_bytes), (0, 0));
    assert_eq!(geometry.estimated_total_bytes, geometry.geometry_bytes);
    assert_eq!(geometry.face_appearances, count as u64);
    // The CPU host itself keeps rejecting the source.
    assert_eq!(admit(&source, &r).unwrap_err().code, ErrorCode::LimitExceeded);
}
// Original fixture: a plane at x=40, with exact +X normals. The perspective
// camera looks diagonally from (50/sqrt(2), -50/sqrt(2), 0). Its actual eye is
// on the -X side of this plane, although camera.back has a positive X component.
// The wide field of view keeps the off-axis plane in the image.
fn weighted_matte_view_scene(explicit: bool) -> ProductScene {
    let color = [96, 160, 220];
    let mut part = square("weighted matte plane", color, 1.0, Vec3::ZERO);
    let mesh = Arc::make_mut(&mut part.mesh);
    for p in &mut mesh.positions {
        *p = Vec3::new(40.0, p.x + 10.0, p.z);
    }
    mesh.normals = vec![Vec3::X; 4];
    if explicit {
        let [r, g, b] = color.map(decode);
        part.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(r, g, b), 0.0, 1.0, 1.5).unwrap());
    }
    scene(vec![part])
}

fn weighted_matte_view_request(perspective: bool) -> ImageRequest {
    let mut input = request();
    input.camera.yaw = -std::f64::consts::FRAC_PI_4;
    input.camera.distance = if perspective { 50.0 } else { 100.0 };
    input.camera.fov = if perspective { 2.4 } else { 0.0 };
    input.transparent_background = false;
    // Deliberately not equal: accidentally using light weights instead of
    // perspective barycentric weights must not be an equivalent calculation.
    input.studio_weights = Some([1.0, 0.35, 0.8]);
    input
}

#[test]
fn weighted_scalar_matte_uses_the_same_perspective_view_as_its_explicit_surface() {
    let input = weighted_matte_view_request(true);
    assert!(input.camera.back().x > 0.0 && input.camera.eye().x < 40.0);
    let scalar = finish(ImageTask::new(weighted_matte_view_scene(false), input.clone(), CancelToken::default()).unwrap());
    let explicit = finish(ImageTask::new(weighted_matte_view_scene(true), input, CancelToken::default()).unwrap());
    // Same admitted geometry, exact unit-axis normals, colour decoder, matte
    // response, lights, camera, buffers and encoder. No tolerance or golden.
    assert_eq!(scalar, explicit);
    let image = image::load_from_memory(&scalar).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
    // Actual view is behind +X everywhere on the plane, so the existing
    // one-sided BRDF returns zero. This also proves visible product coverage.
    assert!(image.pixels().any(|pixel| pixel.0 == [0, 0, 0, 255]));
}

#[test]
fn weighted_scalar_matte_keeps_exact_orthographic_surface_equivalence() {
    let input = weighted_matte_view_request(false);
    let scalar = finish(ImageTask::new(weighted_matte_view_scene(false), input.clone(), CancelToken::default()).unwrap());
    let explicit = finish(ImageTask::new(weighted_matte_view_scene(true), input, CancelToken::default()).unwrap());
    assert_eq!(scalar, explicit);
    let image = image::load_from_memory(&scalar).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
    assert!(image.pixels().any(|pixel| pixel.0 != [20, 40, 60, 255] && pixel.0 != [0, 0, 0, 255]));
}

#[test]
fn weighted_scalar_matte_mute_still_removes_only_product_light() {
    let mut input = weighted_matte_view_request(false);
    let lit = finish(ImageTask::new(weighted_matte_view_scene(false), input.clone(), CancelToken::default()).unwrap());
    input.studio_weights = Some([0.0; 3]);
    let muted = finish(ImageTask::new(weighted_matte_view_scene(false), input, CancelToken::default()).unwrap());
    assert_ne!(lit, muted);
    for bytes in [&lit, &muted] {
        let image = image::load_from_memory(bytes).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
    }
    let image = image::load_from_memory(&muted).unwrap().to_rgba8();
    assert!(image.pixels().any(|pixel| pixel.0 == [0, 0, 0, 255]));
}

// Original analytic reciprocal-w oracle: the expected surface point is found
// by intersecting a closed-form camera ray with a plane, not by asking either
// raster shading branch to interpolate world coordinates for the other.
#[test]
fn weighted_scalar_perspective_uses_closed_form_ray_plane_world_with_unequal_clip_w() {
    let vertices = [Vec3::new(-7.0 / 4.0, -8.0, 7.0 / 4.0), Vec3::new(1.0 / 2.0, -6.0, 7.0 / 2.0), Vec3::new(-3.0, -2.0, -1.0)];
    // The reversed winding faces the eye. This smooth normal is in the
    // geometric hemisphere and deliberately separates the three view oracles.
    let normal = Vec3::new(3.0, 1.0, 0.0) / 10.0_f64.sqrt();
    let part = ProductMesh {
        name: "analytic reciprocal-w matte".into(),
        mesh: Arc::new(Mesh {
            positions: vertices.to_vec(),
            normals: vec![normal; 3],
            triangles: vec![[0, 2, 1]],
            tri_face: vec![0],
            ..Default::default()
        }),
        color: [0; 3],
        opacity: 1.0,
        surface: None,
        transmission: None,
        faces: Vec::new(),
    };
    let mut input = request();
    input.width = 4;
    input.height = 4;
    input.camera =
        Camera { target: Vec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, distance: 10.0, fov: std::f64::consts::FRAC_PI_2, up: crate::UpAxis::Z };
    input.transparent_background = false;
    let light_weights = [1.0, 0.35, 0.8];
    input.studio_weights = Some(light_weights);
    let mut task = ImageTask::new(scene(vec![part]), input, CancelToken::default()).unwrap();
    assert_eq!((task.sw, task.sh), (8, 8));
    let eye = Vec3::new(0.0, -10.0, 0.0);
    let back = Vec3::new(0.0, -1.0, 0.0);
    assert_eq!(task.request.camera.eye(), eye);
    assert_eq!(task.back, back);

    // Actual production projection of [A,C,B], not a hand-filled Triangle.
    // With tan(fov/2)=1, clip w is depth from the eye: [2,8,4].
    let projected = task.project(Draw::default()).unwrap().unwrap();
    assert!(projected.surface.is_none());
    let expected_points = [[0.5, 0.5], [2.5, 4.5], [4.5, 0.5]];
    for ((actual, expected), (actual_inv_w, expected_inv_w)) in
        projected.points.iter().zip(expected_points).zip(projected.inv_w.iter().zip([0.5, 0.125, 0.25]))
    {
        assert!((actual[0] - expected[0]).abs() < 1e-12);
        assert!((actual[1] - expected[1]).abs() < 1e-12);
        assert!((*actual_inv_w - expected_inv_w).abs() < 1e-12);
    }
    assert_eq!(projected.world, [vertices[0], vertices[2], vertices[1]]);

    // Pixel sample (2.5,2.5) gives the analytic forward ray
    // E + t*(-3/8,1,3/8). The triangle plane is 4x-y-4z=-6:
    // plane(E)=10 and plane(ray)=-4, hence t=4 exactly.
    let plane = Vec3::new(4.0, -1.0, -4.0);
    assert_eq!((vertices[2] - vertices[0]).cross(vertices[1] - vertices[0]), plane * 4.0);
    assert!(normal.dot(plane) > 0.0);
    for vertex in vertices {
        assert_eq!(plane.dot(vertex), -6.0);
    }
    let ray = Vec3::new(-3.0 / 8.0, 1.0, 3.0 / 8.0);
    assert_eq!(plane.dot(eye), 10.0);
    assert_eq!(plane.dot(ray), -4.0);
    let expected_world = Vec3::new(-3.0 / 2.0, -6.0, 3.0 / 2.0);
    assert_eq!(eye + ray * 4.0, expected_world);
    let expected_view = Vec3::new(3.0 / 2.0, -4.0, -3.0 / 2.0);
    assert_eq!(eye - expected_world, expected_view);
    // Screen barycentrics [1/4,1/2,1/4] and clip w [2,8,4]
    // imply world barycentrics [1/2,1/4,1/4]. An AFFINE world
    // would instead be (-29/16,-9/2,13/16), a different camera ray.
    let affine_world = Vec3::new(-29.0 / 16.0, -9.0 / 2.0, 13.0 / 16.0);
    let affine_view = eye - affine_world;
    assert!((normal.dot(expected_view) - 1.0 / (2.0 * 10.0_f64.sqrt())).abs() < 1e-14);
    assert!((normal.dot(affine_view) + 1.0 / (16.0 * 10.0_f64.sqrt())).abs() < 1e-14);
    assert!(normal.dot(back) < 0.0);

    // Black base removes diffuse but retains dielectric Fresnel. Only the
    // key light is above this normal. For roughness=1, D=1/pi and Smith
    // visibility reduces to 1/((1+nv)*(1+nl)). F0=(1/5)^2.
    let nv = 1.0 / 820.0_f64.sqrt();
    let nl = 9.0 / (5.0 * 10.0_f64.sqrt());
    let vh = ((1.0 - 3.0 / (5.0 * 82.0_f64.sqrt())) / 2.0).sqrt();
    let fresnel = 1.0 / 25.0 + (24.0 / 25.0) * (1.0 - vh).powi(5);
    let expected_radiance = 3.0 * nl * fresnel / (std::f64::consts::PI * (1.0 + nv) * (1.0 + nl));
    let matte = crate::surface::OpaqueSurface::new(Vec3::ZERO, 0.0, 1.0, 1.5).unwrap();
    let reference = crate::studio::shade_weighted(matte, normal, expected_view, light_weights).unwrap();
    assert!((reference - Vec3::new(expected_radiance, expected_radiance, expected_radiance)).len() < 1e-14);
    assert_eq!(crate::studio::shade_weighted(matte, normal, affine_view, light_weights).unwrap(), Vec3::ZERO);
    assert_eq!(crate::studio::shade_weighted(matte, normal, back, light_weights).unwrap(), Vec3::ZERO);

    // The original radical expression gives radiance0.0144120124528...
    // Reinhard then sRGB yields31.6843707068... on the8-bit scale, hence32.
    // Inspect the real production supersample before downsampling; no fake
    // renderer, injected pixel, shared-branch PNG equality or golden is used.
    for _ in 0..2048 {
        if task.stage == Stage::Downsample {
            break;
        }
        assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
    }
    assert!(task.stage == Stage::Downsample);
    let sample = 2 * task.sw + 2;
    assert!(task.depth[sample].is_finite(), "the analytic sample must be covered");
    assert_eq!(task.pixels[sample], [32, 32, 32, 255]);
    // Finish through the ordinary encoder as well; the unoccupied corner
    // remains the original opaque background.
    let png = finish(task);
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!((image.width(), image.height()), (4, 4));
    assert_eq!(image.get_pixel(3, 3).0, [20, 40, 60, 255]);
}

#[test]
fn explicit_raster_linear_rgb_reaches_png_with_one_display_transfer() {
    let mut part = square("matte", [0; 3], 1.0, Vec3::ZERO);
    part.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap());
    let mut input = request();
    // Orthographic Front is exactly normal to the square, so no perspective-view approximation.
    assert_eq!(input.camera.fov, 0.0);
    input.raster_lights = Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [0.25, 0.5, 1.0], intensity: 2.0 }]);
    let png = finish(ImageTask::new(scene(vec![part]), input, CancelToken::default()).unwrap());
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    // Independently derived: [.4,.5,.5]/pi -> Reinhard -> piecewise sRGB -> nearest byte.
    for (x, y) in [(32, 32), (31, 32), (32, 31)] {
        assert_eq!(image.get_pixel(x, y).0, [94, 104, 104, 255]);
    }
    assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
}

#[test]
fn explicit_raster_three_four_five_cosine_reaches_png() {
    let mut part = square("matte", [0; 3], 1.0, Vec3::ZERO);
    part.surface = Some(crate::surface::OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap());
    let mut input = request();
    input.raster_lights = Some(vec![crate::job::DirectionalLightOptions { direction: [3.0, -4.0, 0.0], color: [1.0; 3], intensity: 2.0 }]);
    let png = finish(ImageTask::new(scene(vec![part]), input, CancelToken::default()).unwrap());
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    // Independently derived: [1.28,.8,.4]/pi -> Reinhard -> sRGB.
    assert_eq!(image.get_pixel(32, 32).0, [146, 124, 94, 255]);
}

#[test]
fn explicit_empty_raster_rig_is_dark_while_absence_keeps_legacy() {
    let part = scene(vec![square("part", [200, 180, 160], 1.0, Vec3::ZERO)]);
    let mut input = request();
    input.transparent_background = false;
    let absent = finish(ImageTask::new(part.clone(), input.clone(), CancelToken::default()).unwrap());
    let wire = serde_json::to_value(&input).unwrap();
    assert!(wire.get("raster_lights").is_none());
    let mut null = wire.clone();
    null["raster_lights"] = serde_json::Value::Null;
    let legacy: ImageRequest = serde_json::from_value(null).unwrap();
    assert_eq!(finish(ImageTask::new(part.clone(), legacy, CancelToken::default()).unwrap()), absent);
    input.raster_lights = Some(Vec::new());
    let dark = finish(ImageTask::new(part, input, CancelToken::default()).unwrap());
    assert_ne!(dark, absent);
    let image = image::load_from_memory(&dark).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(32, 32).0, [0, 0, 0, 255]);
    assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
}

#[test]
fn explicit_raster_request_rejects_capacity_conflicts_and_foreign_integrators() {
    let part = scene(vec![square("part", [200; 3], 1.0, Vec3::ZERO)]);
    let light = crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [1.0; 3], intensity: 1.0 };
    let mut input = request();
    let mut oversized = Vec::with_capacity(9);
    oversized.push(light);
    input.raster_lights = Some(oversized);
    assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
    input.raster_lights = Some(Vec::new());
    input.studio_weights = Some([1.0; 3]);
    assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
    input.studio_weights = None;
    input.raytrace = Some(Default::default());
    assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
    input.raytrace = None;
    input.integrator = crate::job::Integrator::Raytrace;
    assert_eq!(admit(&part, &input).unwrap_err().code, ErrorCode::InvalidRequest);
}

#[test]
fn explicit_raster_scalar_surrogate_preserves_straight_alpha_without_claiming_glass() {
    let part = square("scalar", [0; 3], 0.5, Vec3::ZERO);
    let mut input = request();
    input.raster_lights = Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [1.0; 3], intensity: 2.0 }]);
    let png = finish(ImageTask::new(scene(vec![part.clone()]), input.clone(), CancelToken::default()).unwrap());
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    // Scalar surrogate rho=0, IOR=1.5, roughness=1: F0/4pi=.01/pi.
    // Intensity2 gives .02/pi; Reinhard + sRGB independently rounds to19.
    assert_eq!(image.get_pixel(32, 32).0, [19, 19, 19, 128]);
    let mut physical = part;
    physical.transmission = Some(crate::transmission::Transmission::new(Vec3::ZERO, None).unwrap());
    assert_eq!(admit(&scene(vec![physical]), &input).unwrap_err().code, ErrorCode::InvalidRequest);
}
#[test]
fn direct_raster_task_accounts_empty_rig_capacity_without_changing_dark_or_legacy_paths() {
    let part = scene(vec![square("part", [200; 3], 1.0, Vec3::ZERO)]);
    let legacy = request();
    let legacy_bytes = admit(&part, &legacy).unwrap().estimated_total_bytes;
    assert!(ImageTask::new(part.clone(), legacy, CancelToken::default()).unwrap().request.raster_lights.is_none());
    let fee = (8 * std::mem::size_of::<crate::job::DirectionalLightOptions>()) as u64;
    let mut dark_png = None;
    for capacity in [0, 8] {
        let mut input = request();
        input.raster_lights = Some(Vec::with_capacity(capacity));
        let retained_capacity = input.raster_lights.as_ref().unwrap().capacity();
        assert!(retained_capacity >= capacity && retained_capacity <= 8);
        let actual_bytes = (retained_capacity * std::mem::size_of::<crate::job::DirectionalLightOptions>()) as u64;
        let admitted = admit(&part, &input).unwrap();
        assert_eq!(admitted.estimated_total_bytes, legacy_bytes + fee);
        assert!(admitted.estimated_total_bytes - legacy_bytes >= actual_bytes);
        // This is the public direct constructor, not host try_copy/Record fees.
        let task = ImageTask::new(part.clone(), input, CancelToken::default()).unwrap();
        assert_eq!(task.request.raster_lights.as_ref().unwrap().capacity(), retained_capacity);
        assert_eq!(task.admission.estimated_total_bytes, legacy_bytes + fee);
        let png = finish(task);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(32, 32).0, [0, 0, 0, 255]);
        if let Some(expected) = &dark_png {
            assert_eq!(&png, expected);
        } else {
            dark_png = Some(png);
        }
    }
}

fn raster_local_triangle() -> Triangle {
    let surface = crate::surface::OpaqueSurface::new(Vec3::new(0.8, 0.5, 0.25), 0.0, 1.0, 1.0).unwrap();
    Triangle {
        points: [[-1.0, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]],
        world: [Vec3::new(-1.0, 0.0, 1.0), Vec3::new(3.0, 0.0, 1.0), Vec3::new(-1.0, 0.0, -3.0)],
        normals: [-Vec3::Y; 3],
        inv_w: [1.0; 3],
        colour: [0.0; 3],
        opacity: 1.0,
        surface: Some(surface),
        centre_depth: 0.0,
        area: 16.0,
        bounds: [0, 0, 0, 0],
        x: 0,
        y: 0,
    }
}

fn raster_local_point() -> crate::job::PunctualLightOptions {
    crate::job::PunctualLightOptions {
        position_mm: [0.5, -10.0, -0.5],
        color: [0.25, 0.5, 1.0],
        intensity: 2.0,
        reference_distance_mm: 10.0,
        minimum_distance_mm: 0.001,
        cone: None,
    }
}

fn raster_local_area() -> crate::job::RectangleLightOptions {
    crate::job::RectangleLightOptions {
        center_mm: [0.5, -10.0, -0.5],
        edge_u_mm: [2.0, 0.0, 0.0],
        edge_v_mm: [0.0, 0.0, -2.0],
        color: [0.25, 0.5, 1.0],
        intensity: 2.0,
    }
}

fn raster_local_pixel(triangle: &Triangle, input: &ImageRequest) -> [u8; 4] {
    let mut pixels = [[0; 4]];
    let mut depth = [f32::INFINITY];
    raster_pixel(triangle, 1, -Vec3::Y, None, input, &mut pixels, &mut depth).unwrap();
    pixels[0]
}

fn raster_local_expected(numerators: [f64; 3]) -> [u8; 4] {
    let rgb = numerators.map(|n| {
        let linear = n / std::f64::consts::PI;
        solidcraft_geom::srgb::encode8(linear / (1.0 + linear))
    });
    [rgb[0], rgb[1], rgb[2], 255]
}

#[test]
fn raster_local_point_pixel_matches_independent_lambert_and_single_exposure_oracles() {
    // Affine barycentrics (1/4,3/8,3/8) put this sample at(.5,0,-.5).
    // Point is10mm along the normal, at its10mm reference: RGB*rho*I/pi.
    let mut input = request();
    input.raster_punctual_lights = Some(vec![raster_local_point()]);
    for (ev, expected) in [(-1.0, [69, 77, 77, 255]), (0.0, [94, 104, 104, 255]), (1.0, [124, 135, 135, 255])] {
        input.raster_exposure_ev = ev;
        assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), expected);
    }
    input.raster_exposure_ev = 0.0;
    input.raster_punctual_lights.as_mut().unwrap()[0].position_mm[1] = -20.0;
    assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), raster_local_expected([0.1, 0.125, 0.125]));
}

#[test]
fn raster_local_spot_area_and_mixed_pixels_have_original_closed_form_contributions() {
    let mut point = raster_local_point();
    point.cone = Some(crate::job::SpotConeOptions {
        emitted_axis: [3.0_f64.sqrt(), 1.0, 0.0],
        inner_half_angle_radians: 0.8_f64.acos(),
        outer_half_angle_radians: 0.2_f64.acos(),
    });
    let mut input = request();
    input.raster_punctual_lights = Some(vec![point]);
    // Axis cosine1/2 lies halfway between(4/5,1/5), so smoothstep=1/2.
    assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), raster_local_expected([0.2, 0.25, 0.25]));
    input.raster_punctual_lights = None;
    input.raster_area_lights = Some(vec![raster_local_area()]);
    // Area4 * I2 / r^2(100) =.08 before rho*colour/pi.
    assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), raster_local_expected([0.016, 0.02, 0.02]));
    input.raster_lights = Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [1.0; 3], intensity: 1.0 }]);
    input.raster_punctual_lights = Some(vec![raster_local_point()]);
    assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), raster_local_expected([1.216, 1.02, 0.77]));
    input.raster_lights = None;
    input.raster_punctual_lights = None;
    input.raster_area_lights.as_mut().unwrap()[0].edge_v_mm = [0.0, 0.0, 2.0];
    assert_eq!(raster_local_pixel(&raster_local_triangle(), &input), [0, 0, 0, 255]);
}

#[test]
fn raster_local_pixel_uses_reciprocal_w_world_position_and_zero_coincidence_policy() {
    let mut triangle = raster_local_triangle();
    triangle.inv_w = [1.0, 2.0, 8.0];
    // Weighted world=(-1/4,0,-2), not the affine(.5,0,-.5).
    let mut point = raster_local_point();
    point.position_mm = [-0.25, -1.0, -2.0];
    point.reference_distance_mm = 1.0;
    point.color = [1.0; 3];
    let mut input = request();
    input.raster_punctual_lights = Some(vec![point]);
    assert_eq!(raster_local_pixel(&triangle, &input), raster_local_expected([1.6, 1.0, 0.5]));
    input.raster_punctual_lights.as_mut().unwrap()[0].position_mm = [-0.25, 0.0, -2.0];
    assert_eq!(raster_local_pixel(&triangle, &input), [0, 0, 0, 255]);
}

#[test]
fn raster_local_real_task_empty_rig_is_dark_and_null_keeps_exact_legacy_png() {
    let part = scene(vec![square("opaque", [200, 180, 160], 1.0, Vec3::ZERO)]);
    let mut input = request();
    input.transparent_background = false;
    let legacy = finish(ImageTask::new(part.clone(), input.clone(), CancelToken::default()).unwrap());
    let mut wire = serde_json::to_value(&input).unwrap();
    wire["raster_punctual_lights"] = serde_json::Value::Null;
    wire["raster_area_lights"] = serde_json::Value::Null;
    let decoded: ImageRequest = serde_json::from_value(wire).unwrap();
    assert_eq!(finish(ImageTask::new(part.clone(), decoded, CancelToken::default()).unwrap()), legacy);
    for family in [0, 1] {
        let mut explicit = input.clone();
        if family == 0 {
            explicit.raster_punctual_lights = Some(Vec::new());
        } else {
            explicit.raster_area_lights = Some(Vec::new());
        }
        let png = finish(ImageTask::new(part.clone(), explicit, CancelToken::default()).unwrap());
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(32, 32).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 0).0, [20, 40, 60, 255]);
        assert_ne!(png, legacy);
    }
}

#[test]
fn raster_local_real_task_admits_spare_capacity_and_refuses_physical_transmission() {
    let part = scene(vec![square("opaque", [200; 3], 1.0, Vec3::ZERO)]);
    let mut input = request();
    let legacy_bytes = admit(&part, &input).unwrap().estimated_total_bytes;
    input.raster_punctual_lights = Some(Vec::with_capacity(8));
    input.raster_area_lights = Some(Vec::with_capacity(4));
    let fee = (8 * std::mem::size_of::<crate::job::PunctualLightOptions>() + 4 * std::mem::size_of::<crate::job::RectangleLightOptions>()) as u64;
    assert_eq!(admit(&part, &input).unwrap().estimated_total_bytes, legacy_bytes + fee);
    let capacities = (input.raster_punctual_lights.as_ref().unwrap().capacity(), input.raster_area_lights.as_ref().unwrap().capacity());
    let task = ImageTask::new(part, input, CancelToken::default()).unwrap();
    assert_eq!(task.request.raster_punctual_lights.as_ref().unwrap().capacity(), capacities.0);
    assert_eq!(task.request.raster_area_lights.as_ref().unwrap().capacity(), capacities.1);
    assert_eq!(task.admission.estimated_total_bytes, legacy_bytes + fee);
    let mut physical = square("physical", [200; 3], 1.0, Vec3::ZERO);
    physical.transmission = Some(crate::transmission::Transmission::new(Vec3::ZERO, None).unwrap());
    assert_eq!(admit(&scene(vec![physical]), &task.request).unwrap_err().code, ErrorCode::InvalidRequest);
}

#[test]
fn raster_local_light_work_remains_inside_original_cap_and_step_cancellation() {
    let part = scene(vec![square("opaque", [200; 3], 1.0, Vec3::ZERO)]);
    let mut legacy = request();
    legacy.raster_lights = Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [1.0; 3], intensity: 1.0 }]);
    let mut local = legacy.clone();
    local.raster_punctual_lights = Some(vec![raster_local_point()]);
    local.raster_area_lights = Some(vec![raster_local_area()]);
    let cancel = CancelToken::default();
    let mut task = ImageTask::new(part.clone(), local, cancel.clone()).unwrap();
    let triangle = task.project(Draw { body: 0, triangle: 0, depth: 0.0 }).unwrap().unwrap();
    let [x0, x1, y0, y1] = triangle.bounds;
    let pixels = ((x1 - x0 + 1) * (y1 - y0 + 1)) as u64;
    task.pixel_tests = MAX_PIXEL_TESTS - pixels * 3 + 1;
    assert_eq!(task.prepare_step().unwrap_err().code, ErrorCode::LimitExceeded);
    let mut old_task = ImageTask::new(part, legacy, CancelToken::default()).unwrap();
    old_task.pixel_tests = MAX_PIXEL_TESTS - pixels * 3 + 1;
    old_task.prepare_step().unwrap();
    assert_eq!(old_task.pixel_tests, MAX_PIXEL_TESTS - pixels * 2 + 1, "directional-only budget is unchanged");
    cancel.cancel();
    assert_eq!(task.step(1).err().unwrap().code, ErrorCode::Cancelled);
}

#[test]
fn raster_local_running_progress_counts_pixel_visits_and_reaches_total_before_encoding() {
    // Cost is1 for unchanged implicit/directional-only routes,3 for the minimal
    // mixed rig and20 for8D+8 shared point/spot+4area. Progress counts actual
    // clear/draw/downsample visits, never BRDF evaluations or light intensity.
    for (factor, directional_count, punctual_count, area_count) in [(1u64, 0usize, 0usize, 0usize), (1, 1, 0, 0), (3, 1, 1, 1), (20, 8, 8, 4)] {
        let mut input = request();
        input.width = 8;
        input.height = 8;
        if directional_count > 0 {
            input.raster_lights =
                Some(vec![crate::job::DirectionalLightOptions { direction: [0.0, -1.0, 0.0], color: [1.0; 3], intensity: 1.0 }; directional_count]);
        }
        if punctual_count > 0 {
            let mut points = vec![raster_local_point(); punctual_count];
            points[punctual_count - 1].cone =
                Some(crate::job::SpotConeOptions { emitted_axis: [0.0, 1.0, 0.0], inner_half_angle_radians: 0.2, outer_half_angle_radians: 0.5 });
            input.raster_punctual_lights = Some(points);
        }
        if area_count > 0 {
            input.raster_area_lights = Some(vec![raster_local_area(); area_count]);
        }
        let back = input.camera.back();
        let part = scene(vec![
            square("opaque", [160; 3], 1.0, Vec3::ZERO),
            square("alpha far", [160; 3], 0.5, back * -5.0),
            square("alpha nearer", [160; 3], 0.5, back * -3.0),
        ]);
        let mut task = ImageTask::new(part, input, CancelToken::default()).unwrap();
        for _ in 0..100_000 {
            if task.stage == Stage::Clear {
                break;
            }
            assert!(matches!(task.stage, Stage::Looks | Stage::Prepare | Stage::Sort));
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        }
        assert!(task.stage == Stage::Clear, "bounded fixture must finish preparation");
        let mut bbox_visits = 0u64;
        for draw in task.opaque.iter().chain(&task.glass) {
            let triangle = task.project(*draw).unwrap().unwrap();
            let [x0, x1, y0, y1] = triangle.bounds;
            bbox_visits += ((x1 - x0 + 1) * (y1 - y0 + 1)) as u64;
        }
        assert!(bbox_visits > 0);
        assert_eq!(task.pixel_tests, bbox_visits * factor, "weighted128M work guard remains authoritative");
        let clear_visits = (task.sw * task.sh) as u64;
        let output_visits = u64::from(task.request.width) * u64::from(task.request.height);
        let expected_total = clear_visits + bbox_visits + output_visits;
        let mut last_done = 0;
        let mut saw_opaque = false;
        let mut saw_downsample = false;
        let mut saw_running_complete = false;
        for _ in 0..100_000 {
            if task.stage == Stage::Encode {
                break;
            }
            let progress = task.progress();
            assert_eq!(progress.phase, Phase::Running);
            assert_eq!(progress.total, expected_total, "total must not scale with {factor} lights");
            assert!(progress.done >= last_done && progress.done <= progress.total);
            if !saw_opaque && task.stage == Stage::Opaque {
                assert_eq!(progress.done, clear_visits, "clear units unchanged");
                saw_opaque = true;
            }
            if !saw_downsample && task.stage == Stage::Downsample {
                assert_eq!(progress.done, clear_visits + bbox_visits, "every projected visit completed before downsample");
                saw_downsample = true;
            }
            if progress.done == progress.total {
                saw_running_complete = true;
            }
            last_done = progress.done;
            assert!(matches!(task.step(1).unwrap(), TaskPoll::Pending(_)));
        }
        assert!(task.stage == Stage::Encode);
        assert!(saw_opaque && saw_downsample && saw_running_complete, "Running must reach its boundary before Encoding");
        assert_eq!(last_done, expected_total);
        assert_eq!(task.pixel_done, expected_total);
        assert_eq!(task.progress().phase, Phase::Encoding);
        let png = finish(task);
        assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8().dimensions(), (8, 8));
    }
}

#[test]
fn captured_cpu_preview_cadence_roundtrips_without_saved_sampling_or_nested_aliases() {
    let mut input = request();
    input.integrator = super::super::Integrator::Raytrace;
    input.transparent_background = false;
    input.raytrace = Some(super::super::RaytraceOptions {
        progressive: Some(super::super::CpuProgressiveOptions { samples_per_pixel: 5, seed: 13 }),
        ..Default::default()
    });
    for every_pass in [true, false] {
        input.cpu_progressive_every_pass = every_pass;
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire.get("cpu_progressive_every_pass"), (!every_pass).then_some(&serde_json::Value::Bool(false)));
        assert!(wire["raytrace"].get("cpu_progressive_every_pass").is_none());
        let decoded: ImageRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded.cpu_progressive_every_pass, every_pass);
        assert_eq!(decoded.raytrace.as_ref().unwrap().progressive, input.raytrace.as_ref().unwrap().progressive);
        assert_eq!(decoded.try_clone_for_host().unwrap().cpu_progressive_every_pass, every_pass);
    }
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn legacy_protected_feedback_restores_the_original_global_distribution_and_sixty_four_cap() {
    use super::super::{CpuBudget, ProtectedPreviewPlan};
    let tiny = ProtectedPreviewPlan::with_cadence(2, 1, 5, CpuBudget::Compact, false).unwrap();
    assert_eq!(tiny.quota_limit(), 9);
    assert_eq!((0..9).map(|index| tiny.milestone(index).unwrap()).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let plan = ProtectedPreviewPlan::with_cadence(65, 1, 3, CpuBudget::Compact, false).unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.quota_limit(), 64);
    assert_eq!([0, 1, 31, 32, 63].map(|index| plan.milestone(index).unwrap()), [1, 4, 95, 99, 194]);
    let milestones: Vec<_> = (0..64).map(|index| plan.milestone(index).unwrap()).collect();
    for committed in 0..=195 {
        let expected = milestones.iter().filter(|&&milestone| milestone <= committed).count() as u32;
        assert_eq!(plan.reached_milestones(committed).unwrap(), expected);
    }
    assert!(plan.reached_milestones(196).is_err());
    assert!(plan.milestone(64).is_err());
    assert_eq!(plan.presentation_limit().unwrap(), 64 * plan.snapshot_work().unwrap());
    assert_eq!(plan.ordinary_limit(), CpuBudget::Compact.limits().max_work_units);
    let single = ProtectedPreviewPlan::with_cadence(1, 1, 1, CpuBudget::Compact, false).unwrap();
    assert_eq!((single.quota_limit(), single.reached_milestones(1).unwrap()), (0, 0));
    let one = ProtectedPreviewPlan::with_cadence(1, 1, 2, CpuBudget::Compact, false).unwrap();
    assert_eq!((one.quota_limit(), one.milestone(0).unwrap(), one.reached_milestones(1).unwrap()), (1, 1, 1));
    let high = ProtectedPreviewPlan::with_cadence(1, 1, 1_048_576, CpuBudget::Workstation, false).unwrap();
    assert_eq!((high.quota_limit(), high.milestone(63).unwrap()), (64, 1_048_575));
    assert_eq!(high.reached_milestones(1_048_576).unwrap(), 64);
    let current = ProtectedPreviewPlan::new(65, 1, 3, CpuBudget::Compact).unwrap();
    assert_eq!(current.quota_limit(), 66);
}
