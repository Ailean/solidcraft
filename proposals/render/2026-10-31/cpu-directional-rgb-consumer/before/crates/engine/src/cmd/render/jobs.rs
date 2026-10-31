use crate::{Result, Session, params::bad, render_jobs::job_error};
use serde::Deserialize;
use serde_json::{Value, json};
use solidcraft_doc::render_resources::{CpuHdrEnvironment, GpuHdrEnvironment};
use solidcraft_doc::render_settings::{RenderJob, RenderSetup};
use solidcraft_render::{
    RenderOperation, RenderRequest,
    job::{ImageRequest, Integrator, JobId, RaytraceOptions},
};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Start {
    #[serde(default, deserialize_with = "solidcraft_render::job::deserialize_raster_lights")]
    raster_lights: Option<Vec<solidcraft_render::job::DirectionalLightOptions>>,
    #[serde(default, deserialize_with = "solidcraft_render::job::deserialize_raster_punctual_lights")]
    raster_punctual_lights: Option<Vec<solidcraft_render::job::PunctualLightOptions>>,
    #[serde(default, deserialize_with = "solidcraft_render::job::deserialize_raster_area_lights")]
    raster_area_lights: Option<Vec<solidcraft_render::job::RectangleLightOptions>>,
    progressive_feedback: bool,
    nested_media: bool,
    studio_weights: Option<[f64; 3]>,
    progressive: bool,
    denoise: bool,
    samples: Option<u32>,
    max_depth: Option<u32>,
    seed: Option<u32>,
    integrator: Integrator,
    environment: Option<EnvironmentRequest>,
    raytrace: Option<RaytraceOptions>,
    led_strip: Option<solidcraft_render::job::led_strip::LedStripOptions>,
    led_samples: Option<u32>,
    setup: Option<String>,
    settings: Option<RenderSetup>,
    transparent_background: bool,
    replace: bool,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct EnvironmentRequest {
    path: String,
    resource: Option<solidcraft_doc::render_resources::HdrId>,
    expected_sha256: Option<solidcraft_doc::render_resources::HdrId>,
    rotation_radians: f64,
    intensity: f64,
    samples: u32,
}
impl Default for EnvironmentRequest {
    fn default() -> Self {
        Self { path: String::new(), resource: None, expected_sha256: None, rotation_radians: 0.0, intensity: 1.0, samples: 16 }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProgressiveStart {
    progressive_feedback: bool,
    studio_weights: Option<[f64; 3]>,
    aging: solidcraft_doc::render_aging::SavedAgingStack,
    #[serde(default)]
    nested_media: bool,
    volume: Option<solidcraft_render::job::volume::HomogeneousVolume>,
    adaptive: Option<solidcraft_render::job::adaptive::AdaptiveSampling>,
    transmission_samples: Option<u32>,
    finishing: solidcraft_render::job::finishing::Finishing,
    backdrop: Option<solidcraft_render::job::finishing::Backdrop>,
    linear_exr: bool,
    integrator: Integrator,
    setup: Option<String>,
    settings: Option<RenderSetup>,
    transparent_background: bool,
    replace: bool,
    progressive: bool,
    samples: Option<u32>,
    max_depth: Option<u32>,
    seed: Option<u32>,
    raytrace: super::transmission::Raytrace,
    lens: Option<solidcraft_render::job::ThinLens>,
    lighting: Option<solidcraft_render::job::StudioLighting>,
    tone_map: Option<solidcraft_render::job::ToneMap>,
    environment: Option<super::environment::Input>,
    denoise: bool,
}

struct Prepared {
    progressive_feedback: bool,
    replace: bool,
    scene: solidcraft_render::ProductScene,
    request: ImageRequest,
    reply: Value,
    executor: solidcraft_render::ExecutorId,
    options: solidcraft_render::job::JobOptions,
    environment: Option<solidcraft_render::lighting::environment_input::EnvironmentInput>,
}

fn lower_led(input: &mut Start, command: &str) -> Result<()> {
    if input.led_strip.is_none() && input.led_samples.is_some() {
        return Err(bad(command, "led_samples requires led_strip"));
    }
    if let Some(strip) = input.led_strip {
        if input.integrator != Integrator::Raytrace {
            return Err(bad(command, "led_strip requires integrator: raytrace"));
        }
        strip.append_to(input.raytrace.get_or_insert_default(), input.led_samples.unwrap_or(16)).map_err(job_error)?;
        input.led_strip = None;
        input.led_samples = None;
    }
    Ok(())
}

/// Pure preview lowering. Parse the existing CPU wire before changing its sampling;
/// unsupported effects remain errors rather than disappearing from the request.
fn interactive_params(params: &Value) -> Result<Value> {
    let command = "render.interactive";
    let mut input = Start::deserialize(params).map_err(|e| bad(command, e.to_string()))?;
    let object = params.as_object().ok_or_else(|| bad(command, "Interactive requires an explicit CPU request object"))?;
    if object.contains_key("setup") || object.contains_key("replace") {
        return Err(bad(command, "Interactive requires an explicit settings snapshot without setup or replace"));
    }
    if input.raster_lights.is_some() || input.raster_punctual_lights.is_some() || input.raster_area_lights.is_some() {
        return Err(bad(command, "Interactive does not accept explicit Raster lights"));
    }
    if input.integrator != Integrator::Raytrace || input.progressive || input.transparent_background {
        return Err(bad(command, "Interactive requires opaque CPU ray tracing, not GPU progressive or raster rendering"));
    }
    if input.studio_weights.is_some() || input.denoise || input.samples.is_some() || input.max_depth.is_some() || input.seed.is_some() {
        return Err(bad(command, "Interactive cannot accept GPU studio, denoise or sampling controls"));
    }
    let mut settings = input.settings.take().ok_or_else(|| bad(command, "Interactive requires explicit settings"))?;
    if settings.job.is_some() {
        return Err(bad(command, "Interactive requires a lowered CPU runtime request with settings.job: null"));
    }
    if settings.mode != solidcraft_render::RenderMode::Preview || settings.format != solidcraft_render::RenderFormat::Png {
        return Err(bad(command, "Interactive CPU requires the existing Preview/Image/PNG profile"));
    }
    // Keep the existing CPU runtime exposure authority; never silently neutralize
    // saved/common controls in order to make a request acceptable.
    if settings.exposure_ev != 0.0 || settings.environment_rotation != 0.0 || settings.environment_intensity != 1.0 {
        return Err(bad(command, "Interactive CPU exposure belongs to raytrace.exposure_ev; environment controls belong to the captured input"));
    }
    lower_led(&mut input, command)?;
    if let Some(environment) = &input.environment {
        // Same numerical controls as EnvironmentInput::new, checked before a path
        // can be read. Resource bytes and primaries remain prepare's responsibility.
        if !environment.rotation_radians.is_finite()
            || !environment.intensity.is_finite()
            || !(0.0..=64.0).contains(&environment.intensity)
            || !matches!(environment.samples, 1 | 4 | 16 | 64)
        {
            return Err(bad(command, "invalid Interactive environment controls"));
        }
        if input.raytrace.as_ref().is_some_and(|options| options.environment.is_some()) {
            return Err(bad(command, "choose either captured HDR or constant CPU environment"));
        }
    }
    let final_request = ImageRequest {
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        camera: settings.camera,
        width: settings.width,
        height: settings.height,
        background: settings.background,
        transparent_background: input.transparent_background,
        integrator: input.integrator,
        raytrace: input.raytrace,
        studio_weights: input.studio_weights,
        environment: None,
        textures: None,
    };
    let preview = solidcraft_render::job::interactive::cpu_request(&final_request).map_err(job_error)?;
    settings.width = preview.width;
    settings.height = preview.height;
    let budget = preview.raytrace.as_ref().map_or(solidcraft_render::job::CpuBudget::Compact, |options| options.budget);
    settings.validate_for_cpu(budget).map_err(|e| bad(command, e.to_string()))?;
    // Preserve every original field after strict deserialization. Only the LED
    // recipe has been consumed into real area lights; do not lower it twice.
    let mut lowered = object.clone();
    lowered.remove("led_strip");
    lowered.remove("led_samples");
    lowered.insert("settings".into(), serde_json::to_value(settings).map_err(|e| bad(command, e.to_string()))?);
    lowered.insert("raytrace".into(), serde_json::to_value(preview.raytrace).map_err(|e| bad(command, e.to_string()))?);
    Ok(Value::Object(lowered))
}

/// Capture/admit only. The engine's Interactive owner submits outside the final
/// registry; this helper never starts, replaces, cancels or exports a job.
pub(super) fn interactive(s: &Session, params: &Value) -> Result<crate::render_interactive::Prepared> {
    let params = interactive_params(params)?;
    let prepared = prepare(s, &params, "render.interactive")?;
    if prepared.executor.as_str() != "cpu-raytrace" {
        return Err(bad("render.interactive", "Interactive requires the installed cpu-raytrace executor"));
    }
    Ok(crate::render_interactive::Prepared {
        scene: prepared.scene,
        request: prepared.request,
        options: prepared.options,
        environment: prepared.environment,
    })
}

#[cfg(test)]
mod union_lowering_tests {

    #[cfg(not(target_arch = "wasm32"))]
    fn raster_local_command_fixture() -> (Session,Value) {
        let mut s = Session::default();
        s.execute("solid.box",&json!({"length":20,"width":20,"height":4,"center":[0,0,0],"operation":"new","body_name":"Raster local command fixture"})).unwrap();
        s.execute("render.scene",&json!({})).unwrap();
        s.set_render_job_host(Some(solidcraft_render::job::CpuHost::for_platform().unwrap())).unwrap();
        let mut camera = solidcraft_render::Camera { distance:50.0,..Default::default() };
        camera.set_view(solidcraft_render::StandardView::Top);
        let wire = json!({"settings":{"width":8,"height":8,"camera":camera,"background":[20,40,60],"exposure_ev":1.25,"job":null},
            "raster_punctual_lights":[{"position_mm":[0.0,0.0,20.0],"color":[0.25,0.5,1.0],"intensity":2.0,"reference_distance_mm":20.0,"minimum_distance_mm":0.001}],
            "raster_area_lights":[{"center_mm":[0.0,0.0,20.0],"edge_u_mm":[4.0,0.0,0.0],"edge_v_mm":[0.0,-4.0,0.0],"color":[1.0,0.5,0.25],"intensity":2.0}]});
        (s,wire)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn raster_local_public_preflight_start_status_and_real_png_keep_both_sources_and_one_ev() {
        use solidcraft_render::job::{JobId,Phase};
        let (mut s,wire) = raster_local_command_fixture();
        let original = wire.clone();
        let checked = s.execute("render.preflight",&wire).unwrap();
        assert_eq!(checked["request"]["raster_punctual_lights"],wire["raster_punctual_lights"]);
        assert_eq!(checked["request"]["raster_area_lights"],wire["raster_area_lights"]);
        assert_eq!(checked["request"]["raster_exposure_ev"],1.25);
        assert!(checked["request"].get("raster_lights").is_none(),"no implicit directionals added");
        let started = s.execute("render.start",&wire).unwrap();
        let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
        let handle = s.render_job_handle(id).unwrap();
        let status = handle.wait_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase,Phase::Completed,"{:?}",status.error);
        assert_eq!(status.request.raster_exposure_ev,1.25);
        assert_eq!(serde_json::to_value(&status.request.raster_punctual_lights).unwrap(),wire["raster_punctual_lights"]);
        assert_eq!(serde_json::to_value(&status.request.raster_area_lights).unwrap(),wire["raster_area_lights"]);
        let frame = handle.frame().unwrap().unwrap();
        let bright = image::load_from_memory(frame.png()).unwrap().to_rgba8();
        assert_eq!(bright.dimensions(),(8,8));
        let mut empty = wire.clone();
        empty["raster_punctual_lights"] = json!([]);
        empty["raster_area_lights"] = json!([]);
        let started = s.execute("render.start",&empty).unwrap();
        let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
        let dark_handle = s.render_job_handle(id).unwrap();
        assert_eq!(dark_handle.wait_timeout(std::time::Duration::from_secs(60)).unwrap().progress.phase,Phase::Completed);
        let dark_frame = dark_handle.frame().unwrap().unwrap();
        let dark = image::load_from_memory(dark_frame.png()).unwrap().to_rgba8();
        assert_ne!(bright,dark,"actual local lights must change the real producer output");
        assert_eq!(wire,original,"input intent is immutable");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn raster_local_public_commands_refuse_saved_and_foreign_controls_without_reservation() {
        let (mut s,wire) = raster_local_command_fixture();
        let before_revision = s.revision;
        let mut saved = wire.clone();
        saved["settings"]["job"] = serde_json::to_value(RenderJob::Raster {transparent_background:false,lights:None}).unwrap();
        for command in ["render.preflight","render.start"] {
            let error = s.execute(command,&saved).unwrap_err().to_string();
            assert!(error.contains("saved local-light authoring"),"{error}");
        }
        for (key,value) in [("integrator",json!("raytrace")),("studio_weights",json!([1,1,1])),("raytrace",json!({})),
            ("environment",json!({"path":"must-not-be-opened.hdr"})),("nested_media",json!(true)),("progressive_feedback",json!(true)),
            ("progressive",json!(true)),("samples",json!(1)),("seed",json!(0)),("denoise",json!(true))] {
            let mut invalid = wire.clone();
            invalid[key] = value;
            for command in ["render.preflight","render.start"] {
                assert!(s.execute(command,&invalid).is_err(),"{command} accepted {key}");
                assert_eq!(s.render_job_host().unwrap().admitted_bytes(),0,"no failed request reserved work");
            }
        }
        let mut interactive = wire;
        interactive["integrator"] = json!("raytrace");
        assert!(interactive_params(&interactive).is_err());
        assert_eq!(s.revision,before_revision);
    }

    #[test]
    fn raster_local_public_command_caps_refuse_before_host_lookup_or_scene_capture() {
        let point = json!({"position_mm":[0,0,20],"color":[1,1,1],"intensity":1,"reference_distance_mm":20,"minimum_distance_mm":0.001});
        let area = json!({"center_mm":[0,0,20],"edge_u_mm":[4,0,0],"edge_v_mm":[0,-4,0],"color":[1,1,1],"intensity":1});
        let mut s = Session::default();
        for (key,value,reason) in [("raster_punctual_lights",json!(vec![point;9]),"at most eight"),("raster_area_lights",json!(vec![area;5]),"at most four")] {
            let mut wire = json!({});
            wire[key] = value;
            for command in ["render.preflight","render.start"] {
                let error = s.execute(command,&wire).unwrap_err().to_string();
                assert!(error.contains(reason),"bound must fail before absent host: {error}");
            }
        }
    }

    use super::*;
    #[test]
    fn led_lowering_keeps_hdr_reflection_aa_and_anisotropy_inputs_without_reading_hdr() {
        let wire = json!({"integrator":"raytrace","environment":{"path":"unopened.hdr","rotation_radians":0.75,"intensity":2,"samples":64},"raytrace":{"primary_samples":4,"reflections":{"bodies":[{"body":"receiver","mode":"rough_specular"}],"samples":16,"max_bounces":3},"anisotropic_faces":[{"body_index":0,"body_name":"receiver","face":0,"tangent":[1,0,0],"orientation":"quarter_turn","roughness_x":0.2,"roughness_y":0.5},null],"punctual_lights":[{"position_mm":[0,0,20],"color":[1,1,1],"intensity":1,"reference_distance_mm":20,"minimum_distance_mm":0.001}]},"led_strip":{"center_mm":[0,0,10],"length_mm":8,"width_mm":0.5,"length_axis":[1,0,0],"width_axis":[0,-1,0],"color":[1,0.5,0.25],"intensity":2},"led_samples":64});
        let mut input: Start = serde_json::from_value(wire.clone()).unwrap();
        let before = input.raytrace.clone().unwrap();
        lower_led(&mut input, "render.preflight").unwrap();
        let after = input.raytrace.unwrap();
        assert_eq!(before.primary_samples, after.primary_samples);
        assert_eq!(before.reflections, after.reflections);
        assert_eq!(before.anisotropic_faces, after.anisotropic_faces);
        assert_eq!(before.punctual_lights, after.punctual_lights);
        assert_eq!(after.area_lights.as_ref().unwrap().len(), 1);
        assert_eq!(after.area_samples, Some(64));
        let environment = input.environment.unwrap();
        assert_eq!(environment.path, "unopened.hdr");
        assert_eq!(environment.rotation_radians, 0.75);
        assert_eq!(environment.intensity, 2.0);
        assert_eq!(environment.samples, 64);
        assert!(input.led_strip.is_none() && input.led_samples.is_none());
        assert_eq!(wire["environment"]["path"], json!("unopened.hdr"));
    }
    fn interactive_final_wire() -> Value {
        let camera = solidcraft_render::Camera { fov: 0.8, ..Default::default() };
        json!({"integrator":"raytrace","settings":{
            "width":3840,"height":2160,"camera":camera,"background":[20,40,60],"job":null,
            "stage":[{"id":"floor","center_mm":[0.0,0.0,-5.0],"half_u_mm":[40.0,0.0,0.0],
                "half_v_mm":[0.0,40.0,0.0],"reflectance_linear":[0.1,0.2,0.3]}]},
            "raytrace":{"primary_samples":4,"shadows":false,"exposure_ev":1.25,"lights":[],
                "reflections":{"bodies":[{"body":"receiver","mode":"mirror"}],"samples":16,"max_bounces":3},
                "punctual_lights":[{"position_mm":[0.0,0.0,20.0],"color":[1.0,0.5,0.25],"intensity":2.0,
                    "reference_distance_mm":20.0,"minimum_distance_mm":0.01}],
                "emissions":[{"body_index":0,"body":"receiver","color":[0.2,0.4,0.8],"strength":3.0}],
                "surface_effects":[{"body":"receiver","repeat_mm":[2.0,3.0,4.0],"roughness_amplitude":0.1,"bump_amplitude_mm":0.01}]},
            "environment":{"path":"unopened-original.hdr","expected_sha256":"a".repeat(64),
                "rotation_radians":0.75,"intensity":2.0,"samples":16}})
    }

    #[test]
    fn interactive_fits_4k_before_admission_and_preserves_final_wire_and_transport() {
        let wire = interactive_final_wire();
        let original = wire.clone();
        let lowered = interactive_params(&wire).unwrap();
        let before: Start = serde_json::from_value(wire.clone()).unwrap();
        let after: Start = serde_json::from_value(lowered.clone()).unwrap();
        let final_settings = before.settings.unwrap();
        let preview_settings = after.settings.unwrap();
        assert_eq!((final_settings.width, final_settings.height), (3840, 2160));
        assert_eq!((preview_settings.width, preview_settings.height), (1024, 576));
        assert!(before.raytrace.as_ref().unwrap().budget.validate_image(3840, 2160, None).is_err(), "Compact final exceeds pixel cap");
        let mut expected_settings = final_settings;
        expected_settings.width = 1024;
        expected_settings.height = 576;
        assert_eq!(preview_settings, expected_settings, "camera, stage, background and other settings remain exact");
        let mut expected_options = before.raytrace.unwrap();
        expected_options.primary_samples = 1;
        expected_options.progressive = Some(Default::default());
        expected_options.reflections.as_mut().unwrap().samples = 1;
        assert_eq!(after.raytrace.unwrap(), expected_options, "no lighting, emission, shading effect or EV is discarded");
        assert_eq!(lowered["environment"], wire["environment"], "unopened HDR path and expected digest stay pinned");
        assert_eq!(lowered["raytrace"]["progressive"], json!({"samples_per_pixel":16,"seed":0}));
        assert_eq!(wire, original);
    }

    #[test]
    fn interactive_small_and_extreme_aspects_fit_without_enlarging_or_zero_dimensions() {
        for (width, height, expected) in [(320, 200, (320, 200)), (4096, 1, (1024, 1)), (1, 4096, (1, 1024))] {
            let wire = json!({"integrator":"raytrace","settings":{"width":width,"height":height}});
            let output = interactive_params(&wire).unwrap();
            assert_eq!((output["settings"]["width"].as_u64().unwrap(), output["settings"]["height"].as_u64().unwrap()), (expected.0, expected.1));
        }
    }

    #[test]
    fn interactive_refuses_alternate_authorities_and_unknown_fields_without_io() {
        let s = Session::default();
        let base = interactive_final_wire();
        for (key, value) in [
            ("setup", json!(null)),
            ("setup", json!("Saved")),
            ("replace", json!(false)),
            ("replace", json!(true)),
            ("integrator", json!("raster")),
            ("progressive", json!(true)),
            ("transparent_background", json!(true)),
            ("denoise", json!(true)),
            ("samples", json!(1)),
            ("max_depth", json!(2)),
            ("seed", json!(0)),
            ("studio_weights", json!([1, 1, 1])),
            ("unknown", json!(true)),
            ("lens", json!(null)),
        ] {
            let mut wire = base.clone();
            wire[key] = value;
            let error = match interactive(&s, &wire) {
                Err(error) => error.to_string(),
                Ok(_) => panic!("Interactive accepted conflicting input {key}"),
            };
            assert!(!error.contains("could not read") && !error.contains("no image host"), "refuse before host/IO: {key}: {error}");
        }
        for settings in [
            json!(null),
            json!({"job":{"method":"cpu_raytrace"}}),
            json!({"job":{"method":"gpu_path"}}),
            json!({"exposure_ev":1.0}),
            json!({"environment_rotation":1.0}),
            json!({"width":0}),
            json!({"mode":"final"}),
            json!({"format":"exr"}),
        ] {
            let mut wire = base.clone();
            wire["settings"] = settings;
            assert!(interactive_params(&wire).is_err());
        }
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("settings");
        assert!(interactive_params(&missing).is_err());
        for wire in [json!(null), json!([]), json!(false)] {
            assert!(interactive_params(&wire).is_err());
        }
        assert!(s.render_job_host().is_none());
    }

    #[test]
    fn interactive_refuses_invalid_or_unsupported_effects_before_unopened_hdr() {
        let base = interactive_final_wire();
        // Retain numerically valid dielectric intent for capture-time certification.
        // This fixture still mixes same-body mirror, emission and nonidentity effects;
        // successful lowering is not scene or job admission.
        let mut authored = base.clone();
        authored["raytrace"]["dielectrics"] = json!([{"body_index":0,"body_name":"receiver","ior":1.5,"absorption_per_mm":[0,0,0],
                "boundary":"closed_box","thickness_mm":0}]);
        let original = authored.clone();
        let authored_options: RaytraceOptions = serde_json::from_value(original["raytrace"].clone()).unwrap();
        let mut expected_options = authored_options.clone();
        expected_options.primary_samples = 1;
        expected_options.progressive = Some(solidcraft_render::job::CpuProgressiveOptions { samples_per_pixel: 16, seed: 0 });
        expected_options.reflections.as_mut().unwrap().samples = 1;
        let mut expected_settings: RenderSetup = serde_json::from_value(original["settings"].clone()).unwrap();
        expected_settings.width = 1024;
        expected_settings.height = 576;
        let mut expected_wire = original.clone();
        expected_wire["settings"] = serde_json::to_value(&expected_settings).unwrap();
        expected_wire["raytrace"] = serde_json::to_value(&expected_options).unwrap();

        let lowered = interactive_params(&authored).unwrap();
        let lowered_options: RaytraceOptions = serde_json::from_value(lowered["raytrace"].clone()).unwrap();
        let lowered_settings: RenderSetup = serde_json::from_value(lowered["settings"].clone()).unwrap();
        assert_eq!(lowered_options.dielectrics, authored_options.dielectrics, "the exact original dielectric selector is retained");
        assert_eq!(lowered_options.surface_effects, authored_options.surface_effects, "shading intent is not removed");
        assert_eq!(lowered_options.emissions, authored_options.emissions, "surface emission is not removed");
        assert_eq!(lowered_options.reflections, expected_options.reflections, "only the selected reflection sample count changes");
        assert_eq!(lowered_options, expected_options, "only documented Interactive sampling is normalized");
        assert_eq!(lowered_settings, expected_settings, "camera, stage, background and other settings remain exact");
        assert_eq!(lowered["environment"], original["environment"], "unopened HDR path and expected digest remain exact");
        assert_eq!(lowered, expected_wire, "no other request field changes");
        assert_eq!(authored, original, "the authored final wire remains unchanged");

        for (key, value) in [
            ("primary_samples", json!(0)),
            ("exposure_ev", json!(33.0)),
            (
                "anisotropic_faces",
                json!([{"body_index":0,"body_name":"receiver","face":0,"tangent":[1,0,0],
                "orientation":"zero","roughness_x":0.2,"roughness_y":0.4},null]),
            ),
            ("textures", json!([])),
        ] {
            let mut wire = base.clone();
            wire["raytrace"][key] = value;
            assert!(interactive_params(&wire).is_err(), "unsupported {key} must not disappear");
        }
        for (key, value) in [("thickness_mm", json!(1.0)), ("ior", json!(0.5))] {
            let mut wire = authored.clone();
            wire["raytrace"]["dielectrics"][0][key] = value;
            assert!(interactive_params(&wire).is_err(), "invalid dielectric {key} must refuse before unopened HDR");
        }
        for (key, value) in [("intensity", json!(-1.0)), ("samples", json!(3))] {
            let mut wire = base.clone();
            wire["environment"][key] = value;
            assert!(interactive_params(&wire).is_err());
        }
        let mut ambiguous = base.clone();
        ambiguous["raytrace"]["environment"] = json!({"color":[1.0,1.0,1.0],"intensity":1.0});
        assert!(interactive_params(&ambiguous).is_err());
        let mut orphaned_led_samples = base;
        orphaned_led_samples["led_samples"] = json!(16);
        assert!(interactive_params(&orphaned_led_samples).is_err());
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn interactive_capture_reuses_cpu_scene_stage_and_admission_without_starting_a_job() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length":10,"width":8,"height":4,"body_name":"receiver"})).unwrap();
        s.execute(
            "appearance.assign",
            &json!({"bodies":["receiver"],"color":"#4080c0",
            "pbr":{"metallic":1.0,"roughness":0.25,"dielectric_ior":1.5}}),
        )
        .unwrap();
        s.execute("render.scene", &json!({})).unwrap();
        let subject = crate::product_scene::extract(&s).unwrap();
        let host = solidcraft_render::job::CpuHost::for_platform().unwrap();
        s.set_render_job_host(Some(host.clone())).unwrap();
        let mut wire = interactive_final_wire();
        wire.as_object_mut().unwrap().remove("environment");
        let document = s.doc.clone();
        let before = (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty());
        let captured = interactive(&s, &wire).unwrap();
        assert_eq!((captured.request.width, captured.request.height), (1024, 576));
        assert_eq!(captured.request.integrator, Integrator::Raytrace);
        assert_eq!(captured.request.raytrace.as_ref().unwrap().exposure_ev, 1.25);
        let body = captured.scene.meshes.iter().find(|mesh| mesh.name == "receiver").unwrap();
        assert_eq!(body.surface, subject.meshes[0].surface);
        assert_eq!(body.faces, subject.meshes[0].faces);
        assert!(std::sync::Arc::ptr_eq(&body.mesh, &subject.meshes[0].mesh));
        assert_eq!(captured.scene.meshes.len(), 2, "subject and render-only floor are both captured");
        assert!(captured.scene.meshes.iter().any(|mesh| mesh.name == "__render_stage__/floor"));
        assert_eq!(captured.options, solidcraft_render::job::JobOptions::default());
        assert!(captured.environment.is_none());
        assert_eq!(host.admitted_bytes(), 0, "capture validates but does not submit/reserve a job");
        assert!(std::sync::Arc::ptr_eq(&document, &s.doc));
        assert_eq!(before, (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty()));
        assert_eq!(wire["settings"]["width"], 3840);
    }
    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    fn nested_command_fixture() -> (Session, Value) {
        let mut s = Session::default();
        for (name, length, width, height) in [("Outer", 20, 20, 4), ("Inner", 10, 10, 2)] {
            s.execute(
                "solid.box",
                &json!({
                    "length":length,"width":width,"height":height,"center":[0,0,2],
                    "operation":"new","body_name":name,
                }),
            )
            .unwrap();
        }
        s.execute("render.scene", &json!({})).unwrap();
        let scene = crate::product_scene::extract(&s).unwrap();
        assert_eq!(scene.meshes.len(), 2);
        let dielectrics: Vec<_> = scene
            .meshes
            .iter()
            .enumerate()
            .map(|(index, body)| {
                assert!(matches!(body.name.as_str(), "Outer" | "Inner"));
                json!({
                    "body_index":index,"body_name":body.name,"boundary":"closed_box","ior":1.0,
                    "absorption_per_mm":if body.name == "Outer" { [0.1,0.2,0.0] } else { [0.3,0.4,0.0] },
                    "thickness_mm":0.0,
                })
            })
            .collect();
        let mut camera = solidcraft_render::Camera { distance: 50.0, fov: 0.0, ..Default::default() };
        camera.set_view(solidcraft_render::StandardView::Top);
        camera.target = solidcraft_geom::Vec3::new(0.0, 0.0, 2.0);
        let host = solidcraft_render::job::CpuHost::for_platform().unwrap();
        assert_eq!(host.kind(), solidcraft_render::job::HostKind::NativeWorker);
        assert!(host.is_available(), "this native test requires an available real CPU worker");
        s.set_render_job_host(Some(host)).unwrap();
        let wire = json!({
            "integrator":"raytrace","nested_media":true,
            "settings":{"width":1,"height":1,"camera":camera,"background":[255,255,255],"job":null},
            "raytrace":{"dielectrics":dielectrics,"lights":[],"shadows":false,
                "environment":{"color":[1,1,1],"intensity":1}},
        });
        (s, wire)
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn command_nested_opt_in_reaches_retained_job_options_and_real_beer_transport() {
        use solidcraft_render::job::{JobId, Phase};
        let (mut s, wire) = nested_command_fixture();
        let before = (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty());
        let document = s.doc.clone();
        for value in [None, Some(false)] {
            let mut default = wire.clone();
            if let Some(value) = value {
                default["nested_media"] = json!(value);
            } else {
                default.as_object_mut().unwrap().remove("nested_media");
            }
            for command in ["render.preflight", "render.start"] {
                let error = s.execute(command, &default).unwrap_err().to_string();
                assert!(error.contains("nested, touching or overlapping dielectric volumes are unsupported"), "{command}: {error}");
            }
        }
        let checked = s.execute("render.preflight", &wire).unwrap();
        assert_eq!(checked["resolved"]["executor"], "cpu-raytrace");
        assert_eq!(checked["request"]["raytrace"]["dielectrics"], wire["raytrace"]["dielectrics"]);
        let started = s.execute("render.start", &wire).unwrap();
        let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
        let handle = s.render_job_handle(id).unwrap();
        let status = handle.wait_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase, Phase::Completed, "{:?}", status.error);
        assert_eq!(status.options, solidcraft_render::job::JobOptions { nested_media: true, ..Default::default() });
        let retained = s.execute("render.status", &json!({"job":id})).unwrap();
        assert_eq!(retained["options"]["nested_media"], true);
        let frame = handle.frame().unwrap().unwrap();
        let image = image::load_from_memory(frame.png()).unwrap().to_rgba8();
        // Independent Beer oracle: outer2mm + inner2mm, replacing the parent
        // interval, yields exp(-.8), exp(-1.2),1 -> Reinhard+sRGB [151,132,188].
        assert_eq!(image.get_pixel(0, 0).0, [151, 132, 188, 255]);
        assert!(std::sync::Arc::ptr_eq(&document, &s.doc));
        assert_eq!(before, (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty()));
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn command_nesting_keeps_default_false_rejects_gpu_controls_and_captures_progressive_glass() {
        use solidcraft_render::job::{JobId, JobOptions, Phase};
        let (mut s, wire) = nested_command_fixture();
        for (key, value) in [("samples", json!(2)), ("max_depth", json!(2)), ("seed", json!(7)), ("denoise", json!(true)), ("lens", json!({}))] {
            let mut invalid = wire.clone();
            invalid[key] = value;
            for command in ["render.preflight", "render.start"] {
                assert!(s.execute(command, &invalid).is_err(), "{command} accepted unrelated control {key}");
            }
        }
        let mut progressive = wire.clone();
        progressive["raytrace"]["progressive"] = json!({"samples_per_pixel":2,"seed":0});
        // Original narrow perspective fixture: every possible pixel jitter ray
        // enters both boxes, rather than allowing primary misses/background.
        progressive["settings"]["camera"]["fov"] = json!(1.0e-6);
        // This previously blanket-negative branch is now an actual supported
        // progressive transport/capture case. GPU-only control refusals above
        // and the default nesting assertions below are unchanged.
        let checked = s.execute("render.preflight", &progressive).unwrap();
        assert_eq!(checked["resolved"]["executor"], "cpu-raytrace");
        assert_eq!(checked["request"]["raytrace"]["dielectrics"], wire["raytrace"]["dielectrics"]);
        assert_eq!(checked["request"]["raytrace"]["progressive"], progressive["raytrace"]["progressive"]);
        let started = s.execute("render.start", &progressive).unwrap();
        let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
        let handle = s.render_job_handle(id).unwrap();
        let status = handle.wait_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase, Phase::Completed, "{:?}", status.error);
        assert!(status.options.nested_media);
        let captured = status.request.raytrace.as_ref().unwrap();
        assert_eq!(captured.progressive.unwrap().samples_per_pixel, 2);
        assert_eq!(captured.progressive.unwrap().seed, 0);
        assert_eq!(serde_json::to_value(&captured.dielectrics).unwrap(), wire["raytrace"]["dielectrics"]);
        let frame = handle.frame().unwrap().unwrap();
        let image = image::load_from_memory(frame.png()).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (1, 1));
        // Camera is exterior at z52. At every box face its full jitter cone
        // lies within52*tan(1e-6) < 0.000053mm of the axis, far inside the
        // inner +/-5mm face. Even using twice the actual half-angle bounds,
        // cos(theta)>1-2e-12. Parent-only and child lengths are each2/cos mm.
        // IOR1 removes Fresnel/refraction; scalar Beer is exp(-.8/cos),
        // exp(-1.2/cos),1. Their bounded variation is far below the nearest
        // integer thresholds of existing Reinhard/sRGB: [151,132,188].
        // Actual primary-background pixels would be255 and fail this oracle.
        // This remains a command-route smoke, not a Fresnel/TIR quality case.
        assert_eq!(image.get_pixel(0, 0).0, [151, 132, 188, 255]);
        let interactive = interactive_params(&wire).unwrap();
        assert_eq!(interactive["raytrace"]["dielectrics"], wire["raytrace"]["dielectrics"]);
        assert_eq!(interactive["nested_media"], true);
        assert!(interactive["raytrace"]["progressive"].is_object());

        // Ordinary CPU requests retain the exact default JobOptions; omission is
        // not silently promoted to nesting by adding this explicit opt-in.
        let mut opaque = wire;
        opaque.as_object_mut().unwrap().remove("nested_media");
        opaque["raytrace"].as_object_mut().unwrap().remove("dielectrics");
        s.execute("render.preflight", &opaque).unwrap();
        let started = s.execute("render.start", &opaque).unwrap();
        let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
        let status = s.render_job_handle(id).unwrap().wait_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(status.progress.phase, Phase::Completed, "{:?}", status.error);
        assert_eq!(status.options, JobOptions::default());
        let retained = s.execute("render.status", &json!({"job":id})).unwrap();
        assert_eq!(retained["options"]["nested_media"], false);
    }
}

/// Resolve portable intent before choosing an executor. Legacy requests are left intact;
/// complete saved jobs cannot be silently overridden by a second set of runtime controls.
enum SavedHdr {
    Cpu(CpuHdrEnvironment),
    Gpu(GpuHdrEnvironment),
}

struct Normalized {
    params: Value,
    hdr: Option<SavedHdr>,
    surfaces: solidcraft_doc::render_surface::SurfaceBindings,
    optics: solidcraft_doc::render_optics::OpticalBindings,
}

fn normalize_saved_job(s: &Session, params: &Value, command: &str) -> Result<Option<Normalized>> {
    // Validate the new payload by reference even if this turns out to be an
    // unsaved request. Never clone an unbounded caller-owned light array.
    let raster_override = params
        .get("raster_lights")
        .map(|value| solidcraft_render::job::deserialize_raster_lights(value).map_err(|e| bad(command, e.to_string())))
        .transpose()?;
    // Validate both local arrays by reference before any saved setup or JSON clone.
    if let Some(value) = params.get("raster_punctual_lights") {
        let _ = solidcraft_render::job::deserialize_raster_punctual_lights(value).map_err(|e| bad(command, e.to_string()))?;
    }
    if let Some(value) = params.get("raster_area_lights") {
        let _ = solidcraft_render::job::deserialize_raster_area_lights(value).map_err(|e| bad(command, e.to_string()))?;
    }
    let local_intent = ["raster_punctual_lights", "raster_area_lights"].into_iter().any(|key| params.get(key).is_some_and(|v| !v.is_null()));
    if local_intent {
        let inline_saved = params.get("settings").and_then(|v| v.get("job")).is_some_and(|v| !v.is_null());
        let named_saved = params.get("setup").and_then(Value::as_str).and_then(|name| s.doc.render.setups.get(name)).is_some_and(|v| v.job.is_some());
        if inline_saved || named_saved {
            return Err(bad(command, "runtime Raster local lights require settings.job: null; saved local-light authoring is not supported yet"));
        }
    }
    let Some(object) = params.as_object() else {
        return Ok(None);
    };
    let named = object.get("setup").filter(|value| !value.is_null());
    let inline = object.get("settings").filter(|value| !value.is_null());
    if named.is_some() && inline.is_some() {
        return Err(bad(command, "choose either setup or settings"));
    }
    let mut settings = if let Some(value) = inline {
        if value.get("job").is_none_or(Value::is_null) {
            return Ok(None);
        }
        RenderSetup::deserialize(value).map_err(|e| bad(command, e.to_string()))?
    } else if let Some(settings) = named.and_then(Value::as_str).and_then(|name| s.doc.render.setups.get(name)) {
        if settings.job.is_none() {
            return Ok(None);
        }
        settings.validate().map_err(|e| bad(command, e.to_string()))?;
        settings.clone()
    } else {
        return Ok(None);
    };
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "setup" | "settings" | "replace" | "studio_weights" | "progressive_feedback" | "raster_lights" | "raster_punctual_lights" | "raster_area_lights"))
    {
        return Err(bad(
            command,
            format!(
                "saved job already defines the active method and its options; runtime `{key}` cannot override it (use job: null for a runtime request)"
            ),
        ));
    }
    settings.validate().map_err(|e| bad(command, e.to_string()))?;
    s.doc.render.validate_resources(&settings).map_err(|e| bad(command, e.to_string()))?;
    let Some(job) = settings.job.take() else {
        return Ok(None);
    };
    let optics = std::mem::take(&mut settings.optics);
    let surfaces = std::mem::take(&mut settings.surfaces);
    let mut hdr = None;
    let mut lowered = match job {
        RenderJob::Raster { transparent_background, lights } => {
            if lights.is_some() && object.contains_key("raster_lights") {
                return Err(bad(command, "saved Raster lights are authoritative; remove the runtime raster_lights override"));
            }
            let mut request = json!({"transparent_background":transparent_background});
            if lights.is_some() {
                request["raster_lights"] = serde_json::to_value(lights).map_err(|e| bad(command, e.to_string()))?;
            }
            request
        }
        RenderJob::CpuRaytrace { options } => {
            hdr = options.hdr.map(SavedHdr::Cpu);
            let raytrace = options.to_runtime(settings.exposure_ev).map_err(|e| bad(command, e.to_string()))?;
            // The legacy CPU path reads EV from RaytraceOptions. There is one saved authority.
            settings.exposure_ev = 0.0;
            json!({"integrator":"raytrace","raytrace":raytrace})
        }
        RenderJob::GpuPath { transparent_background, options } => {
            hdr = options.hdr.map(SavedHdr::Gpu);
            json!({
            "progressive":true,
            "transparent_background":transparent_background,
            "samples":options.samples,"max_depth":options.max_depth,"seed":options.seed,
            "lens":options.lens,"lighting":options.lighting,"tone_map":options.tone_map,
            "denoise":options.denoise,"environment":options.environment,
            "finishing":options.finishing,"backdrop":options.backdrop,"linear_exr":options.linear_exr,
            "adaptive":options.adaptive,"transmission_samples":options.transmission_samples,"volume":options.volume,"nested_media":options.nested_media,"aging":options.aging
            })
        }
    };
    lowered["settings"] = serde_json::to_value(settings).map_err(|e| bad(command, e.to_string()))?;
    if let Some(lights) = raster_override {
        // Only the already validated, bounded representation is materialized.
        lowered["raster_lights"] = serde_json::to_value(lights).map_err(|e| bad(command, e.to_string()))?;
    }
    if let Some(weights) = object.get("studio_weights") {
        lowered["studio_weights"] = weights.clone();
    }
    if let Some(replace) = object.get("replace") {
        lowered["replace"] = replace.clone();
    }
    if let Some(feedback) = object.get("progressive_feedback") {
        lowered["progressive_feedback"] = feedback.clone();
    }
    Ok(Some(Normalized { params: lowered, hdr, surfaces, optics }))
}

fn prepare(s: &Session, params: &Value, command: &str) -> Result<Prepared> {
    let normalized = normalize_saved_job(s, params, command)?;
    let saved_hdr = normalized.as_ref().and_then(|normalized| normalized.hdr.as_ref());
    let params = normalized.as_ref().map_or(params, |normalized| &normalized.params);
    if params.get("progressive").and_then(Value::as_bool) == Some(true) {
        let hdr = match saved_hdr {
            Some(SavedHdr::Gpu(hdr)) => Some(*hdr),
            Some(SavedHdr::Cpu(_)) => return Err(bad(command, "saved CPU HDR cannot enter a GPU request")),
            None => None,
        };
        return prepare_progressive(
            s,
            params,
            command,
            normalized.is_some(),
            hdr,
            normalized.as_ref().map(|n| &n.surfaces),
            normalized.as_ref().map(|n| &n.optics),
        );
    }
    let gpu_raytrace =
        params.get("raytrace").and_then(Value::as_object).is_some_and(|o| ["clear_coats", "textures"].iter().any(|key| o.contains_key(*key)));
    if gpu_raytrace
        || params.get("environment").is_some_and(|e| e.get("kind").is_some())
        || ["lens", "lighting", "tone_map"].iter().any(|key| params.get(*key).is_some())
        || params.get("denoise").is_some_and(|value| value != &json!(false))
    {
        return Err(bad(
            command,
            "runtime GPU transmission, texture, lens, environment, lighting, tone-map and denoise controls require progressive rendering",
        ));
    }
    let mut input = Start::deserialize(params).map_err(|e| bad(command, e.to_string()))?;
    solidcraft_render::job::validate_raster_lights(&input.raster_lights).map_err(job_error)?;
    solidcraft_render::job::validate_raster_punctual_lights(&input.raster_punctual_lights).map_err(job_error)?;
    solidcraft_render::job::validate_raster_area_lights(&input.raster_area_lights).map_err(job_error)?;
    if (input.raster_lights.is_some() || input.raster_punctual_lights.is_some() || input.raster_area_lights.is_some())
        && (input.integrator != Integrator::Raster
            || input.progressive
            || input.progressive_feedback
            || input.nested_media
            || input.raytrace.is_some()
            || input.studio_weights.is_some()
            || input.environment.is_some())
    {
        return Err(bad(command, "explicit Raster lights require Raster without CPU/GPU controls, environment or studio_weights"));
    }
    if input.progressive_feedback
        && (input.integrator != Integrator::Raytrace || input.raytrace.as_ref().and_then(|options| options.progressive).is_none())
    {
        return Err(bad(command, "progressive_feedback requires CPU progressive raytracing"));
    }
    validate_studio_weights(input.studio_weights, command)?;
    if input.integrator == Integrator::Raytrace && input.studio_weights.is_some() {
        return Err(bad(command, "studio_weights apply to raster or the implicit GPU studio; use explicit CPU light intensities"));
    }
    if input.progressive {
        return prepare_progressive(s, params, command, false, None, None, None);
    }
    if input.denoise {
        return Err(bad(command, "denoise requires GPU progressive rendering"));
    }
    if input.samples.is_some() || input.max_depth.is_some() || input.seed.is_some() {
        return Err(bad(command, "sampling controls require progressive rendering"));
    }
    if input.setup.is_some() && input.settings.is_some() {
        return Err(bad(command, "choose either setup or settings"));
    }
    // Reject unavailable execution before capturing authoritative product meshes.
    // Runtime services are independent of the document and can be checked cold.
    let host = s.render_job_host().ok_or_else(|| bad(command, "no image host is installed"))?;
    if !host.is_available() {
        return Err(bad(command, "image host is unavailable"));
    }
    host.validate_integrator(input.integrator).map_err(job_error)?;
    lower_led(&mut input, command)?;
    if input.integrator == Integrator::Raster && input.raytrace.is_some() {
        return Err(bad(command, "raytrace options require integrator: raytrace"));
    }
    if input.integrator == Integrator::Raytrace && input.transparent_background {
        return Err(bad(command, "raytrace supports opaque backgrounds only"));
    }
    if let Some(options) = &input.raytrace {
        options.retained_bytes().map_err(job_error)?;
        options.budget.validate_execution().map_err(job_error)?;
        // Check the complete image envelope before any source file is read.
        let dimensions = if let Some(name) = &input.setup {
            let settings = s.doc.render.setups.get(name).ok_or_else(|| bad(command, "no saved render setup with that name"))?;
            (settings.width, settings.height)
        } else if let Some(settings) = &input.settings {
            (settings.width, settings.height)
        } else {
            let defaults = RenderSetup::default();
            (defaults.width, defaults.height)
        };
        options.budget.validate_image(dimensions.0, dimensions.1, options.progressive).map_err(job_error)?;
    }
    let environment = if let Some(saved_hdr) = saved_hdr {
        let SavedHdr::Cpu(hdr) = saved_hdr else {
            return Err(bad(command, "saved GPU HDR cannot enter a CPU request"));
        };
        if input.integrator != Integrator::Raytrace || input.environment.is_some() {
            return Err(bad(command, "saved CPU HDR requires raytrace without a runtime environment override"));
        }
        let resource = s.doc.render.resources.required(&hdr.resource).map_err(|e| bad(command, e.to_string()))?;
        Some(
            solidcraft_render::lighting::environment_input::EnvironmentInput::new(resource.source(), hdr.rotation_rad, hdr.intensity, hdr.samples)
                .map_err(|e| bad(command, e.to_string()))?,
        )
    } else if let Some(environment) = &input.environment {
        if input.integrator != Integrator::Raytrace {
            return Err(bad(command, "environment requires raytrace"));
        }
        let source = if let Some(id) = environment.resource {
            if !environment.path.is_empty() || environment.expected_sha256.is_some() {
                return Err(bad(command, "choose an embedded HDR resource or a file path with optional expected_sha256"));
            }
            s.doc.render.resources.required(&id).map_err(|e| bad(command, e.to_string()))?.source()
        } else {
            if environment.path.is_empty()
                || environment.path.len() > 4096
                || environment.path.capacity() > 4096
                || environment.path.chars().any(char::is_control)
            {
                return Err(bad(command, "environment requires a bounded HDR input path"));
            }
            let bytes =
                solidcraft_io::vfs::read(&environment.path, solidcraft_render::lighting::MAX_HDR_BYTES).map_err(|e| bad(command, e.to_string()))?;
            if environment.expected_sha256.is_some_and(|id| id != solidcraft_doc::render_resources::HdrId::of(&bytes)) {
                return Err(bad(command, "HDR file changed since selection; inspect and select it again"));
            }
            solidcraft_render::lighting::HdrSource::new(bytes).map_err(|e| bad(command, e.to_string()))?
        };
        Some(
            solidcraft_render::lighting::environment_input::EnvironmentInput::new(
                source,
                environment.rotation_radians,
                environment.intensity,
                environment.samples,
            )
            .map_err(|e| bad(command, e.to_string()))?,
        )
    } else {
        None
    };
    let mut scene = crate::product_scene::extract(s)?;
    if environment.is_some() {
        input.raytrace.as_ref().unwrap_or(&RaytraceOptions::default()).validate_environment_reflections(&scene).map_err(job_error)?;
    }
    let settings = if let Some(name) = &input.setup {
        s.doc.render.setups.get(name).cloned().ok_or_else(|| bad(command, "no saved render setup with that name"))?
    } else if let Some(settings) = &input.settings {
        settings.clone()
    } else {
        let mut settings = RenderSetup::default();
        if let Some(camera) = s.camera {
            settings.camera = camera;
        } else {
            settings.camera.fit(&scene.bounds);
        }
        settings
    };
    if input.integrator == Integrator::Raster && !settings.stage.is_empty() {
        return Err(bad(command, "passive stages require a raytrace executor"));
    }
    super::staging::apply(&mut scene, &settings, command)?;
    let budget = input.raytrace.as_ref().map_or(solidcraft_render::job::CpuBudget::Compact, |o| o.budget);
    if input.integrator == Integrator::Raytrace {
        settings.validate_for_cpu(budget).map_err(|e| bad(command, e.to_string()))?;
        budget.validate_execution().map_err(job_error)?;
        budget.validate_image(settings.width, settings.height, input.raytrace.as_ref().and_then(|o| o.progressive)).map_err(job_error)?;
    } else {
        settings.validate().map_err(|e| bad(command, e.to_string()))?;
    }
    // Raster captures the existing exposure authority. Legacy environment controls
    // still have no transport here and are reported/refused under their old policy.
    if (input.raster_lights.is_some() || input.raster_punctual_lights.is_some() || input.raster_area_lights.is_some()) && (settings.environment_rotation != 0.0 || settings.environment_intensity != 1.0) {
        return Err(bad(command, "explicit Raster lights do not implement environment controls"));
    }
    let ignored = if input.integrator == Integrator::Raster { ignored_controls(&settings) } else { Vec::new() };
    if input.integrator != Integrator::Raster
        && (settings.exposure_ev != 0.0 || settings.environment_rotation != 0.0 || settings.environment_intensity != 1.0)
    {
        return Err(bad(command, "exposure and environment controls are not supported by the R4a CPU image executor"));
    }
    let mut descriptor = host.descriptor_for(input.integrator).map_err(job_error)?;
    if input.integrator == Integrator::Raytrace {
        descriptor.limits = budget.image_limits(input.raytrace.as_ref().is_some_and(|o| o.progressive.is_some()));
    }
    let expected_executor = descriptor.id.clone();
    let mut backend = s.render_backend();
    backend.executors = vec![descriptor];
    let mut features = scene.required_features();
    if environment.is_some() {
        features.push(solidcraft_render::RenderFeature::Hdri);
    }
    let refraction = input.raytrace.as_ref().is_some_and(|options| !options.dielectrics.is_empty());
    if refraction && !features.contains(&solidcraft_render::RenderFeature::Refraction) {
        features.push(solidcraft_render::RenderFeature::Refraction);
    }
    let environment_info = environment.as_ref().map(|e| e.metadata());
    let resolved = solidcraft_render::resolve_request(
        &[backend],
        &RenderRequest {
            backend: Some(settings.backend.clone()),
            mode: settings.mode,
            operation: RenderOperation::Image,
            width: u64::from(settings.width),
            height: u64::from(settings.height),
            format: Some(settings.format),
            features,
            policy: Default::default(),
        },
    )
    .map_err(|e| bad(command, e.to_string()))?;
    if resolved.executor != expected_executor {
        return Err(bad(command, "resolved executor is not installed as an image job host"));
    }
    let request = ImageRequest {
        raster_punctual_lights: input.raster_punctual_lights.take(),
        raster_area_lights: input.raster_area_lights.take(),
        raster_exposure_ev: if input.integrator == Integrator::Raster { settings.exposure_ev } else { 0.0 },
        raster_lights: input.raster_lights.take(),
        studio_weights: validate_studio_weights(input.studio_weights, command)?,
        environment: None,
        textures: None,
        integrator: input.integrator,
        raytrace: input.raytrace.take(),
        camera: settings.camera,
        width: settings.width,
        height: settings.height,
        background: settings.background,
        transparent_background: input.transparent_background,
    };
    let options = solidcraft_render::job::JobOptions { nested_media: input.nested_media, ..Default::default() };
    let mut admission = if input.progressive_feedback {
        host.admit_cpu_protected_partial_with_environment_and_options(&scene, &request, options, environment.as_ref()).map_err(job_error)?
    } else {
        host.admit(&scene, &request, options).map_err(job_error)?
    };
    let retained_lights = request.raytrace.as_ref().map_or(Ok(0), |o| o.retained_bytes()).map_err(job_error)?;
    let retained_lights = retained_lights
        .checked_add(request.raster_lights_retained_bytes().map_err(job_error)?)
        .ok_or_else(|| bad(command, "retained light metadata overflow"))?;
    let retained_environment =
        if environment_info.is_some() { std::mem::size_of::<solidcraft_render::lighting::environment_input::EnvironmentInfo>() as u64 } else { 0 };
    let retained_request_bytes = retained_lights.checked_add(retained_environment).ok_or_else(|| bad(command, "request metadata overflow"))?;

    if !input.progressive_feedback {
        let environment_bytes = environment.as_ref().map_or(Ok(0), |e| e.required_bytes()).map_err(|e| bad(command, e.to_string()))?;
        admission.estimated_total_bytes = admission
            .estimated_total_bytes
            .checked_add(environment_bytes)
            .and_then(|v| v.checked_add(retained_request_bytes))
            .filter(|v| *v <= budget.limits().max_job_bytes)
            .ok_or_else(|| bad(command, "environment job exceeds per-job memory limit"))?;
        admission.preparation_bytes =
            admission.preparation_bytes.checked_add(environment_bytes).ok_or_else(|| bad(command, "environment admission overflow"))?;
    }
    let transparency = match input.integrator {
        Integrator::Raster => "sorted_alpha_approximation",
        Integrator::Raytrace if refraction => "physical_transmission",
        Integrator::Raytrace => "opaque_only",
    };
    let reply = json!({"ignored_controls":ignored,"stage_panels":settings.stage.len(),"stage_transport":"CPU direct/specular only; no diffuse interreflection", "cpu_budget":budget,"cpu_limits":budget.limits(),"environment":environment_info,"retained_request_bytes":retained_request_bytes,"resolved":resolved,"admission":admission,"request":request,"scene_generation":s.revision,"host":host.kind(),"transparency":transparency,"refraction":refraction,"dielectric_boundaries":if refraction {vec!["thin_sheet","closed_box"]} else {Vec::<&str>::new()}});
    Ok(Prepared {
        progressive_feedback: input.progressive_feedback,
        replace: input.replace,
        scene,
        request,
        reply,
        environment,
        executor: resolved.executor,
        options,
    })
}

fn prepare_progressive(
    s: &Session,
    params: &Value,
    command: &str,
    require_gpu_path: bool,
    saved_hdr: Option<GpuHdrEnvironment>,
    saved_surfaces: Option<&solidcraft_doc::render_surface::SurfaceBindings>,
    saved_optics: Option<&solidcraft_doc::render_optics::OpticalBindings>,
) -> Result<Prepared> {
    let cpu_raytrace = params
        .get("raytrace")
        .and_then(Value::as_object)
        .is_some_and(|o| o.keys().any(|key| !matches!(key.as_str(), "dielectrics" | "clear_coats" | "textures")));
    if params.get("integrator").is_some_and(|v| v.as_str() != Some("raster"))
        || cpu_raytrace
        || params.get("led_strip").is_some()
        || params.get("led_samples").is_some()
        || params.get("environment").is_some_and(|e| e.is_object() && e.get("kind").is_none())
    {
        return Err(bad(command, "progressive rendering does not accept CPU raytrace, environment or LED options"));
    }
    let mut input: ProgressiveStart = serde_json::from_value(params.clone()).map_err(|e| bad(command, e.to_string()))?;
    if input.progressive_feedback {
        return Err(bad(command, "progressive_feedback requires CPU progressive raytracing, not GPU progressive rendering"));
    }
    validate_studio_weights(input.studio_weights, command)?;
    if input.studio_weights.is_some() && (input.lighting.is_some() || input.environment.is_some() || saved_hdr.is_some()) {
        return Err(bad(command, "studio_weights require the implicit GPU studio without environment lighting"));
    }
    if input.integrator != Integrator::Raster || !input.progressive {
        return Err(bad(command, "progressive rendering requires the registered GPU request method"));
    }
    if input.setup.is_some() && input.settings.is_some() {
        return Err(bad(command, "choose either setup or settings"));
    }
    let mut scene = crate::product_scene::extract(s)?;
    if !input.progressive
        && (!input.raytrace.dielectrics.is_empty()
            || !input.raytrace.clear_coats.is_empty()
            || !input.raytrace.textures.is_empty()
            || input.lens.is_some()
            || input.lighting.is_some()
            || input.tone_map.is_some()
            || input.environment.is_some()
            || input.denoise)
    {
        return Err(bad(
            command,
            "runtime transmission, clear-coat, texture, lens, environment, lighting, tone-map and denoise controls require progressive rendering",
        ));
    }
    if let Some(bindings) = saved_surfaces.filter(|bindings| !bindings.is_empty()) {
        if !input.raytrace.textures.is_empty() {
            return Err(bad(command, "saved surfaces cannot be combined with runtime texture overrides"));
        }
        input.raytrace.textures = super::surface_bindings::resolve(s, &scene, bindings, command)?;
    }
    if let Some(bindings) = saved_optics.filter(|b| !b.is_empty()) {
        if !input.raytrace.dielectrics.is_empty() || !input.raytrace.clear_coats.is_empty() {
            return Err(bad(command, "saved optical bindings cannot be mixed with transient optical overrides"));
        }
        input.raytrace.dielectrics = super::optical_bindings::resolve(s, &scene, bindings, command)?;
    }
    super::transmission::apply(&mut scene, &input.raytrace.dielectrics, command)?;
    super::clear_coat::apply(&mut scene, &input.raytrace.clear_coats, command)?;
    let settings = if let Some(name) = &input.setup {
        s.doc.render.setups.get(name).cloned().ok_or_else(|| bad(command, "no saved render setup with that name"))?
    } else if let Some(settings) = &input.settings {
        settings.clone()
    } else {
        let mut settings = RenderSetup::default();
        if let Some(camera) = s.camera {
            settings.camera = camera;
        } else {
            settings.camera.fit(&scene.bounds);
        }
        settings
    };
    settings.validate_for_gpu().map_err(|e| bad(command, e.to_string()))?;
    if settings.environment_rotation != 0.0 || settings.environment_intensity != 1.0 {
        return Err(bad(command, "environment controls are not supported by the installed image executors"));
    }
    if !input.progressive && (input.samples.is_some() || input.max_depth.is_some() || input.seed.is_some() || settings.exposure_ev != 0.0) {
        return Err(bad(command, "sampling and exposure controls require progressive rendering"));
    }
    super::staging::apply(&mut scene, &settings, command)?;
    let mut options = solidcraft_render::job::JobOptions::default();
    if let Some(samples) = input.samples {
        options.samples = samples;
    }
    if let Some(depth) = input.max_depth {
        options.max_depth = depth;
    }
    if let Some(seed) = input.seed {
        options.seed = seed;
    }
    options.exposure_ev = settings.exposure_ev;
    options.lens = input.lens;
    options.lighting = input.lighting;
    options.tone_map = input.tone_map.unwrap_or_default();
    options.denoise = input.denoise;
    options.finishing = input.finishing;
    options.backdrop = input.backdrop;
    options.linear_exr = input.linear_exr;
    options.aging = super::effects::resolve_aging(s, &scene, &input.aging, command)?;
    options.nested_media = input.nested_media;
    options.volume = input.volume;
    options.adaptive = input.adaptive;
    options.transmission_samples = input.transmission_samples.unwrap_or(1);
    options.validate().map_err(job_error)?;
    if let Some(super::environment::Input::BuiltinHdr { id, .. }) = &input.environment
        && options.lighting.as_ref().is_some_and(|lighting| lighting.sun.is_some())
        && super::library::owns_sun(id, command)?
    {
        return Err(bad(command, "built-in HDR and analytic lighting cannot both own the sun"));
    }
    let environment = if let Some(hdr) = saved_hdr {
        if input.environment.is_some() {
            return Err(bad(command, "saved GPU HDR cannot have a runtime environment override"));
        }
        let resource = s.doc.render.resources.required(&hdr.resource).map_err(|e| bad(command, e.to_string()))?;
        Some(std::sync::Arc::new(
            solidcraft_render::lighting::environment::EnvironmentSource::from_hdr(
                resource.source(),
                hdr.rotation_rad,
                hdr.intensity,
                hdr.assume_linear_srgb,
            )
            .map_err(|e| bad(command, e.to_string()))?,
        ))
    } else {
        input.environment.as_ref().map(|source| source.capture(&s.doc.render.resources, command)).transpose()?
    };
    let mut features = scene.required_features();
    if environment.is_some() {
        features.push(solidcraft_render::RenderFeature::Hdri);
    }
    if input.progressive {
        features.push(solidcraft_render::RenderFeature::Progressive);
        if scene.has_transmission() {
            features.push(solidcraft_render::RenderFeature::Refraction);
        }
    }
    let mut backend = s.render_backend();
    if require_gpu_path {
        backend.executors.retain(|executor| executor.id.as_str() == "gpu-path");
        if backend.executors.is_empty() {
            return Err(bad(command, "saved gpu_path method requires an installed gpu-path executor"));
        }
    }
    let resolved = solidcraft_render::resolve_request(
        &[backend],
        &RenderRequest {
            backend: Some(settings.backend.clone()),
            mode: settings.mode,
            operation: RenderOperation::Image,
            width: u64::from(settings.width),
            height: u64::from(settings.height),
            format: Some(settings.format),
            features,
            policy: Default::default(),
        },
    )
    .map_err(|e| bad(command, e.to_string()))?;
    if require_gpu_path && resolved.executor.as_str() != "gpu-path" {
        return Err(bad(command, "saved gpu_path method requires the gpu-path executor; another progressive executor cannot substitute for it"));
    }
    if options.denoise && resolved.executor.as_str() != "gpu-path" {
        return Err(bad(command, "denoise requires the native gpu-path executor"));
    }
    let host = s.render_jobs.host_for(&resolved.executor).ok_or_else(|| bad(command, "resolved executor is not installed as an image job host"))?;
    if !host.is_available() {
        return Err(bad(command, "image host is unavailable"));
    }
    let request = ImageRequest {
        raster_punctual_lights: None,
        raster_area_lights: None,
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: validate_studio_weights(input.studio_weights, command)?,
        integrator: Default::default(),
        raytrace: None,
        environment,
        camera: settings.camera,
        width: settings.width,
        height: settings.height,
        background: settings.background,
        transparent_background: input.transparent_background,
        textures: super::textures::capture(&scene, &input.raytrace.textures, command)?,
    };
    let mut admission = host.admit(&scene, &request, options).map_err(job_error)?;
    admission.estimated_total_bytes = admission
        .estimated_total_bytes
        .checked_add(request.retained_resource_metadata_bytes())
        .filter(|bytes| *bytes <= host.job_limit_bytes())
        .ok_or_else(|| bad(command, "GPU work and retained metadata exceed the per-job limit"))?;
    let refraction = input.progressive && scene.has_transmission();
    let reply = json!({"stage_panels":settings.stage.len(),"resolved":resolved,"admission":admission,"request":request,"textures":request.textures.as_ref().map(|resources|resources.summary()),"scene_generation":s.revision,"host":host.kind(),"transparency":if refraction {"physical_transmission"} else if input.progressive {"opaque_only"} else {"sorted_alpha_approximation"},"options":options,"environment":request.environment.as_ref().map(|source|source.info()),"refraction":refraction,"dielectric_boundaries": if resolved.executor.as_str() == "gpu-path" {vec!["thin_sheet","closed_box","closed_mesh"]} else {Vec::<&str>::new()}});
    let mut reply = reply;
    if options.denoise {
        reply["denoising"] = json!({
            "method":"edge_aware_atrous", "passes":3, "steps":[1,2,4],
            "guides":"deterministic_center_ray_albedo_normal_position_depth",
            "bypass":"glass, smooth metal/coat, emitter, background and partial coverage first hits",
            "limitation":"first-hit guides do not reconstruct reflection/refraction or depth-of-field correspondences",
            "raw_linear_accumulator_preserved":true, "output_stage":"linear_before_tone_map"
        });
    }
    Ok(Prepared {
        progressive_feedback: false,
        replace: input.replace,
        scene,
        request,
        reply,
        executor: resolved.executor,
        options,
        environment: None,
    })
}

/// Nonneutral environment controls the Raster image executor still cannot apply.
fn ignored_controls(settings: &RenderSetup) -> Vec<&'static str> {
    let mut ignored = Vec::new();
    if settings.environment_rotation != 0.0 {
        ignored.push("environment_rotation");
    }
    if settings.environment_intensity != 1.0 {
        ignored.push("environment_intensity");
    }
    ignored
}

