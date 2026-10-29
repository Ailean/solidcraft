//! PDF-only adapter for the explicitly reviewed subsetter 0.2.6 API.
//!
//! Takes original GIDs from the SAME admitted font bytes/face, without shaping,
//! Unicode/CID allocation, text measurement, font lookup or PDF-object emission.
//! The dependency allocates internally without a fallible allocator/budget API;
//! local caps and returned-byte checks do not prove process-OOM or panic safety.

use subsetter::GlyphRemapper;

use super::glyph_closure::{
    check_subset_program, check_subsetter_component_instruction_flags, glyph_closure,
};
use crate::{IoError, MAX_DRAWING_BYTES, Result};

/// Fixed-resource returned-byte policy, not a bound on dependency allocations.
const MAX_SUBSET_BYTES: usize = 410_820;
/// Must not loosen the reviewed closure's own cap.
const MAX_SUBSET_GLYPHS: usize = 16_384;

/// FontFile2 bytes and ORIGINAL font GID -> SUBSET font GID.
/// These IDs are not Unicode scalars, PDF character codes or logical clusters.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SubsetFont {
    bytes: Vec<u8>,
    mapping: Vec<(u16, u16)>,
}

impl SubsetFont {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn mapping(&self) -> &[(u16, u16)] {
        &self.mapping
    }
}

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(message.into())
}

