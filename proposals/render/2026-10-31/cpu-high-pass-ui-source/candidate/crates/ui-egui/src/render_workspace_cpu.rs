//! Transient CPU image controls; engine commands own file capture and admission.
use serde_json::{Value, json};
use solidcraft_engine::render::job::{CpuBudget, CpuDielectricTransport, RaytraceEnvironmentOptions, ReflectionBody, ReflectionMode};

use super::hdr;
use crate::{SolveApp, ui_kit};

pub(super) fn validate_progressive_image(app: &SolveApp) -> Result<(), String> {
    if app.render_workspace.view.method == super::ImageMethod::CpuRaytrace {
        let view = &app.render_workspace.view;
        view.raytrace.budget.validate_image(view.draft.width, view.draft.height, view.raytrace.progressive).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn image_limits(app: &SolveApp) -> solidcraft_engine::render::RenderLimits {
    if app.render_workspace.view.method == super::ImageMethod::GpuRaytrace {
        return solidcraft_engine::render::job::GPU_PATH_LIMITS;
    }
    if app.render_workspace.view.method != super::ImageMethod::CpuRaytrace {
        return Default::default();
    }
    let options = &app.render_workspace.view.raytrace;
    let mut limits = options.budget.image_limits(options.progressive.is_some());
    if let Some(progressive) = options.progressive {
        limits.max_pixels = limits.max_pixels.min(options.budget.limits().max_camera_samples / u64::from(progressive.samples_per_pixel.max(1)));
    }
    limits
}

fn budget_key(budget: CpuBudget) -> &'static str {
    match budget {
        CpuBudget::Compact => "render.children.compact",
        CpuBudget::Studio => "render.children.studio",
        CpuBudget::Workstation => "render.children.workstation",
    }
}

fn budget_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    let budgets = CpuBudget::ALL;
    let mut selected = budgets.iter().position(|b| *b == app.render_workspace.view.raytrace.budget).unwrap_or(0);
    let labels = budgets.map(|b| app.tr(budget_key(b)).to_string());
    let response = ui.scope(|ui| {
        ui_kit::segmented(
            ui,
            &mut selected,
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            ui_kit::Segments {
                compact: true,
                targets: &["render.cpu_budget_compact", "render.cpu_budget_studio", "render.cpu_budget_workstation"],
                ..Default::default()
            },
        )
    });
    crate::scenario::publish_rect("render:cpu-budget", response.response.rect);
    if response.inner
        && let Some(budget) = budgets.get(selected)
    {
        app.render_workspace.view.raytrace.budget = *budget;
        app.render_workspace.view.dirty = true;
    }
    let limits = app.render_workspace.view.raytrace.budget.limits();
    let note = format!(
        "{}: {:.2} GiB · {}: {} · {}: {} · {}: {}. {}",
        app.tr("render.cpu_budget_memory"),
        limits.max_job_bytes as f64 / 1_073_741_824.0,
        app.tr("render.cpu_budget_triangles"),
        limits.max_triangles,
        app.tr("render.cpu_budget_bodies"),
        limits.max_bodies,
        app.tr("render.cpu_progressive_spp"),
        limits.max_samples_per_pixel,
        app.tr("render.cpu_budget_not_guarantee")
    );
    ui_kit::text(ui, &note, ui_kit::Text::Note("render.cpu_budget_help"));
}
fn set_progressive(app: &mut SolveApp, enabled: bool) {
    let options = &mut app.render_workspace.view.raytrace;
    if enabled {
        options.progressive.get_or_insert_default();
        options.primary_samples = 1;
    } else {
        options.progressive = None;
    }
    app.render_workspace.view.dirty = true;
}

/// The saved CPU choice is independent of sampling, profiles and material bindings.
fn set_dielectric_transport(app: &mut SolveApp, policy: CpuDielectricTransport) -> bool {
    let view = &mut app.render_workspace.view;
    if view.raytrace.dielectric_transport == policy {
        return false;
    }
    view.raytrace.dielectric_transport = policy;
    view.dirty = true;
    true
}

fn dielectric_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    let mut selected = usize::from(app.render_workspace.view.raytrace.dielectric_transport == CpuDielectricTransport::StrictTir);
    let labels = [app.tr("render.cpu_glass_standard").to_string(), app.tr("render.cpu_glass_continue").to_string()];
    let label = app.tr("render.cpu_glass_label").to_string();
    if ui_kit::row(ui, &label, |ui| {
        ui_kit::segmented(
            ui,
            &mut selected,
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            ui_kit::Segments { compact: true, targets: &["render:cpu-glass-standard", "render:cpu-glass-continue"], ..Default::default() },
        )
    }) {
        set_dielectric_transport(app, if selected == 1 { CpuDielectricTransport::StrictTir } else { CpuDielectricTransport::LegacyBounded });
    }
    ui_kit::text(ui, app.tr("render.cpu_glass_help"), ui_kit::Text::Note("render.cpu_glass_help"));
}

