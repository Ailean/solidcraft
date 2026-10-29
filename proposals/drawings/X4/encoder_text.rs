//! Private X4 proposal: NOT APPLIED, NOT COMPILED, NOT RUN.
//! Intended as an internal dxf_out submodule after Engineer/W1 contract review.
//! No DxfFile, geometry, numeric formatter, reader or caller migration is defined here.
//! Controls other than line endings are refused pending explicit owner policy.

use std::fmt::Write as _;

use crate::{IoError, Result};

const TEXT_CHAR_LIMIT: usize = 256;
const MTEXT_CHUNK_BYTES: usize = 250;

fn invalid(reason: impl std::fmt::Display) -> IoError {
    IoError::Invalid(format!("DXF text: {reason}"))
}

// Validate the WHOLE original string, including beyond the historical 250-character cap.
// The writer adapter adds entity/layer identity to this original character position.
fn validate(source: &str) -> Result<()> {
    for (index, ch) in source.chars().enumerate() {
        if u32::from(ch) > 0xffff {
            return Err(invalid(format!(
                "character {ch:?} (U+{:04X}) at position {} cannot be written as DXF AC1015 text; only characters through U+FFFF are supported",
                u32::from(ch),
                index + 1
            )));
        }
    }
    for (index, ch) in source.chars().enumerate() {
        if ch.is_control() && ch != '\n' && ch != '\r' {
            return Err(invalid(format!("control U+{:04X} at position {} is not supported", u32::from(ch), index + 1)));
        }
    }
    Ok(())
}

fn unicode(out: &mut String, ch: char) -> Result<()> {
    write!(out, "\\U+{:04X}", u32::from(ch)).map_err(|error| invalid(format!("encoding failed: {error}")))
}

fn encoded_len(source: &str, mtext: bool) -> Result<usize> {
    let mut bytes = 0_usize;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        let count = match ch {
            '\r' if mtext => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                2
            }
            '\n' if mtext => 2,
            '%' if mtext => 7,
            '\\' | '{' | '}' if mtext => 2,
            '%' | '°' if !mtext => 3,
            '\\' if !mtext => 7,
            c if !c.is_ascii() => 7,
            _ => 1,
        };
        bytes = bytes.checked_add(count).ok_or_else(|| invalid("encoded size overflow"))?;
    }
    Ok(bytes)
}

// The caller supplies the remaining encoded-text byte budget; no new global resource policy.
// This excludes DXF group/record overhead, which the file adapter must also account for.
fn encode(source: &str, mtext: bool, max_encoded_bytes: usize) -> Result<String> {
    let needed = encoded_len(source, mtext)?;
    if needed > max_encoded_bytes {
        return Err(invalid("encoded text exceeds the caller's byte budget"));
    }
    let mut out = String::new();
    out.try_reserve(needed).map_err(|error| invalid(format!("cannot reserve encoded text: {error}")))?;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' if mtext => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("\\P");
            }
            '\n' if mtext => out.push_str("\\P"),
            '%' if mtext => unicode(&mut out, ch)?,
            '\\' | '{' | '}' if mtext => {
                out.push('\\');
                out.push(ch);
            }
            '%' if !mtext => out.push_str("%%%"),
            '°' if !mtext => out.push_str("%%d"),
            '\\' if !mtext => unicode(&mut out, ch)?,
            c if !c.is_ascii() => unicode(&mut out, c)?,
            c => out.push(c),
        }
    }
    Ok(out)
}

/// One TEXT group-1 value from plain single-line text, never truncated.
pub(super) fn encode_text_value(source: &str, max_encoded_bytes: usize) -> Result<String> {
    validate(source)?;
    if source.contains(['\n', '\r']) || source.chars().count() > TEXT_CHAR_LIMIT {
        return Err(invalid("TEXT requires one line of at most 256 characters; use MTEXT"));
    }
    encode(source, false, max_encoded_bytes)
}