/// The caller must obtain used IDs from these SAME font bytes and face.
/// Admission occurs inside the real glyph_closure; never hash the font twice.
/// Nonfinal-only composite instruction flags are temporarily unsupported by
/// this adapter, without redefining ordinary TrueType glyph-closure validity.
/// This produces a PDF CID font program, not a general installable font.
pub(super) fn subset_font(font: &[u8], face_index: u32, used: &[u16], max_bytes: usize) -> Result<SubsetFont> {
    let closure = glyph_closure(font, face_index, used)?;
    check_subsetter_component_instruction_flags(font, &closure)?;

    let max_bytes = max_bytes.min(MAX_SUBSET_BYTES).min(MAX_DRAWING_BYTES);
    if max_bytes == 0 {
        return Err(invalid("subset font byte allowance is zero"));
    }
    if closure.is_empty() || closure.len() > MAX_SUBSET_GLYPHS || closure.first().copied() != Some(0) {
        return Err(invalid("subset font closure exceeds its contract"));
    }
    let mut mapping = Vec::new();
    mapping.try_reserve_exact(closure.len()).map_err(|_| invalid("cannot allocate subset font mapping"))?;

    // External API: new/remap/get allocate internally without try_reserve.
    // Admission fixes the source to 2620 GIDs; local closure cap is also < u16::MAX.
    let mut remapper = GlyphRemapper::new();
    for (index, old) in closure.iter().copied().enumerate() {
        let expected = u16::try_from(index).map_err(|_| invalid("subset font index exceeds u16"))?;
        let new = remapper.remap(old);
        if new != expected || remapper.get(old) != Some(new) {
            return Err(invalid("subset font remapper is not dense and ordered"));
        }
        mapping.push((old, new));
    }
    if usize::from(remapper.num_gids()) != closure.len()
        || !remapper.remapped_gids().eq(closure.iter().copied())
    {
        return Err(invalid("subset font remapper does not match the complete closure"));
    }

    // The dependency clones the remapper and closes components again. Passing
    // the complete preflighted closure prevents it needing unreported new IDs.
    let bytes = subsetter::subset(font, face_index, &remapper)
        .map_err(|error| IoError::Invalid(format!("cannot subset the admitted PDF font: {error}")))?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(invalid("subset font exceeds its returned-byte allowance"));
    }
    check_subset_program(&bytes, mapping.len())?;
    if mapping.iter().any(|(_, new)| usize::from(*new) >= mapping.len()) {
        return Err(invalid("subset font mapping is outside its program"));
    }
    Ok(SubsetFont { bytes, mapping })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/liberation-sans/LiberationSans-Regular.ttf"));

    fn table<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
        let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
        for index in 0..usize::from(count) {
            let record = 12 + index * 16;
            if &bytes[record..record + 4] == tag {
                let start = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
                let len = u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap()) as usize;
                return &bytes[start..start + len];
            }
        }
        panic!("required test table absent");
    }

    fn metric(bytes: &[u8], gid: usize) -> (u16, i16) {
        let hhea = table(bytes, b"hhea");
        let hmtx = table(bytes, b"hmtx");
        let count = usize::from(u16::from_be_bytes(hhea[34..36].try_into().unwrap()));
        assert!(count > 0);
        let advance_at = gid.min(count - 1) * 4;
        let lsb_at = if gid < count { gid * 4 + 2 } else { count * 4 + (gid - count) * 2 };
        (
            u16::from_be_bytes(hmtx[advance_at..advance_at + 2].try_into().unwrap()),
            i16::from_be_bytes(hmtx[lsb_at..lsb_at + 2].try_into().unwrap()),
        )
    }

    #[test]
    fn fixed_simple_and_empty_glyphs_have_literal_original_metrics() {
        // Read-only source metadata, not candidate-generated expectations:
        // loca[0..2]=[0,76,76]; hmtx0=(1536,205), hmtx1=(0,0).
        let result = subset_font(BUNDLED, 0, &[1, 1, 0], MAX_SUBSET_BYTES).unwrap();
        assert_eq!(result.mapping, [(0, 0), (1, 1)]);
        assert_eq!(u16::from_be_bytes(table(&result.bytes, b"maxp")[4..6].try_into().unwrap()), 2);
        assert_eq!(metric(&result.bytes, 0), (1536, 205));
        assert_eq!(metric(&result.bytes, 1), (0, 0));
        assert_eq!(table(&result.bytes, b"head")[18..20], [8, 0]); // 2048 units/em.
        for tag in [b"cvt ", b"fpgm", b"prep"] {
            assert_eq!(table(&result.bytes, tag), table(BUNDLED, tag), "opaque hint table");
        }
    }

    #[test]
    fn original_ids_are_separate_from_dense_subset_ids_and_order_is_stable() {
        // Original loca gives empty spans for1,2,3; widths2,3 are both569.
        let a = subset_font(BUNDLED, 0, &[3, 2, 3], MAX_SUBSET_BYTES).unwrap();
        let b = subset_font(BUNDLED, 0, &[2, 3], MAX_SUBSET_BYTES).unwrap();
        assert_eq!(a.mapping, [(0, 0), (2, 1), (3, 2)]);
        assert_eq!(metric(&a.bytes, 0), (1536, 205));
        assert_eq!(metric(&a.bytes, 1), (569, 0));
        assert_eq!(metric(&a.bytes, 2), (569, 0));
        assert_eq!(a, b);
    }

    #[test]
    fn empty_request_still_embeds_notdef_without_inventing_text() {
        let result = subset_font(BUNDLED, 0, &[], MAX_SUBSET_BYTES).unwrap();
        assert_eq!(result.mapping, [(0, 0)]);
        assert_eq!(metric(&result.bytes, 0), (1536, 205));
    }

    #[test]
    fn admission_refuses_wrong_face_changed_resource_and_invalid_gids() {
        assert!(subset_font(BUNDLED, 1, &[0], MAX_SUBSET_BYTES).is_err());
        assert!(subset_font(BUNDLED, u32::MAX, &[0], MAX_SUBSET_BYTES).is_err());
        assert!(subset_font(BUNDLED, 0, &[2620], MAX_SUBSET_BYTES).is_err());
        assert!(subset_font(BUNDLED, 0, &vec![0; 65_537], MAX_SUBSET_BYTES).is_err());
        let mut changed = BUNDLED.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        assert!(subset_font(&changed, 0, &[0], MAX_SUBSET_BYTES).is_err());
        assert!(subset_font(&[0; 32], 0, &[0], MAX_SUBSET_BYTES).is_err());
    }

    #[test]
    fn returned_byte_allowance_is_inclusive_and_never_a_preallocation_claim() {
        let expected = subset_font(BUNDLED, 0, &[1], MAX_SUBSET_BYTES).unwrap();
        let exact = subset_font(BUNDLED, 0, &[1], expected.bytes.len()).unwrap();
        assert_eq!(exact, expected);
        assert!(subset_font(BUNDLED, 0, &[1], expected.bytes.len() - 1).is_err());
        assert!(subset_font(BUNDLED, 0, &[1], 0).is_err());
        assert!(subset_font(BUNDLED, 0, &[1], 1).is_err());
    }

    #[test]
    fn program_readback_rejects_count_mismatch_and_truncated_ranges() {
        let result = subset_font(BUNDLED, 0, &[1], MAX_SUBSET_BYTES).unwrap();
        assert!(check_subset_program(&result.bytes, 3).is_err());
        assert!(check_subset_program(&result.bytes, 0).is_err());
        for end in [0, 4, 11, 12] {
            assert!(check_subset_program(&result.bytes[..end], 2).is_err());
        }
        // The original resource has cmap and all original glyphs, so it is not
        // accepted as this two-glyph PDF-only subset just because it is admitted.
        assert!(check_subset_program(BUNDLED, 2).is_err());
        assert!(check_subset_program(BUNDLED, 2620).unwrap_err().to_string().contains("cmap"));
    }
}

