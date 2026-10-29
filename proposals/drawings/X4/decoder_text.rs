//! Private production-code proposal for dxf/text.rs. NOT APPLIED / NOT COMPILED / NOT RUN.
//! Decodes values only, not a second DXF parser. Caller supplies raw, untrimmed
//! text fields and finite byte/chunk budgets. No font-dependent styling fallback.

use crate::{IoError, Result};

fn invalid(reason: impl std::fmt::Display) -> IoError {
    IoError::Invalid(format!("DXF text value: {reason}"))
}

fn push(out: &mut String, ch: char, max_bytes: usize) -> Result<()> {
    let needed = out.len().checked_add(ch.len_utf8()).ok_or_else(|| invalid("decoded size overflow"))?;
    if needed > max_bytes {
        return Err(invalid("decoded byte budget exceeded"));
    }
    out.try_reserve(ch.len_utf8()).map_err(|error| invalid(format!("cannot reserve decoded text: {error}")))?;
    out.push(ch);
    Ok(())
}

fn cif(chars: &mut impl Iterator<Item = char>) -> Result<char> {
    if chars.next() != Some('+') {
        return Err(invalid("malformed CIF Unicode prefix"));
    }
    let mut code = 0_u32;
    for _ in 0..4 {
        let digit = chars.next().and_then(|ch| ch.to_digit(16)).ok_or_else(|| invalid("CIF Unicode escape requires four hexadecimal digits"))?;
        code = code * 16 + digit;
    }
    let ch = char::from_u32(code).ok_or_else(|| invalid("CIF Unicode escape is not a scalar value; surrogate pairs are unsupported"))?;
    if ch.is_control() {
        return Err(invalid("not supported yet: a control character encoded as CIF"));
    }
    Ok(ch)
}

fn percent(chars: &mut impl Iterator<Item = char>, mtext: bool) -> Result<char> {
    match chars.next() {
        Some('%') if !mtext => Ok('%'),
        Some('d') => Ok('°'),
        Some('p') => Ok('±'),
        // Autodesk's published symbol mapping is U+2205, not U+2300/U+00D8.
        Some('c') => Ok('\u{2205}'),
        _ => Err(invalid("not supported yet: percent formatting code")),
    }
}

fn decode(value: &str, mtext: bool, max_decoded_bytes: usize) -> Result<String> {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        let decoded = match ch {
            // No hidden CR/LF/tab normalization or stripping. A paragraph is
            // admitted only through the explicit MTEXT escape below.
            c if c.is_control() => return Err(invalid("not supported yet: raw text control character")),
            '\\' => match chars.peek().copied() {
                Some('U') => {
                    chars.next();
                    cif(&mut chars)?
                }
                Some('\\' | '{' | '}') if mtext => chars.next().ok_or_else(|| invalid("incomplete MTEXT escape"))?,
                Some('P') if mtext => {
                    chars.next();
                    '\n'
                }
                _ if mtext => return Err(invalid("not supported yet: MTEXT formatting escape")),
                _ => '\\',
            },
            '%' if chars.peek() == Some(&'%') => {
                chars.next();
                percent(&mut chars, mtext)?
            }
            '{' | '}' if mtext => return Err(invalid("not supported yet: MTEXT formatting group")),
            c => c,
        };
        // Never feed decoded output back into the scanner. Decoded percent,
        // backslash, braces and 'P' remain literal rather than becoming syntax.
        push(&mut out, decoded, max_decoded_bytes)?;
    }
    Ok(out)
}

/// TEXT's untrimmed group-1 value. Raw Unicode scalar text is preserved, including
/// non-BMP UTF8 in an imported newer-format file. That is distinct from the
/// AC1015 writer's no-non-BMP admission rule; this helper does not select a version.
pub(super) fn decode_text_value(value: &str, max_encoded_bytes: usize, max_decoded_bytes: usize) -> Result<String> {
    if value.len() > max_encoded_bytes {
        return Err(invalid("encoded byte budget exceeded"));
    }
    decode(value, false, max_decoded_bytes)
}

