//! Private PDF font resources for one admitted subset; no B1 or content writer.
//!
//! Identity-H explicitly maps two-byte codes to CIDs. A separate stream maps
//! CIDs to subset GIDs; logical text remains keyed by code in real ToUnicode.
//! W widths come from actual B1 original-GID advances, never subset hmtx.
//! Every declared emitted CID has an explicit W width; no DW fallback is used.
//! Descriptor metrics must come from the SAME admitted source font. Numeric
//! validation does not prove that provenance, and no metrics are estimated.
//!
//! pdf-writer 0.15.0 allocates internally without fallible allocation controls.
//! This returns a local Chunk, never mutating a caller-owned Pdf on Result errors.
//! A later extend has no rollback or OOM guarantee. Caller owns global Ref/tag
//! reservations, page resources, content, positioning and document budgets.

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Chunk, Name, Rect, Ref, Str};

use solidcraft_geom::paper::text::Font;

use super::subset_font::SubsetFont;
use super::text_plan::{FontPlanView, check_builtin_source};
use super::to_unicode::write_to_unicode;
use crate::{IoError, MAX_DRAWING_BYTES, Result};

const MAX_REFS_ID: i32 = 100_000;
const MAX_CODES: usize = 65_536;
const MAX_GLYPHS: usize = 16_384;
const MAX_FONT_BYTES: usize = 410_820;
const MAX_MAP_BYTES: usize = 131_072;
const MAX_UTF8_BYTES: usize = 4 << 20;
const MAX_CLUSTER_BYTES: usize = 768;
const MAX_CHUNK_BYTES: usize = 16 << 20;
const MAX_METRIC: f32 = 1_000_000.0;
const ROW_BYTES: usize = 128;
const FIXED_BYTES: usize = 32 << 10;
const POSTSCRIPT_NAME: &[u8] = b"LiberationSans";

/// Six already reserved real references. Local validation cannot reserve them
/// globally or discover collisions with future P3/font objects.
pub(super) struct FontObjectRefs {
    pub(super) type0: Ref,
    pub(super) cid_font: Ref,
    pub(super) descriptor: Ref,
    pub(super) font_file2: Ref,
    pub(super) to_unicode: Ref,
    pub(super) cid_to_gid: Ref,
}

impl FontObjectRefs {
    fn all(&self) -> [Ref; 6] {
        [self.type0, self.cid_font, self.descriptor, self.font_file2, self.to_unicode, self.cid_to_gid]
    }

    fn validate(&self) -> Result<()> {
        let ids = self.all();
        for (index, id) in ids.iter().enumerate() {
            if !(6..=MAX_REFS_ID).contains(&id.get()) || ids.iter().take(index).any(|prior| prior == id) {
                return Err(invalid("font object references must be distinct, outside core 1..5, and at most 100000"));
            }
        }
        Ok(())
    }
}

/// Reserve the next six font objects from the ACTUAL many-page parent Refs.
/// This is a private pure seam; it neither writes a Pdf nor invents a parallel
/// document allocator, subset tag or page-resource entry. The parent retains
/// its catalog/pages/Info/layers window and calls this after its reservations.
/// Failed local validation/overflow leaves the parent's cursor unchanged.
pub(super) fn reserve_font_refs(refs: &mut super::Refs) -> Result<FontObjectRefs> {
    if refs.0 <= 0 {
        return Err(invalid("font object allocator must start at a positive object number"));
    }
    // Refs has no Clone API at this pinned revision. A local staging cursor of
    // that SAME real type makes the six-number reservation all-or-nothing.
    // No fabricated Ref::new arithmetic duplicates the parent's next() policy.
    let mut staged = super::Refs(refs.0);
    let reserved = FontObjectRefs {
        type0: staged.next()?,
        cid_font: staged.next()?,
        descriptor: staged.next()?,
        font_file2: staged.next()?,
        to_unicode: staged.next()?,
        cid_to_gid: staged.next()?,
    };
    // Reuse this helper's existing distinct/core/range policy, rather than
    // duplicating its 100000 cap in a second allocator or skipping it.
    reserved.validate()?;
    *refs = staged;
    Ok(reserved)
}

/// A caller-assigned two-byte code, interpreted as a CID by Identity-H.
/// Neither code nor logical text is inferred from subset_gid.
pub(super) struct CodeToGlyph<'a> {
    pub(super) code: u16,
    pub(super) subset_gid: u16,
    pub(super) logical: &'a str,
}

/// Explicit PDF descriptor values, not an existing B1/provider record.
/// Lengths use 1000-unit glyph space; italic angle uses degrees.
/// Caller supplies faithful source metrics; no StemV/ascent defaults exist here.
pub(super) struct DescriptorMetrics {
    pub(super) bbox_1000: [f32; 4],
    pub(super) ascent_1000: f32,
    pub(super) descent_1000: f32,
    pub(super) cap_height_1000: f32,
    pub(super) italic_angle_degrees: f32,
    pub(super) stem_v_1000: f32,
    pub(super) flags: u32,
}

impl DescriptorMetrics {
    fn validate(&self) -> Result<FontFlags> {
        let numbers = [
            self.ascent_1000, self.descent_1000, self.cap_height_1000,
            self.italic_angle_degrees, self.stem_v_1000,
        ];
        if self.bbox_1000.iter().chain(numbers.iter()).any(|n| {
            !n.is_finite() || n.abs() > MAX_METRIC || (*n != 0.0 && n.abs() < f32::MIN_POSITIVE)
        }) {
            return Err(invalid("font descriptor numbers must be finite and bounded"));
        }
        let [left, bottom, right, top] = self.bbox_1000;
        if left >= right || bottom >= top || self.ascent_1000 <= 0.0
            || self.descent_1000 >= 0.0 || self.cap_height_1000 <= 0.0
            || self.cap_height_1000 > self.ascent_1000 || self.stem_v_1000 <= 0.0
            || self.italic_angle_degrees.abs() > 90.0
        {
            return Err(invalid("font descriptor has invalid bounds, signs, cap height or stem"));
        }
        let flags = FontFlags::from_bits(self.flags).ok_or_else(|| invalid("font descriptor contains reserved flags"))?;
        let symbolic = flags.contains(FontFlags::SYMBOLIC);
        let nonsymbolic = flags.contains(FontFlags::NON_SYMBOLIC);
        if symbolic == nonsymbolic {
            return Err(invalid("font descriptor must select exactly one Symbolic or Nonsymbolic flag"));
        }
        Ok(flags)
    }
}

/// Production parameters: the source, subset, descriptor and codes are bound
/// by an opaque view obtainable only from a successful TextPlan.
pub(super) struct FontObjectInput<'a> {
    pub(super) source: FontPlanView<'a>,
    pub(super) subset_tag: [u8; 6],
    pub(super) max_chunk_bytes: usize,
}

/// Internal preparation components, never an independently callable production
/// input. Component tests below retain their synthetic scope via private aliases.
struct FontObjectParts<'a> {
    source_font: &'a Font,
    subset: &'a SubsetFont,
    codes: &'a [CodeToGlyph<'a>],
    descriptor: Option<&'a DescriptorMetrics>,
    postscript_name: &'a [u8],
    subset_tag: [u8; 6],
    max_chunk_bytes: usize,
}

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(message.into())
}

/// One identity-bound B1 width provider over an immutable, already validated
/// subset artifact. No font tables are read here: original FontGlyph advances
/// are the sole W authority; subset IDs are joined back to ORIGINAL GIDs.
struct B1Widths<'a> {
    font: &'a Font,
    subset: &'a SubsetFont,
}

// Identity of the exact already admitted regular resource, with face 0 LE
// appended as B1b does. This is not the different raw-resource SHA256.
const REVIEWED_B1_ID: [u8; 32] = [
    0xaf, 0x02, 0xdb, 0xc8, 0x83, 0x9b, 0x7c, 0xad,
    0xae, 0x15, 0xe5, 0x28, 0xaa, 0x3d, 0x9f, 0xf8,
    0x5c, 0x54, 0x5b, 0x71, 0x5b, 0x55, 0x1d, 0x21,
    0x8b, 0xf3, 0x8a, 0x93, 0x7e, 0xe7, 0xe2, 0xb0,
];

