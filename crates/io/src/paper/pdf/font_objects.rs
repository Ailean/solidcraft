//! Private PDF font resources for one admitted subset; no B1 or content writer.
//!
//! Identity-H explicitly maps two-byte codes to CIDs. A separate stream maps
//! CIDs to subset GIDs; logical text remains keyed by code in real ToUnicode.
//! Descriptor metrics must come from the SAME admitted source font. Numeric
//! validation does not prove that provenance, and no metrics are estimated.
//!
//! pdf-writer 0.15.0 allocates internally without fallible allocation controls.
//! This returns a local Chunk, never mutating a caller-owned Pdf on Result errors.
//! A later extend has no rollback or OOM guarantee. Caller owns global Ref/tag
//! reservations, page resources, content, positioning and document budgets.

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Chunk, Name, Rect, Ref, Str};

use super::subset_font::SubsetFont;
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

/// Pure IO parameters, not a fabricated B1 record or global resource allocator.
pub(super) struct FontObjectInput<'a> {
    pub(super) subset: &'a SubsetFont,
    pub(super) codes: &'a [CodeToGlyph<'a>],
    pub(super) descriptor: Option<&'a DescriptorMetrics>,
    pub(super) postscript_name: &'a [u8],
    pub(super) subset_tag: [u8; 6],
    pub(super) max_chunk_bytes: usize,
}

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(message.into())
}

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
    default_width: f32,
    name: [u8; 21],
    descriptor: &'a DescriptorMetrics,
    flags: FontFlags,
    limit: usize,
    bound: usize,
}

fn prepare(input: FontObjectInput<'_>) -> Result<Prepared<'_>> {
    let descriptor = input.descriptor.ok_or_else(|| invalid("essential font descriptor metrics are missing"))?;
    let flags = descriptor.validate()?;
    let name = fixed_font_name(input.postscript_name, input.subset_tag)?;
    if input.codes.len() > MAX_CODES {
        return Err(invalid("PDF font code count exceeds 65536"));
    }
    let metrics = MetricView::read(input.subset)?;
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
        default_width: metrics.advance(0)?, name, descriptor, flags, limit, bound,
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
    refs.validate()?;
    let prepared = prepare(input)?;
    let Prepared { font, unicode, cid_map, widths, default_width, name, descriptor, flags, limit, bound } = prepared;
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
            .font_descriptor(refs.descriptor).default_width(default_width)
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
    use super::super::subset_font::subset_font;

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

    fn input<'a>(subset: &'a SubsetFont, codes: &'a [CodeToGlyph<'a>], descriptor: &'a DescriptorMetrics) -> FontObjectInput<'a> {
        FontObjectInput {
            subset, codes, descriptor: Some(descriptor),
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
        assert_eq!(prepared.default_width, 750.0); //1536*1000/2048.
        assert_eq!(prepared.widths, [(42, empty_width), (300, 750.0)]);
        // Subsetter compresses the repeated569 width of the two empty glyphs.
        let view = MetricView::read(&subset).unwrap();
        assert_eq!(view.advance(1).unwrap(), empty_width);
        assert_eq!(view.advance(2).unwrap(), empty_width);
        let chunk = font_objects(refs(6), input(&subset, &codes, &descriptor)).unwrap();
        let parsed = objects(chunk.as_bytes());
        let cid = object(&parsed, 7).dict;
        assert!(cid.contains("/DW 750"));
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
}


