//! Deterministic ToUnicode CMap bytes for already assigned two-byte character codes.
//!
//! ISO 32000-1:2008, 9.10.3: UTF-16BE destinations, at most 512 bytes each.
//! Adobe Technical Note 5411, 1.4.1: at most 100 mappings per bfchar block.
//! This does not assign CIDs/GIDs, shape text, subset/embed a font or infer clusters.
//! The caller must use a matching TWO-BYTE font encoding; this is not for simple
//! one-byte fonts. Text is literal: no normalization, replacement or implicit BOM.
//!
//! https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf
//! https://pdfa.org/norm-refs/5411.ToUnicode.pdf

use crate::{IoError, MAX_DRAWING_BYTES, Result};

const MAX_MAPPINGS: usize = 65_536;
const MAX_CLUSTER_UTF8_BYTES: usize = 768; // 256 BMP scalars of three UTF-8 bytes.
const MAX_DESTINATION_UNITS: usize = 256; // The specification's 512 destination bytes.
const MAX_INPUT_BYTES: usize = 4 << 20;
const MAX_UTF16_UNITS: usize = 1 << 20;
const MAX_OUTPUT_BYTES: usize = 8 << 20;
const MAX_WORK: usize = 64 << 20;
const BLOCK_MAPPINGS: usize = 100;

const PREFIX: &[u8] = b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /SolidCraft-ToUnicode16 def
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
";
const SUFFIX: &[u8] = b"endcmap
CMapName currentdict /CMap defineresource pop
end
end
";
const BLOCK_BEGIN: &[u8] = b" beginbfchar\n";
const BLOCK_END: &[u8] = b"endbfchar\n";

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(message.into())
}

#[derive(Clone, Copy)]
struct Limits {
    mappings: usize,
    input_bytes: usize,
    utf16_units: usize,
    output_bytes: usize,
    work: usize,
}

impl Limits {
    const MAX: Self = Self {
        mappings: MAX_MAPPINGS,
        input_bytes: MAX_INPUT_BYTES,
        utf16_units: MAX_UTF16_UNITS,
        output_bytes: MAX_OUTPUT_BYTES,
        work: MAX_WORK,
    };

    fn bounded(self) -> Self {
        Self {
            mappings: self.mappings.min(MAX_MAPPINGS),
            input_bytes: self.input_bytes.min(MAX_INPUT_BYTES),
            utf16_units: self.utf16_units.min(MAX_UTF16_UNITS),
            output_bytes: self.output_bytes.min(MAX_OUTPUT_BYTES).min(MAX_DRAWING_BYTES),
            work: self.work.min(MAX_WORK),
        }
    }
}

/// Serialize literal Unicode clusters in strictly increasing source-code order.
/// Codes 0 and 65535 are valid; distinct codes may have the same destination.
/// Empty clusters, duplicate/out-of-order codes and over-budget input are refused.
/// max_bytes can only tighten the hard output cap; it cannot increase it.
/// Returns a complete CMap stream payload, not a PDF object/dictionary or font.
pub(super) fn write_to_unicode(mappings: &[(u16, &str)], max_bytes: usize) -> Result<Vec<u8>> {
    serialize(mappings, Limits { output_bytes: max_bytes, ..Limits::MAX })
}

/// Checked budget changes are atomic even when the proposed addition fails.
fn charge(total: &mut usize, amount: usize, limit: usize, message: &'static str) -> Result<()> {
    let next = total.checked_add(amount).filter(|next| *next <= limit).ok_or_else(|| invalid(message))?;
    *total = next;
    Ok(())
}

