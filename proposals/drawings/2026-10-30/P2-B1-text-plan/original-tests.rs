
// P2 B1 text-plan acceptance: original records, not a replacement layout engine.
// Font constants are literal fields of the pinned original Liberation resource;
// synthetic logical aliases below test the record contract, not real shaping.
fn b1_plan_font(ids: &[(u16, u16)]) -> solidcraft_geom::paper::text::Font {
    use sha2::{Digest, Sha256};
    use solidcraft_geom::paper::text::{Font, FontFlags, FontGlyph, FontMetrics};
    let bytes: std::sync::Arc<[u8]> = std::sync::Arc::from(&include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/liberation-sans/LiberationSans-Regular.ttf"))[..]);
    let id = Sha256::new().chain_update(bytes.as_ref()).chain_update(0u32.to_le_bytes()).finalize().into();
    Font {
        bundled: 0, id, bytes, face: 0, postscript_name: "LiberationSans".into(), num_glyphs: 2620, units_per_em: 2048,
        metrics: FontMetrics {
            ascent: 1854, descent: -434, cap_height: 1409, bbox: [-1114, -621, 2666, 2007],
            italic_angle: 0.0, flags: FontFlags::default(), stem_v: 180,
        },
        glyphs: ids.iter().map(|&(id, advance)| (id, FontGlyph { advance, outline: vec![] })).collect(),
    }
}

fn b1_plan_run(text: &str, ids: &[u16]) -> solidcraft_geom::paper::text::TextRun {
    use solidcraft_geom::paper::text::{Cluster, PlacedGlyph, TextRun};
    assert_eq!(text.chars().count(), ids.len());
    let chars: Vec<_> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
    TextRun {
        font: 0, scale: 1.0 / 2048.0, origin: solidcraft_geom::Vec2::new(0.0, 0.0), angle: 0.0, slant: 0.0, rtl: false,
        glyphs: ids.iter().enumerate().map(|(i, &id)| PlacedGlyph { id, x: i as f64 * 0.375, rise: 0.0, advance: 0.5 }).collect(),
        text: text.into(),
        clusters: ids.iter().enumerate().map(|(i, _)| Cluster { glyphs: i as u32..i as u32 + 1, text: chars[i] as u32..chars[i + 1] as u32 }).collect(),
    }
}

#[test]
fn b1_text_plan_keeps_intrinsic_width_separate_from_placed_advance() {
    let font = b1_plan_font(&[(36, 1366), (57, 1366)]);
    let run = b1_plan_run("AV", &[36, 57]);
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    assert_eq!(plan.runs()[0].font_size_mm, 1.0);
    assert_eq!(plan.runs()[0].text, "AV");
    assert_eq!(plan.runs()[0].glyphs, 0..2);
    let g = &plan.glyphs()[1];
    assert_eq!((g.original_gid, g.intrinsic_advance_fu, g.placed_advance_mm), (57, 1366, 0.5));
    assert_eq!(g.matrix_mm, [1.0, 0.0, 0.0, 1.0, 0.375, 0.0]);
    let input = plan.font_input(*b"ABCDEF", 1 << 20);
    assert_eq!(input.postscript_name, b"LiberationSans");
    assert_eq!(input.descriptor.unwrap().ascent_1000, 905.2734375);
}

#[test]
fn b1_text_plan_same_gid_with_distinct_logical_text_has_distinct_codes() {
    let font = b1_plan_font(&[(36, 1366)]);
    let first = b1_plan_run("A", &[36]);
    let second = b1_plan_run("B", &[36]);
    let plan = super::text_plan::plan_text(&font, &[&second, &first, &first], 1 << 20, 410_820).unwrap();
    assert_eq!(plan.glyphs().iter().map(|g| (g.code, g.logical)).collect::<Vec<_>>(), [(2, "B"), (1, "A"), (1, "A")]);
    let input = plan.font_input(*b"ABCDEF", 1 << 20);
    assert_eq!(input.codes.len(), 2);
    assert_eq!(input.codes[0].subset_gid, input.codes[1].subset_gid);
    assert_eq!((input.codes[0].logical, input.codes[1].logical), ("A", "B"));
}

#[test]
fn b1_text_plan_preserves_quarter_turn_slant_and_mark_rise_once() {
    let font = b1_plan_font(&[(36, 1366)]);
    let mut run = b1_plan_run("A", &[36]);
    run.origin = solidcraft_geom::Vec2::new(100.0, 50.0);
    run.angle = std::f64::consts::FRAC_PI_2;
    run.slant = 0.25;
    run.glyphs[0].x = 3.0;
    run.glyphs[0].rise = 2.0;
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    for (actual, expected) in plan.glyphs()[0].matrix_mm.into_iter().zip([0.0, 1.0, -1.0, 0.25, 98.0, 53.0]) {
        assert!((actual - expected).abs() < 1e-12, "fixed quarter-turn f64 trig allowance: {actual} != {expected}");
    }
    assert_eq!(run.origin, solidcraft_geom::Vec2::new(100.0, 50.0));
    assert_eq!((run.glyphs[0].x, run.glyphs[0].rise), (3.0, 2.0));
}

#[test]
fn b1_text_plan_single_glyph_literal_cluster_is_not_split_or_normalized() {
    let font = b1_plan_font(&[(36, 1366)]);
    let mut run = b1_plan_run("A", &[36]);
    run.text = "fi\u{301}".into();
    run.clusters[0].text = 0..4;
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    assert_eq!(plan.glyphs()[0].logical, "fi\u{301}");
    assert_eq!(plan.font_input(*b"ABCDEF", 1 << 20).codes[0].logical, "fi\u{301}");
}

#[test]
fn b1_text_plan_rtl_keeps_positions_but_uses_logical_cluster_order() {
    let font = b1_plan_font(&[(36, 1366), (37, 1366)]);
    let mut run = b1_plan_run("AB", &[37, 36]);
    run.rtl = true;
    run.clusters[0].text = 1..2;
    run.clusters[1].text = 0..1;
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    assert!(plan.runs()[0].rtl);
    assert_eq!(plan.glyphs().iter().map(|g| g.logical).collect::<String>(), "AB");
    assert_eq!(plan.glyphs().iter().map(|g| g.source_glyph_index).collect::<Vec<_>>(), [1, 0]);
    assert_eq!((plan.glyphs()[0].matrix_mm[4], plan.glyphs()[1].matrix_mm[4]), (0.375, 0.0));
}

#[test]
fn b1_text_plan_multi_glyph_cluster_refuses_instead_of_duplicating_text() {
    let font = b1_plan_font(&[(36, 1366), (37, 1366)]);
    let mut run = b1_plan_run("AB", &[36, 37]);
    run.text = "A".into();
    run.clusters.truncate(1);
    run.clusters[0].glyphs = 0..2;
    run.clusters[0].text = 0..1;
    let error = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).err().unwrap().to_string();
    assert!(error.contains("multi-glyph cluster extraction is not supported"), "{error}");
}