pub(super) fn progressive_controls(app: &mut SolveApp, ui: &mut egui::Ui) -> bool {
    budget_controls(app, ui);
    dielectric_controls(app, ui);
    let mut selected = usize::from(app.render_workspace.view.raytrace.progressive.is_some());
    let labels = [app.tr("render.children.single_image").to_string(), app.tr("render.cpu_progressive").to_string()];
    let response = ui.scope(|ui| {
        ui_kit::row(ui, app.tr("render.children.sampling"), |ui| {
            ui_kit::segmented(
                ui,
                &mut selected,
                &labels.iter().map(String::as_str).collect::<Vec<_>>(),
                ui_kit::Segments { compact: true, targets: &["render:cpu-fixed", "render:cpu-progressive"], ..Default::default() },
            )
        })
    });
    crate::scenario::publish_target("render:cpu-sampling", response.response.rect);
    if response.inner {
        set_progressive(app, selected == 1);
    }
    let enabled = app.render_workspace.view.raytrace.progressive.is_some();
    let mut options = app.render_workspace.view.raytrace.progressive.unwrap_or_default();
    if !enabled {
        ui_kit::text(ui, app.tr("render.children.progressive_reason"), ui_kit::Text::Note("render.children.progressive_reason"));
    }
    ui.add_enabled_ui(enabled, |ui| {
        let response =
            ui_kit::row(ui, app.tr("render.cpu_progressive_spp"), |ui| ui_kit::number_field(ui, &mut options.samples_per_pixel, 1..=solidcraft_engine::render::job::MAX_CPU_SAMPLES_PER_PIXEL, 1.0, ""));
        app.render_workspace.view.dirty |= response.changed();
        crate::scenario::publish_target("render:cpu-spp", response.rect);
        let response =
            ui_kit::row(ui, app.tr("render.cpu_progressive_seed"), |ui| ui_kit::number_field(ui, &mut options.seed, 0..=u32::MAX, 1.0, ""));
        app.render_workspace.view.dirty |= response.changed();
        crate::scenario::publish_target("render:cpu-seed", response.rect);
    });
    if enabled {
        app.render_workspace.view.raytrace.progressive = Some(options);
    }
    ui_kit::text(ui, app.tr("render.cpu_progressive_help"), ui_kit::Text::Note("render.cpu_progressive_help"));
    enabled
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Environment {
    #[default]
    None,
    Constant,
    Hdr,
}

#[derive(Clone, Debug)]
pub(super) struct View {
    #[cfg(test)]
    pub selected_body: Option<String>,
    #[cfg(test)]
    pub body_filter: String,
    pub environment: Environment,
    pub hdr: hdr::Input,
    pub rotation_rad: f64,
    pub intensity: f64,
    pub samples: u32,
    pub color: [f64; 3],
    pub checked: Option<Checked>,
    /// Request-local, explicitly selected certified-box nesting; never inferred from GPU state.
    pub nested_media: bool,
    bindings: std::collections::BTreeMap<String, Binding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    ordinal: usize,
    feature: u64,
    component: u64,
}

#[derive(Clone, Debug)]
struct Binding {
    target: Target,
    revision: u64,
    document: std::sync::Weak<solidcraft_engine::doc::Document>,
    // Interactive may follow appearance/render-only edits only while this exact
    // evaluated world survives in the same open document. Weak ownership avoids
    // retaining old geometry and prevents pointer reuse after the world drops.
    session_uid: u64,
    world: std::sync::Weak<solidcraft_engine::doc::ModelState>,
}

#[derive(Clone, Debug)]
pub(super) struct Checked {
    pub request: Value,
    pub revision: u64,
    pub environment: Value,
    document: std::sync::Weak<solidcraft_engine::doc::Document>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            selected_body: None,
            #[cfg(test)]
            body_filter: String::new(),
            environment: Environment::None,
            hdr: hdr::Input::default(),
            rotation_rad: 0.0,
            intensity: 1.0,
            samples: 16,
            color: [1.0; 3],
            checked: None,
            nested_media: false,
            bindings: Default::default(),
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}

fn target(app: &SolveApp, name: &str) -> Option<Target> {
    if !valid_name(name) || app.session.model.is_stale() {
        return None;
    }
    let world = app.session.world_state();
    let mut matches = world.bodies.iter().filter(|body| app.session.body_visible(&body.name)).enumerate().filter(|(_, body)| body.name == name);
    let (ordinal, body) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(Target { ordinal, feature: body.feature, component: body.component })
}

fn unique_visible_body(app: &SolveApp, name: &str) -> bool {
    target(app, name).is_some()
}

fn binding_current(app: &SolveApp, name: &str) -> bool {
    app.render_workspace.view.cpu.bindings.get(name).is_some_and(|binding| {
        binding.revision == app.session.revision
            && std::sync::Weak::ptr_eq(&binding.document, &std::sync::Arc::downgrade(&app.session.doc))
            && target(app, name).as_ref() == Some(&binding.target)
    })
}

// Unlike final Check/Start, Interactive may refresh harmless appearance/render
// edits. The engine retains the exact world Arc for those edits (refresh_render).
// No geometry extraction, cold meshing, name-only rebinding or revision stamping.
fn binding_interactive_world_current(app: &SolveApp, name: &str) -> bool {
    app.render_workspace.view.cpu.bindings.get(name).is_some_and(|binding| {
        binding.session_uid == app.session.uid
            && !app.session.model.is_stale()
            && app
                .session
                .interactive_cached_world_state()
                .is_some_and(|world| std::sync::Weak::ptr_eq(&binding.world, &std::sync::Arc::downgrade(&world)))
    })
}

fn binding_interactive_current(app: &SolveApp, name: &str) -> bool {
    binding_interactive_world_current(app, name)
        && app.render_workspace.view.cpu.bindings.get(name).is_some_and(|binding| target(app, name).as_ref() == Some(&binding.target))
}

pub(super) fn revalidate(app: &mut SolveApp, name: &str) -> Result<(), String> {
    let current = target(app, name).ok_or_else(|| app.tr("render.cpu_body_missing").to_string())?;
    if app.render_workspace.view.cpu.bindings.get(name).is_none_or(|binding| binding.target != current) {
        return Err(app.tr("render.cpu_body_missing").to_string());
    }
    let revision = app.session.revision;
    let document = std::sync::Arc::downgrade(&app.session.doc);
    let world = std::sync::Arc::downgrade(&app.session.world_state());
    let session_uid = app.session.uid;
    if let Some(binding) = app.render_workspace.view.cpu.bindings.get_mut(name) {
        binding.revision = revision;
        binding.document = document;
        binding.session_uid = session_uid;
        binding.world = world;
    }
    Ok(())
}

/// A removed, hidden or ambiguous body never silently becomes a different target.
pub(super) fn set_reflection(app: &mut SolveApp, name: &str, mode: Option<ReflectionMode>) -> Result<(), String> {
    let selected = if mode.is_some() { Some(target(app, name).ok_or_else(|| app.tr("render.cpu_body_missing").to_string())?) } else { None };
    // Changing the mode of an existing row is not a new body selection. Only an
    // explicit confirmation of the same target, or Off followed by Add, may
    // establish a fresh binding after the document or body identity changes.
    if mode.is_some()
        && app.render_workspace.view.raytrace.reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|entry| entry.body == name))
        && !binding_current(app, name)
    {
        return Err(app.tr("render.cpu_binding_stale").to_string());
    }
    let message = app.tr("render.cpu_reflection_limit").to_string();
    let options = &mut app.render_workspace.view.raytrace;
    if let Some(mode) = mode {
        let mut reflections = options.reflections.clone().unwrap_or_default();
        if let Some(entry) = reflections.bodies.iter_mut().find(|entry| entry.body == name) {
            entry.mode = mode;
        } else {
            if reflections.bodies.len() >= 64 {
                return Err(message);
            }
            reflections.bodies.try_reserve_exact(1).map_err(|_| message.clone())?;
            if reflections.bodies.capacity() > 64 {
                return Err(message);
            }
            reflections.bodies.push(ReflectionBody { body: name.to_string(), mode });
        }
        options.reflections = Some(reflections);
    } else if let Some(reflections) = &mut options.reflections {
        reflections.bodies.retain(|entry| entry.body != name);
        if reflections.bodies.is_empty() {
            options.reflections = None;
        }
    }
    if let Some(target) = selected {
        app.render_workspace.view.cpu.bindings.insert(
            name.to_string(),
            Binding {
                target,
                revision: app.session.revision,
                document: std::sync::Arc::downgrade(&app.session.doc),
                session_uid: app.session.uid,
                world: std::sync::Arc::downgrade(&app.session.world_state()),
            },
        );
    } else {
        app.render_workspace.view.cpu.bindings.remove(name);
    }
    Ok(())
}

