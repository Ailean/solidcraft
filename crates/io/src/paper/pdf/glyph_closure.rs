//! Dependency closure for already chosen TrueType glyph IDs in the one admitted font.
//!
//! No font lookup, text shaping, CID mapping, glyph rendering or font-byte output.
//! The public entry MUST pass font_admission's exact-byte/face policy first.
//! Internal parser tests use original structural fixtures, not approved fonts.
//! Specification: Microsoft OpenType glyf, loca, head, maxp and file organization.

use super::font_admission::check_liberation_sans;
use crate::{IoError, Result};

const MAX_FONT_BYTES: usize = 410_820;
const MAX_TABLES: usize = 19;
const MAX_USED: usize = 65_536;
const MAX_GLYPHS: usize = 16_384;
const MAX_COMPONENTS: usize = 65_536;
const MAX_PER_GLYPH: usize = 1_024;
const MAX_DEPTH: usize = 64;
const MAX_WORK: usize = 4 << 20;
const MAX_STATE_BYTES: usize = 128 << 10;
const MAX_CONTOURS: usize = 4_096;
const MAX_POINTS: usize = 65_536;

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(message.into())
}

fn part(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    let end = at.checked_add(len).ok_or_else(|| invalid("glyph closure offset overflow"))?;
    bytes.get(at..end).ok_or_else(|| invalid("truncated glyph closure data"))
}

fn word(bytes: &[u8], at: usize) -> Result<u16> {
    let value = part(bytes, at, 2)?.try_into().map_err(|_| invalid("invalid glyph closure word"))?;
    Ok(u16::from_be_bytes(value))
}

fn dword(bytes: &[u8], at: usize) -> Result<u32> {
    let value = part(bytes, at, 4)?.try_into().map_err(|_| invalid("invalid glyph closure dword"))?;
    Ok(u32::from_be_bytes(value))
}

fn count32(value: u32) -> Result<usize> {
    usize::try_from(value).map_err(|_| invalid("glyph closure value exceeds platform"))
}

fn charge(value: &mut usize, next: usize, limit: usize, message: &'static str) -> Result<()> {
    let sum = value.checked_add(next).filter(|sum| *sum <= limit).ok_or_else(|| invalid(message))?;
    *value = sum;
    Ok(())
}

#[derive(Clone, Copy)]
struct Limits {
    glyphs: usize,
    components: usize,
    depth: usize,
    bytes: usize,
    work: usize,
}

impl Limits {
    const MAX: Self = Self {
        glyphs: MAX_GLYPHS, components: MAX_COMPONENTS, depth: MAX_DEPTH,
        bytes: MAX_FONT_BYTES, work: MAX_WORK,
    };

    fn bounded(self) -> Self {
        Self {
            glyphs: self.glyphs.min(MAX_GLYPHS), components: self.components.min(MAX_COMPONENTS),
            depth: self.depth.min(MAX_DEPTH), bytes: self.bytes.min(MAX_FONT_BYTES),
            work: self.work.min(MAX_WORK),
        }
    }
}

struct Budget {
    limits: Limits,
    require_final_instructions: bool,
    glyphs: usize,
    components: usize,
    bytes: usize,
    work: usize,
}

impl Budget {
    fn new(limits: Limits) -> Self {
        Self { limits: limits.bounded(), require_final_instructions: false, glyphs: 0, components: 0, bytes: 0, work: 0 }
    }

    fn work(&mut self, count: usize) -> Result<()> {
        charge(&mut self.work, count, self.limits.work, "glyph closure work limit exceeded")
    }

    fn glyph(&mut self, bytes: usize) -> Result<()> {
        charge(&mut self.glyphs, 1, self.limits.glyphs, "glyph closure entry limit exceeded")?;
        charge(&mut self.bytes, bytes, self.limits.bytes, "glyph closure byte limit exceeded")?;
        self.work(bytes.checked_add(1).ok_or_else(|| invalid("glyph closure work overflow"))?)
    }

    fn component(&mut self) -> Result<()> {
        charge(&mut self.components, 1, self.limits.components, "glyph closure component limit exceeded")?;
        self.work(1)
    }
}

struct Font<'a> {
    loca: &'a [u8],
    glyf: &'a [u8],
    glyphs: usize,
    format: u16,
}