/// MTEXT group-3 chunks and the final group-1 tail, from plain text.
/// All values are ASCII/CIF. Group-3 values have EXACTLY 250 bytes, the final group-1
/// fewer than 250 (possibly empty). Exact-multiple empty tails are a proposed policy:
/// owner/independent-reader acceptance is PENDING, not format conformance evidence.
/// A CIF escape may straddle a group boundary:
/// the reader must concatenate all group-3 values plus group-1 BEFORE decoding once.
pub(super) fn encode_mtext_groups(source: &str, max_encoded_bytes: usize) -> Result<Vec<(i32, String)>> {
    validate(source)?;
    let encoded = encode(source, true, max_encoded_bytes)?;
    let count = encoded.len() / MTEXT_CHUNK_BYTES + 1;
    let mut groups = Vec::new();
    groups.try_reserve(count).map_err(|error| invalid(format!("cannot reserve MTEXT chunks: {error}")))?;
    let mut start = 0;
    while encoded.len() - start >= MTEXT_CHUNK_BYTES {
        let end = start + MTEXT_CHUNK_BYTES;
        let chunk = encoded.get(start..end).ok_or_else(|| invalid("invalid ASCII chunk boundary"))?;
        let mut value = String::new();
        value.try_reserve(chunk.len()).map_err(|error| invalid(format!("cannot reserve MTEXT chunk: {error}")))?;
        value.push_str(chunk);
        groups.push((3, value));
        start = end;
    }
    let tail = encoded.get(start..).ok_or_else(|| invalid("invalid ASCII tail boundary"))?;
    let mut value = String::new();
    value.try_reserve(tail.len()).map_err(|error| invalid(format!("cannot reserve MTEXT tail: {error}")))?;
    value.push_str(tail);
    groups.push((1, value));
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::{encode_mtext_groups, encode_text_value};

    #[test]
    fn short_text_keeps_all_256_plain_characters_and_refuses_257() {
        for n in [0, 1, 255, 256] {
            let plain = "x".repeat(n);
            assert_eq!(encode_text_value(&plain, 4096).unwrap(), plain);
        }
        assert!(encode_text_value(&"x".repeat(257), 4096).is_err());
        assert!(encode_text_value("one\ntwo", 4096).is_err());
        assert!(encode_text_value("one\rtwo", 4096).is_err());
        assert_eq!(encode_text_value(&"中".repeat(256), 1792).unwrap(), "\\U+4E2D".repeat(256));
    }

    #[test]
    fn plain_percent_degree_and_literal_unicode_escape_have_distinct_text_bytes() {
        assert_eq!(encode_text_value("% %%d ° \\U+0041 {}", 4096).unwrap(), "%%% %%%%%%d %%d \\U+005CU+0041 {}");
        assert_eq!(encode_text_value("\u{ffff}", 7).unwrap(), "\\U+FFFF");
        assert_eq!(encode_text_value("B1 UP 90° R1", 4096).unwrap(), "B1 UP 90%%d R1");
    }

    #[test]
    fn mtext_plain_metacharacters_cannot_turn_into_formatting() {
        let groups = encode_mtext_groups("% %%d ° \\P {x}\nnext\r\nlast\rend", 4096).unwrap();
        assert_eq!(groups, vec![(1, "\\U+0025 \\U+0025\\U+0025d \\U+00B0 \\\\P \\{x\\}\\Pnext\\Plast\\Pend".into())]);
    }

    #[test]
    fn mtext_chunk_boundaries_have_full_group_threes_and_one_terminal_tail() {
        for (n, expected) in [
            (0, vec![(1, 0)]),
            (249, vec![(1, 249)]),
            (250, vec![(3, 250), (1, 0)]),
            (251, vec![(3, 250), (1, 1)]),
            (500, vec![(3, 250), (3, 250), (1, 0)]),
            (501, vec![(3, 250), (3, 250), (1, 1)]),
        ] {
            let plain = "x".repeat(n);
            let groups = encode_mtext_groups(&plain, 4096).unwrap();
            assert_eq!(groups.iter().map(|(code, text)| (*code, text.len())).collect::<Vec<_>>(), expected);
            assert_eq!(groups.iter().map(|(_, text)| text.as_str()).collect::<String>(), plain);
        }
    }

    #[test]
    fn cif_escape_across_group_boundary_is_reassembled_before_interpretation() {
        let plain = format!("{}中z", "a".repeat(248));
        let groups = encode_mtext_groups(&plain, 256).unwrap();
        assert_eq!(groups, vec![(3, format!("{}\\U", "a".repeat(248))), (1, "+4E2Dz".into())]);
        assert_eq!(groups.iter().map(|(_, text)| text.as_str()).collect::<String>(), format!("{}\\U+4E2Dz", "a".repeat(248)));
    }

    #[test]
    fn non_bmp_is_never_substituted_even_beyond_old_cap_and_before_budget_error() {
        for plain in ["𠀋".into(), "𝑥".into(), format!("{}𠀋", "x".repeat(300))] {
            let text = encode_text_value(&plain, 0).unwrap_err().to_string();
            let mtext = encode_mtext_groups(&plain, 0).unwrap_err().to_string();
            assert!(text.contains("U+2000B") || text.contains("U+1D465"));
            assert_eq!(text, mtext);
            if plain.starts_with('x') {
                assert!(text.contains("position 301"));
            }
        }
    }

    #[test]
    fn encoded_budget_counts_expansion_and_is_inclusive() {
        assert_eq!(encode_text_value("°", 3).unwrap(), "%%d");
        assert!(encode_text_value("°", 2).is_err());
        assert_eq!(encode_mtext_groups("%", 7).unwrap(), vec![(1, "\\U+0025".into())]);
        assert!(encode_mtext_groups("%", 6).is_err());
        assert_eq!(encode_mtext_groups("", 0).unwrap(), vec![(1, String::new())]);
    }

    #[test]
    fn unsupported_controls_refuse_instead_of_disappearing_pending_owner_policy() {
        for ch in ['\0', '\t', '\u{1f}', '\u{7f}', '\u{85}'] {
            let plain = format!("a{ch}b");
            assert!(encode_text_value(&plain, 4096).is_err());
            assert!(encode_mtext_groups(&plain, 4096).is_err());
        }
    }
}