/// Shared by the parent Check and Start builder. This does not read an HDR file.
pub(super) fn params(app: &SolveApp, params: &mut Value) -> Result<(), String> {
    params_with_binding_policy(app, params, binding_current)
}

/// Interactive-only wire lowering. This does not requalify final Check/Start,
/// change a stored binding or read an HDR file; the engine still captures and admits it.
pub(super) fn params_interactive(app: &SolveApp, params: &mut Value) -> Result<(), String> {
    // runtime_image_request composes the explicit material DTO immediately
    // after these numerical controls, exactly once. The engine then converts
    // the separate Interactive request to progressive sampling and admits its
    // complete optics; a Fixed editor never licenses dropping that intent.
    // Refuse unproved bound worlds before shared lowering checks visibility via
    // target()/world_state(). Empty reflections never require a warm world.
    if app
        .render_workspace
        .view
        .raytrace
        .reflections
        .as_ref()
        .is_some_and(|r| r.bodies.iter().any(|entry| !binding_interactive_world_current(app, &entry.body)))
    {
        return Err(app.tr("render.cpu_binding_stale").to_string());
    }
    params_with_binding_policy(app, params, binding_interactive_current)
}

fn params_with_binding_policy(app: &SolveApp, params: &mut Value, current: fn(&SolveApp, &str) -> bool) -> Result<(), String> {
    let base = &app.render_workspace.view.raytrace;
    if base.anisotropic_faces.iter().any(Option::is_some)
        || !base.dielectrics.is_empty()
        || base.environment.is_some()
        || base.surface_effects.is_some()
        || base.emissions.is_some()
    {
        return Err(app.tr("render.cpu_options_pending").to_string());
    }
    if base.reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|entry| !unique_visible_body(app, &entry.body))) {
        return Err(app.tr("render.cpu_body_missing").to_string());
    }
    if base.reflections.as_ref().is_some_and(|r| r.bodies.iter().any(|entry| !current(app, &entry.body))) {
        return Err(app.tr("render.cpu_binding_stale").to_string());
    }
    let view = &app.render_workspace.view.cpu;
    let mut options = base.clone();
    if options.area_samples.is_some_and(|n| !matches!(n, 1 | 4 | 16 | 64)) {
        return Err(crate::i18n::tr("render.error_area_samples").into());
    }
    if options.area_lights.as_ref().is_none_or(Vec::is_empty) {
        options.area_samples = None;
        options.seed = None;
    }
    options.exposure_ev = app.render_workspace.view.draft.exposure_ev;
    // Runtime CPU requests keep the same single saved EV authority as typed jobs.
    params["settings"]["exposure_ev"] = json!(0.0);
    if view.environment != Environment::None {
        if !view.intensity.is_finite() || !(0.0..=64.0).contains(&view.intensity) {
            return Err(app.tr("render.cpu_environment_bounds").to_string());
        }
        match view.environment {
            Environment::Constant => options.environment = Some(RaytraceEnvironmentOptions { color: view.color, intensity: view.intensity }),
            Environment::Hdr => {
                if !view.rotation_rad.is_finite() || view.rotation_rad.abs() > 1e6 || !matches!(view.samples, 1 | 4 | 16 | 64) {
                    return Err(app.tr("render.cpu_environment_bounds").to_string());
                }
                let mut environment = hdr::request(app, hdr::Target::Cpu)?;
                environment["rotation_radians"] = json!(view.rotation_rad);
                environment["intensity"] = json!(view.intensity);
                environment["samples"] = json!(view.samples);
                params["environment"] = environment;
            }
            Environment::None => {}
        }
    }
    options.validate().map_err(|error| error.to_string())?;
    params["integrator"] = json!("raytrace");
    params["raytrace"] = json!(options);
    Ok(())
}