/// Concatenate already-parsed MTEXT group-3 fields then its single group-1 tail,
/// preserving their stored order and spaces, and decode ONCE. Only pass codes1/3.
/// Chunk-length/empty-tail format policy remains the caller's responsibility:
/// this is a bounded value combiner, not a file/chunk-conformance validator.
pub(super) fn decode_mtext_values<'a>(
    values: impl IntoIterator<Item = (i32, &'a str)>, max_encoded_bytes: usize, max_decoded_bytes: usize, max_chunks: usize,
) -> Result<String> {
    let mut joined = String::new();
    let mut tail_seen = false;
    let mut count = 0_usize;
    for (code, value) in values {
        count = count.checked_add(1).ok_or_else(|| invalid("MTEXT chunk count overflow"))?;
        if count > max_chunks {
            return Err(invalid("MTEXT chunk budget exceeded"));
        }
        if tail_seen {
            return Err(invalid("MTEXT text field follows its group-1 tail"));
        }
        match code {
            3 => {}
            1 => tail_seen = true,
            _ => return Err(invalid("MTEXT value combiner accepts group1/3 only")),
        }
        let needed = joined.len().checked_add(value.len()).ok_or_else(|| invalid("MTEXT encoded size overflow"))?;
        if needed > max_encoded_bytes {
            return Err(invalid("MTEXT encoded byte budget exceeded"));
        }
        joined.try_reserve(value.len()).map_err(|error| invalid(format!("cannot reserve MTEXT text: {error}")))?;
        joined.push_str(value);
    }
    if !tail_seen {
        return Err(invalid("MTEXT is missing its group-1 tail"));
    }
    decode(&joined, true, max_decoded_bytes)
}

#[cfg(test)]
mod tests {
    use super::{decode_mtext_values, decode_text_value};

    #[test]
    fn text_plain_percent_control_codes_and_cif_have_literal_expected_values() {
        assert_eq!(decode_text_value("%%% %%%%%%d %%d %%p %%c \\U+FFFF", 4096, 4096).unwrap(), "% %%d ° ± \u{2205} \u{ffff}");
    }

    #[test]
    fn text_decodes_once_and_preserves_spaces_and_literal_backslash_syntax() {
        assert_eq!(decode_text_value("  \\U+005CU+0041 \\P \\{x\\}  ", 4096, 4096).unwrap(), "  \\U+0041 \\P \\{x\\}  ");
        assert_eq!(decode_text_value("\\U+0025\\U+0025d", 4096, 4096).unwrap(), "%%d");
        assert_eq!(decode_text_value("trailing\\", 4096, 4096).unwrap(), "trailing\\");
    }

    #[test]
    fn mtext_literals_and_paragraphs_decode_once_without_style_interpretation() {
        let fields = [(1, "\\U+0025 \\U+0025\\U+0025d \\U+00B0 \\\\P \\{x\\}\\Pnext")];
        assert_eq!(decode_mtext_values(fields, 4096, 4096, 1).unwrap(), "% %%d ° \\P {x}\nnext");
        assert_eq!(decode_mtext_values([(1, "\\\\U+0041")], 4096, 4096, 1).unwrap(), "\\U+0041");
    }

    #[test]
    fn mtext_joins_cif_escape_across_real_250_byte_boundary_before_decoding() {
        let head = format!("{}\\U", "a".repeat(248));
        let fields = [(3, head.as_str()), (1, "+4E2Dz")];
        assert_eq!(head.len(), 250);
        assert_eq!(decode_mtext_values(fields, 256, 252, 2).unwrap(), format!("{}中z", "a".repeat(248)));
    }