impl<'a> Font<'a> {
    fn parse(bytes: &'a [u8], budget: &mut Budget) -> Result<Self> {
        if bytes.len() > MAX_FONT_BYTES || dword(bytes, 0)? != 0x0001_0000 {
            return Err(invalid("unsupported glyph closure SFNT envelope"));
        }
        let count = usize::from(word(bytes, 4)?);
        if count == 0 || count > MAX_TABLES {
            return Err(invalid("glyph closure table count exceeds its limit"));
        }
        let directory_end = count.checked_mul(16).and_then(|n| n.checked_add(12))
            .ok_or_else(|| invalid("glyph closure directory overflow"))?;
        part(bytes, 0, directory_end)?;
        budget.work(count.checked_mul(count + 1).ok_or_else(|| invalid("glyph closure work overflow"))?)?;
        let (mut head, mut maxp, mut loca, mut glyf) = (None, None, None, None);
        let mut previous = None;
        for index in 0..count {
            // count <= 19 bounds these record offsets independently of the font.
            let record = 12 + index * 16;
            let tag = dword(bytes, record)?;
            if previous.is_some_and(|previous| tag <= previous) {
                return Err(invalid("glyph closure table tags must be unique and sorted"));
            }
            previous = Some(tag);
            let start = count32(dword(bytes, record + 8)?)?;
            let len = count32(dword(bytes, record + 12)?)?;
            if start < directory_end || start % 4 != 0 {
                return Err(invalid("invalid glyph closure table offset"));
            }
            let table = part(bytes, start, len)?;
            let end = start.checked_add(len).ok_or_else(|| invalid("glyph closure table range overflow"))?;
            for prior in 0..index {
                let prior_record = 12 + prior * 16;
                let low = count32(dword(bytes, prior_record + 8)?)?;
                let size = count32(dword(bytes, prior_record + 12)?)?;
                let high = low.checked_add(size).ok_or_else(|| invalid("glyph closure table range overflow"))?;
                if len != 0 && size != 0 && start < high && low < end {
                    return Err(invalid("overlapping glyph closure tables"));
                }
            }
            match part(bytes, record, 4)? {
                b"head" => head = Some(table),
                b"maxp" => maxp = Some(table),
                b"loca" => loca = Some(table),
                b"glyf" => glyf = Some(table),
                _ => {}
            }
        }
        let head = head.ok_or_else(|| invalid("glyph closure requires head"))?;
        let maxp = maxp.ok_or_else(|| invalid("glyph closure requires maxp"))?;
        let loca = loca.ok_or_else(|| invalid("glyph closure requires loca"))?;
        let glyf = glyf.ok_or_else(|| invalid("glyph closure requires glyf"))?;
        part(head, 0, 54)?;
        part(maxp, 0, 32)?;
        if dword(head, 0)? != 0x0001_0000 || dword(head, 12)? != 0x5f0f_3cf5 || word(head, 52)? != 0 {
            return Err(invalid("unsupported glyph closure head revision"));
        }
        if dword(maxp, 0)? != 0x0001_0000 {
            return Err(invalid("glyph closure requires TrueType maxp 1.0"));
        }
        let glyphs = usize::from(word(maxp, 4)?);
        if glyphs == 0 {
            return Err(invalid("glyph closure font has no glyph zero"));
        }
        let format = word(head, 50)?;
        let width = match format {
            0 => 2,
            1 => 4,
            _ => return Err(invalid("unsupported glyph closure loca format")),
        };
        let entries = glyphs.checked_add(1).ok_or_else(|| invalid("glyph closure loca count overflow"))?;
        let expected = entries.checked_mul(width).ok_or_else(|| invalid("glyph closure loca length overflow"))?;
        if loca.len() != expected {
            return Err(invalid("glyph closure loca length does not match numGlyphs"));
        }
        // Admit the complete loca scan before beginning it; no offset array is copied.
        budget.work(entries)?;
        let font = Self { loca, glyf, glyphs, format };
        let mut prior = 0usize;
        for index in 0..entries {
            let offset = font.offset(index)?;
            if offset < prior || offset > glyf.len() {
                return Err(invalid("glyph closure loca offsets are descending or outside glyf"));
            }
            prior = offset;
        }
        Ok(font)
    }

    fn offset(&self, index: usize) -> Result<usize> {
        let width = if self.format == 0 { 2 } else { 4 };
        let at = index.checked_mul(width).ok_or_else(|| invalid("glyph closure loca offset overflow"))?;
        if self.format == 0 {
            usize::from(word(self.loca, at)?).checked_mul(2).ok_or_else(|| invalid("glyph closure short loca overflow"))
        } else {
            count32(dword(self.loca, at)?)
        }
    }

    fn glyph(&self, gid: u16) -> Result<&'a [u8]> {
        let index = usize::from(gid);
        if index >= self.glyphs {
            return Err(invalid("glyph closure glyph ID is outside numGlyphs"));
        }
        let next = index.checked_add(1).ok_or_else(|| invalid("glyph closure glyph index overflow"))?;
        let start = self.offset(index)?;
        let end = self.offset(next)?;
        self.glyf.get(start..end).ok_or_else(|| invalid("glyph closure glyph range is invalid"))
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let value = part(self.bytes, self.at, count)?;
        self.at = self.at.checked_add(count).ok_or_else(|| invalid("glyph closure cursor overflow"))?;
        Ok(value)
    }

    fn word(&mut self) -> Result<u16> {
        let value = self.take(2)?.try_into().map_err(|_| invalid("invalid glyph closure component word"))?;
        Ok(u16::from_be_bytes(value))
    }

    fn byte(&mut self) -> Result<u8> {
        self.take(1)?.first().copied().ok_or_else(|| invalid("missing glyph closure flag"))
    }

    fn padding(&self) -> Result<()> {
        let tail = self.bytes.get(self.at..).ok_or_else(|| invalid("invalid glyph closure padding range"))?;
        if tail.iter().any(|byte| *byte != 0) {
            return Err(invalid("nonzero trailing glyph closure data"));
        }
        Ok(())
    }
}