#[cfg(test)]
pub(super) fn controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    for (id, key, section) in [
        ("render_cpu_reflections", "render.cpu_reflections", 0),
        ("render_cpu_environment", "render.cpu_environment", 1),
        ("render_cpu_lights", "render.cpu_lights", 2),
    ] {
        let title = app.tr(key).to_string();
        ui_kit::section(ui, id, &title, false, |ui| match section {
            0 => reflection_controls(app, ui),
            1 => environment_controls(app, ui),
            _ => light_controls(app, ui),
        });
    }
    ui.small(app.tr("render.cpu_runtime_help"));
}

#[cfg(test)]
pub(super) fn reflection_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    ui.small(app.tr("render.cpu_reflection_help"));
    let entries = app.render_workspace.view.raytrace.reflections.as_ref().map(|r| r.bodies.clone()).unwrap_or_default();
    for entry in entries {
        ui.label(&entry.body);
        if !binding_current(app, &entry.body) {
            ui.small(app.tr("render.cpu_binding_stale"));
            if ui_kit::button(ui, app.tr("render.cpu_binding_confirm"), ui_kit::Button::Normal).clicked()
                && let Err(error) = revalidate(app, &entry.body)
            {
                app.set_status(error, true);
            }
        }
        let selected = match entry.mode {
            ReflectionMode::Mirror => "render.cpu_reflection_mirror",
            ReflectionMode::RoughSpecular => "render.cpu_reflection_rough",
        };
        let label = app.tr(selected).to_string();
        let mode_control = ui_kit::dropdown_menu(ui, ("cpu_reflection_mode", &entry.body), &label, |ui| {
            for (mode, key) in [
                (None, "render.cpu_reflection_off"),
                (Some(ReflectionMode::Mirror), "render.cpu_reflection_mirror"),
                (Some(ReflectionMode::RoughSpecular), "render.cpu_reflection_rough"),
            ] {
                let enabled = mode.is_none() || binding_current(app, &entry.body);
                if ui.add_enabled_ui(enabled, |ui| ui_kit::list_picker(ui, key, app.tr(key), mode == Some(entry.mode))).inner.clicked() {
                    if let Err(error) = set_reflection(app, &entry.body, mode) {
                        app.set_status(error, true);
                    }
                    ui.close();
                }
            }
        });
        crate::scenario::publish_handle(&format!("render:cpu-reflection-mode:{}", entry.body), mode_control.rect.center());
    }
    let hint = app.tr("render.cpu_body_filter").to_string();
    ui_kit::bounded_cell_editor(ui, ui.id().with("cpu_body_filter"), &mut app.render_workspace.view.cpu.body_filter, &hint, false, 128);
    let selected = app.render_workspace.view.cpu.selected_body.clone();
    let label = selected.clone().unwrap_or_else(|| app.tr("render.cpu_body_choose").to_string());
    let world = app.session.world_state();
    let filter = app.render_workspace.view.cpu.body_filter.to_lowercase();
    let candidates: Vec<_> = world
        .bodies
        .iter()
        .filter(|body| app.session.body_visible(&body.name) && body.name.to_lowercase().contains(&filter))
        .take(257)
        .map(|body| body.name.as_str())
        .collect();
    let dropdown = ui_kit::dropdown_menu(ui, "cpu_reflection_body", &label, |ui| {
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for (shown, name) in candidates.iter().enumerate() {
                if shown == 256 {
                    ui.small(app.tr("render.cpu_body_filter_more"));
                    break;
                }
                let response =
                    ui.add_enabled_ui(valid_name(name), |ui| ui_kit::list_picker(ui, name, name, selected.as_deref() == Some(*name))).inner;
                if response.clicked() {
                    app.render_workspace.view.cpu.selected_body = Some((*name).to_string());
                    ui.close();
                }
            }
        });
    });
    crate::scenario::publish_handle("render:cpu-reflection-body", dropdown.rect.center());
    let can_add = selected.as_ref().is_some_and(|name| unique_visible_body(app, name))
        && app.render_workspace.view.raytrace.reflections.as_ref().is_none_or(|r| r.bodies.len() < 64);
    let add = ui_kit::button(ui, app.tr("render.cpu_reflection_add"), ui_kit::Button::Enabled(can_add));
    crate::scenario::publish_handle("render:cpu-reflection-add", add.rect.center());
    if add.clicked()
        && let Some(name) = selected
        && let Err(error) = set_reflection(app, &name, Some(ReflectionMode::RoughSpecular))
    {
        app.set_status(error, true);
    }
    reflection_sampling_controls(app, ui);
}