pub(super) fn preflight(s: &mut Session, params: &Value) -> Result<Value> {
    prepare(s, params, "render.preflight").map(|p| p.reply)
}

pub(super) fn start(s: &mut Session, params: &Value) -> Result<Value> {
    let p = prepare(s, params, "render.start")?;
    let execution = crate::render_jobs::Execution { executor: p.executor, options: p.options, environment: p.environment };
    let status = if p.progressive_feedback {
        s.render_jobs.start_with_environment_and_feedback(s.revision, p.scene, p.request, p.replace, execution, true)
    } else {
        s.render_jobs.start_with_environment(s.revision, p.scene, p.request, p.replace, execution)
    }
    .map_err(job_error)?;
    let mut status = json!(status);
    if let (Some(out), Some(ignored)) = (status.as_object_mut(), p.reply.get("ignored_controls")) {
        out.insert("ignored_controls".into(), ignored.clone());
    }
    Ok(status)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Regrade {
    job: JobId,
    presentation: solidcraft_render::job::presentation::Presentation,
}
pub(super) fn regrade(s: &mut Session, params: &Value) -> Result<Value> {
    let input: Regrade = serde_json::from_value(params.clone()).map_err(|e| bad("render.result.regrade", e.to_string()))?;
    input.presentation.validate().map_err(job_error)?;
    Ok(json!(s.render_jobs.regrade(input.job, input.presentation).map_err(job_error)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    job: JobId,
}

pub(super) fn status(s: &mut Session, params: &Value) -> Result<Value> {
    let input: Job = serde_json::from_value(params.clone()).map_err(|e| bad("render.status", e.to_string()))?;
    Ok(json!(s.render_jobs.status(input.job).map_err(job_error)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    job: Option<JobId>,
}
pub(super) fn pin(s: &mut Session, params: &Value) -> Result<Value> {
    if params.get("job").is_none() {
        return Err(bad("render.result.pin", "job or explicit null is required"));
    }
    let input: Pin = serde_json::from_value(params.clone()).map_err(|e| bad("render.result.pin", e.to_string()))?;
    Ok(json!({"frame":s.render_jobs.pin(input.job).map_err(job_error)?}))
}

pub(super) fn copy(s: &mut Session, params: &Value) -> Result<Value> {
    let input: Job = serde_json::from_value(params.clone()).map_err(|e| bad("render.result.copy", e.to_string()))?;
    let frame = s
        .render_jobs
        .handle(input.job)
        .map_err(job_error)?
        .frame()
        .map_err(job_error)?
        .ok_or_else(|| bad("render.result.copy", "render has no completed image"))?;
    let size = (u64::from(frame.info.width), u64::from(frame.info.height));
    if solidcraft_render::RenderLimits::default().fit(size.0, size.1, None) != Some(size) {
        return Err(bad("render.result.copy", "clipboard image exceeds 4096 per side or 4194304 pixels; export the PNG instead"));
    }
    Ok(json!({"frame":frame.info}))
}

pub(super) fn cancel(s: &mut Session, params: &Value) -> Result<Value> {
    let input: Job = serde_json::from_value(params.clone()).map_err(|e| bad("render.cancel", e.to_string()))?;
    let status = s.render_jobs.status(input.job).map_err(job_error)?;
    if status.progress.phase.terminal() {
        return Ok(json!(status));
    }
    Ok(json!(s.render_jobs.handle(input.job).map_err(job_error)?.cancel().map_err(job_error)?))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExportFormat {
    #[default]
    Png,
    Exr,
}
impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Exr => "exr",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    #[serde(default)]
    format: ExportFormat,
    job: JobId,
    path: String,
    #[serde(default)]
    overwrite: bool,
}

pub(super) fn export(s: &mut Session, params: &Value) -> Result<Value> {
    let input: Export = serde_json::from_value(params.clone()).map_err(|e| bad("render.export", e.to_string()))?;
    let path = std::path::Path::new(&input.path);
    if input.path.len() > 4096
        || input.path.chars().any(char::is_control)
        || !path.extension().is_some_and(|x| x.eq_ignore_ascii_case(input.format.extension()))
    {
        return Err(bad("render.export", "a bounded destination with the explicitly selected png/exr extension is required"));
    }
    // Atomic native writers are also used for the app's own files and do not
    // apply a request's path scope. Check the caller's destination explicitly.
    solidcraft_io::vfs::check(&input.path).map_err(|e| bad("render.export", e.to_string()))?;
    let frame = s.render_jobs.frame(input.job).map_err(job_error)?;
    let bytes = match input.format {
        ExportFormat::Png => frame.png(),
        ExportFormat::Exr => frame
            .linear_exr()
            .ok_or_else(|| bad("render.export", "this completed job has no raw linear master; enable retention before rendering"))?,
    };
    // Replace atomically without reading an arbitrarily large previous target into memory.
    // Design saves retain their independent backup policy.
    let result =
        if input.overwrite { solidcraft_io::write_atomic(path, bytes, false) } else { solidcraft_io::safe_write::create_atomic(path, bytes) };
    result.map_err(|e| bad("render.export", format!("could not export completed {}: {e}", input.format.extension())))?;
    Ok(
        json!({"job":input.job,"path":input.path,"bytes":bytes.len(),"format":input.format.extension(),"encoding":if input.format==ExportFormat::Exr {"scene_linear_srgb_d65"} else {"srgb"},"alpha":if input.format==ExportFormat::Exr {"coverage_premultiplied"} else {"straight"},"frame":frame.info,"linear_master":frame.linear_master_info(),"presentation":frame.presentation()}),
    )
}

fn validate_studio_weights(weights: Option<[f64; 3]>, command: &str) -> Result<Option<[f64; 3]>> {
    if weights.is_some_and(|weights| weights.iter().any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))) {
        return Err(bad(command, "studio visibility weights must be finite in 0..1"));
    }
    Ok(weights)
}