/// Numeric conversion only: no shaping, placed advance, default or font parse.
/// u16 inputs cannot encode NaN/Infinity. Still check the division and f32
/// serialization envelope explicitly; the integer product is checked too.
fn width_1000(advance: u16, units_per_em: u16) -> Result<f32> {
    if !(16..=16384).contains(&units_per_em) {
        return Err(invalid("B1 font units per em are outside 16..16384"));
    }
    let numerator = u32::from(advance).checked_mul(1000)
        .ok_or_else(|| invalid("B1 intrinsic width product overflow"))?;
    let value = f64::from(numerator) / f64::from(units_per_em);
    let width = value as f32;
    if !value.is_finite() || !width.is_finite() || width.abs() > MAX_METRIC
        || (width != 0.0 && width.abs() < f32::MIN_POSITIVE)
    {
        return Err(invalid("PDF B1 glyph width is not finite and bounded"));
    }
    Ok(width)
}

impl<'a> B1Widths<'a> {
    fn read(font: &'a Font, subset: &'a SubsetFont) -> Result<Self> {
        // Public B1 fields are bounded before digest, table walks or allocation.
        // Production obtains these bound records only through FontPlanView;
        // TextPlan already checked every used original-GID advance and the
        // single admitted subset chain. These repeated cheap identity/mapping
        // guards also retain the independent synthetic component-test scope.
        if font.bytes.len() != MAX_FONT_BYTES {
            return Err(invalid("B1 font byte envelope differs from the reviewed resource"));
        }
        if font.face != 0 || font.postscript_name.as_bytes() != POSTSCRIPT_NAME
            || font.num_glyphs != 2620 || font.units_per_em != 2048
            || font.glyphs.len() > usize::from(font.num_glyphs)
            || font.glyphs.keys().any(|gid| *gid >= font.num_glyphs)
        {
            return Err(invalid("B1 font metadata differs from the reviewed regular face"));
        }
        // Exact current builtin bytes/face replace the removed numeric index.
        // The plan already checked bytes+face ID and admitted the SAME source
        // through subset_font. Compare the fixed B1 ID here without rehashing.
        check_builtin_source(font)?;
        if font.id != REVIEWED_B1_ID {
            return Err(invalid("B1 font bytes/face/id do not identify the reviewed resource"));
        }
        let glyphs = subset.mapping().len();
        if glyphs == 0 || glyphs > MAX_GLYPHS || subset.bytes().is_empty()
            || subset.bytes().len() > MAX_FONT_BYTES
        {
            return Err(invalid("font subset exceeds the admitted fixed-resource contract"));
        }
        // Preserve the old non-parser mapping guards. The immutable factory's
        // check_subset_program still validates maxp/head/hhea/hmtx structure.
        let mut previous = None;
        for (expected, &(old, new)) in subset.mapping().iter().enumerate() {
            if previous.is_some_and(|prior| old <= prior) || usize::from(new) != expected
                || old >= font.num_glyphs || (expected == 0 && old != 0)
            {
                return Err(invalid("font subset mapping is not original-zero ordered and dense"));
            }
            previous = Some(old);
        }
        Ok(Self { font, subset })
    }

    fn advance(&self, subset_gid: u16) -> Result<f32> {
        let &(original_gid, mapped) = self.subset.mapping().get(usize::from(subset_gid))
            .ok_or_else(|| invalid("PDF code maps outside the subset glyph range"))?;
        if mapped != subset_gid {
            return Err(invalid("PDF subset glyph does not match its original-GID mapping"));
        }
        let glyph = self.font.glyphs.get(&original_gid)
            .ok_or_else(|| invalid("B1 font lacks an intrinsic width for an emitted original glyph"))?;
        width_1000(glyph.advance, self.font.units_per_em)
    }


}

fn fixed_font_name(name: &[u8], tag: [u8; 6]) -> Result<[u8; 21]> {
    if name != POSTSCRIPT_NAME || !tag.iter().all(u8::is_ascii_uppercase) {
        return Err(invalid("PDF font requires LiberationSans and a caller-supplied six-uppercase subset tag"));
    }
    let mut result = *b"AAAAAA+LiberationSans";
    for (slot, byte) in result.iter_mut().take(6).zip(tag) {
        *slot = byte;
    }
    Ok(result)
}

/// Conservative byte bound for this exact six-object call sequence/settings.
/// W rows: at most 5 CID digits + 64 finite-f32 bytes + punctuation/spaces < 128.
/// Fixed dictionaries/three streams: 31 pairs including nested SystemInfo,
/// keys <= 16, scalar/name/ref tokens <= 64, bbox value < 262. Each fixed
/// pair costs < 512 bytes; all headers/trailers < 4096. Thus fixed bytes
/// < 31*512+4096 = 19968 < 32768, excluding W rows and raw stream payloads.
/// Stream payload bytes are counted separately, exactly once each in Chunk.
/// This bounds serialized bytes, not Vec capacity, transient memory or OOM.
fn byte_bound(font: usize, cmap: usize, map: usize, rows: usize) -> Result<usize> {
    rows.checked_mul(ROW_BYTES).and_then(|n| n.checked_add(FIXED_BYTES))
        .and_then(|n| n.checked_add(font)).and_then(|n| n.checked_add(cmap))
        .and_then(|n| n.checked_add(map)).ok_or_else(|| invalid("font object byte budget overflow"))
}

struct Prepared<'a> {
    font: &'a [u8],
    unicode: Vec<u8>,
    cid_map: Vec<u8>,
    widths: Vec<(u16, f32)>,
    name: [u8; 21],
    descriptor: &'a DescriptorMetrics,
    flags: FontFlags,
    limit: usize,
    bound: usize,
}