/// Existing global reflection budget, shared during the Sampling-editor migration.
pub(super) fn reflection_sampling_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    let depth = app.tr("render.cpu_reflection_depth").to_string();
    let samples = app.tr("render.cpu_reflection_samples").to_string();
    if let Some(reflections) = &mut app.render_workspace.view.raytrace.reflections {
        ui_kit::row(ui, &depth, |ui| ui_kit::number_field(ui, &mut reflections.max_bounces, 1..=8, 1.0, ""));
        ui_kit::row(ui, &samples, |ui| ui_kit::number_field(ui, &mut reflections.samples, 1..=64, 1.0, ""));
    } else {
        ui.small(app.tr("render.cpu_reflection_empty"));
    }
}

#[cfg(test)]
pub(super) fn environment_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    let selected = app.render_workspace.view.cpu.environment;
    let key = match selected {
        Environment::None => "render.cpu_environment_none",
        Environment::Constant => "render.cpu_environment_constant",
        Environment::Hdr => "render.studio_hdr",
    };
    let label = app.tr(key).to_string();
    ui_kit::dropdown_menu(ui, "cpu_environment_kind", &label, |ui| {
        for (value, key) in [
            (Environment::None, "render.cpu_environment_none"),
            (Environment::Constant, "render.cpu_environment_constant"),
            (Environment::Hdr, "render.studio_hdr"),
        ] {
            if ui_kit::list_picker(ui, key, app.tr(key), value == selected).clicked() {
                app.render_workspace.view.cpu.environment = value;
                ui.close();
            }
        }
    });
    if app.render_workspace.view.cpu.environment == Environment::None {
        return;
    }
    if app.render_workspace.view.cpu.environment == Environment::Constant {
        let color = app.tr("render.cpu_linear_color").to_string();
        vector(ui, &color, &mut app.render_workspace.view.cpu.color, 0.0..=1.0, 0.01, "");
        ui.small(app.tr("render.cpu_constant_help"));
    } else {
        hdr::controls(app, ui, hdr::Target::Cpu);
        ui.small(app.tr("render.cpu_hdr_help"));
        ui.label(app.tr("render.studio_rotation"));
        let mut degrees = app.render_workspace.view.cpu.rotation_rad.to_degrees();
        if ui_kit::number_field(ui, &mut degrees, -1e6_f64.to_degrees()..=1e6_f64.to_degrees(), 1.0, "°").changed() {
            app.render_workspace.view.cpu.rotation_rad = degrees.to_radians();
        }
        ui.label(app.tr("render.cpu_environment_samples"));
        let samples = app.render_workspace.view.cpu.samples;
        ui_kit::dropdown_menu(ui, "cpu_hdr_samples", &samples.to_string(), |ui| {
            for value in [1, 4, 16, 64] {
                if ui_kit::list_picker(ui, &value.to_string(), &value.to_string(), value == samples).clicked() {
                    app.render_workspace.view.cpu.samples = value;
                    ui.close();
                }
            }
        });
        ui.small(app.tr("render.cpu_hdr_reflection_help"));
    }
    ui.label(app.tr("render.studio_intensity"));
    ui_kit::number_field(ui, &mut app.render_workspace.view.cpu.intensity, 0.0..=64.0, 0.1, "");
    ui.small(app.tr("render.cpu_environment_background"));
}

#[cfg(test)]
fn vector(ui: &mut egui::Ui, label: &str, value: &mut [f64; 3], range: std::ops::RangeInclusive<f64>, speed: f64, suffix: &str) {
    ui.label(label);
    ui.horizontal(|ui| {
        for component in value {
            ui_kit::number_field(ui, component, range.clone(), speed, suffix);
        }
    });
}

#[cfg(test)]
pub(super) fn light_controls(app: &mut SolveApp, ui: &mut egui::Ui) {
    super::light::lighting(app, ui);
}

pub(super) fn record_checked(app: &mut SolveApp, params: &Value, reply: &Value) {
    if params["integrator"] == "raytrace" || params["settings"]["job"]["method"] == "cpu_raytrace" {
        app.render_workspace.view.cpu.checked = Some(Checked {
            request: params.clone(),
            revision: app.session.revision,
            environment: reply["environment"].clone(),
            document: std::sync::Arc::downgrade(&app.session.doc),
        });
    }
}