fn simple(cursor: &mut Cursor<'_>, contours: usize, budget: &mut Budget) -> Result<()> {
    if contours > MAX_CONTOURS {
        return Err(invalid("glyph closure contour limit exceeded"));
    }
    // A zero-contour glyph can be exactly its header, with no instructions.
    if contours == 0 && cursor.at == cursor.bytes.len() {
        return Ok(());
    }
    let bytes = contours.checked_mul(2).ok_or_else(|| invalid("glyph closure contour size overflow"))?;
    let endpoints = cursor.take(bytes)?;
    budget.work(contours)?;
    let mut previous = None;
    let mut points = 0usize;
    for pair in endpoints.chunks_exact(2) {
        let end = u16::from_be_bytes(pair.try_into().map_err(|_| invalid("invalid glyph closure endpoint"))?);
        if previous.is_some_and(|previous| end <= previous) {
            return Err(invalid("glyph closure contour endpoints must increase"));
        }
        previous = Some(end);
        points = usize::from(end).checked_add(1).ok_or_else(|| invalid("glyph closure point count overflow"))?;
    }
    if points > MAX_POINTS {
        return Err(invalid("glyph closure point limit exceeded"));
    }
    let instructions = usize::from(cursor.word()?);
    cursor.take(instructions)?; // Opaque bytes: never executed or rewritten.
    let (mut seen, mut x_bytes, mut y_bytes) = (0usize, 0usize, 0usize);
    while seen < points {
        let flags = cursor.byte()?;
        if flags & 0x80 != 0 || (seen != 0 && flags & 0x40 != 0) {
            return Err(invalid("unsupported glyph closure simple flags"));
        }
        let run = if flags & 0x08 != 0 { usize::from(cursor.byte()?) + 1 } else { 1 };
        if run > points - seen {
            return Err(invalid("glyph closure repeated flags exceed point count"));
        }
        budget.work(run)?;
        let x = if flags & 0x02 != 0 { 1 } else if flags & 0x10 != 0 { 0 } else { 2 };
        let y = if flags & 0x04 != 0 { 1 } else if flags & 0x20 != 0 { 0 } else { 2 };
        let x = run.checked_mul(x).ok_or_else(|| invalid("glyph closure coordinate byte overflow"))?;
        let y = run.checked_mul(y).ok_or_else(|| invalid("glyph closure coordinate byte overflow"))?;
        x_bytes = x_bytes.checked_add(x).ok_or_else(|| invalid("glyph closure coordinate byte overflow"))?;
        y_bytes = y_bytes.checked_add(y).ok_or_else(|| invalid("glyph closure coordinate byte overflow"))?;
        seen += run; // run <= points-seen and points <= 65536.
    }
    cursor.take(x_bytes.checked_add(y_bytes).ok_or_else(|| invalid("glyph closure coordinate byte overflow"))?)?;
    cursor.padding()
}

