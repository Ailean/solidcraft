//! Original SFNT metadata fixtures; no system font or external test asset.
use super::*;

const BUNDLED: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/liberation-sans/LiberationSans-Regular.ttf"));

fn metadata_fixture(flags: u16) -> Vec<u8> {
    let mut bytes = vec![0; 124];
    bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
    bytes[4..6].copy_from_slice(&1_u16.to_be_bytes());
    bytes[12..16].copy_from_slice(b"OS/2");
    bytes[20..24].copy_from_slice(&28_u32.to_be_bytes());
    bytes[24..28].copy_from_slice(&96_u32.to_be_bytes());
    bytes[28..30].copy_from_slice(&3_u16.to_be_bytes());
    bytes[36..38].copy_from_slice(&flags.to_be_bytes());
    bytes
}

#[test]
fn current_bundled_revision_and_face_zero_are_admitted_without_resolution() {
    assert_eq!(BUNDLED.len(), 410_820);
    assert!(check_liberation_sans(BUNDLED, 0).is_ok());
    assert!(reviewed_embedding_metadata(BUNDLED).is_ok());
    assert!(check_liberation_sans(BUNDLED, 1).is_err());
    assert!(check_liberation_sans(BUNDLED, u32::MAX).is_err());
}

#[test]
fn metadata_fixture_is_not_an_approved_font_resource() {
    assert!(reviewed_embedding_metadata(&metadata_fixture(0)).is_ok());
    assert!(check_liberation_sans(&metadata_fixture(0), 0).is_err());
    let mut changed = BUNDLED.to_vec();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(check_liberation_sans(&changed, 0).is_err());
    assert!(check_liberation_sans(&BUNDLED[..BUNDLED.len() - 1], 0).is_err());
    changed.push(0);
    assert!(check_liberation_sans(&changed, 0).is_err());
}

#[test]
fn nonzero_permissions_and_restrictions_are_outside_the_one_resource_policy() {
    for flags in [0x0001, 0x0002, 0x0004, 0x0008, 0x000c, 0x0010, 0x0100, 0x0200, 0x0300, 0x8000, 0xffff] {
        let bytes = metadata_fixture(flags);
        let before = bytes.clone();
        assert!(reviewed_embedding_metadata(&bytes).is_err(), "flags {flags:04x}");
        assert_eq!(bytes, before);
    }
}

#[test]
fn missing_truncated_or_out_of_envelope_metadata_is_refused() {
    let valid = metadata_fixture(0);
    for length in 0..valid.len() {
        assert!(reviewed_embedding_metadata(&valid[..length]).is_err(), "length {length}");
    }
    for (at, value) in [(0, 0x74), (4, 1), (5, 0), (5, 20), (12, b'X'), (28, 1), (29, 2)] {
        let mut bytes = valid.clone();
        bytes[at] = value;
        assert!(reviewed_embedding_metadata(&bytes).is_err(), "byte {at} value {value}");
    }
    assert!(reviewed_embedding_metadata(&vec![0; FONT_BYTES + 1]).is_err());
}

#[test]
fn hostile_directory_offsets_and_lengths_never_become_table_slices() {
    for (offset, length) in [(0, 96), (27, 96), (28, 95), (28, 97), (29, 96), (u32::MAX, 96), (28, u32::MAX)] {
        let mut bytes = metadata_fixture(0);
        bytes[20..24].copy_from_slice(&offset.to_be_bytes());
        bytes[24..28].copy_from_slice(&length.to_be_bytes());
        assert!(reviewed_embedding_metadata(&bytes).is_err(), "offset {offset}, length {length}");
    }
}

#[test]
fn duplicate_permission_tables_are_refused_instead_of_selecting_one() {
    let one = metadata_fixture(0);
    let mut bytes = vec![0; 140];
    bytes[..12].copy_from_slice(&one[..12]);
    bytes[4..6].copy_from_slice(&2_u16.to_be_bytes());
    for start in [12, 28] {
        bytes[start..start + 4].copy_from_slice(b"OS/2");
        bytes[start + 8..start + 12].copy_from_slice(&44_u32.to_be_bytes());
        bytes[start + 12..start + 16].copy_from_slice(&96_u32.to_be_bytes());
    }
    bytes[44..].copy_from_slice(&one[28..]);
    assert!(reviewed_embedding_metadata(&bytes).is_err());
}