fn metadata(app: &SolveApp, ui: &mut egui::Ui, info: &Value) {
    let Some(hash) = info["source_sha256"].as_array().filter(|hash| hash.len() == 32) else { return };
    let digest: Option<Vec<_>> = hash.iter().map(|byte| byte.as_u64().and_then(|n| u8::try_from(n).ok())).collect();
    let Some(digest) = digest else { return };
    let digest: String = digest.into_iter().map(|byte| format!("{byte:02x}")).collect();
    let width = info["header"]["width"].as_u64().unwrap_or(0);
    let height = info["header"]["height"].as_u64().unwrap_or(0);
    ui.label(format!("{width} × {height} px"));
    ui.label(format!("{}: {}", app.tr("render.cpu_environment_samples"), info["samples"].as_u64().unwrap_or(0)));
    ui.label(format!("{}: {:.2}", app.tr("render.studio_intensity"), info["intensity"].as_f64().unwrap_or(0.0)));
    ui.label(format!("{}: {:.1}°", app.tr("render.studio_rotation"), info["rotation_radians"].as_f64().unwrap_or(0.0).to_degrees()));
    ui.label(format!("SHA-256: {digest}"));
}

pub(super) fn checked_status(app: &SolveApp, ui: &mut egui::Ui, params: Option<&Value>) {
    if app.render_workspace.view.method != super::ImageMethod::CpuRaytrace {
        return;
    }
    if let Some(checked) = &app.render_workspace.view.cpu.checked {
        let title = app.tr("render.cpu_last_check").to_string();
        ui_kit::section(ui, "cpu_last_check", &title, false, |ui| {
            if params != Some(&checked.request)
                || checked.revision != app.session.revision
                || !std::sync::Weak::ptr_eq(&checked.document, &std::sync::Arc::downgrade(&app.session.doc))
            {
                ui.small(app.tr("render.cpu_check_changed"));
            }
            metadata(app, ui, &checked.environment);
            ui.small(app.tr("render.cpu_check_capture_help"));
        });
    }
}

pub(super) fn captured_status(app: &SolveApp, ui: &mut egui::Ui, status: &Value) {
    if status["request"]["integrator"] == "raytrace" && status["environment"]["source_sha256"].is_array() {
        let title = app.tr("render.cpu_captured_hdr").to_string();
        ui_kit::section(ui, "cpu_captured_hdr", &title, false, |ui| metadata(app, ui, &status["environment"]));
    }
}

#[cfg(test)]
fn dielectric_policy_harness() -> crate::scenario::Harness {
    let mut h = crate::scenario::Harness::new();
    h.app.ui.tab = "RENDER".into();
    h.app.render_workspace.view.method = super::ImageMethod::CpuRaytrace;
    h.app.render_workspace.view.result = false;
    h.app.render_workspace.view.name = "Policy".into();
    h.app.render_workspace.view.draft.width = 1;
    h.app.render_workspace.view.draft.height = 1;
    h.app.render_workspace.view.draft.exposure_ev = 0.0;
    h.app.render_workspace.view.raytrace = Default::default();
    h.app.render_workspace.view.cpu = View::default();
    h
}

#[cfg(test)]
#[test]
fn cpu_glass_policy_no_binding_ui_snapshot_save_reopen_load_preserves_portable_choice() {
    use solidcraft_engine::doc::render_settings::RenderJob;
    let mut h = dielectric_policy_harness();
    assert!(!set_dielectric_transport(&mut h.app, CpuDielectricTransport::LegacyBounded));
    assert!(set_dielectric_transport(&mut h.app, CpuDielectricTransport::StrictTir));
    assert!(h.app.render_workspace.view.dirty);
    assert!(!super::setups::has_runtime_inputs(&h.app), "the choice itself is portable");
    let saved = super::setups::snapshot(&h.app).unwrap();
    let Some(RenderJob::CpuRaytrace { options }) = &saved.job else { panic!("CPU draft changed method") };
    assert_eq!(options.dielectric_transport, CpuDielectricTransport::StrictTir);
    let wire = super::image_request(&h.app).unwrap();
    assert_eq!(wire["settings"]["job"]["options"]["dielectric_transport"], "strict_tir");
    super::setups::save(&mut h.app).unwrap();
    let document = solidcraft_engine::doc::Document::from_json(&h.app.session.doc.to_json()).unwrap();
    h.app.session = solidcraft_engine::Session::new(document);
    h.app.render_workspace.view.raytrace.dielectric_transport = CpuDielectricTransport::LegacyBounded;
    super::setups::load(&mut h.app);
    assert_eq!(h.app.render_workspace.view.raytrace.dielectric_transport, CpuDielectricTransport::StrictTir);
    assert_eq!(super::setups::snapshot(&h.app).unwrap(), saved);
    assert_eq!(super::image_request(&h.app).unwrap()["settings"]["job"]["options"]["dielectric_transport"], "strict_tir");
    assert!(set_dielectric_transport(&mut h.app, CpuDielectricTransport::LegacyBounded));
    assert!(serde_json::to_value(super::setups::snapshot(&h.app).unwrap()).unwrap()["job"]["options"].get("dielectric_transport").is_none());
}