#[test]
fn b1_text_plan_tounicode_destination_limit_is_inclusive_and_never_split() {
    let font = b1_plan_font(&[(36, 1366)]);
    let mut run = b1_plan_run("A", &[36]);
    run.text = "A".repeat(256);
    run.clusters[0].text = 0..256;
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    assert_eq!(plan.glyphs()[0].logical.len(), 256);
    drop(plan);
    run.text.push('A');
    run.clusters[0].text.end += 1;
    let error = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).err().unwrap().to_string();
    assert!(error.contains("512-byte destination"), "{error}");
}

#[test]
fn b1_text_plan_space_keeps_advance_and_text_despite_empty_outline() {
    let font = b1_plan_font(&[(3, 569)]);
    let run = b1_plan_run(" ", &[3]);
    let plan = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    assert_eq!((plan.glyphs()[0].original_gid, plan.glyphs()[0].intrinsic_advance_fu, plan.glyphs()[0].logical), (3, 569, " "));
}

#[test]
fn b1_text_plan_refuses_wrong_identity_descriptor_face_and_intrinsic_width() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    for kind in 0..5 {
        let mut wrong = font.clone();
        match kind {
            0 => wrong.id[0] ^= 1,
            1 => wrong.face = 1,
            2 => wrong.metrics.stem_v = 181,
            3 => wrong.metrics.flags.italic = true,
            _ => wrong.glyphs.get_mut(&36).unwrap().advance = 1024,
        }
        assert!(super::text_plan::plan_text(&wrong, &[&run], 1 << 20, 410_820).is_err());
    }
}

