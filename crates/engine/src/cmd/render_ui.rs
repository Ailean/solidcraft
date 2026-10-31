//! Planned Render interactions, with explicit non-mutating refusals.
use super::CommandSpec;
use crate::schema::render_lights::{MuteParams, SoloParams};
use crate::schema::{
    TypedContract, output_schema_for,
    render_ui::{CheckReport, InteractiveParams},
};
use crate::{Result, Session, params::bad};
use serde::Deserialize;
use serde_json::{Value, json};

const SOLO: crate::schema::TypedContract<SoloParams> = crate::schema::TypedContract::new();
const MUTE: crate::schema::TypedContract<MuteParams> = crate::schema::TypedContract::new();

fn solo(_: &mut crate::Session, params: &serde_json::Value) -> crate::Result<serde_json::Value> {
    let command = "render.lights.solo";
    if !params.is_object() {
        return Err(crate::params::bad(command, "expected an object"));
    }
    let input: SoloParams = serde_json::from_value(params.clone()).map_err(|e| crate::params::bad(command, e.to_string()))?;
    if input.solo && input.id.is_none() {
        return Err(crate::params::bad(command, "solo requires a light id"));
    }
    if let Some(id) = &input.id {
        crate::render_lights::parse_id(id).map_err(|e| crate::params::bad(command, e))?;
    }
    serde_json::to_value(input).map_err(|e| crate::params::bad(command, e.to_string()))
}

fn mute(_: &mut crate::Session, params: &serde_json::Value) -> crate::Result<serde_json::Value> {
    let command = "render.lights.mute";
    if !params.is_object() {
        return Err(crate::params::bad(command, "expected an object"));
    }
    let input: MuteParams = serde_json::from_value(params.clone()).map_err(|e| crate::params::bad(command, e.to_string()))?;
    crate::render_lights::parse_id(&input.id).map_err(|e| crate::params::bad(command, e))?;
    serde_json::to_value(input).map_err(|e| crate::params::bad(command, e.to_string()))
}

macro_rules! waiting {
    ($id:literal, $label:literal, $tab:literal, $panel:literal, $icon:literal, $reason:literal) => {
        CommandSpec::new($id, $label, |_, _| Err(crate::EngineError::Other(concat!("not supported yet: ", $reason).into())))
            .enabled(|_| Err(concat!("not supported yet: ", $reason).into()))
            .planned("5493")
            .noundo()
            .doc_only()
            .at($tab, $panel)
            .icon($icon)
            .params("No supported parameters. Always refuses without changing the design or history.")
    };
}

const CHECK_REPORT: TypedContract<CheckReport> = TypedContract::new();
const INTERACTIVE: TypedContract<InteractiveParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("render.interactive", "Interactive Render", crate::checked!("render.interactive", interactive))
        .icon("render_image")
        .contract(&INTERACTIVE)
        .output(output_schema_for::<InteractiveParams>)
        .noundo()
        .doc_only()
        .params("action: on|off|pause|resume. Validates and echoes an explicit CPU Interactive preview request for the UI adapter. Headless calls never start, pause or cancel a render job, open UI, or change the design, final render settings or exportable images."),
    CommandSpec::new("render.preflight.copy", "Copy Render Check Report", crate::checked!("render.preflight.copy", copy_check))
        .icon("copy")
        .contract(&CHECK_REPORT)
        .output(output_schema_for::<CheckReport>)
        .noundo()
        .doc_only()
        .params("text: nonempty report, at most 65536 bytes. Validated clipboard request; headless echo only, never renders or edits a design."),
    CommandSpec::new("render.output.size", "Set Render Output Size", output_size)
        .icon("settings")
        .noundo()
        .doc_only()
        .params("width, height: 1..16384; custom?: bool. Validated Render output draft request; headless echo only, no design edits or job."),
    CommandSpec::new("render.result.view", "View Render Image", result_view)
        .icon("render_image")
        .noundo()
        .doc_only()
        .params("view: model|image|fit|actual. Validated result canvas request; headless echo only, never starts a job."),
    CommandSpec::new("render.result.close", "Close Render Image", close_result)
        .icon("close")
        .key("Escape")
        .noundo()
        .doc_only()
        .params("No parameters. Return to Model Preview; headless echo only, never cancels a job."),
    CommandSpec::new("appearance.paint", "Paint Appearance", super::inspect::appearance_gestures::paint)
        .icon("appearance").key("B").noundo()
        .params("appearance or color/name/opacity/pbr: appearance.assign look parameters; validates and returns look for ui.start paint. done?: true ends the UI tool. Programmatic calls never open UI."),
    CommandSpec::new("appearance.pick_up", "Pick Up Appearance", super::inspect::appearance_gestures::pick_up)
        .icon("select").key("I").noundo()
        .params("target?: appearance.chain target; returns its resolved look, or requests the UI sampling tool when omitted. Programmatic calls never open UI."),

    CommandSpec::new("render.lights.solo","Solo Light",crate::checked!("render.lights.solo",solo))
        .noundo().doc_only().icon("render_scene").at("RENDER SCENE","LIGHTS")
        .contract(&SOLO).output(crate::schema::output_schema_for::<SoloParams>)
        .params("id?: kind:slot light address; solo:bool. Enabling requires an id; false clears solo. Validates and echoes per-document UI view state. Never saves settings, evaluates geometry, opens an editor or changes history."),
    CommandSpec::new("render.lights.mute","Mute Light",crate::checked!("render.lights.mute",mute))
        .noundo().doc_only().icon("render_scene").at("RENDER SCENE","LIGHTS")
        .contract(&MUTE).output(crate::schema::output_schema_for::<MuteParams>)
        .params("id: kind:slot light address; mute:bool. Validates and echoes per-document UI view state, applied to live preview and captured render copies only. Never changes saved settings or history."),
    waiting!("render.compare", "Compare Images", "RENDER", "RESULT", "compare", "retained image comparison"),
    waiting!("render.batch", "Render All Shots", "RENDER", "BATCH", "render_image", "the saved-shot render queue"),
];