#[cfg(test)]
#[test]
fn cpu_glass_policy_numeric_lowering_and_active_light_filter_preserve_the_exact_choice() {
    use solidcraft_engine::render::job::DirectionalLightOptions;
    let mut h = dielectric_policy_harness();
    h.app.render_workspace.view.raytrace.lights = Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
    h.app.render_workspace.view.lights.visibility.muted.insert("directional:0".into());
    for policy in [CpuDielectricTransport::LegacyBounded, CpuDielectricTransport::StrictTir] {
        set_dielectric_transport(&mut h.app, policy);
        let source = h.app.render_workspace.view.raytrace.clone();
        for interactive in [false, true] {
            let mut wire = json!({"settings":{"job":null}});
            if interactive {
                params_interactive(&h.app, &mut wire).unwrap();
            } else {
                params(&h.app, &mut wire).unwrap();
            }
            super::materials::params(&h.app, &mut wire).unwrap();
            let wire = super::light::filter_request(&h.app, wire).unwrap();
            let options: solidcraft_engine::render::job::RaytraceOptions = serde_json::from_value(wire["raytrace"].clone()).unwrap();
            assert_eq!(options.dielectric_transport, policy);
            assert_eq!(options.lights.as_ref().unwrap()[0].intensity, 0.0);
            assert_eq!(h.app.render_workspace.view.raytrace, source, "transient visibility must not rewrite authored input");
        }
        let portable = super::image_request(&h.app).unwrap();
        let options: solidcraft_engine::doc::render_settings::CpuJob =
            serde_json::from_value(portable["settings"]["job"]["options"].clone()).unwrap();
        assert_eq!(options.dielectric_transport, policy);
    }
}