    #[test]
    fn mtext_joins_escaped_backslash_and_percent_without_second_decode() {
        let head = format!("{}\\", "a".repeat(249));
        assert_eq!(head.len(), 250);
        assert_eq!(decode_mtext_values([(3, head.as_str()), (1, "\\P")], 4096, 4096, 2).unwrap(), format!("{}\\P", "a".repeat(249)));
        assert_eq!(decode_mtext_values([(3, "\\U+002"), (1, "5\\U+0025d")], 4096, 4096, 2).unwrap(), "%%d");
    }

    #[test]
    fn value_combiner_preserves_empty_tail_without_deciding_writer_chunk_policy() {
        let full = "x".repeat(250);
        assert_eq!(decode_mtext_values([(3, full.as_str()), (1, "")], 250, 250, 2).unwrap(), full);
        assert_eq!(decode_mtext_values([(1, "")], 0, 0, 1).unwrap(), "");
        assert_eq!(decode_mtext_values([(3, "  one "), (1, " two  ")], 4096, 4096, 2).unwrap(), "  one  two  ");
    }

    #[test]
    fn malformed_or_styled_escapes_are_explicit_errors_not_stripped_text() {
        for value in ["\\U+", "\\U+XYZ0", "\\U+123", "\\U0041", "\\U+D800", "\\U+DC00", "%%", "%%q", "%%u", "%%123"] {
            assert!(decode_text_value(value, 4096, 4096).is_err(), "{value}");
        }
        for value in ["\\", "\\H2x;Hi", "{styled}", "%%%", "\\~", "\\Lline"] {
            assert!(decode_mtext_values([(1, value)], 4096, 4096, 1).is_err(), "{value}");
        }
    }

    #[test]
    fn raw_controls_and_cif_controls_do_not_silently_normalize() {
        for value in ["a\rb", "a\nb", "a\tb", "\\U+0000", "\\U+000A", "\\U+000D", "\\U+0085"] {
            assert!(decode_text_value(value, 4096, 4096).is_err());
            assert!(decode_mtext_values([(1, value)], 4096, 4096, 1).is_err());
        }
    }

    #[test]
    fn encoded_decoded_and_chunk_budgets_are_independently_inclusive() {
        assert_eq!(decode_text_value("\\U+FFFF", 7, 3).unwrap(), "\u{ffff}");
        assert!(decode_text_value("\\U+FFFF", 6, 3).is_err());
        assert!(decode_text_value("\\U+FFFF", 7, 2).is_err());
        assert_eq!(decode_mtext_values([(3, "\\U+"), (1, "FFFF")], 7, 3, 2).unwrap(), "\u{ffff}");
        assert!(decode_mtext_values([(3, "\\U+"), (1, "FFFF")], 6, 3, 2).is_err());
        assert!(decode_mtext_values([(3, "\\U+"), (1, "FFFF")], 7, 2, 2).is_err());
        assert!(decode_mtext_values([(3, "\\U+"), (1, "FFFF")], 7, 3, 1).is_err());
        assert!(decode_mtext_values([(1, "")], 0, 0, 0).is_err());
    }

    #[test]
    fn mtext_missing_duplicate_or_out_of_order_tail_refuses() {
        assert!(decode_mtext_values([], 4096, 4096, 8).is_err());
        assert!(decode_mtext_values([(3, "text")], 4096, 4096, 8).is_err());
        assert!(decode_mtext_values([(1, "a"), (1, "b")], 4096, 4096, 8).is_err());
        assert!(decode_mtext_values([(1, "a"), (3, "b")], 4096, 4096, 8).is_err());
        assert!(decode_mtext_values([(8, "LAYER"), (1, "a")], 4096, 4096, 8).is_err());
    }

    #[test]
    fn direct_utf8_input_is_not_confused_with_ac1015_writer_admission() {
        assert_eq!(decode_text_value("中𠀋", 7, 7).unwrap(), "中𠀋");
        assert_eq!(decode_mtext_values([(1, "中𠀋")], 7, 7, 1).unwrap(), "中𠀋");
    }
}