fn copy_check(_: &mut Session, params: &Value) -> Result<Value> {
    let report: CheckReport = serde_json::from_value(params.clone()).map_err(|e| bad("render.preflight.copy", e.to_string()))?;
    if report.text.is_empty() || report.text.len() > 65536 {
        return Err(bad("render.preflight.copy", "report must contain 1..65536 bytes"));
    }
    Ok(json!({"text":report.text}))
}

#[cfg(test)]
#[test]
fn render_lights_remaining_planned_commands_refuse_without_mutation() {
    use serde_json::{Value, json};
    assert!(super::find_command("appearance.replace").is_some_and(|command| command.planned.is_none()));
    let mut session = crate::Session::default();
    session.execute("render.settings.set", &json!({"name":"Studio","set":{"width":800}})).unwrap();
    session.execute("render.settings.set", &json!({"name":"Studio","set":{"width":900}})).unwrap();
    session.execute("edit.undo", &json!({})).unwrap();
    let document = session.doc.clone();
    let before = (session.undo.len(), session.redo.len(), session.revision, session.evaluations, session.is_dirty());
    for command in COMMANDS {
        if command.planned.is_none() {
            continue;
        }
        assert!(!command.info(&session).enabled);
        for params in [json!({}), Value::Null, json!([]), json!({"target":1e300,"name":"hostile"})] {
            let error = session.execute(command.id, &params).unwrap_err().to_string();
            assert!(error.contains("not supported yet"), "{error}");
            assert!(std::sync::Arc::ptr_eq(&document, &session.doc));
            assert_eq!(before, (session.undo.len(), session.redo.len(), session.revision, session.evaluations, session.is_dirty()));
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputSize {
    width: u32,
    height: u32,
    #[serde(default)]
    custom: bool,
}
fn output_size(_: &mut Session, params: &Value) -> Result<Value> {
    let input: OutputSize = serde_json::from_value(params.clone()).map_err(|e| bad("render.output.size", e.to_string()))?;
    if !(1..=16384).contains(&input.width) || !(1..=16384).contains(&input.height) {
        return Err(bad("render.output.size", "dimensions must be in 1..16384"));
    }
    let mut reply = json!({"width":input.width,"height":input.height});
    if input.custom {
        reply["custom"] = json!(true);
    }
    Ok(reply)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultView {
    view: String,
}
fn result_view(_: &mut Session, params: &Value) -> Result<Value> {
    let input: ResultView = serde_json::from_value(params.clone()).map_err(|e| bad("render.result.view", e.to_string()))?;
    if !matches!(input.view.as_str(), "model" | "image" | "fit" | "actual") {
        return Err(bad("render.result.view", "view must be model, image, fit or actual"));
    }
    Ok(json!({"view":input.view}))
}
fn close_result(_: &mut Session, params: &Value) -> Result<Value> {
    if !params.as_object().is_some_and(|p| p.is_empty()) {
        return Err(bad("render.result.close", "expected no parameters"));
    }
    Ok(json!({"view":"model"}))
}

#[cfg(test)]
#[test]
fn output_viewer_requests_are_validated_headless_echoes() {
    let mut s = Session::default();
    let document = s.doc.clone();
    let before = (s.revision, s.undo.len(), s.evaluations);
    for size in [json!({"width":1280,"height":720}), json!({"width":1920,"height":1080})] {
        assert_eq!(s.execute("render.output.size", &size).unwrap(), size);
    }
    let custom = json!({"width":1920,"height":1080,"custom":true});
    assert_eq!(s.execute("render.output.size", &custom).unwrap(), custom);
    assert!(s.execute("render.output.size", &json!({"width":1920,"height":1080,"custom":"yes"})).is_err());
    for view in ["model", "image", "fit", "actual"] {
        let request = json!({"view":view});
        assert_eq!(s.execute("render.result.view", &request).unwrap(), request);
    }
    assert_eq!(s.execute("render.result.close", &json!({})).unwrap(), json!({"view":"model"}));
    for request in
        [json!(null), json!({}), json!({"width":0,"height":1}), json!({"width":u64::MAX,"height":1}), json!({"width":1,"height":1,"extra":true})]
    {
        assert!(s.execute("render.output.size", &request).is_err());
    }
    for request in [json!(null), json!({"view":"unknown"}), json!({"view":"image","extra":true})] {
        assert!(s.execute("render.result.view", &request).is_err());
        // Session normalizes null parameters to an empty object for no-argument commands.
        if request.is_object() {
            assert!(s.execute("render.result.close", &request).is_err());
        }
    }
    assert!(std::sync::Arc::ptr_eq(&document, &s.doc));
    assert_eq!(before, (s.revision, s.undo.len(), s.evaluations));
}

#[cfg(test)]
#[test]
fn output_sections_copy_report_validates_without_design_history_or_job_changes() {
    let mut s = Session::default();
    let doc = s.doc.clone();
    let before = (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty());
    let report = json!({"text":"Check\n✓ Size is within the renderer limit"});
    assert_eq!(s.execute("render.preflight.copy", &report).unwrap(), report);
    for request in [json!({}), json!({"text":""}), json!({"text":true}), json!({"text":"check","job":1}), json!({"text":"x".repeat(65537)})] {
        assert!(s.execute("render.preflight.copy", &request).is_err());
    }
    assert!(std::sync::Arc::ptr_eq(&doc, &s.doc));
    assert_eq!(before, (s.revision, s.undo.len(), s.redo.len(), s.evaluations, s.is_dirty()));
}

/// The UI adapter owns provisional preview actions; headless execution only echoes.
fn interactive(_: &mut Session, params: &Value) -> Result<Value> {
    let command = "render.interactive";
    if !params.is_object() {
        return Err(bad(command, "expected an object"));
    }
    let request: InteractiveParams = serde_json::from_value(params.clone()).map_err(|error| bad(command, error.to_string()))?;
    serde_json::to_value(request).map_err(|error| bad(command, error.to_string()))
}

#[cfg(test)]
#[test]
fn interactive_contract_describes_exact_actions_and_a_nonmutating_echo() {
    use crate::schema::{input_schema_for, render_ui::Action};
    let command = super::find_command("render.interactive").unwrap();
    assert!(!command.undoable);
    assert!(!command.reads_model);
    assert!(command.planned.is_none());
    assert_eq!(command.icon, "render_image");
    let contract = command.contract.unwrap();
    assert_eq!(contract.allowed_keys().iter().map(String::as_str).collect::<Vec<_>>(), ["action"]);
    assert!(command.output.is_some());
    let action_schema = serde_json::to_value(input_schema_for::<Action>()).unwrap();
    assert_eq!(action_schema["enum"], json!(["on", "off", "pause", "resume"]));
    let request_schema = serde_json::to_value(input_schema_for::<InteractiveParams>()).unwrap();
    assert_eq!(request_schema["additionalProperties"], false);
    assert_eq!(request_schema["required"], json!(["action"]));
    let reply_schema = serde_json::to_value(output_schema_for::<InteractiveParams>()).unwrap();
    assert_eq!(reply_schema["required"], json!(["action"]));
    assert_eq!(reply_schema["properties"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["action"]);
}

#[cfg(test)]
#[test]
fn interactive_valid_and_hostile_requests_preserve_headless_document_history_and_host() {
    use crate::schema::ParamCheck;
    let mut session = Session::default();
    session.execute("render.settings.set", &json!({"name":"Original","set":{"width":800}})).unwrap();
    session.execute("render.settings.set", &json!({"name":"Original","set":{"width":900}})).unwrap();
    session.execute("edit.undo", &json!({})).unwrap();
    let document = session.doc.clone();
    let before = (session.revision, session.undo.len(), session.redo.len(), session.evaluations, session.is_dirty());
    assert!(session.render_job_host().is_none());
    for level in [ParamCheck::Off, ParamCheck::Warn, ParamCheck::Strict] {
        session.param_check = level;
        for action in ["on", "off", "pause", "resume"] {
            let params = json!({"action":action});
            assert_eq!(session.execute("render.interactive", &params).unwrap(), params);
        }
        for hostile in [
            json!(null),
            json!(true),
            json!(0),
            json!("on"),
            json!([]),
            json!(["on"]),
            json!({}),
            json!({"action":null}),
            json!({"action":true}),
            json!({"action":1e300}),
            json!({"action":["on"]}),
            json!({"action":{"on":true}}),
            json!({"action":"start"}),
            json!({"action":"On"}),
            json!({"action":" on"}),
            json!({"action":"on\n"}),
            json!({"action":"x".repeat(65536)}),
            json!({"action":"on","slot":0}),
            json!({"action":"off","job":{"session":1,"serial":1,"generation":1}}),
            json!({"action":"pause","replace":true}),
            json!({"action":"resume","settings":{}}),
            json!({"action":"on","path":"must-not-be-read.hdr"}),
        ] {
            assert!(session.execute("render.interactive", &hostile).is_err(), "{level:?}: {hostile}");
        }
        assert!(std::sync::Arc::ptr_eq(&document, &session.doc));
        assert_eq!(before, (session.revision, session.undo.len(), session.redo.len(), session.evaluations, session.is_dirty()));
        assert!(session.render_job_host().is_none());
    }
}

#[cfg(test)]
#[test]
fn interactive_echo_preserves_a_queued_final_job_and_its_resource_lease() {
    use solidcraft_render::job::{CpuHost, JobId};
    let host = CpuHost::cooperative();
    let mut session = Session::default();
    session.set_render_job_host(Some(host.clone())).unwrap();
    // Queue a tiny raster job without pumping the cooperative host or opening a device.
    let started = session.execute("render.start", &json!({"settings":{"width":16,"height":16}})).unwrap();
    let first_id: JobId = serde_json::from_value(started["job"].clone()).unwrap();
    let first = session.render_job_handle(first_id).unwrap();
    let status = serde_json::to_value(first.status().unwrap()).unwrap();
    let admitted = host.admitted_bytes();
    assert!(admitted > 0);
    let document = session.doc.clone();
    let before = (session.revision, session.undo.len(), session.redo.len(), session.evaluations, session.is_dirty());
    for action in ["on", "pause", "resume", "off"] {
        let params = json!({"action":action});
        assert_eq!(session.execute("render.interactive", &params).unwrap(), params);
        assert_eq!(serde_json::to_value(first.status().unwrap()).unwrap(), status);
        assert_eq!(host.admitted_bytes(), admitted);
        assert!(session.render_job_host().unwrap().same_host(&host));
        assert!(std::sync::Arc::ptr_eq(&document, &session.doc));
        assert_eq!(before, (session.revision, session.undo.len(), session.redo.len(), session.evaluations, session.is_dirty()));
    }
    assert!(session.execute("render.interactive", &json!({"action":"off","job":first_id})).is_err());
    assert_eq!(serde_json::to_value(first.status().unwrap()).unwrap(), status);
    assert_eq!(host.admitted_bytes(), admitted);
    let next = session.execute("render.start", &json!({"settings":{"width":16,"height":16}})).unwrap();
    let next_id: JobId = serde_json::from_value(next["job"].clone()).unwrap();
    assert_eq!(next_id.session, first_id.session);
    assert_eq!(next_id.serial, first_id.serial + 1, "echo actions must not allocate or replace final job identities");
}