#[cfg(test)]
#[test]
fn cpu_glass_policy_selection_preserves_unsupported_opaque_reflection_and_advanced_intent() {
    use solidcraft_engine::render::job::{EmissionSides, SurfaceEmission};
    let mut h = dielectric_policy_harness();
    h.app.session.execute("solid.box", &json!({"length":20,"width":20,"height":10})).unwrap();
    h.app.session.execute("render.scene", &json!({})).unwrap();
    let target = super::materials::read_targets(&mut h.app).unwrap().remove(0);
    set_reflection(&mut h.app, &target.name, Some(ReflectionMode::Mirror)).unwrap();
    let authored = h.app.render_workspace.view.raytrace.clone();
    set_dielectric_transport(&mut h.app, CpuDielectricTransport::StrictTir);
    assert_eq!(h.app.render_workspace.view.raytrace.reflections, authored.reflections);
    assert!(super::image_request(&h.app).is_err());
    assert!(super::interactive_request(&h.app).is_err());
    assert_eq!(h.app.render_workspace.view.raytrace.reflections, authored.reflections);
    assert_eq!(h.app.render_workspace.view.raytrace.dielectric_transport, CpuDielectricTransport::StrictTir);
    h.app.render_workspace.view.raytrace.reflections = None;
    h.app.render_workspace.view.raytrace.emissions = Some(vec![SurfaceEmission {
        body_index: target.ordinal,
        body: target.name,
        color: [1.0, 0.5, 0.25],
        strength: 1.0,
        sides: EmissionSides::Front,
    }]);
    let authored = h.app.render_workspace.view.raytrace.clone();
    assert!(super::image_request(&h.app).is_err());
    assert!(super::interactive_request(&h.app).is_err());
    assert_eq!(h.app.render_workspace.view.raytrace, authored);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn native_cpu_glass_policy_public_ui_check_start_and_interactive_preserve_real_material_and_visibility() {
    use solidcraft_engine::render::job::interactive::VisualRevision;
    use solidcraft_engine::render::job::{CpuHost, CpuProgressiveOptions, DirectionalLightOptions, HostKind, Integrator, JobId, Phase};
    use solidcraft_engine::schema::render_ui::Action;
    use std::time::Duration;
    let host = CpuHost::for_platform().unwrap();
    assert_eq!(host.kind(), HostKind::NativeWorker);
    host.validate_integrator(Integrator::Raytrace).unwrap();
    eprintln!("CPU_GLASS_POLICY_UI_BRANCH cpu_supported=true");
    for policy in [CpuDielectricTransport::LegacyBounded, CpuDielectricTransport::StrictTir] {
        let mut h = dielectric_policy_harness();
        h.app.session.set_render_job_host(Some(host.clone())).unwrap();
        h.app.session.execute("solid.box", &json!({"length":20,"width":20,"height":10})).unwrap();
        h.app.session.execute("render.scene", &json!({})).unwrap();
        set_dielectric_transport(&mut h.app, policy);
        h.app.render_workspace.view.raytrace.lights =
            Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
        h.app.render_workspace.view.lights.visibility.muted.insert("directional:0".into());
        let target = super::materials::read_targets(&mut h.app).unwrap().remove(0);
        let mut binding = super::materials::Binding::new(target, &h.app);
        binding.glass_enabled = true;
        binding.cpu_optics = true;
        // Explicit index-matched certified box: this is capture/consumer
        // acceptance, not a new oracle for general reflective glass quality.
        binding.glass.ior = 1.0;
        binding.glass.boundary = super::materials::Boundary::Box;
        binding.glass.thickness = 0.0;
        h.app.render_workspace.view.materials.enabled = true;
        h.app.render_workspace.view.materials.bindings.push(binding);
        assert!(super::setups::snapshot(&h.app).is_err(), "captured body bindings remain runtime-only");
        for progressive in [false, true] {
            h.app.render_workspace.view.raytrace.progressive = progressive.then_some(CpuProgressiveOptions { samples_per_pixel: 3, seed: 411 });
            let wire = super::image_request(&h.app).unwrap();
            assert_eq!(wire["raytrace"]["dielectrics"].as_array().unwrap().len(), 1);
            let checked = h.app.session.execute("render.preflight", &wire).unwrap();
            record_checked(&mut h.app, &wire, &checked);
            let revision = h.app.session.revision;
            let other =
                if policy == CpuDielectricTransport::StrictTir { CpuDielectricTransport::LegacyBounded } else { CpuDielectricTransport::StrictTir };
            set_dielectric_transport(&mut h.app, other);
            assert_ne!(Some(&super::image_request(&h.app).unwrap()), h.app.render_workspace.view.cpu.checked.as_ref().map(|c| &c.request));
            assert_eq!(h.app.session.revision, revision);
            set_dielectric_transport(&mut h.app, policy);
            let started = h.app.session.execute("render.start", &wire).unwrap();
            let id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
            let final_handle = h.app.session.render_job_handle(id).unwrap();
            let final_status = final_handle.wait_timeout(Duration::from_secs(60)).unwrap();
            assert_eq!(final_status.progress.phase, Phase::Completed, "{:?}", final_status.error);
            assert_eq!(json!(final_status.request), checked["request"]);
            assert_eq!(final_status.request.raytrace.as_ref().unwrap().dielectric_transport, policy);
            assert!(final_handle.frame().unwrap().is_some());
            let preview_wire = super::interactive_request(&h.app).unwrap();
            let context = h.app.session.interactive_render().unwrap();
            context.enable(VisualRevision { scene_generation: revision, visual_revision: 1 }, Duration::ZERO).unwrap();
            let ticket = context.capture(Duration::from_millis(201)).unwrap().unwrap();
            assert!(context.snapshot(&h.app.session, ticket, &preview_wire).unwrap().run().unwrap());
            let preview_handle = context.current().unwrap().unwrap();
            let status = preview_handle.wait_timeout(Duration::from_secs(60)).unwrap();
            assert_eq!(status.progress.phase, Phase::Completed, "{:?}", status.error);
            let captured = status.request.raytrace.as_ref().unwrap();
            assert_eq!(captured.dielectric_transport, policy);
            assert_eq!(captured.dielectrics, final_status.request.raytrace.as_ref().unwrap().dielectrics);
            assert_eq!(captured.lights.as_ref().unwrap()[0].intensity, 0.0);
            assert_ne!(status.job, final_status.job);
            assert!(final_handle.frame().unwrap().is_some(), "Interactive cannot replace the final source");
            context.disable().unwrap();
        }
        // Exercise actual UI intent observation while paused, without a fake
        // receiver or a second capture worker. No timer result is asserted.
        super::interactive::apply_command(&mut h.app, Action::On).unwrap();
        let before = super::interactive::inspect(&h.app)["visual_revision"].as_u64().unwrap();
        super::interactive::apply_command(&mut h.app, Action::Pause).unwrap();
        let revision = h.app.session.revision;
        set_dielectric_transport(
            &mut h.app,
            if policy == CpuDielectricTransport::StrictTir { CpuDielectricTransport::LegacyBounded } else { CpuDielectricTransport::StrictTir },
        );
        super::interactive::apply_command(&mut h.app, Action::Resume).unwrap();
        assert!(super::interactive::inspect(&h.app)["visual_revision"].as_u64().unwrap() > before);
        assert_eq!(h.app.session.revision, revision);
        super::interactive::apply_command(&mut h.app, Action::Off).unwrap();
        h.app.render_workspace.view.materials.bindings[0].glass.shadows = true;
        let shadow_wire = super::image_request(&h.app).unwrap();
        let shadow_options: solidcraft_engine::render::job::RaytraceOptions = serde_json::from_value(shadow_wire["raytrace"].clone()).unwrap();
        assert!(shadow_options.directional_shadow_transmission);
        assert!(shadow_options.shadows);
        assert_eq!(shadow_options.dielectric_transport, h.app.render_workspace.view.raytrace.dielectric_transport);
        assert_eq!(shadow_options.dielectrics[0].ior, 1.0);
        let shadow_check = h.app.session.execute("render.preflight", &shadow_wire).unwrap();
        let shadow_request: solidcraft_engine::render::job::ImageRequest = serde_json::from_value(shadow_check["request"].clone()).unwrap();
        assert_eq!(shadow_request.raytrace.as_ref(), Some(&shadow_options));
        assert!(!h.app.render_workspace.view.raytrace.directional_shadow_transmission, "the bridge is request-local");
        assert!(h.app.render_workspace.view.materials.bindings[0].glass.shadows);
        h.app.render_workspace.view.materials.bindings[0].glass.shadows = false;
        h.app.render_workspace.view.materials.bindings[0].maps_enabled = true;
        assert!(super::interactive_request(&h.app).is_err());
        assert!(h.app.render_workspace.view.materials.bindings[0].maps_enabled);
    }
}