#[test]
fn b1_text_plan_refuses_invalid_partitions_unmatched_font_and_nonfinite_positions() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    for kind in 0..6 {
        let mut wrong = run.clone();
        match kind {
            0 => wrong.clusters[0].glyphs.end = 2,
            1 => wrong.clusters[0].text.end = 2,
            2 => wrong.clusters.clear(),
            3 => wrong.font = 1,
            4 => wrong.glyphs[0].x = f64::NAN,
            _ => { wrong.text = "\u{e9}".into(); wrong.clusters[0].text = 0..1; }
        }
        assert!(super::text_plan::plan_text(&font, &[&wrong], 1 << 20, 410_820).is_err());
    }
}

#[test]
fn b1_text_plan_bounds_storage_subsets_run_count_and_exact_used_font_table() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    for (plan_bytes, subset_bytes) in [(0, 410_820), (1, 410_820), (1 << 20, 0), (1 << 20, 1)] {
        assert!(super::text_plan::plan_text(&font, &[&run], plan_bytes, subset_bytes).is_err());
    }
    assert!(super::text_plan::plan_text(&font, &vec![&run; 1025], 1 << 20, 410_820).is_err());
    let extra = b1_plan_font(&[(36, 1366), (37, 1366)]);
    assert!(super::text_plan::plan_text(&extra, &[&run], 1 << 20, 410_820).is_err());
}

#[test]
fn b1_text_plan_repeats_real_subset_mapping_and_resource_inputs() {
    let font = b1_plan_font(&[(36, 1366), (57, 1366)]);
    let run = b1_plan_run("AV", &[36, 57]);
    let first = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    let second = super::text_plan::plan_text(&font, &[&run], 1 << 20, 410_820).unwrap();
    let a = first.font_input(*b"ABCDEF", 1 << 20);
    let b = second.font_input(*b"ABCDEF", 1 << 20);
    assert_eq!(a.subset.bytes(), b.subset.bytes());
    assert_eq!(a.subset.mapping(), [(0, 0), (36, 1), (57, 2)]);
    assert_eq!(a.subset.mapping(), b.subset.mapping());
    assert_eq!(a.codes.iter().map(|g| (g.code, g.subset_gid, g.logical)).collect::<Vec<_>>(), [(1, 1, "A"), (2, 2, "V")]);
    assert_eq!(a.descriptor.unwrap().flags, pdf_writer::types::FontFlags::NON_SYMBOLIC.bits());
}

#[test]
fn b1_text_plan_font_byte_envelope_refuses_before_identity_or_table_work() {
    let font = b1_plan_font(&[(36, 1366)]);
    let run = b1_plan_run("A", &[36]);
    for len in [0, 410_819, 410_821] {
        let mut wrong = font.clone();
        // Identity is deliberately stale too: byte-envelope refusal has priority.
        wrong.bytes = std::sync::Arc::from(vec![0; len]);
        let error = super::text_plan::plan_text(&wrong, &[&run], 1, 410_820).err().unwrap().to_string();
        assert!(error.contains("font byte length is outside the reviewed resource envelope"), "{error}");
    }
}
