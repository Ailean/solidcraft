//! Private cfg(test)-only X4/X2/Out bridge proposal. NOT APPLIED / NOT COMPILED / NOT RUN.
//! This writes one TEXT value field, not an entity or a DXF file. No production caller.

use super::file_model::DxfFile;
use super::text::encode_text_value;
use super::{DxfShape, Out};
use crate::{IoError, Result};

fn write_owned_text_value(out: &mut Out, file: &DxfFile<DxfShape>, index: usize, remaining_record_bytes: usize) -> Result<()> {
    let entity = file.entities().get(index).ok_or_else(|| IoError::Invalid("DXF private TEXT bridge: no such entity".into()))?;
    let layer = file.layer(entity.layer())?;
    let context = |error: IoError| IoError::Invalid(format!("DXF TEXT entity {} on layer {:?}: {error}", index + 1, layer.name()));
    let DxfShape::Text { text, .. } = entity.geometry() else {
        return Err(context(IoError::Invalid("not supported by private TEXT-only bridge: non-TEXT geometry".into())));
    };
    // Explicit partial bridge domain, NOT a product policy for control characters.
    // Neither MTEXT nor any disputed control/tail policy is invoked by this bridge.
    if text.chars().any(char::is_control) || text.chars().count() > 256 {
        return Err(context(IoError::Invalid("not supported by private TEXT-only bridge: controls, multiline or more than 256 characters".into())));
    }
    // The real Out::code(1, value) writes three code bytes plus two newlines.
    // Encode before the framing-floor check so the codec still validates the whole
    // supported-domain source for non-BMP when the remaining budget is zero.
    let encoded = encode_text_value(text, remaining_record_bytes.saturating_sub(5)).map_err(context)?;
    if remaining_record_bytes < 5 {
        return Err(context(IoError::Invalid("TEXT group1 framing exceeds remaining record bytes".into())));
    }
    out.code(1, encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DxfFile, DxfShape, Out, write_owned_text_value};
    use crate::DxfLinetype;
    use solidcraft_geom::Vec2;

    fn file_with_text(text: &str) -> DxfFile<DxfShape> {
        let mut file = DxfFile::new().unwrap();
        let layer = file.declare_layer("NOTES", 7, DxfLinetype::Continuous, None).unwrap();
        let shape = DxfShape::Text { at: Vec2::new(3.0, 4.0), height: 5.0, angle_deg: 0.0, text: text.into() };
        file.push(layer, shape, None).unwrap();
        file
    }

    fn sentinel() -> Out {
        Out { s: "sentinel\n".into(), next: 0x20 }
    }

    #[test]
    fn owned_text_uses_actual_out_group1_framing_only() {
        let file = file_with_text("plain");
        let mut out = sentinel();
        write_owned_text_value(&mut out, &file, 0, 10).unwrap();
        assert_eq!(out.s, "sentinel\n  1\nplain\n");
        assert_eq!(out.next, 0x20, "a text-value field does not allocate entity handles");
        assert_eq!(file.layer(file.entities()[0].layer()).unwrap().name(), "NOTES");
    }

    #[test]
    fn literal_percent_and_plain_degree_reach_real_out_without_preencoding() {
        let file = file_with_text("% %%d ° \\U+0041");
        let mut out = sentinel();
        write_owned_text_value(&mut out, &file, 0, 4096).unwrap();
        assert_eq!(out.s, "sentinel\n  1\n%%% %%%%%%d %%d \\U+005CU+0041\n");
    }

    #[test]
    fn non_bmp_error_has_real_entity_layer_and_leaves_out_untouched() {
        let file = file_with_text("ab𠀋");
        let mut out = sentinel();
        let error = write_owned_text_value(&mut out, &file, 0, 0).unwrap_err().to_string();
        assert!(error.contains("entity 1 on layer \"NOTES\""), "{error}");
        assert!(error.contains("U+2000B") && error.contains("position 3"), "{error}");
        assert_eq!(out.s, "sentinel\n");
        assert_eq!(out.next, 0x20);
    }

    #[test]
    fn unsupported_domain_refuses_before_writing_any_value() {
        for text in ["a\nb".into(), "a\rb".into(), "a\tb".into(), "x".repeat(257)] {
            let file = file_with_text(&text);
            let mut out = sentinel();
            let error = write_owned_text_value(&mut out, &file, 0, 4096).unwrap_err().to_string();
            assert!(error.contains("not supported by private TEXT-only bridge"), "{error}");
            assert_eq!(out.s, "sentinel\n");
        }
    }

    #[test]
    fn inclusive_record_budget_counts_encoding_plus_five_framing_bytes() {
        let file = file_with_text("中");
        let mut out = sentinel();
        write_owned_text_value(&mut out, &file, 0, 12).unwrap();
        assert_eq!(out.s, "sentinel\n  1\n\\U+4E2D\n");
        let mut small = sentinel();
        assert!(write_owned_text_value(&mut small, &file, 0, 11).is_err());
        assert_eq!(small.s, "sentinel\n");
        let empty = file_with_text("");
        let mut out = sentinel();
        write_owned_text_value(&mut out, &empty, 0, 5).unwrap();
        assert_eq!(out.s, "sentinel\n  1\n\n");
        let mut small = sentinel();
        assert!(write_owned_text_value(&mut small, &empty, 0, 4).is_err());
        assert_eq!(small.s, "sentinel\n");
        let long = file_with_text(&"中".repeat(256));
        let mut out = sentinel();
        write_owned_text_value(&mut out, &long, 0, 1797).unwrap();
        assert_eq!(out.s, format!("sentinel\n  1\n{}\n", "\\U+4E2D".repeat(256)));
    }

    #[test]
    fn wrong_geometry_and_missing_entity_cannot_write_a_partial_record() {
        let mut file = DxfFile::new().unwrap();
        file.push(file.default_layer(), DxfShape::Line(Vec2::ZERO, Vec2::new(1.0, 2.0)), None).unwrap();
        let mut out = sentinel();
        assert!(write_owned_text_value(&mut out, &file, 0, 4096).is_err());
        assert!(write_owned_text_value(&mut out, &file, usize::MAX, 4096).is_err());
        assert_eq!(out.s, "sentinel\n");
        assert_eq!(out.next, 0x20);
    }
}