fn prepare(input: FontObjectParts<'_>) -> Result<Prepared<'_>> {
    let descriptor = input.descriptor.ok_or_else(|| invalid("essential font descriptor metrics are missing"))?;
    let flags = descriptor.validate()?;
    let name = fixed_font_name(input.postscript_name, input.subset_tag)?;
    if input.codes.len() > MAX_CODES {
        return Err(invalid("PDF font code count exceeds 65536"));
    }
    let metrics = B1Widths::read(input.source_font, input.subset)?;
    let limit = input.max_chunk_bytes.min(MAX_CHUNK_BYTES).min(MAX_DRAWING_BYTES);
    let max_cid = input.codes.last().map_or(0, |row| usize::from(row.code));
    let map_len = max_cid.checked_add(1).and_then(|n| n.checked_mul(2))
        .filter(|n| *n <= MAX_MAP_BYTES).ok_or_else(|| invalid("CID to GID map byte limit exceeded"))?;
    let without_unicode = byte_bound(input.subset.bytes().len(), 0, map_len, input.codes.len())?;
    let unicode_allowance = limit.checked_sub(without_unicode).ok_or_else(|| invalid("font objects exceed their preflight byte allowance"))?;

    let (mut previous, mut utf8) = (None, 0usize);
    // Counts/order/ranges/lengths before allocating code-derived buffers.
    for row in input.codes {
        if previous.is_some_and(|prior| row.code <= prior) {
            return Err(invalid("PDF font codes must be strictly increasing and unique"));
        }
        if row.code == 0 && row.subset_gid != 0 {
            return Err(invalid("CID zero must map to subset glyph zero"));
        }
        metrics.advance(row.subset_gid)?;
        if row.logical.is_empty() || row.logical.len() > MAX_CLUSTER_BYTES {
            return Err(invalid("PDF font code requires a nonempty bounded literal cluster"));
        }
        utf8 = utf8.checked_add(row.logical.len()).filter(|n| *n <= MAX_UTF8_BYTES)
            .ok_or_else(|| invalid("PDF font logical text input exceeds its byte limit"))?;
        previous = Some(row.code);
    }

    let mut pairs = Vec::new();
    pairs.try_reserve_exact(input.codes.len()).map_err(|_| invalid("cannot allocate PDF ToUnicode pairs"))?;
    let mut widths = Vec::new();
    widths.try_reserve_exact(input.codes.len()).map_err(|_| invalid("cannot allocate PDF CID widths"))?;
    let mut cid_map = Vec::new();
    cid_map.try_reserve_exact(map_len).map_err(|_| invalid("cannot allocate PDF CID to GID map"))?;
    cid_map.resize(map_len, 0);
    for row in input.codes {
        pairs.push((row.code, row.logical));
        widths.push((row.code, metrics.advance(row.subset_gid)?));
        let at = usize::from(row.code).checked_mul(2).ok_or_else(|| invalid("CID map offset overflow"))?;
        let end = at.checked_add(2).ok_or_else(|| invalid("CID map offset overflow"))?;
        cid_map.get_mut(at..end).ok_or_else(|| invalid("CID map range exceeds its allowance"))?
            .copy_from_slice(&row.subset_gid.to_be_bytes());
    }
    let unicode = write_to_unicode(&pairs, unicode_allowance)?;
    let bound = byte_bound(input.subset.bytes().len(), unicode.len(), cid_map.len(), widths.len())?;
    if bound > limit {
        return Err(invalid("font objects exceed their complete preflight byte allowance"));
    }
    // Stream::start panics above i32::MAX, so check each real stream explicitly.
    for length in [input.subset.bytes().len(), unicode.len(), cid_map.len()] {
        i32::try_from(length).map_err(|_| invalid("font stream length exceeds the PDF writer limit"))?;
    }
    Ok(Prepared {
        font: input.subset.bytes(), unicode, cid_map, widths,
        name, descriptor, flags, limit, bound,
    })
}

fn check_chunk(chunk: &Chunk, limit: usize, bound: usize) -> Result<()> {
    if chunk.len() > limit || chunk.len() > bound {
        return Err(invalid("serialized font objects exceed their checked byte bound"));
    }
    Ok(())
}

/// Prepare six independent resource objects locally. Caller must reserve their
/// Ref IDs/tag globally and later attach the Type0 ref to page resources.
/// No caller Pdf is borrowed, so Result refusals cannot mutate that document.
pub(super) fn font_objects(refs: FontObjectRefs, input: FontObjectInput<'_>) -> Result<Chunk> {
    let source = input.source;
    font_objects_parts(refs, FontObjectParts {
        source_font: source.font(),
        subset: source.subset(),
        codes: source.codes(),
        descriptor: Some(source.descriptor()),
        postscript_name: source.font().postscript_name.as_bytes(),
        subset_tag: input.subset_tag,
        max_chunk_bytes: input.max_chunk_bytes,
    })
}

/// The raw preparation path is private to this module. Outside callers cannot
/// supply an independent Font/Subset pair or manufacture a validated view.
fn font_objects_parts(refs: FontObjectRefs, input: FontObjectParts<'_>) -> Result<Chunk> {
    refs.validate()?;
    let prepared = prepare(input)?;
    let Prepared { font, unicode, cid_map, widths, name, descriptor, flags, limit, bound } = prepared;
    let mut chunk = Chunk::new();
    {
        let mut writer = chunk.type0_font(refs.type0);
        writer.base_font(Name(&name)).encoding_predefined(Name(b"Identity-H"))
            .descendant_font(refs.cid_font).to_unicode(refs.to_unicode);
    }
    check_chunk(&chunk, limit, bound)?;
    {
        let mut writer = chunk.cid_font(refs.cid_font);
        writer.subtype(CidFontType::Type2).base_font(Name(&name))
            .system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 })
            .font_descriptor(refs.descriptor)
            .cid_to_gid_map_stream(refs.cid_to_gid);
        let mut array = writer.widths();
        for &(cid, width) in &widths {
            array.consecutive(cid, std::iter::once(width));
        }
    }
    check_chunk(&chunk, limit, bound)?;
    {
        let [left, bottom, right, top] = descriptor.bbox_1000;
        let mut writer = chunk.font_descriptor(refs.descriptor);
        writer.name(Name(&name)).flags(flags).bbox(Rect::new(left, bottom, right, top))
            .italic_angle(descriptor.italic_angle_degrees).ascent(descriptor.ascent_1000)
            .descent(descriptor.descent_1000).cap_height(descriptor.cap_height_1000)
            .stem_v(descriptor.stem_v_1000).font_file2(refs.font_file2);
    }
    check_chunk(&chunk, limit, bound)?;
    {
        let length = i32::try_from(font.len()).map_err(|_| invalid("FontFile2 length exceeds i32"))?;
        chunk.stream(refs.font_file2, font).pair(Name(b"Length1"), length);
    }
    check_chunk(&chunk, limit, bound)?;
    {
        chunk.stream(refs.to_unicode, &unicode);
    }
    check_chunk(&chunk, limit, bound)?;
    {
        chunk.stream(refs.cid_to_gid, &cid_map);
    }
    check_chunk(&chunk, limit, bound)?;
    if !chunk.refs().eq(refs.all()) {
        return Err(invalid("font object references differ from the six reserved IDs"));
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic serialization/refusal components are not proof constructors.
    // Only these descendant tests can call the module-private raw parts path.
    use super::{FontObjectParts as FontObjectInput, font_objects_parts as font_objects};
    use sha2::{Digest, Sha256};
    use super::super::subset_font::subset_font;

    // Unchanged legacy metric reader retained as a test-only independent
    // subset/width oracle. Production resource emission has no SFNT reader.
    fn part(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
        let end = at.checked_add(len).ok_or_else(|| invalid("font metric offset overflow"))?;
        bytes.get(at..end).ok_or_else(|| invalid("truncated font metric data"))
    }

    fn word(bytes: &[u8], at: usize) -> Result<u16> {
        Ok(u16::from_be_bytes(part(bytes, at, 2)?.try_into().map_err(|_| invalid("invalid font metric word"))?))
    }

    fn dword(bytes: &[u8], at: usize) -> Result<usize> {
        let raw = u32::from_be_bytes(part(bytes, at, 4)?.try_into().map_err(|_| invalid("invalid font metric offset"))?);
        usize::try_from(raw).map_err(|_| invalid("font metric offset exceeds platform"))
    }

    /// Small bounded table view over the already validated, immutable SubsetFont.
    /// This does not repeat admission, glyph closure or composite parsing.
    fn table<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Result<&'a [u8]> {
        let count = usize::from(word(bytes, 4)?);
        if count == 0 || count > 19 {
            return Err(invalid("font metric table count exceeds the admitted subset contract"));
        }
        part(bytes, 0, 12 + count * 16)?; // count <= 19.
        for index in 0..count {
            let record = 12 + index * 16;
            if part(bytes, record, 4)? == tag {
                return part(bytes, dword(bytes, record + 8)?, dword(bytes, record + 12)?);
            }
        }
        Err(invalid("subset lacks a required metric table"))
    }

    struct MetricView<'a> {
        hmtx: &'a [u8],
        glyphs: usize,
        full_metrics: usize,
        units_per_em: u16,
    }

    impl<'a> MetricView<'a> {
        fn read(subset: &'a SubsetFont) -> Result<Self> {
            let glyphs = subset.mapping().len();
            if glyphs == 0 || glyphs > MAX_GLYPHS || subset.bytes().len() > MAX_FONT_BYTES {
                return Err(invalid("font subset exceeds the admitted fixed-resource contract"));
            }
            let mut previous = None;
            for (expected, &(old, new)) in subset.mapping().iter().enumerate() {
                if previous.is_some_and(|prior| old <= prior) || usize::from(new) != expected {
                    return Err(invalid("font subset mapping is not ordered and dense"));
                }
                if expected == 0 && old != 0 {
                    return Err(invalid("font subset lacks original glyph zero"));
                }
                previous = Some(old);
            }
            let bytes = subset.bytes();
            let head = table(bytes, b"head")?;
            let hhea = table(bytes, b"hhea")?;
            let hmtx = table(bytes, b"hmtx")?;
            let maxp = table(bytes, b"maxp")?;
            if usize::from(word(maxp, 4)?) != glyphs {
                return Err(invalid("font subset glyph count differs from its mapping"));
            }
            let units_per_em = word(head, 18)?;
            if units_per_em != 2048 {
                return Err(invalid("font subset units per em differ from the one admitted font"));
            }
            let full_metrics = usize::from(word(hhea, 34)?);
            if full_metrics == 0 || full_metrics > glyphs {
                return Err(invalid("subset has invalid numberOfHMetrics"));
            }
            let expected = full_metrics.checked_mul(4)
                .and_then(|full| (glyphs - full_metrics).checked_mul(2).and_then(|tail| full.checked_add(tail)))
                .ok_or_else(|| invalid("subset metric length overflow"))?;
            if hmtx.len() != expected {
                return Err(invalid("subset hmtx length does not match its glyph metrics"));
            }
            Ok(Self { hmtx, glyphs, full_metrics, units_per_em })
        }

        fn advance(&self, gid: u16) -> Result<f32> {
            let gid = usize::from(gid);
            if gid >= self.glyphs {
                return Err(invalid("PDF code maps outside the subset glyph range"));
            }
            let index = gid.min(self.full_metrics - 1); // full_metrics checked > 0.
            let advance = word(self.hmtx, index.checked_mul(4).ok_or_else(|| invalid("font advance offset overflow"))?)?;
            let width = (f64::from(advance) * 1000.0 / f64::from(self.units_per_em)) as f32;
            if !width.is_finite() || width.abs() > MAX_METRIC {
                return Err(invalid("PDF glyph width is not finite and bounded"));
            }
            Ok(width)
        }
    }

    const BUNDLED: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/liberation-sans/LiberationSans-Regular.ttf"));

    // SYNTHETIC descriptor for serialization/refusal tests. These are NOT
    // measured Liberation values or a choice of the real provider's policies.
    fn synthetic_descriptor() -> DescriptorMetrics {
        DescriptorMetrics {
            bbox_1000: [-100.0, -250.0, 1100.0, 900.0],
            ascent_1000: 800.0, descent_1000: -200.0, cap_height_1000: 700.0,
            italic_angle_degrees: 0.0, stem_v_1000: 80.0,
            flags: FontFlags::NON_SYMBOLIC.bits(),
        }
    }

    fn admitted_subset() -> SubsetFont {
        // Read-only original metadata: glyph0 metric(1536,205), 2/3=(569,0),
        // original IDs0,2,3 become subset IDs0,1,2. No B1/shaping assertion.
        subset_font(BUNDLED, 0, &[3, 2, 3], MAX_FONT_BYTES).unwrap()
    }

    fn refs(first: i32) -> FontObjectRefs {
        FontObjectRefs {
            type0: Ref::new(first), cid_font: Ref::new(first + 1),
            descriptor: Ref::new(first + 2), font_file2: Ref::new(first + 3),
            to_unicode: Ref::new(first + 4), cid_to_gid: Ref::new(first + 5),
        }
    }

    // Synthetic B1 resource record for the existing serialization/refusal
    // tests only: not a valid exactly-used Scene or a shaper/layout assertion.
    fn source_font() -> &'static Font {
        static FONT: std::sync::OnceLock<Font> = std::sync::OnceLock::new();
        FONT.get_or_init(|| {
            use solidcraft_geom::paper::text::{FontFlags as B1Flags, FontGlyph, FontMetrics};
            Font {
                id: REVIEWED_B1_ID, bytes: std::sync::Arc::from(BUNDLED), face: 0,
                postscript_name: "LiberationSans".into(), num_glyphs: 2620, units_per_em: 2048,
                metrics: FontMetrics {
                    ascent: 1854, descent: -434, cap_height: 1409, bbox: [-1114, -621, 2666, 2007],
                    italic_angle: 0.0, flags: B1Flags::default(), stem_v: 180,
                },
                glyphs: [(0, 1536), (2, 569), (3, 569)].into_iter()
                    .map(|(gid, advance)| (gid, FontGlyph { advance, outline: vec![] })).collect(),
            }
        })
    }

    fn input<'a>(subset: &'a SubsetFont, codes: &'a [CodeToGlyph<'a>], descriptor: &'a DescriptorMetrics) -> FontObjectInput<'a> {
        FontObjectInput {
            source_font: source_font(), subset, codes, descriptor: Some(descriptor),
            postscript_name: POSTSCRIPT_NAME, subset_tag: *b"ABCDEF",
            max_chunk_bytes: MAX_CHUNK_BYTES,
        }
    }

    struct Parsed<'a> {
        id: i32,
        dict: &'a str,
        stream: Option<&'a [u8]>,
    }

    fn skip_space(bytes: &[u8], at: &mut usize) {
        while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) { *at += 1; }
    }

    fn number(dict: &str, key: &str) -> usize {
        dict.split_ascii_whitespace().skip_while(|token| *token != key)
            .nth(1).unwrap().parse().unwrap()
    }

    // Original tiny reader for THIS fixed dictionary/stream grammar, not a
    // general PDF parser. Binary font bytes are skipped by declared Length,
    // never scanned for endobj/endstream text or used as an expected golden.
    fn objects(bytes: &[u8]) -> Vec<Parsed<'_>> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            skip_space(bytes, &mut at);
            if at == bytes.len() { break; }
            let start = at;
            while bytes[at].is_ascii_digit() { at += 1; }
            let id = std::str::from_utf8(&bytes[start..at]).unwrap().parse().unwrap();
            assert!(bytes[at..].starts_with(b" 0 obj\n"));
            at += b" 0 obj\n".len();
            let start = at;
            assert!(bytes[at..].starts_with(b"<<"));
            let mut depth = 0;
            loop {
                if bytes[at..].starts_with(b"<<") { depth += 1; at += 2; }
                else if bytes[at..].starts_with(b">>") {
                    depth -= 1;
                    at += 2;
                    if depth == 0 { break; }
                } else { at += 1; }
            }
            let dict = std::str::from_utf8(&bytes[start..at]).unwrap();
            skip_space(bytes, &mut at);
            let stream = if bytes[at..].starts_with(b"stream\n") {
                at += b"stream\n".len();
                let len = number(dict, "/Length");
                let payload = &bytes[at..at + len];
                at += len;
                assert!(bytes[at..].starts_with(b"\nendstream"));
                at += b"\nendstream".len();
                skip_space(bytes, &mut at);
                Some(payload)
            } else { None };
            assert!(bytes[at..].starts_with(b"endobj"));
            at += b"endobj".len();
            out.push(Parsed { id, dict, stream });
        }
        out
    }

    fn object<'a, 'b>(objects: &'b [Parsed<'a>], id: i32) -> &'b Parsed<'a> {
        objects.iter().find(|object| object.id == id).unwrap()
    }

    #[test]
    fn six_real_refs_link_exact_objects_and_preserve_raw_font_bytes() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let chunk = font_objects(refs(6), input(&subset, &[], &descriptor)).unwrap();
        let parsed = objects(chunk.as_bytes());
        assert_eq!(parsed.iter().map(|o| o.id).collect::<Vec<_>>(), [6, 7, 8, 9, 10, 11]);
        let type0 = object(&parsed, 6).dict;
        assert!(type0.contains("/Subtype /Type0"));
        assert!(type0.contains("/Encoding /Identity-H"));
        assert!(type0.contains("/DescendantFonts [7 0 R]"));
        assert!(type0.contains("/ToUnicode 10 0 R"));
        assert!(type0.contains("/BaseFont /ABCDEF+LiberationSans"));
        let cid = object(&parsed, 7).dict;
        assert!(cid.contains("/Subtype /CIDFontType2"));
        assert!(cid.contains("/CIDToGIDMap 11 0 R"));
        assert!(cid.contains("/FontDescriptor 8 0 R"));
        assert!(cid.contains("/Registry (Adobe)") && cid.contains("/Ordering (Identity)"));
        let desc = object(&parsed, 8).dict;
        assert!(desc.contains("/FontName /ABCDEF+LiberationSans"));
        assert!(desc.contains("/FontFile2 9 0 R"));
        assert!(desc.contains("/StemV 80") && desc.contains("/Ascent 800"));
        assert!(desc.contains("/CapHeight 700") && desc.contains("/Flags 32"));
        let file = object(&parsed, 9);
        assert_eq!(file.stream.unwrap(), subset.bytes());
        assert_eq!(number(file.dict, "/Length1"), subset.bytes().len());
        assert!(!file.dict.contains("/Filter"));
        assert_eq!(object(&parsed, 11).stream.unwrap(), [0, 0]);
    }

    #[test]
    fn distinct_codes_share_a_gid_without_losing_literal_logical_clusters() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        // Synthetic routing semantics only; no claim that these words are
        // shaped or rendered by the space/empty glyph in the fixed resource.
        let codes = [
            CodeToGlyph { code: 2, subset_gid: 1, logical: "one" },
            CodeToGlyph { code: 7, subset_gid: 1, logical: "two" },
            CodeToGlyph { code: 9, subset_gid: 0, logical: "notdef" },
        ];
        let chunk = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let map = object(&parsed, 11).stream.unwrap();
        let mut expected = vec![0; 20];
        expected[5] = 1;
        expected[15] = 1;
        assert_eq!(map, expected);
        let cmap = std::str::from_utf8(object(&parsed, 10).stream.unwrap()).unwrap();
        assert!(cmap.contains("<0002> <006F006E0065>"));
        assert!(cmap.contains("<0007> <00740077006F>"));
        assert!(cmap.contains("<0009> <006E006F0074006400650066>"));
        assert!(!cmap.contains("<0001> <"));
    }

    #[test]
    fn sparse_maximum_cid_has_bounded_map_and_zero_stays_notdef() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: u16::MAX, subset_gid: 2, logical: "z" }];
        let chunk = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let map = object(&parsed, 11).stream.unwrap();
        assert_eq!(map.len(), 131_072);
        assert!(map[..131_070].iter().all(|byte| *byte == 0));
        assert_eq!(&map[131_070..], [0, 2]);
        let bad = [CodeToGlyph { code: 0, subset_gid: 1, logical: "x" }];
        assert!(font_objects(refs(6), input(&subset, &bad, &descriptor)).is_err());
        let zero = [CodeToGlyph { code: 0, subset_gid: 0, logical: "notdef" }];
        assert!(font_objects(refs(6), input(&subset, &zero, &descriptor)).is_ok());
    }

    // Test-only parser for this oracle's EXACT two-row W shape. It preserves
    // the outer array and one-element inner arrays instead of flattening them.
    // Bounded input/tokens, no deliberate panic, and no general PDF claim.
    struct WidthField<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl WidthField<'_> {
        fn literal(&mut self, token: &[u8]) -> Result<()> {
            skip_space(self.bytes, &mut self.at);
            let end = self.at.checked_add(token.len()).ok_or_else(|| invalid("W oracle offset overflow"))?;
            if self.bytes.get(self.at..end) != Some(token) {
                return Err(invalid("W oracle expected an array delimiter or field key"));
            }
            self.at = end;
            Ok(())
        }

        fn cid(&mut self) -> Result<u16> {
            skip_space(self.bytes, &mut self.at);
            let start = self.at;
            while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                if self.at - start == 5 {
                    return Err(invalid("W oracle CID token exceeds five digits"));
                }
                self.at += 1;
            }
            let token = self.bytes.get(start..self.at).ok_or_else(|| invalid("W oracle CID range"))?;
            let token = std::str::from_utf8(token).map_err(|_| invalid("W oracle CID encoding"))?;
            token.parse::<u16>().map_err(|_| invalid("W oracle expected a u16 CID"))
        }

        fn width(&mut self) -> Result<f32> {
            skip_space(self.bytes, &mut self.at);
            let start = self.at;
            while let Some(byte) = self.bytes.get(self.at) {
                if byte.is_ascii_whitespace() || matches!(*byte, b'[' | b']') {
                    break;
                }
                if self.at - start == 64 {
                    return Err(invalid("W oracle width token exceeds its limit"));
                }
                self.at += 1;
            }
            let token = self.bytes.get(start..self.at).ok_or_else(|| invalid("W oracle width range"))?;
            if !token.iter().any(u8::is_ascii_digit)
                || token.iter().any(|byte| !byte.is_ascii_digit() && !matches!(*byte, b'+' | b'-' | b'.'))
            {
                return Err(invalid("W oracle expected a decimal width token"));
            }
            let token = std::str::from_utf8(token).map_err(|_| invalid("W oracle width encoding"))?;
            let width = token.parse::<f32>().map_err(|_| invalid("W oracle decimal width"))?;
            if !width.is_finite() || width < 0.0 {
                return Err(invalid("W oracle requires a finite nonnegative width"));
            }
            Ok(width)
        }

        fn row(&mut self) -> Result<(u16, [f32; 1])> {
            let cid = self.cid()?;
            self.literal(b"[")?;
            let width = self.width()?;
            self.literal(b"]")?;
            Ok((cid, [width]))
        }
    }

    // Test-only structural oracle: keep both W array levels and reject trailing
    // fields. Literal limits are independent of the emitter's constants.
    fn parse_all_width_rows(field: &str) -> Result<Vec<(u16, [f32; 1])>> {
        if field.len() > (16 << 20) {
            return Err(invalid("W coverage oracle field exceeds 16 MiB"));
        }
        let mut reader = WidthField { bytes: field.as_bytes(), at: 0 };
        reader.literal(b"/W")?;
        reader.literal(b"[")?;
        let mut rows = Vec::new();
        loop {
            skip_space(reader.bytes, &mut reader.at);
            if reader.bytes.get(reader.at) == Some(&b']') {
                reader.at += 1;
                break;
            }
            if rows.len() == 65_536 {
                return Err(invalid("W coverage oracle exceeds 65536 rows"));
            }
            rows.push(reader.row()?);
        }
        skip_space(reader.bytes, &mut reader.at);
        if reader.at != reader.bytes.len() {
            return Err(invalid("W coverage oracle has trailing data"));
        }
        Ok(rows)
    }

    fn parse_two_width_rows(field: &str) -> Result<[(u16, [f32; 1]); 2]> {
        if field.len() > 512 {
            return Err(invalid("W oracle field exceeds its byte limit"));
        }
        let mut reader = WidthField { bytes: field.as_bytes(), at: 0 };
        reader.literal(b"/W")?;
        reader.literal(b"[")?;
        let first = reader.row()?;
        let second = reader.row()?;
        reader.literal(b"]")?;
        skip_space(reader.bytes, &mut reader.at);
        if reader.at != reader.bytes.len() {
            return Err(invalid("W oracle has trailing data or additional rows"));
        }
        Ok([first, second])
    }

    #[test]
    fn widths_use_the_mapped_subset_gid_but_are_indexed_by_cid() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let codes = [
            CodeToGlyph { code: 42, subset_gid: 2, logical: "synthetic" },
            CodeToGlyph { code: 300, subset_gid: 0, logical: "notdef" },
        ];
        let prepared = prepare(input(&subset, &codes, &descriptor)).unwrap();
        // Independent original-font integer metadata, not emitter output.
        let empty_width = (569_000.0_f64 / 2048.0) as f32;
        assert_eq!(prepared.widths, [(42, empty_width), (300, 750.0)]);
        // Subsetter compresses the repeated569 width of the two empty glyphs.
        let view = MetricView::read(&subset).unwrap();
        assert_eq!(view.advance(1).unwrap(), empty_width);
        assert_eq!(view.advance(2).unwrap(), empty_width);
        let chunk = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"), "all emitted CIDs use explicit W rows");
        // W is the last entry of this emitted dictionary; remove ONLY its
        // dictionary terminator, then parse all of W including both array levels.
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap()
            .strip_suffix(">>").unwrap().trim_end();
        let expected = [(42, [empty_width]), (300, [750.0])];
        assert_eq!(parse_two_width_rows(field).unwrap(), expected);
        // Independent literals, not emitter-generated goldens. The former
        // flattened-token oracle would miss these nesting/trailing mutations.
        for malformed in [
            "/W [42 277.83203125 300 750]",
            "/W [42 [277.83203125] 300 [750] 999 [123]]",
            "/W [42 [277.83203125] 300 [750]] 999 [123]",
            "/W [42 [277.83203125] 300 [750]",
            "/W [42 [277.83203125 999] 300 [750]]",
        ] {
            assert!(parse_two_width_rows(malformed).is_err(), "accepted malformed W: {malformed}");
        }
        assert_eq!(parse_two_width_rows("/W [42 [277.83203125] 300 [750]]").unwrap(), expected);
    }

    #[test]
    fn invalid_code_order_gid_and_logical_clusters_refuse() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        for codes in [
            vec![CodeToGlyph { code: 2, subset_gid: 3, logical: "outside" }],
            vec![CodeToGlyph { code: 2, subset_gid: 1, logical: "" }],
            vec![CodeToGlyph { code: 2, subset_gid: 1, logical: "a" }, CodeToGlyph { code: 2, subset_gid: 1, logical: "b" }],
            vec![CodeToGlyph { code: 2, subset_gid: 1, logical: "a" }, CodeToGlyph { code: 1, subset_gid: 1, logical: "b" }],
        ] {
            assert!(font_objects(refs(6), input(&subset, &codes, &descriptor)).is_err());
        }
        let too_long = "x".repeat(769);
        let codes = [CodeToGlyph { code: 1, subset_gid: 1, logical: &too_long }];
        assert!(font_objects(refs(6), input(&subset, &codes, &descriptor)).is_err());
        let too_many_utf16 = "x".repeat(257); // Within UTF8 cap, beyond512UTF16BE bytes.
        let codes = [CodeToGlyph { code: 1, subset_gid: 1, logical: &too_many_utf16 }];
        assert!(font_objects(refs(6), input(&subset, &codes, &descriptor)).is_err());
    }

    #[test]
    fn essential_descriptor_fields_refuse_missing_invalid_and_underflow_values() {
        let subset = admitted_subset();
        let valid = synthetic_descriptor();
        let mut missing = input(&subset, &[], &valid);
        missing.descriptor = None;
        assert!(font_objects(refs(6), missing).is_err());
        for case in 0..12 {
            let mut descriptor = synthetic_descriptor();
            match case {
                0 => descriptor.stem_v_1000 = 0.0,
                1 => descriptor.ascent_1000 = 0.0,
                2 => descriptor.descent_1000 = 0.0,
                3 => descriptor.cap_height_1000 = -1.0,
                4 => descriptor.cap_height_1000 = 900.0,
                5 => descriptor.bbox_1000 = [0.0; 4],
                6 => descriptor.stem_v_1000 = f32::NAN,
                7 => descriptor.bbox_1000[3] = f32::INFINITY,
                8 => descriptor.ascent_1000 = MAX_METRIC + 1.0,
                9 => descriptor.italic_angle_degrees = 91.0,
                10 => descriptor.stem_v_1000 = f32::from_bits(1),
                _ => descriptor.bbox_1000 = [10.0, 0.0, 0.0, 900.0],
            }
            assert!(font_objects(refs(6), input(&subset, &[], &descriptor)).is_err(), "synthetic case{case}");
        }
        for flags in [0, 1 << 31, FontFlags::SYMBOLIC.bits() | FontFlags::NON_SYMBOLIC.bits()] {
            let mut descriptor = synthetic_descriptor();
            descriptor.flags = flags;
            assert!(font_objects(refs(6), input(&subset, &[], &descriptor)).is_err());
        }
    }

    #[test]
    fn fixed_name_tag_and_real_ref_constraints_are_explicit() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let mut wrong_name = input(&subset, &[], &descriptor);
        wrong_name.postscript_name = b"OtherFont";
        assert!(font_objects(refs(6), wrong_name).is_err());
        for tag in [*b"abcDEF", *b"ABCDE ", [255; 6]] {
            let mut wrong_tag = input(&subset, &[], &descriptor);
            wrong_tag.subset_tag = tag;
            assert!(font_objects(refs(6), wrong_tag).is_err());
        }
        let mut duplicate = refs(6);
        duplicate.cid_font = duplicate.type0;
        assert!(font_objects(duplicate, input(&subset, &[], &descriptor)).is_err());
        assert!(font_objects(refs(1), input(&subset, &[], &descriptor)).is_err());
        let mut too_high = refs(6);
        too_high.cid_to_gid = Ref::new(100_001);
        assert!(font_objects(too_high, input(&subset, &[], &descriptor)).is_err());
        let mut inclusive = refs(6);
        inclusive.cid_to_gid = Ref::new(100_000);
        assert!(font_objects(inclusive, input(&subset, &[], &descriptor)).is_ok());
    }

    #[test]
    fn preflight_byte_allowance_is_inclusive_and_actual_serialization_is_below_it() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: 42, subset_gid: 1, logical: "x" }];
        let prepared = prepare(input(&subset, &codes, &descriptor)).unwrap();
        let bound = prepared.bound; // Boundary property, not a generated font golden.
        let mut exact = input(&subset, &codes, &descriptor);
        exact.max_chunk_bytes = bound;
        let chunk = font_objects(refs(6), exact).unwrap();
        assert!(chunk.len() <= bound);
        let mut short = input(&subset, &codes, &descriptor);
        short.max_chunk_bytes = bound - 1;
        assert!(font_objects(refs(6), short).is_err());
        let mut zero = input(&subset, &codes, &descriptor);
        zero.max_chunk_bytes = 0;
        assert!(font_objects(refs(6), zero).is_err());
        assert!(byte_bound(usize::MAX, 0, 0, 0).is_err());
        assert!(byte_bound(0, 0, 0, usize::MAX).is_err());
    }

    #[test]
    fn repeated_complete_inputs_are_deterministic_and_leave_source_unchanged() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let source = subset.bytes().to_vec();
        let mapping = subset.mapping().to_vec();
        let codes = [CodeToGlyph { code: 10, subset_gid: 1, logical: "x" }];
        let a = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        let b = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_eq!(source, subset.bytes());
        assert_eq!(mapping, subset.mapping());
    }

    #[test]
    fn full_u16_code_space_is_inclusive_and_one_extra_record_refuses() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        let mut codes: Vec<_> = (0..=u16::MAX).map(|code| CodeToGlyph {
            code, subset_gid: if code == 0 { 0 } else { 1 }, logical: "x",
        }).collect();
        let chunk = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        assert_eq!(chunk.refs().count(), 6);
        assert!(chunk.len() <= MAX_CHUNK_BYTES);
        let parsed = objects(chunk.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        let rows = parse_all_width_rows(field).unwrap();
        assert_eq!(rows.len(), 65_536);
        for (index, &(cid, [width])) in rows.iter().enumerate() {
            assert_eq!(usize::from(cid), index, "every declared CID occurs once, including both extremes");
            assert!(width.is_finite());
            assert_eq!(width, if index == 0 { 750.0 } else { 277.832_031_25_f64 as f32 });
        }
        codes.push(CodeToGlyph { code: 0, subset_gid: 0, logical: "x" });
        assert!(font_objects(refs(6), input(&subset, &codes, &descriptor)).is_err());
    }

    #[test]
    fn source_derived_p3_offset_arithmetic_is_a_caller_reservation_not_a_registry() {
        let subset = admitted_subset();
        let descriptor = synthetic_descriptor();
        for layers in [0, 5] {
            let first = 6 + layers;
            let chunk = font_objects(refs(first), input(&subset, &[], &descriptor)).unwrap();
            let ids: Vec<_> = chunk.refs().map(Ref::get).collect();
            assert_eq!(ids, (first..=first + 5).collect::<Vec<_>>());
            assert!(ids.iter().all(|id| *id > 5 + layers));
        }
        // This only checks the derived offsets with synthetic descriptors.
        // The actual P3 allocator/page resource integration is NOT exercised.
    }
    // New ORIGINAL B1 width-provider acceptance. These are synthetic record
    // contracts over the real resource, not an exactly-used Scene or shaping
    // claim. Real producer + page-content acceptance remains separate.
    fn b1_width_font(ids: &[(u16, u16)]) -> Font {
        use solidcraft_geom::paper::text::FontGlyph;
        let mut font = source_font().clone();
        font.glyphs = ids.iter().map(|&(gid, advance)| (gid, FontGlyph { advance, outline: vec![] })).collect();
        font
    }

    fn input_from_b1<'a>(subset: &'a SubsetFont, codes: &'a [CodeToGlyph<'a>], descriptor: &'a DescriptorMetrics, font: &'a Font) -> FontObjectInput<'a> {
        let mut result = input(subset, codes, descriptor);
        result.source_font = font;
        result
    }

    #[test]
    fn b1_widths_use_original_gid_join_and_cid_index_with_distinct_real_advances() {
        let subset = subset_font(BUNDLED, 0, &[36, 3], MAX_FONT_BYTES).unwrap();
        assert_eq!(subset.mapping(), [(0, 0), (3, 1), (36, 2)]);
        // Original hmtx metadata: space3=569, A36=1366, .notdef0=1536.
        // A wrong font.glyphs[subset_gid] join lacks 1/2 and cannot pass.
        let font = b1_width_font(&[(0, 1536), (3, 569), (36, 1366)]);
        let descriptor = synthetic_descriptor();
        let codes = [
            CodeToGlyph { code: 42, subset_gid: 2, logical: "A" },
            CodeToGlyph { code: 300, subset_gid: 1, logical: " " },
        ];
        let prepared = prepare(input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        assert_eq!(prepared.widths, [(42, (666.992_187_5_f64 as f32)), (300, (277.832_031_25_f64 as f32))]);
        let chunk = font_objects(refs(16), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        let parsed = objects(chunk.as_bytes());
        assert_eq!(object(&parsed, 19).stream.unwrap(), subset.bytes());
        let field = object(&parsed, 17).dict;
        assert!(!field.contains("/DW"));
        let field = field.get(field.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert_eq!(parse_two_width_rows(field).unwrap(), [(42, [(666.992_187_5_f64 as f32)]), (300, [(277.832_031_25_f64 as f32)])]);
        let cmap = std::str::from_utf8(object(&parsed, 20).stream.unwrap()).unwrap();
        assert!(cmap.contains("<002A> <0041>") && cmap.contains("<012C> <0020>"));
    }

    #[test]
    fn b1_widths_keep_real_zero_advance_and_exact_intrinsic_units() {
        let subset = subset_font(BUNDLED, 0, &[1], MAX_FONT_BYTES).unwrap();
        let font = b1_width_font(&[(0, 1536), (1, 0)]); // original glyph1=(0,0).
        let provider = B1Widths::read(&font, &subset).unwrap();
        assert_eq!(provider.advance(1).unwrap(), 0.0);
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: 9, subset_gid: 1, logical: "zero" }];
        let result = prepare(input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        assert_eq!(result.widths, [(9, 0.0)]);
        let chunk = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert_eq!(parse_all_width_rows(field).unwrap(), [(9, [0.0])]);
    }

    #[test]
    fn b1_widths_arithmetic_has_literal_zero_fraction_and_u16_envelope_oracles() {
        assert_eq!(width_1000(0, 2048).unwrap(), 0.0);
        assert_eq!(width_1000(569, 2048).unwrap(), (277.832_031_25_f64 as f32));
        assert_eq!(width_1000(1366, 2048).unwrap(), (666.992_187_5_f64 as f32));
        assert_eq!(width_1000(1536, 2048).unwrap(), 750.0);
        assert_eq!(width_1000(u16::MAX, 16384).unwrap(), (65_535_000.0_f64 / 16384.0) as f32);
        // Every valid u16 product fits u32; the checked multiply remains a guard
        // against future widening. No overflow input is representable today.
        assert_eq!(u32::from(u16::MAX).checked_mul(1000), Some(65_535_000));
    }

    #[test]
    fn b1_widths_arithmetic_refuses_zero_and_out_of_contract_units_without_dividing() {
        for units in [0, 1, 15, 16385, u16::MAX] {
            assert!(width_1000(1, units).is_err(), "units={units}");
            assert!(width_1000(0, units).is_err(), "zero advance does not excuse units={units}");
        }
    }

    #[test]
    fn b1_widths_arithmetic_preserves_inclusive_serialized_metric_bound() {
        assert_eq!(width_1000(16000, 16).unwrap(), 1_000_000.0);
        assert!(width_1000(16001, 16).is_err());
        assert!(width_1000(u16::MAX, 16).is_err());
        // B1 fields are u16, so NaN/Infinity cannot enter this arithmetic.
        // The original descriptor hostile-float cases remain unchanged.
    }

    #[test]
    fn b1_widths_missing_original_advance_refuses_without_subset_metric_fallback() {
        let subset = subset_font(BUNDLED, 0, &[36], MAX_FONT_BYTES).unwrap();
        let font = b1_width_font(&[(0, 1536)]);
        let provider = B1Widths::read(&font, &subset).unwrap();
        let e = provider.advance(1).unwrap_err().to_string();
        assert!(e.contains("lacks an intrinsic width for an emitted original glyph"), "{e}");
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: 5, subset_gid: 1, logical: "A" }];
        assert!(font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).is_err());
    }

    #[test]
    fn b1_widths_unemitted_closure_glyphs_do_not_require_invented_b1_entries() {
        let subset = subset_font(BUNDLED, 0, &[3, 36], MAX_FONT_BYTES).unwrap();
        // Original36 is embedded but no code emits it. No width lookup for it.
        let font = b1_width_font(&[(0, 1536), (3, 569)]);
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: 5, subset_gid: 1, logical: " " }];
        let prepared = prepare(input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        assert_eq!(prepared.widths, [(5, (277.832_031_25_f64 as f32))]);
        assert_eq!(prepared.font, subset.bytes());
    }

    #[test]
    fn b1_widths_omitted_notdef_uses_complete_explicit_rows_without_a_default() {
        let subset = subset_font(BUNDLED, 0, &[36], MAX_FONT_BYTES).unwrap();
        let font = b1_width_font(&[(36, 1366)]);
        let provider = B1Widths::read(&font, &subset).unwrap();
        assert_eq!(provider.advance(1).unwrap(), (666.992_187_5_f64 as f32));
        let descriptor = synthetic_descriptor();
        let codes = [CodeToGlyph { code: 1, subset_gid: 1, logical: "A" }];
        let chunk = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert_eq!(parse_all_width_rows(field).unwrap(), [(1, [666.992_187_5_f64 as f32])]);
        assert_eq!(object(&parsed, 9).stream.unwrap(), subset.bytes());
        assert_eq!(subset.mapping().first(), Some(&(0, 0)), ".notdef remains in the embedded program");
        // No declared emitted CIDs means no W rows, not an invented CID0 width.
        let empty = font_objects(refs(6), input_from_b1(&subset, &[], &descriptor, &font)).unwrap();
        let parsed = objects(empty.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert!(parse_all_width_rows(field).unwrap().is_empty());
        assert!(!font.glyphs.contains_key(&0));
    }

    #[test]
    fn b1_widths_raw_digest_cannot_replace_b1_bytes_plus_face_identity() {
        let subset = admitted_subset();
        let mut font = source_font().clone();
        let raw: [u8; 32] = Sha256::digest(BUNDLED).into();
        assert_ne!(raw, font.id);
        font.id = raw;
        assert!(B1Widths::read(&font, &subset).is_err());
        font = source_font().clone();
        font.id[0] ^= 1;
        assert!(B1Widths::read(&font, &subset).is_err());
        let mut changed = BUNDLED.to_vec();
        changed[4096] ^= 1;
        font.bytes = std::sync::Arc::from(changed);
        font.id = Sha256::new().chain_update(font.bytes.as_ref()).chain_update(0u32.to_le_bytes()).finalize().into();
        assert!(B1Widths::read(&font, &subset).is_err(), "internally matching id is not resource admission");
    }

    #[test]
    fn b1_widths_byte_envelope_refuses_before_identity_work() {
        let subset = admitted_subset();
        for length in [0, 410819, 410821] {
            let mut font = source_font().clone();
            font.bytes = std::sync::Arc::from(vec![0u8; length]);
            font.id = [0; 32];
            let e = match B1Widths::read(&font, &subset) {
                Ok(_) => panic!("accepted length {length}"),
                Err(e) => e.to_string(),
            };
            assert!(e.contains("byte envelope differs"), "{length}: {e}");
        }
        // This checks the observable refusal reason and source order, not a
        // runtime timing guarantee or a digest-call counter.
    }

    #[test]
    fn b1_widths_fixed_face_upem_glyph_count_and_used_id_caps_stay_closed() {
        let subset = admitted_subset();
        for case in 0..7 {
            let mut font = source_font().clone();
            match case {
                0 => font.face = 1,
                1 => { let mut bytes = font.bytes.to_vec(); bytes[4096] ^= 1; font.bytes = std::sync::Arc::from(bytes); },
                2 => font.units_per_em = 0,
                3 => font.units_per_em = 1024,
                4 => font.num_glyphs = 0,
                5 => font.postscript_name = "other".into(),
                _ => { font.glyphs.insert(2620, solidcraft_geom::paper::text::FontGlyph { advance: 1, outline: vec![] }); }
            }
            assert!(B1Widths::read(&font, &subset).is_err(), "case={case}");
        }
    }

    #[test]
    fn b1_widths_missing_subset_ids_refuse_even_when_b1_has_other_widths() {
        let subset = admitted_subset();
        let provider = B1Widths::read(source_font(), &subset).unwrap();
        for gid in [3, u16::MAX] {
            assert!(provider.advance(gid).is_err(), "subset GID {gid}");
        }
    }

    #[test]
    fn b1_widths_binding_and_emission_leave_source_records_program_and_mapping_unchanged() {
        let subset = admitted_subset();
        let font = source_font().clone();
        let before_font = font.clone();
        let before_bytes = subset.bytes().to_vec();
        let before_mapping = subset.mapping().to_vec();
        let descriptor = synthetic_descriptor();
        let codes = [
            CodeToGlyph { code: 42, subset_gid: 2, logical: "alias1" },
            CodeToGlyph { code: 300, subset_gid: 2, logical: "alias2" },
        ];
        let a = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        let b = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_eq!(font, before_font);
        assert_eq!(subset.bytes(), before_bytes);
        assert_eq!(subset.mapping(), before_mapping);
        let prepared = prepare(input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        assert_eq!(prepared.widths, [(42, (277.832_031_25_f64 as f32)), (300, (277.832_031_25_f64 as f32))]);
    }

    // Owner DW decision: #7626 comment6534292604. These cover resource rows,
    // not a parent page-content emission or PDF-reader/native qualification.
    #[test]
    fn explicit_widths_cover_sparse_extreme_cids_without_a_notdef_record() {
        let subset = subset_font(BUNDLED, 0, &[3, 36], MAX_FONT_BYTES).unwrap();
        let font = b1_width_font(&[(3, 569), (36, 1366)]);
        let descriptor = synthetic_descriptor();
        let codes = [
            CodeToGlyph { code: 1, subset_gid: 2, logical: "A" },
            CodeToGlyph { code: u16::MAX, subset_gid: 1, logical: " " },
        ];
        let chunk = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).unwrap();
        let parsed = objects(chunk.as_bytes());
        assert_eq!(parsed.iter().map(|object| object.id).collect::<Vec<_>>(), [6, 7, 8, 9, 10, 11]);
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert_eq!(parse_two_width_rows(field).unwrap(),
            [(1, [666.992_187_5_f64 as f32]), (65_535, [277.832_031_25_f64 as f32])]);
        let map = object(&parsed, 11).stream.unwrap();
        assert_eq!(map.len(), 131_072);
        assert_eq!(&map[..2], &[0, 0], "unemitted CID0 stays a map placeholder, not an emitted width");
        assert_eq!(&map[2..4], &[0, 2]);
        assert_eq!(&map[131_070..], &[0, 1]);
        assert_eq!(object(&parsed, 9).stream.unwrap(), subset.bytes());
        assert_eq!(subset.mapping().first(), Some(&(0, 0)));
        let cmap = std::str::from_utf8(object(&parsed, 10).stream.unwrap()).unwrap();
        assert!(cmap.contains("<0001> <0041>") && cmap.contains("<FFFF> <0020>"));
        assert!(!font.glyphs.contains_key(&0));
    }

    #[test]
    fn an_explicit_zero_cid_cannot_borrow_a_width_from_the_program_notdef() {
        let subset = subset_font(BUNDLED, 0, &[36], MAX_FONT_BYTES).unwrap();
        let font = b1_width_font(&[(36, 1366)]);
        let descriptor = synthetic_descriptor();
        assert_eq!(subset.mapping().first(), Some(&(0, 0)));
        let codes = [CodeToGlyph { code: 0, subset_gid: 0, logical: "synthetic" }];
        let error = font_objects(refs(6), input_from_b1(&subset, &codes, &descriptor, &font)).err().unwrap().to_string();
        assert!(error.contains("lacks an intrinsic width for an emitted original glyph"), "{error}");
        let wrong_map = [CodeToGlyph { code: 0, subset_gid: 1, logical: "synthetic" }];
        let error = font_objects(refs(6), input_from_b1(&subset, &wrong_map, &descriptor, &font)).err().unwrap().to_string();
        assert!(error.contains("CID zero must map to subset glyph zero"), "{error}");
        assert!(!font.glyphs.contains_key(&0));
    }

    #[test]
    fn actual_f0_producer_plan_emits_complete_explicit_width_resources() {
        use solidcraft_geom::paper::{Group, Item, ObjectKind, Scene};
        let Some((runs, fonts)) = solidcraft_doc::sketch::paper_text(
            "AV", None, solidcraft_geom::Vec2::new(20.0, 30.0), 5.0, 0.0,
            &solidcraft_doc::sketch::TextStyle::default(),
        ).unwrap() else { panic!("nonempty supported text must produce records") };
        let mut scene = Scene::new(solidcraft_geom::Vec2::new(210.0, 297.0),
            vec![Group { object: 1, kind: ObjectKind::TitleBlock, items: runs.iter().cloned().map(Item::Text).collect() }]);
        scene.add_fonts(fonts);
        let prepared = scene.prepare().unwrap();
        let borrowed: Vec<_> = runs.iter().collect();
        let plan = super::super::text_plan::plan_prepared_text(&prepared, &borrowed, 1 << 20, MAX_FONT_BYTES).unwrap();
        let input = plan.font_input(*b"ABCDEF", 1 << 20);
        assert!(!input.source.font().glyphs.contains_key(&0));
        assert_eq!(input.source.subset().mapping(), [(0, 0), (36, 1), (57, 2)]);
        let program = input.source.subset().bytes().to_vec();
        // Explicit super calls the opaque production boundary, not this test
        // module's historical raw-component alias.
        let chunk = super::font_objects(refs(6), input).unwrap();
        let parsed = objects(chunk.as_bytes());
        assert_eq!(parsed.iter().map(|object| object.id).collect::<Vec<_>>(), [6, 7, 8, 9, 10, 11]);
        let cid = object(&parsed, 7).dict;
        assert!(!cid.contains("/DW"));
        let field = cid.get(cid.find("/W ").unwrap()..).unwrap().strip_suffix(">>").unwrap().trim_end();
        assert_eq!(parse_two_width_rows(field).unwrap(),
            [(1, [666.992_187_5_f64 as f32]), (2, [666.992_187_5_f64 as f32])]);
        assert_eq!(object(&parsed, 9).stream.unwrap(), program.as_slice());
        assert_eq!(object(&parsed, 11).stream.unwrap(), [0, 0, 0, 1, 0, 2]);
        let cmap = std::str::from_utf8(object(&parsed, 10).stream.unwrap()).unwrap();
        assert!(cmap.contains("<0001> <0041>") && cmap.contains("<0002> <0056>"));
        assert!(!prepared.font(&runs[0]).unwrap().glyphs.contains_key(&0));
    }

}