fn preflight(mappings: &[(u16, &str)], limits: Limits) -> Result<usize> {
    if mappings.len() > limits.mappings {
        return Err(invalid("ToUnicode mapping count exceeds its limit"));
    }
    let (mut input, mut work, mut previous) = (0usize, 0usize, None);
    // Length/order-only pass before scanning ANY Unicode payload or allocating output.
    for &(code, text) in mappings {
        if previous.is_some_and(|previous| code <= previous) {
            return Err(invalid("ToUnicode codes must be unique and strictly increasing"));
        }
        previous = Some(code);
        if text.is_empty() {
            return Err(invalid("ToUnicode requires a nonempty literal Unicode cluster"));
        }
        if text.len() > MAX_CLUSTER_UTF8_BYTES {
            return Err(invalid("ToUnicode cluster exceeds its UTF-8 input bound"));
        }
        charge(&mut input, text.len(), limits.input_bytes, "ToUnicode UTF-8 input exceeds its limit")?;
        // A conservative linear-work budget for validation and both UTF-16 passes,
        // including four hexadecimal bytes per unit. Not a wall-clock guarantee.
        let cost = text.len().checked_mul(8).and_then(|cost| cost.checked_add(32))
            .ok_or_else(|| invalid("ToUnicode work count overflow"))?;
        charge(&mut work, cost, limits.work, "ToUnicode work exceeds its limit")?;
    }
    let mut bytes = 0usize;
    charge(&mut bytes, PREFIX.len(), limits.output_bytes, "ToUnicode output exceeds its limit")?;
    charge(&mut bytes, SUFFIX.len(), limits.output_bytes, "ToUnicode output exceeds its limit")?;
    let mut total_units = 0usize;
    for block in mappings.chunks(BLOCK_MAPPINGS) {
        let digits = if block.len() == 100 { 3 } else if block.len() >= 10 { 2 } else { 1 };
        charge(&mut bytes, digits + BLOCK_BEGIN.len() + BLOCK_END.len(), limits.output_bytes, "ToUnicode output exceeds its limit")?;
        for &(_, text) in block {
            // Bounded even before the cumulative destination budget is checked.
            let units = text.encode_utf16().take(MAX_DESTINATION_UNITS + 1).count();
            if units > MAX_DESTINATION_UNITS {
                return Err(invalid("ToUnicode destination exceeds 512 UTF-16BE bytes"));
            }
            charge(&mut total_units, units, limits.utf16_units, "ToUnicode UTF-16 payload exceeds its limit")?;
            // <hhhh> < + four ASCII hex bytes per UTF-16 unit + > and newline.
            let row_bytes = units.checked_mul(4).and_then(|hex| hex.checked_add(10))
                .ok_or_else(|| invalid("ToUnicode output byte count overflow"))?;
            charge(&mut bytes, row_bytes, limits.output_bytes, "ToUnicode output exceeds its limit")?;
        }
    }
    Ok(bytes)
}

struct Output {
    bytes: Vec<u8>,
    limit: usize,
}

