// NEGATIVE COMPILE OBLIGATIONS, NOT RUN.
// Compile each function as a separate sibling module of text_plan/font_objects
// under paper::pdf in a coherent future acceptance assembly. Do not include
// these intentional errors in the normal test module or infer a pass here.
// No trybuild dependency or test-only proof constructor is introduced.

// Expected: E0451, FontPlanView fields private to text_plan.
fn cannot_pair_arbitrary_raw_font_and_subset<'a>(
    font: &'a solidcraft_geom::paper::text::Font,
    subset: &'a super::subset_font::SubsetFont,
    descriptor: &'a super::font_objects::DescriptorMetrics,
    codes: &'a [super::font_objects::CodeToGlyph<'a>],
) -> super::text_plan::FontPlanView<'a> {
    super::text_plan::FontPlanView { font, subset, descriptor, codes }
}

// Expected: E0560: no raw source_font/subset/descriptor/codes fields on the
// production input. A validated view cannot be replaced by serde assertions.
fn cannot_restore_the_raw_production_input<'a>(
    font: &'a solidcraft_geom::paper::text::Font,
    subset: &'a super::subset_font::SubsetFont,
) -> super::font_objects::FontObjectInput<'a> {
    super::font_objects::FontObjectInput {
        source_font: font,
        subset,
        codes: &[],
        descriptor: None,
        postscript_name: b"LiberationSans",
        subset_tag: *b"ABCDEF",
        max_chunk_bytes: 1 << 20,
    }
}

// Expected: Deserialize is not implemented; there is no token rehydration API.
fn cannot_deserialize_a_production_view() {
    let _: super::text_plan::FontPlanView<'_> = serde_json::from_str("{}").unwrap();
}

// Expected: E0603: the raw preparation entry is private to font_objects.
fn cannot_call_the_component_emitter_from_a_sibling() {
    let _ = super::font_objects::font_objects_parts;
}