fn children(font: &Font<'_>, bytes: &[u8], budget: &mut Budget) -> Result<Vec<u16>> {
    budget.glyph(bytes.len())?;
    if bytes.is_empty() {
        return Ok(Vec::new()); // Equal consecutive loca offsets: no outline/instructions.
    }
    let mut cursor = Cursor { bytes, at: 0 };
    let contours = cursor.word()? as i16;
    cursor.take(8)?; // Four signed bounding-box coordinates, not evaluated as geometry.
    if contours >= 0 {
        simple(&mut cursor, contours as usize, budget)?;
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut instructions = false;
    loop {
        if out.len() >= MAX_PER_GLYPH {
            return Err(invalid("glyph closure per-glyph component limit exceeded"));
        }
        budget.component()?; // Before copying or reserving a component entry.
        let flags = cursor.word()?;
        let child = cursor.word()?;
        if flags & 0xe010 != 0 || (flags & 0x00c8).count_ones() > 1 || flags & 0x1800 == 0x1800 {
            return Err(invalid("unsupported glyph closure composite flags"));
        }
        if (out.is_empty() && flags & 0x0002 == 0) || (!out.is_empty() && flags & 0x0400 != 0) {
            return Err(invalid("invalid glyph closure first/overlap component flags"));
        }
        if usize::from(child) >= font.glyphs {
            return Err(invalid("glyph closure component ID is outside numGlyphs"));
        }
        cursor.take(if flags & 0x0001 != 0 { 4 } else { 2 })?;
        let transform = if flags & 0x0008 != 0 { 2 } else if flags & 0x0040 != 0 { 4 } else if flags & 0x0080 != 0 { 8 } else { 0 };
        cursor.take(transform)?;
        out.try_reserve(1).map_err(|_| invalid("glyph closure component allocation failed"))?;
        out.push(child);
        instructions |= flags & 0x0100 != 0; // Any component may set this flag.
        if flags & 0x0020 == 0 {
            if budget.require_final_instructions && instructions && flags & 0x0100 == 0 {
                return Err(invalid("subsetter 0.2.6 cannot preserve nonfinal-only composite instructions"));
            }
            break;
        }
    }
    if instructions {
        let length = usize::from(cursor.word()?);
        cursor.take(length)?;
    }
    cursor.padding()?;
    Ok(out)
}

struct Frame {
    gid: u16,
    children: Vec<u16>,
    next: usize,
    height: usize,
}

#[derive(Clone, Copy)]
struct State {
    colour: u8,
    height: u8,
}

fn include_child_height(frame: &mut Frame, child: usize, limit: usize) -> Result<()> {
    let height = child.checked_add(1).filter(|height| *height <= limit)
        .ok_or_else(|| invalid("glyph closure depth limit exceeded"))?;
    frame.height = frame.height.max(height);
    Ok(())
}

fn enter(font: &Font<'_>, gid: u16, states: &mut [State], stack: &mut Vec<Frame>, budget: &mut Budget) -> Result<()> {
    if stack.len() >= budget.limits.depth {
        return Err(invalid("glyph closure depth limit exceeded"));
    }
    let children = children(font, font.glyph(gid)?, budget)?;
    let state = states.get_mut(usize::from(gid)).ok_or_else(|| invalid("invalid glyph closure state index"))?;
    if state.colour != 0 {
        return Err(invalid("glyph closure entered a nonwhite node"));
    }
    stack.try_reserve(1).map_err(|_| invalid("glyph closure stack allocation failed"))?;
    state.colour = 1;
    stack.push(Frame { gid, children, next: 0, height: 1 });
    Ok(())
}

fn collect(font_bytes: &[u8], used: &[u16], limits: Limits) -> Result<Vec<u16>> {
    if used.len() > MAX_USED {
        return Err(invalid("glyph closure input ID count exceeds its limit"));
    }
    let mut budget = Budget::new(limits);
    let font = Font::parse(font_bytes, &mut budget)?;
    budget.work(used.len().checked_add(font.glyphs).ok_or_else(|| invalid("glyph closure work overflow"))?)?;
    for gid in used {
        if usize::from(*gid) >= font.glyphs {
            return Err(invalid("glyph closure input ID is outside numGlyphs"));
        }
    }
    if font.glyphs.checked_mul(std::mem::size_of::<State>()).is_none_or(|bytes| bytes > MAX_STATE_BYTES) {
        return Err(invalid("glyph closure state byte limit exceeded"));
    }
    let mut states = Vec::new();
    states.try_reserve_exact(font.glyphs).map_err(|_| invalid("glyph closure state allocation failed"))?;
    states.resize(font.glyphs, State { colour: 0, height: 0 });
    let mut stack = Vec::new();
    for root in std::iter::once(0u16).chain(used.iter().copied()) {
        if states.get(usize::from(root)).is_some_and(|state| state.colour == 2) {
            continue;
        }
        enter(&font, root, &mut states, &mut stack, &mut budget)?;
        while !stack.is_empty() {
            let next = {
                let frame = stack.last_mut().ok_or_else(|| invalid("missing glyph closure frame"))?;
                match frame.children.get(frame.next).copied() {
                    Some(child) => {
                        frame.next += 1; // children.len <= 1024.
                        Some(child)
                    }
                    None => None,
                }
            };
            if let Some(child) = next {
                match states.get(usize::from(child)).copied() {
                    Some(State { colour: 0, .. }) => enter(&font, child, &mut states, &mut stack, &mut budget)?,
                    Some(State { colour: 1, .. }) => return Err(invalid("cyclic glyph closure")),
                    Some(State { colour: 2, height }) => {
                        let frame = stack.last_mut().ok_or_else(|| invalid("missing glyph closure parent frame"))?;
                        include_child_height(frame, usize::from(height), budget.limits.depth)?;
                    }
                    _ => return Err(invalid("invalid glyph closure child state")),
                }
            } else {
                let frame = stack.pop().ok_or_else(|| invalid("missing glyph closure closing frame"))?;
                let state = states.get_mut(usize::from(frame.gid)).ok_or_else(|| invalid("invalid glyph closure closing state"))?;
                state.height = u8::try_from(frame.height).map_err(|_| invalid("glyph closure cached height exceeds its bound"))?;
                state.colour = 2;
                if let Some(parent) = stack.last_mut() {
                    include_child_height(parent, frame.height, budget.limits.depth)?;
                }
            }
        }
    }
    let mut out = Vec::new();
    out.try_reserve_exact(budget.glyphs).map_err(|_| invalid("glyph closure result allocation failed"))?;
    for (index, state) in states.into_iter().enumerate() {
        if state.colour == 2 {
            out.push(u16::try_from(index).map_err(|_| invalid("glyph closure result index exceeds u16"))?);
        } else if state.colour != 0 {
            return Err(invalid("glyph closure contains an unfinished node"));
        }
    }
    Ok(out)
}

/// Sorted unique GIDs needed by used, including glyph zero and recursive components.
/// Exact bundled identity/face and the existing metadata guard are mandatory.
/// The caller must obtain used IDs from these SAME font bytes and face.
/// A numerically valid GID cannot establish its source-font identity; this
/// helper checks ranges, not shaping provenance or text-to-glyph correctness.
/// Returned IDs are not remapped and no font bytes, metrics or hint program is emitted.
pub(super) fn glyph_closure(font: &[u8], face_index: u32, used: &[u16]) -> Result<Vec<u16>> {
    check_liberation_sans(font, face_index)?;
    collect(font, used, Limits::MAX)
}

/// Temporary subsetter-0.2.6 policy, not general TrueType validity.
/// Caller MUST first obtain closure from glyph_closure on these same bytes/face.
/// Reuses the original bounded parser; never duplicates font admission or hashing.
pub(super) fn check_subsetter_component_instruction_flags(font: &[u8], closure: &[u16]) -> Result<()> {
    if closure.len() > MAX_GLYPHS {
        return Err(invalid("subsetter preflight glyph count exceeds its limit"));
    }
    let mut budget = Budget::new(Limits::MAX);
    budget.require_final_instructions = true;
    let parsed = Font::parse(font, &mut budget)?;
    for gid in closure {
        children(&parsed, parsed.glyph(*gid)?, &mut budget)?;
    }
    Ok(())
}

/// Structural read-back of a PDF-only subset, not admission as an input font.
/// Reuses the same parser without applying the original-font identity guard.
pub(super) fn check_subset_program(bytes: &[u8], glyphs: usize) -> Result<()> {
    if glyphs == 0 || glyphs > MAX_GLYPHS {
        return Err(invalid("invalid subset glyph count"));
    }
    let mut budget = Budget::new(Limits::MAX);
    let parsed = Font::parse(bytes, &mut budget)?;
    if parsed.glyphs != glyphs {
        return Err(invalid("subset glyph count does not match its mapping"));
    }
    // Font::parse already checked this directory's bounds, uniqueness and ranges.
    // Eleven tables are emitted for this fixed hinted font by subsetter 0.2.6.
    let required = [*b"cvt ", *b"fpgm", *b"glyf", *b"head", *b"hhea", *b"hmtx", *b"loca", *b"maxp", *b"name", *b"post", *b"prep"];
    let mut present = 0u16;
    let (mut hhea, mut hmtx) = (None, None);
    for index in 0..usize::from(word(bytes, 4)?) {
        let at = 12 + index * 16; // Parsed table count <= 19.
        let tag = part(bytes, at, 4)?;
        if tag == b"cmap" {
            return Err(invalid("a FontFile2 CID subset must not contain cmap"));
        }
        if let Some(bit) = required.iter().position(|required| required.as_slice() == tag) {
            present |= 1u16 << bit; // Eleven fixed entries; shift is bounded.
        }
        let table = part(bytes, count32(dword(bytes, at + 8)?)?, count32(dword(bytes, at + 12)?)?)?;
        match tag {
            b"hhea" => hhea = Some(table),
            b"hmtx" => hmtx = Some(table),
            _ => {}
        }
    }
    if present != (1u16 << required.len()) - 1 {
        return Err(invalid("subset is missing a required fixed-font table"));
    }
    let hhea = hhea.ok_or_else(|| invalid("subset requires hhea"))?;
    part(hhea, 0, 36)?;
    let metrics = usize::from(word(hhea, 34)?);
    if metrics == 0 || metrics > glyphs {
        return Err(invalid("subset numberOfHMetrics is outside its glyph count"));
    }
    let expected = metrics.checked_mul(4)
        .and_then(|full| (glyphs - metrics).checked_mul(2).and_then(|tail| full.checked_add(tail)))
        .ok_or_else(|| invalid("subset hmtx length overflow"))?;
    if hmtx.ok_or_else(|| invalid("subset requires hmtx"))?.len() != expected {
        return Err(invalid("subset hmtx length does not match numberOfHMetrics"));
    }
    for gid in 0..glyphs {
        let gid = u16::try_from(gid).map_err(|_| invalid("subset glyph ID exceeds u16"))?;
        children(&parsed, parsed.glyph(gid)?, &mut budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/liberation-sans/LiberationSans-Regular.ttf"));

    fn put16(bytes: &mut [u8], at: usize, value: u16) {
        bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn put32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    // Original structural SFNT fixtures for the PRIVATE parser, not fonts admitted
    // by glyph_closure. No names, cmap, outlines or third-party font program copied.
    fn fixture(glyphs: &[Vec<u8>], short: bool) -> Vec<u8> {
        let mut glyf = Vec::new();
        let mut offsets = vec![0u32];
        for glyph in glyphs {
            glyf.extend_from_slice(glyph);
            if glyf.len() % 2 != 0 { glyf.push(0); }
            offsets.push(u32::try_from(glyf.len()).unwrap());
        }
        let mut loca = Vec::new();
        for offset in offsets {
            if short {
                loca.extend_from_slice(&u16::try_from(offset / 2).unwrap().to_be_bytes());
            } else {
                loca.extend_from_slice(&offset.to_be_bytes());
            }
        }
        let mut head = vec![0; 54];
        put32(&mut head, 0, 0x0001_0000);
        put32(&mut head, 12, 0x5f0f_3cf5);
        put16(&mut head, 50, if short { 0 } else { 1 });
        let mut maxp = vec![0; 32];
        put32(&mut maxp, 0, 0x0001_0000);
        put16(&mut maxp, 4, u16::try_from(glyphs.len()).unwrap());
        let tables = [(*b"glyf", glyf), (*b"head", head), (*b"loca", loca), (*b"maxp", maxp)];
        let mut out = vec![0; 12 + 4 * 16];
        put32(&mut out, 0, 0x0001_0000);
        put16(&mut out, 4, 4);
        for (index, (tag, data)) in tables.into_iter().enumerate() {
            while out.len() % 4 != 0 { out.push(0); }
            let start = u32::try_from(out.len()).unwrap();
            let len = u32::try_from(data.len()).unwrap();
            let record = 12 + index * 16;
            out[record..record + 4].copy_from_slice(&tag);
            put32(&mut out, record + 8, start);
            put32(&mut out, record + 12, len);
            out.extend_from_slice(&data);
        }
        out
    }

    fn simple_glyph() -> Vec<u8> {
        let mut out = vec![0; 10];
        put16(&mut out, 0, 1);
        // One contour ending at point0, no instructions, one on-curve point
        // whose x/y equal their previous value, then zero padding.
        out.extend_from_slice(&[0, 0, 0, 0, 0x31, 0]);
        out
    }

    fn composite(records: &[(u16, u16)], instructions: Option<&[u8]>) -> Vec<u8> {
        let mut out = vec![0; 10];
        put16(&mut out, 0, 0xffff);
        for (index, &(flags, gid)) in records.iter().enumerate() {
            let flags = flags | if index + 1 < records.len() { 0x0020 } else { 0 };
            out.extend_from_slice(&flags.to_be_bytes());
            out.extend_from_slice(&gid.to_be_bytes());
            out.extend(std::iter::repeat_n(0, if flags & 1 != 0 { 4 } else { 2 }));
            let transform = if flags & 0x0008 != 0 { 2 } else if flags & 0x0040 != 0 { 4 } else if flags & 0x0080 != 0 { 8 } else { 0 };
            out.extend(std::iter::repeat_n(0, transform));
        }
        if let Some(bytes) = instructions {
            out.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    fn graph() -> Vec<Vec<u8>> {
        vec![Vec::new(), simple_glyph(), composite(&[(3, 1)], None), composite(&[(3, 2), (3, 1)], None)]
    }

    fn record(bytes: &[u8], tag: &[u8; 4]) -> usize {
        (0..usize::from(word(bytes, 4).unwrap())).map(|index| 12 + index * 16)
            .find(|at| &bytes[*at..*at + 4] == tag).unwrap()
    }

    fn table_start(bytes: &[u8], tag: &[u8; 4]) -> usize {
        usize::try_from(dword(bytes, record(bytes, tag) + 8).unwrap()).unwrap()
    }

    #[test]
    fn approved_resource_simple_case_is_derived_from_fixed_byte_metadata() {
        // SHA075e71..., maxp.numGlyphs2620; long loca[0..2]=[0,76,76].
        // glyph0 has two simple contours; glyph1 has no bytes/components.
        assert_eq!(glyph_closure(BUNDLED, 0, &[1, 1, 0]).unwrap(), [0, 1]);
        assert!(glyph_closure(BUNDLED, 1, &[0]).is_err());
        assert!(glyph_closure(BUNDLED, u32::MAX, &[0]).is_err());
        assert!(glyph_closure(BUNDLED, 0, &[2620]).is_err());
        let mut changed = BUNDLED.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        assert!(glyph_closure(&changed, 0, &[0]).is_err());
        assert!(glyph_closure(&fixture(&[simple_glyph()], false), 0, &[0]).is_err(), "synthetic parser fixtures are not admitted fonts");
    }

    #[test]
    fn literal_nested_graph_closure_is_sorted_unique_and_includes_zero() {
        for short in [false, true] {
            let bytes = fixture(&graph(), short);
            assert_eq!(collect(&bytes, &[3, 3, 2], Limits::MAX).unwrap(), [0, 1, 2, 3]);
            assert_eq!(collect(&bytes, &[1, 3], Limits::MAX).unwrap(), [0, 1, 2, 3]);
            assert_eq!(collect(&bytes, &[], Limits::MAX).unwrap(), [0]);
        }
    }

    #[test]
    fn cycles_refuse_but_shared_finished_components_are_not_cycles() {
        for glyphs in [
            vec![simple_glyph(), composite(&[(3, 1)], None)],
            vec![simple_glyph(), composite(&[(3, 2)], None), composite(&[(3, 1)], None)],
        ] {
            assert!(collect(&fixture(&glyphs, false), &[1], Limits::MAX).unwrap_err().to_string().contains("cyclic"));
        }
        assert_eq!(collect(&fixture(&graph(), false), &[3], Limits::MAX).unwrap(), [0, 1, 2, 3]);
    }

    #[test]
    fn component_argument_and_transform_widths_and_any_instruction_flag_are_read() {
        for flags in [2, 3, 3 | 8, 3 | 0x40, 3 | 0x80] {
            let bytes = fixture(&[simple_glyph(), composite(&[(flags, 0)], None)], false);
            assert_eq!(collect(&bytes, &[1], Limits::MAX).unwrap(), [0, 1]);
        }
        // The FIRST component sets instructions, not the last. Both must be read.
        let bytes = fixture(&[simple_glyph(), composite(&[(3 | 0x100, 0), (3, 0)], Some(&[0, 1, 2]))], false);
        assert_eq!(collect(&bytes, &[1], Limits::MAX).unwrap(), [0, 1]);
        // Negative contour counts denote composites; -1 is recommended, not mandatory.
        let mut negative = composite(&[(3, 0)], None);
        put16(&mut negative, 0, 0xfffe);
        assert_eq!(collect(&fixture(&[simple_glyph(), negative], false), &[1], Limits::MAX).unwrap(), [0, 1]);
    }

    #[test]
    fn malformed_flags_component_ids_and_instruction_ranges_are_not_repaired() {
        for flags in [3 | 0x10, 3 | 0x2000, 3 | 8 | 0x40, 3 | 0x1800, 1] {
            let bytes = fixture(&[simple_glyph(), composite(&[(flags, 0)], None)], false);
            assert!(collect(&bytes, &[1], Limits::MAX).is_err(), "flags{flags:x}");
        }
        for bad in [
            composite(&[(3, 2)], None), // Two glyphs only: gid2 does not exist.
            composite(&[(3 | 0x20, 0)], None), // MORE without another record.
            composite(&[(3 | 0x100, 0)], None), // No instruction length.
            vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0], // Partial record.
        ] {
            assert!(collect(&fixture(&[simple_glyph(), bad], false), &[1], Limits::MAX).is_err());
        }
        let mut instructions = composite(&[(3 | 0x100, 0)], Some(&[1]));
        put16(&mut instructions, 18, 5); // At most two bytes available after padding.
        assert!(collect(&fixture(&[simple_glyph(), instructions], false), &[1], Limits::MAX).is_err());
        let mut trailing = composite(&[(3, 0)], None);
        trailing.push(0xff);
        assert!(collect(&fixture(&[simple_glyph(), trailing], false), &[1], Limits::MAX).is_err());
    }

    #[test]
    fn simple_records_require_complete_flags_coordinates_and_instruction_payload() {
        let mut bad_reserved = simple_glyph();
        bad_reserved[14] |= 0x80;
        let mut bad_repeat = simple_glyph();
        bad_repeat[14] = 0x39;
        bad_repeat[15] = 1; // Two repetitions for one point.
        let mut bad_coordinates = simple_glyph();
        bad_coordinates[14] = 1; // Needs four coordinate bytes, absent here.
        let mut bad_instructions = simple_glyph();
        put16(&mut bad_instructions, 12, 20);
        // The fixture pads odd lengths to even; eight bytes remain truncated.
        for bad in [vec![0; 8], bad_reserved, bad_repeat, bad_coordinates, bad_instructions] {
            assert!(collect(&fixture(&[bad], false), &[], Limits::MAX).is_err());
        }
        assert_eq!(collect(&fixture(&[vec![0; 10]], false), &[], Limits::MAX).unwrap(), [0], "header-only zero contour");
    }

    #[test]
    fn missing_duplicate_overlapping_and_out_of_payload_tables_refuse() {
        let original = fixture(&[simple_glyph()], false);
        for (tag, replacement) in [(*b"glyf", *b"glya"), (*b"head", *b"heae"), (*b"loca", *b"loaa"), (*b"maxp", *b"maxo")] {
            let mut bad = original.clone();
            let at = record(&bad, &tag);
            bad[at..at + 4].copy_from_slice(&replacement);
            assert!(collect(&bad, &[], Limits::MAX).is_err());
        }
        let mut duplicate = original.clone();
        duplicate[28..32].copy_from_slice(b"glyf");
        assert!(collect(&duplicate, &[], Limits::MAX).is_err());
        let mut overlap = original.clone();
        let start = table_start(&overlap, b"glyf");
        let r = record(&overlap, b"head");
        put32(&mut overlap, r + 8, start as u32);
        assert!(collect(&overlap, &[], Limits::MAX).is_err());
        let mut outside = original.clone();
        put32(&mut outside, 20, u32::MAX);
        assert!(collect(&outside, &[], Limits::MAX).is_err());
        for end in 0..76 {
            assert!(collect(&original[..end], &[], Limits::MAX).is_err());
        }
    }

    #[test]
    fn loca_format_length_order_and_bounds_are_checked() {
        let original = fixture(&[simple_glyph(), simple_glyph()], false);
        let mut format = original.clone();
        let h = table_start(&format, b"head");
        put16(&mut format, h + 50, 2);
        assert!(collect(&format, &[], Limits::MAX).is_err());
        let mut length = original.clone();
        let r = record(&length, b"loca");
        put32(&mut length, r + 12, 8);
        assert!(collect(&length, &[], Limits::MAX).is_err());
        let mut descending = original.clone();
        let l = table_start(&descending, b"loca");
        put32(&mut descending, l + 4, 32);
        put32(&mut descending, l + 8, 16);
        assert!(collect(&descending, &[], Limits::MAX).is_err());
        let mut outside = original.clone();
        put32(&mut outside, l + 8, u32::MAX);
        assert!(collect(&outside, &[], Limits::MAX).is_err());
        assert!(collect(&original, &[u16::MAX], Limits::MAX).is_err());
        assert!(collect(&fixture(&[], true), &[], Limits::MAX).is_err());
    }

    #[test]
    fn table_versions_required_headers_and_directory_caps_refuse_explicitly() {
        let original = fixture(&[simple_glyph()], false);
        for (tag, offset, value) in [(*b"head", 0, 0), (*b"head", 12, 0), (*b"maxp", 0, 0x0000_5000)] {
            let mut bad = original.clone();
            let start = table_start(&bad, &tag);
            put32(&mut bad, start + offset, value);
            assert!(collect(&bad, &[], Limits::MAX).is_err());
        }
        let mut format = original.clone();
        let start = table_start(&format, b"head");
        put16(&mut format, start + 52, 1);
        assert!(collect(&format, &[], Limits::MAX).is_err());
        for (tag, len) in [(*b"head", 53), (*b"maxp", 31)] {
            let mut bad = original.clone();
            let r = record(&bad, &tag);
            put32(&mut bad, r + 12, len);
            assert!(collect(&bad, &[], Limits::MAX).is_err());
        }
        for count in [0, 20, u16::MAX] {
            let mut bad = original.clone();
            put16(&mut bad, 4, count);
            assert!(collect(&bad, &[], Limits::MAX).is_err());
        }
    }

    #[test]
    fn closure_caps_are_inclusive_and_refusal_exposes_no_partial_result() {
        let glyphs = graph();
        let bytes = fixture(&glyphs, false);
        let exact = Limits { glyphs: 4, components: 3, depth: 3, bytes: 60, work: MAX_WORK };
        assert_eq!(collect(&bytes, &[3], exact).unwrap(), [0, 1, 2, 3]);
        for limits in [
            Limits { glyphs: 3, ..exact }, Limits { components: 2, ..exact },
            Limits { depth: 2, ..exact }, Limits { bytes: 59, ..exact },
            Limits { work: 0, ..exact },
        ] {
            assert!(collect(&bytes, &[3], limits).is_err());
        }
        // Cached finished nodes must not hide a long path when used IDs are
        // supplied in leaf-first order. Admission is independent of that order.
        assert!(collect(&bytes, &[1, 2, 3], Limits { depth: 2, ..exact }).is_err());
        assert_eq!(collect(&bytes, &[1, 2, 3], exact).unwrap(), [0, 1, 2, 3]);
        assert!(collect(&bytes, &vec![0; MAX_USED + 1], Limits::MAX).is_err());
        assert_eq!(glyphs, graph(), "borrowed fixture source unchanged");
        let mut count = usize::MAX;
        assert!(charge(&mut count, 1, usize::MAX, "overflow").is_err());
        assert_eq!(count, usize::MAX);
    }

    #[test]
    fn per_glyph_component_limit_is_inclusive_even_for_repeated_child_ids() {
        let records = vec![(3, 0); MAX_PER_GLYPH];
        assert_eq!(collect(&fixture(&[simple_glyph(), composite(&records, None)], false), &[1], Limits::MAX).unwrap(), [0, 1]);
        let too_many = vec![(3, 0); MAX_PER_GLYPH + 1];
        let bytes = fixture(&[simple_glyph(), composite(&too_many, None)], false);
        assert!(collect(&bytes, &[1], Limits::MAX).unwrap_err().to_string().contains("per-glyph component limit"));
    }

    #[test]
    fn temporary_subsetter_policy_does_not_redefine_general_closure() {
        let bytes = fixture(&[simple_glyph(), composite(&[(3 | 0x100, 0), (3, 0)], Some(&[0, 1, 2]))], false);
        assert_eq!(collect(&bytes, &[1], Limits::MAX).unwrap(), [0, 1]);
        assert!(check_subsetter_component_instruction_flags(&bytes, &[0, 1]).unwrap_err().to_string().contains("nonfinal-only"));
        let last = fixture(&[simple_glyph(), composite(&[(3, 0), (3 | 0x100, 0)], Some(&[0, 1, 2]))], false);
        assert!(check_subsetter_component_instruction_flags(&last, &[0, 1]).is_ok());
        let both = fixture(&[simple_glyph(), composite(&[(3 | 0x100, 0), (3 | 0x100, 0)], Some(&[0, 1, 2]))], false);
        assert!(check_subsetter_component_instruction_flags(&both, &[0, 1]).is_ok());
        assert!(check_subsetter_component_instruction_flags(&bytes, &vec![0; MAX_GLYPHS + 1]).is_err());
    }
}
