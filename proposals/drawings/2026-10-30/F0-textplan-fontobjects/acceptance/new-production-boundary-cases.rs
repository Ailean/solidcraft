// Own production-boundary acceptance. UNREGISTERED, UNCOMPILED, UNRUN.
// Depends on the retained13 and unchanged four F0 wrapper cases/helpers.

#[test]
fn f0_builtin_identity_rejects_changed_bytes_even_with_a_matching_recomputed_id() {
    use sha2::{Digest, Sha256};
    let mut font = b1_plan_font(&[(36, 1366)]);
    let mut bytes = font.bytes.to_vec();
    bytes[4096] ^= 1;
    font.bytes = std::sync::Arc::from(bytes);
    font.id = Sha256::new().chain_update(font.bytes.as_ref())
        .chain_update(font.face.to_le_bytes()).finalize().into();
    let mut run = b1_plan_run("A", &[36]);
    run.font = font.id;
    let error = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).err().unwrap().to_string();
    assert!(error.contains("builtin drawing font"), "{error}");
}

#[test]
fn f0_prepared_public_advance_is_rechecked_before_any_production_view_exists() {
    let mut font = b1_plan_font(&[(36, 1366)]);
    font.glyphs.get_mut(&36).unwrap().advance = 1367;
    let run = b1_plan_run("A", &[36]);
    let scene = f0_plan_scene(vec![run.clone()], vec![font]);
    let prepared = scene.prepare().unwrap();
    let error = super::text_plan::plan_prepared_text(&prepared, &[&run], 1 << 20, 410_820).err().unwrap().to_string();
    assert!(error.contains("intrinsic advance does not match the admitted hmtx"), "{error}");
}

#[test]
fn f0_structural_preparation_does_not_attest_changed_descriptor_metrics() {
    let mut font = b1_plan_font(&[(36, 1366)]);
    font.metrics.ascent = 1855;
    let run = b1_plan_run("A", &[36]);
    let scene = f0_plan_scene(vec![run.clone()], vec![font]);
    let prepared = scene.prepare().unwrap();
    assert!(super::text_plan::plan_prepared_text(&prepared, &[&run], 1 << 20, 410_820).is_err());
}

#[test]
fn f0_production_view_keeps_the_same_borrowed_source_codes_and_subset() {
    let font = b1_plan_font(&[(36, 1366), (57, 1366)]);
    let run = b1_plan_run("AV", &[36, 57]);
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    let input = plan.font_input(*b"ABCDEF", 1 << 20);
    assert!(std::ptr::eq(input.source.font(), &font));
    assert_eq!(input.source.subset().mapping(), [(0, 0), (36, 1), (57, 2)]);
    assert_eq!(input.source.codes().iter().map(|row| (row.code, row.subset_gid, row.logical)).collect::<Vec<_>>(),
        [(1, 1, "A"), (2, 2, "V")]);
    assert_eq!(input.source.font().glyphs[&36].advance, 1366);
    assert_eq!(plan.glyphs()[0].placed_advance_mm, 0.5);
}

#[test]
fn f0_opaque_production_input_does_not_bypass_the_unresolved_default_width_gate() {
    use pdf_writer::Ref;
    let font = b1_plan_font(&[(36, 1366)]);
    let before = font.clone();
    let run = b1_plan_run("A", &[36]);
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    let input = plan.font_input(*b"ABCDEF", 1 << 20);
    let refs = super::font_objects::FontObjectRefs {
        type0: Ref::new(6), cid_font: Ref::new(7), descriptor: Ref::new(8),
        font_file2: Ref::new(9), to_unicode: Ref::new(10), cid_to_gid: Ref::new(11),
    };
    let error = super::font_objects::font_objects(refs, input).err().unwrap().to_string();
    assert!(error.contains("PDF default-width policy is unresolved"), "{error}");
    assert_eq!(font, before);
    assert!(!font.glyphs.contains_key(&0));
}
