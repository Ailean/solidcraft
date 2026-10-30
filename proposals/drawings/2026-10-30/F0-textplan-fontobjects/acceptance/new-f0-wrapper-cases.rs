// Proposed own acceptance appendix. NOT REGISTERED, UNCOMPILED, UNRUN.
// Append after the preserved own13 cases only when Engineer selects a coherent
// F0 base and resolves the existing Font.bundled policy seam. No Cam test edit.
fn f0_plan_scene(
    runs: Vec<solidcraft_geom::paper::text::TextRun>,
    fonts: Vec<solidcraft_geom::paper::text::Font>,
) -> solidcraft_geom::paper::Scene {
    use solidcraft_geom::paper::{Group, Item, ObjectKind, Scene};
    Scene {
        fonts,
        ..Scene::new(
            solidcraft_geom::Vec2::new(210.0, 297.0),
            vec![Group { object: 1, kind: ObjectKind::TitleBlock, items: runs.into_iter().map(Item::Text).collect() }],
        )
    }
}

#[test]
fn f0_text_plan_resolves_the_real_id_through_prepared_without_moving_glyphs() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    assert_eq!(run.font, font.id);
    let scene = f0_plan_scene(vec![run.clone()], vec![font]);
    let prepared = scene.prepare().unwrap();
    let plan = super::text_plan::plan_prepared_text(&prepared, &[&run], 1 << 20, 410_820).unwrap();
    assert_eq!(plan.glyphs()[0].original_gid, 36);
    assert_eq!(plan.glyphs()[0].intrinsic_advance_fu, 1366);
    assert_eq!(plan.glyphs()[0].placed_advance_mm, 0.5);
    assert_eq!(plan.glyphs()[0].matrix_mm, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(plan.runs()[0].text, "A");
}

#[test]
fn f0_text_plan_refuses_empty_or_unknown_id_without_a_numeric_font_fallback() {
    let empty = f0_plan_scene(vec![], vec![]);
    let prepared = empty.prepare().unwrap();
    assert!(super::text_plan::plan_prepared_text(&prepared, &[], 1 << 20, 410_820).is_err());
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    let scene = f0_plan_scene(vec![run.clone()], vec![font]);
    let prepared = scene.prepare().unwrap();
    let mut wrong = run.clone();
    wrong.font[0] ^= 1;
    assert!(prepared.font(&wrong).is_none());
    let error = super::text_plan::plan_prepared_text(&prepared, &[&wrong], 1 << 20, 410_820).err().unwrap().to_string();
    assert!(error.contains("run font ID is absent from the prepared scene"));
    assert!(super::text_plan::plan_prepared_text(&prepared, &[&run, &wrong], 1 << 20, 410_820).is_err());
}

#[test]
fn f0_prepared_boundary_refuses_missing_duplicate_and_unsorted_font_tables() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    assert!(f0_plan_scene(vec![run.clone()], vec![]).prepare().is_err());
    assert!(f0_plan_scene(vec![run.clone()], vec![font.clone(), font.clone()]).prepare().is_err());
    // Synthetic second identity tests only the Scene ordering refusal. It is not
    // a claim that another face is admitted for embedding by this P2 adapter.
    let mut other_font = font.clone();
    other_font.id[0] ^= 1;
    let mut other_run = run.clone();
    other_run.font = other_font.id;
    let mut fonts = vec![font, other_font];
    fonts.sort_by_key(|font| font.id);
    fonts.reverse();
    assert!(f0_plan_scene(vec![run, other_run], fonts).prepare().is_err());
}

#[test]
fn f0_serialized_scene_cannot_supply_prepared_fonts_without_trusted_bytes() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    let scene = f0_plan_scene(vec![run], vec![font]);
    let value = serde_json::to_value(&scene).unwrap();
    assert!(value["fonts"][0].get("bytes").is_none());
    let read: solidcraft_geom::paper::Scene = serde_json::from_value(value).unwrap();
    assert!(read.fonts[0].bytes.is_empty());
    assert!(read.prepare().is_err(), "no Prepared value reaches the consumer after untrusted JSON");
}
