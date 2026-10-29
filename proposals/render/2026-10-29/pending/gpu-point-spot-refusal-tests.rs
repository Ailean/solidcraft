// PRIVATE UNAPPLIED TEST PROPOSAL. Append inside existing
// crates/engine/src/cmd/render/settings_tests.rs, using its existing
// light_fixture, Session, json and Arc imports. No device/host is opened.
// This guards the CURRENT unsupported contract, not future GPU light support.
// When support lands, replace this refusal oracle with actual lowering coverage.
#[test]
fn gpu_unrepresented_light_families_refuse_without_saving_partial_rig() {
    for kind in ["point", "spot", "second_directional"] {
        let mut s = Session::default();
        light_fixture(&mut s, "gpu_path");
        if kind == "second_directional" {
            s.execute("render.lights.add", &json!({"name":"Studio","set":{"kind":"directional"}})).unwrap();
        }
        s.mark_saved();
        let before = s.doc.clone();
        let state = (s.revision, s.undo.len(), s.redo.len(), s.is_dirty());
        let params = if kind == "second_directional" {
            json!({"name":"Studio","id":"directional:0"})
        } else {
            json!({"name":"Studio","set":{"kind":kind}})
        };
        let command = if kind == "second_directional" { "render.lights.duplicate" } else { "render.lights.add" };
        let error = s.execute(command, &params).unwrap_err();
        assert!(error.to_string().contains("GPU lighting supports one Sun and four Areas"), "{kind}: {error}");
        assert!(Arc::ptr_eq(&before, &s.doc));
        assert_eq!(state, (s.revision, s.undo.len(), s.redo.len(), s.is_dirty()));
    }
}