impl Output {
    fn new(limit: usize) -> Result<Self> {
        if limit > MAX_OUTPUT_BYTES.min(MAX_DRAWING_BYTES) {
            return Err(invalid("ToUnicode output exceeds its limit"));
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(limit).map_err(|_| invalid("ToUnicode output allocation failed"))?;
        Ok(Self { bytes, limit })
    }

    fn append(&mut self, text: &[u8]) -> Result<()> {
        if self.bytes.len().checked_add(text.len()).is_none_or(|len| len > self.limit) {
            return Err(invalid("ToUnicode output exceeds its limit"));
        }
        self.bytes.try_reserve(text.len()).map_err(|_| invalid("ToUnicode output allocation failed"))?;
        self.bytes.extend_from_slice(text);
        Ok(())
    }

    fn block_count(&mut self, count: usize) -> Result<()> {
        if count == 0 || count > BLOCK_MAPPINGS {
            return Err(invalid("ToUnicode bfchar block must contain 1 to 100 mappings"));
        }
        if count == 100 {
            self.append(b"100")
        } else {
            let count = u8::try_from(count).map_err(|_| invalid("invalid ToUnicode block count"))?;
            if count >= 10 {
                self.append(&[b'0' + count / 10, b'0' + count % 10])
            } else {
                self.append(&[b'0' + count])
            }
        }
    }
}

/// The nibbles are masked, so every returned byte is an ASCII hexadecimal digit.
fn hex16(unit: u16) -> [u8; 4] {
    [12, 8, 4, 0].map(|shift| {
        let nibble = ((unit >> shift) & 15) as u8;
        if nibble < 10 { b'0' + nibble } else { b'A' + nibble - 10 }
    })
}

fn serialize(mappings: &[(u16, &str)], limits: Limits) -> Result<Vec<u8>> {
    let expected = preflight(mappings, limits.bounded())?;
    let mut out = Output::new(expected)?;
    out.append(PREFIX)?;
    for block in mappings.chunks(BLOCK_MAPPINGS) {
        out.block_count(block.len())?;
        out.append(BLOCK_BEGIN)?;
        for &(code, text) in block {
            out.append(b"<")?;
            out.append(&hex16(code))?;
            out.append(b"> <")?;
            for unit in text.encode_utf16() {
                out.append(&hex16(unit))?;
            }
            out.append(b">\n")?;
        }
        out.append(BLOCK_END)?;
    }
    out.append(SUFFIX)?;
    if out.bytes.len() != expected {
        return Err(invalid("ToUnicode output size differs from its preflight"));
    }
    Ok(out.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_stream_is_literal_complete_and_deterministic() {
        let expected = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /SolidCraft-ToUnicode16 def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 beginbfchar\n<0042> <0041>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
        assert_eq!(write_to_unicode(&[(0x42, "A")], MAX_OUTPUT_BYTES).unwrap(), expected);
        assert_eq!(write_to_unicode(&[(0x42, "A")], MAX_OUTPUT_BYTES).unwrap(), expected);
    }

    #[test]
    fn bmp_supplementary_and_literal_clusters_keep_their_sequence() {
        let maps = [(1, "Å"), (2, "中"), (3, "🚀"), (4, "e\u{301}"), (5, "ffi"), (6, "\u{feff}\0"), (7, "\u{10ffff}")];
        let bytes = write_to_unicode(&maps, MAX_OUTPUT_BYTES).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        for row in [
            "<0001> <00C5>\n", "<0002> <4E2D>\n", "<0003> <D83DDE80>\n",
            "<0004> <00650301>\n", "<0005> <006600660069>\n",
            "<0006> <FEFF0000>\n", "<0007> <DBFFDFFF>\n",
        ] {
            assert_eq!(text.matches(row).count(), 1, "{row}");
        }
        assert!(!text.contains("<FEFF00C5>"), "no implicit BOM");
        assert!(!text.contains("<00E9>"), "no normalization of e + combining acute");
        assert!(bytes.is_ascii());
    }

    #[test]
    fn codes_are_not_cids_or_glyph_ids_and_both_u16_extremes_are_allowed() {
        let bytes = write_to_unicode(&[(0, "A"), (u16::MAX, "A")], MAX_OUTPUT_BYTES).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("<0000> <0041>\n"));
        assert!(text.contains("<FFFF> <0041>\n"));
        assert!(!text.contains("begincidchar"));
        for maps in [vec![(7, "A"), (7, "B")], vec![(8, "A"), (7, "B")], vec![(1, "")]] {
            assert!(write_to_unicode(&maps, MAX_OUTPUT_BYTES).is_err());
        }
    }

    #[test]
    fn empty_mapping_table_has_codespace_but_no_empty_bfchar_block() {
        let bytes = write_to_unicode(&[], MAX_OUTPUT_BYTES).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n"));
        assert!(!text.contains("beginbfchar"));
        assert!(text.ends_with("end\nend\n"));
    }

    #[test]
    fn destination_limit_counts_utf16_units_instead_of_scalars_or_utf8_bytes() {
        for value in ["A".repeat(256), "中".repeat(256), "🚀".repeat(128)] {
            assert!(write_to_unicode(&[(1, &value)], MAX_OUTPUT_BYTES).is_ok());
        }
        for value in ["A".repeat(257), "🚀".repeat(129)] {
            assert!(write_to_unicode(&[(1, &value)], MAX_OUTPUT_BYTES).unwrap_err().to_string().contains("512 UTF-16BE"));
        }
        let too_many_bytes = "中".repeat(257);
        assert!(write_to_unicode(&[(1, &too_many_bytes)], MAX_OUTPUT_BYTES).unwrap_err().to_string().contains("UTF-8 input bound"));
    }

    #[test]
    fn bfchar_blocks_end_at_one_hundred_and_never_emit_a_zero_block() {
        for (count, expected) in [(99, vec![99]), (100, vec![100]), (101, vec![100, 1]), (200, vec![100, 100]), (201, vec![100, 100, 1])] {
            let maps: Vec<_> = (0..count).map(|code| (code as u16, "x")).collect();
            let bytes = write_to_unicode(&maps, MAX_OUTPUT_BYTES).unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            let counts: Vec<usize> = text.lines().filter_map(|line| line.strip_suffix(" beginbfchar")).map(|n| n.parse().unwrap()).collect();
            assert_eq!(counts, expected);
            assert_eq!(text.lines().filter(|line| line.starts_with('<') && line.ends_with("<0078>")).count(), count);
            assert_eq!(text.lines().filter(|line| *line == "endbfchar").count(), expected.len());
        }
    }

    #[test]
    fn literal_operator_text_is_only_hex_data_and_cannot_close_the_cmap() {
        let bytes = write_to_unicode(&[(1, "\nendcmap\n%<>")], MAX_OUTPUT_BYTES).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("<0001> <000A0065006E00640063006D00610070000A0025003C003E>\n"));
        assert_eq!(text.lines().filter(|line| *line == "endcmap").count(), 1);
        assert!(!text.contains("\n%<>"));
    }

