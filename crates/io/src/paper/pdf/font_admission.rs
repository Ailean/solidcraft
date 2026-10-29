//! Admission of the one reviewed Liberation Sans resource for future PDF use.
//! This checks resource identity and embedding metadata only, not glyph layout,
//! font validity in general, licence compliance of a subset, or a B1 record.
//! OS/2 reference: https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype
use sha2::{Digest, Sha256};
use crate::{IoError, Result};

const FONT_BYTES: usize = 410_820;
const MAX_TABLES: usize = 19;
const FONT_HASH: [u8; 32] = [0x07, 0x5e, 0x71, 0xbe, 0x4c, 0x58, 0xbe, 0xda, 0x17, 0x2e, 0xcb, 0x57, 0x4a, 0x79, 0x93, 0x08, 0xf0, 0x4c, 0x3d, 0x05, 0x4c, 0x6a, 0x5b, 0x74, 0x34, 0x95, 0x4d, 0x3b, 0x0f, 0x1b, 0x28, 0xd8];

/// Only the reviewed byte revision, face zero, and its installable embedding
/// flags are admitted. No name/path lookup, fallback, filesystem or cache access.
/// Nonzero flags are outside this single-resource policy, not a declaration
/// that every other OpenType embedding mode is prohibited by the specification.
pub(super) fn check_liberation_sans(bytes: &[u8], face_index: u32) -> Result<()> {
    if face_index != 0 || bytes.len() != FONT_BYTES {
        return Err(invalid("unapproved bundled font identity"));
    }
    let hash: [u8; 32] = Sha256::digest(bytes).into();
    if hash != FONT_HASH {
        return Err(invalid("unapproved bundled font identity"));
    }
    reviewed_embedding_metadata(bytes)
}

fn invalid(message: &str) -> IoError {
    IoError::Invalid(format!("PDF bundled font: {message}"))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let end = at.checked_add(2).ok_or_else(|| invalid("table range overflow"))?;
    let pair = bytes.get(at..end).and_then(|s| s.try_into().ok()).ok_or_else(|| invalid("truncated table"))?;
    Ok(u16::from_be_bytes(pair))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).ok_or_else(|| invalid("table range overflow"))?;
    let word = bytes.get(at..end).and_then(|s| s.try_into().ok()).ok_or_else(|| invalid("truncated table"))?;
    Ok(u32::from_be_bytes(word))
}

// Bounded metadata reader, also tested independently so digest refusal cannot
// hide malformed-range or permission-check defects. No font data is modified.
fn reviewed_embedding_metadata(bytes: &[u8]) -> Result<()> {
    if bytes.len() > FONT_BYTES || bytes.get(..4) != Some(&[0, 1, 0, 0]) {
        return Err(invalid("unsupported font container"));
    }
    let count = usize::from(u16_at(bytes, 4)?);
    if count == 0 || count > MAX_TABLES {
        return Err(invalid("table count exceeds reviewed envelope"));
    }
    let end = count.checked_mul(16).and_then(|n| n.checked_add(12)).ok_or_else(|| invalid("table range overflow"))?;
    if bytes.get(..end).is_none() {
        return Err(invalid("truncated table directory"));
    }
    let mut os2 = None;
    for i in 0..count {
        // count <= 19 proves these directory offsets cannot overflow.
        let record = 12 + 16 * i;
        let start = usize::try_from(u32_at(bytes, record + 8)?).map_err(|_| invalid("table offset exceeds platform"))?;
        let length = usize::try_from(u32_at(bytes, record + 12)?).map_err(|_| invalid("table length exceeds platform"))?;
        let stop = start.checked_add(length).ok_or_else(|| invalid("table range overflow"))?;
        if start < end || bytes.get(start..stop).is_none() {
            return Err(invalid("table outside font payload"));
        }
        if bytes.get(record..record + 4) == Some(b"OS/2") {
            if os2.is_some() {
                return Err(invalid("duplicate OS/2 table"));
            }
            os2 = bytes.get(start..stop);
        }
    }
    let table = os2.ok_or_else(|| invalid("missing OS/2 table"))?;
    // The current approved resource has OS/2 v3, length 96 and fsType zero.
    // Legacy flag interpretations and other revisions are not admitted here.
    if table.len() != 96 || u16_at(table, 0)? != 3 {
        return Err(invalid("unsupported OS/2 revision"));
    }
    if u16_at(table, 8)? != 0 {
        return Err(invalid("embedding flags outside reviewed policy"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "font_admission_tests.rs"]
mod tests;