    #[test]
    fn each_budget_accepts_its_inclusive_boundary_and_refuses_one_less() {
        let maps = [(1, "A")];
        let expected = write_to_unicode(&maps, MAX_OUTPUT_BYTES).unwrap();
        let exact = Limits { mappings: 1, input_bytes: 1, utf16_units: 1, output_bytes: expected.len(), work: 40 };
        assert_eq!(serialize(&maps, exact).unwrap(), expected);
        assert_eq!(write_to_unicode(&maps, expected.len()).unwrap(), expected);
        assert!(write_to_unicode(&maps, expected.len() - 1).is_err());
        assert_eq!(write_to_unicode(&maps, usize::MAX).unwrap(), expected, "caller cannot remove hard caps");
        for limits in [
            Limits { mappings: 0, ..exact }, Limits { input_bytes: 0, ..exact },
            Limits { utf16_units: 0, ..exact }, Limits { output_bytes: expected.len() - 1, ..exact },
            Limits { work: 39, ..exact },
        ] {
            assert!(serialize(&maps, limits).is_err());
        }
    }

    #[test]
    fn input_work_admission_precedes_unicode_payload_scanning_and_output() {
        // Its 257 UTF-16 units would fail later; length/work admission wins first.
        let cluster = "A".repeat(257);
        let maps = [(1, cluster.as_str())];
        assert!(serialize(&maps, Limits { input_bytes: 256, ..Limits::MAX }).unwrap_err().to_string().contains("UTF-8 input exceeds"));
        assert!(serialize(&maps, Limits { work: 32, ..Limits::MAX }).unwrap_err().to_string().contains("work exceeds"));
        assert_eq!(maps[0].1, cluster);
    }

    #[test]
    fn cumulative_budgets_and_count_are_independent_of_per_cluster_admission() {
        let maps = [(1, "A"), (2, "B")];
        assert!(serialize(&maps, Limits { input_bytes: 1, ..Limits::MAX }).is_err());
        assert!(serialize(&maps, Limits { utf16_units: 1, ..Limits::MAX }).is_err());
        let too_many = vec![(0, "A"); MAX_MAPPINGS + 1];
        assert!(write_to_unicode(&too_many, MAX_OUTPUT_BYTES).unwrap_err().to_string().contains("mapping count"));
        assert!(serialize(&too_many, Limits { mappings: usize::MAX, ..Limits::MAX }).unwrap_err().to_string().contains("mapping count"));
    }

    #[test]
    fn checked_budget_and_output_refusals_preserve_their_private_state() {
        let mut count = usize::MAX;
        assert!(charge(&mut count, 1, usize::MAX, "overflow").is_err());
        assert_eq!(count, usize::MAX);
        let mut out = Output::new(2).unwrap();
        out.append(b"AB").unwrap();
        assert!(out.append(b"C").is_err());
        assert_eq!(out.bytes, b"AB");
        assert!(out.block_count(0).is_err());
        assert!(out.block_count(101).is_err());
        assert_eq!(out.bytes, b"AB");
    }
}
