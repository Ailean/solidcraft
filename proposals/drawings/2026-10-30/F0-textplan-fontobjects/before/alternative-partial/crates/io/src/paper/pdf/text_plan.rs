//! Bounded PDF planning from the actual B1a Font/TextRun records (#4890 P2).
//!
//! This neither shapes text nor writes a page. It keeps intrinsic hmtx widths
//! separate from the producer's placed glyph x/rise/advance. One glyph per logical
//! cluster is supported, including a glyph carrying several characters; a
//! multi-glyph cluster is refused until its extraction/content policy is supplied.
//! The ToUnicode destination limit stays 512 UTF-16BE bytes, not a silent split.
//!
//! Plans retain no new font/document Arc: strings/font records are borrowed.
//! Local vector storage and returned subset bytes have separate allowances. The
//! existing subsetter/pdf-writer internal allocation limitations still apply.
//! No parent registration, global PDF Ref allocator, page resource or text arm is
//! introduced here. The parent must preserve the planned placements and reading
//! order and qualify real PDF rendering/extraction; these records prove neither.

use std::mem::size_of;
use std::ops::Range;

use pdf_writer::types::FontFlags;
use sha2::{Digest, Sha256};
use solidcraft_geom::paper::text::{Font, TextRun};
use solidcraft_geom::paper::MAX_COORDINATE_MM;

use super::font_objects::{CodeToGlyph, DescriptorMetrics, FontObjectInput};
use super::subset_font::{SubsetFont, subset_font};
use crate::{IoError, Result};

const MAX_RUNS: usize = 1024;
const MAX_GLYPHS: usize = 16_384;
const MAX_PLAN_BYTES: usize = 16 << 20;
const MAX_LOGICAL_BYTES: usize = 4 << 20;
const MAX_CLUSTER_BYTES: usize = 768;
const MAX_CLUSTER_UNITS: usize = 256;
const APPROVED_FONT_BYTES: usize = 410_820;

/// One actual glyph, with a code for its original GID AND literal logical text.
/// The matrix has unit basis vectors and a translation in sheet millimetres;
/// font_size_mm is on its run. Parent applies its mm-to-point scale only once.
pub(super) struct PlannedGlyph<'a> {
    pub(super) code: u16,
    pub(super) original_gid: u16,
    pub(super) source_glyph_index: usize,
    pub(super) intrinsic_advance_fu: u16,
    pub(super) placed_advance_mm: f64,
    pub(super) matrix_mm: [f64; 6],
    pub(super) logical: &'a str,
}

/// Glyphs occupy one contiguous range in the plan, in logical reading order.
/// For an RTL run their SOURCE indices descend; positions are never recomputed
/// from font widths. Input run order remains unchanged.
pub(super) struct PlannedRun<'a> {
    pub(super) glyphs: Range<usize>,
    pub(super) font_size_mm: f64,
    pub(super) rtl: bool,
    pub(super) text: &'a str,
}

pub(super) struct TextPlan<'a> {
    font: &'a Font,
    subset: SubsetFont,
    descriptor: DescriptorMetrics,
    codes: Vec<CodeToGlyph<'a>>,
    runs: Vec<PlannedRun<'a>>,
    glyphs: Vec<PlannedGlyph<'a>>,
}

impl TextPlan<'_> {
    pub(super) fn runs(&self) -> &[PlannedRun<'_>] {
        &self.runs
    }

    pub(super) fn glyphs(&self) -> &[PlannedGlyph<'_>] {
        &self.glyphs
    }

    /// Caller still supplies six globally reserved refs to font_objects, a
    /// globally unique subset tag, page resources and a cumulative output budget.
    pub(super) fn font_input(&self, subset_tag: [u8; 6], max_chunk_bytes: usize) -> FontObjectInput<'_> {
        FontObjectInput {
            source_font: self.font,
            subset: &self.subset,
            codes: &self.codes,
            descriptor: Some(&self.descriptor),
            postscript_name: self.font.postscript_name.as_bytes(),
            subset_tag,
            max_chunk_bytes,
        }
    }
}

fn invalid(message: &'static str) -> IoError {
    IoError::Invalid(format!("PDF text plan: {message}"))
}

fn reserve<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid("cannot allocate bounded plan storage"))?;
    Ok(values)
}

fn bytes<T>(count: usize) -> Result<usize> {
    count.checked_mul(size_of::<T>()).ok_or_else(|| invalid("plan storage overflow"))
}

fn total(parts: &[usize], limit: usize) -> Result<usize> {
    parts.iter().try_fold(0usize, |n, part| {
        n.checked_add(*part).filter(|n| *n <= limit).ok_or_else(|| invalid("plan storage allowance exceeded"))
    })
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let end = at.checked_add(2).ok_or_else(|| invalid("font table range overflow"))?;
    let value = bytes.get(at..end).and_then(|b| b.try_into().ok()).ok_or_else(|| invalid("truncated admitted font table"))?;
    Ok(u16::from_be_bytes(value))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).ok_or_else(|| invalid("font table range overflow"))?;
    let value = bytes.get(at..end).and_then(|b| b.try_into().ok()).ok_or_else(|| invalid("truncated admitted font table"))?;
    Ok(u32::from_be_bytes(value))
}

/// A bounded original hmtx view, not a general font parser. The subset adapter
/// performs the existing exact-resource/embedding admission before plan return.
fn table<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Result<&'a [u8]> {
    let count = usize::from(u16_at(bytes, 4)?);
    if count == 0 || count > 19 {
        return Err(invalid("font directory exceeds the reviewed resource"));
    }
    let mut found = None;
    for i in 0..count {
        let at = 12 + 16 * i;
        if bytes.get(at..at + 4) != Some(tag) {
            continue;
        }
        if found.is_some() {
            return Err(invalid("duplicate font table"));
        }
        let start = usize::try_from(u32_at(bytes, at + 8)?).map_err(|_| invalid("font offset exceeds the platform"))?;
        let length = usize::try_from(u32_at(bytes, at + 12)?).map_err(|_| invalid("font length exceeds the platform"))?;
        let end = start.checked_add(length).ok_or_else(|| invalid("font table range overflow"))?;
        found = bytes.get(start..end);
        if found.is_none() {
            return Err(invalid("font table is outside its bytes"));
        }
    }
    found.ok_or_else(|| invalid("required font table is absent"))
}

fn intrinsic_advance(font: &Font, gid: u16) -> Result<u16> {
    if gid >= font.num_glyphs {
        return Err(invalid("glyph is outside the admitted font"));
    }
    let count = usize::from(u16_at(table(&font.bytes, b"hhea")?, 34)?);
    if count == 0 || count > usize::from(font.num_glyphs) {
        return Err(invalid("invalid hmtx metric count"));
    }
    u16_at(table(&font.bytes, b"hmtx")?, usize::from(gid).min(count - 1) * 4)
}

fn descriptor(font: &Font) -> Result<DescriptorMetrics> {
    // Public Font is hostile input too. Refuse its cheap byte envelope before
    // hashing or looking through tables, even when the caller's plan budget is tiny.
    if font.bytes.len() != APPROVED_FONT_BYTES {
        return Err(invalid("font byte length is outside the reviewed resource envelope"));
    }
    // B1 hashes bytes + faceLE. The existing subset admission hashes RAW bytes;
    // their digests are different identities and are never compared to each other.
    let m = &font.metrics;
    // Original fixed-resource metadata, not fitted/default descriptor values.
    // Values come from this exact Liberation Sans head/hhea/OS2/post and l glyph.
    if font.bundled != 0 || font.face != 0 || font.postscript_name != "LiberationSans"
        || font.num_glyphs != 2620 || font.units_per_em != 2048
        || m.bbox != [-1114, -621, 2666, 2007] || m.ascent != 1854 || m.descent != -434
        || m.cap_height != 1409 || m.stem_v != 180 || m.italic_angle != 0.0
        || m.flags.fixed_pitch || m.flags.serif || m.flags.italic || m.flags.symbolic
    {
        return Err(invalid("B1 font identity or metrics differ from the reviewed regular bundled face"));
    }
    let identity: [u8; 32] = Sha256::new().chain_update(font.bytes.as_ref()).chain_update(font.face.to_le_bytes()).finalize().into();
    if font.id != identity {
        return Err(invalid("B1 bytes-and-face identity does not match its supplied bytes"));
    }
    let k = 1000.0 / f32::from(font.units_per_em);
    Ok(DescriptorMetrics {
        bbox_1000: m.bbox.map(|n| f32::from(n) * k),
        ascent_1000: f32::from(m.ascent) * k,
        descent_1000: f32::from(m.descent) * k,
        cap_height_1000: f32::from(m.cap_height) * k,
        italic_angle_degrees: m.italic_angle as f32,
        stem_v_1000: f32::from(m.stem_v) * k,
        flags: FontFlags::NON_SYMBOLIC.bits(),
    })
}

/// Check the actual B1 cluster partition and the narrower first P2 contract
/// BEFORE allocating or subsetting. Returning an error exposes no partial plan.
fn check_run(font: &Font, run: &TextRun) -> Result<usize> {
    if run.font != font.id || !(1..=4000).contains(&run.glyphs.len()) || run.text.len() > 4000 {
        return Err(invalid("run font, glyph count or text length is outside B1"));
    }
    let cap = run.scale * f64::from(font.metrics.cap_height);
    let within = |x: f64| x.is_finite() && x.abs() <= MAX_COORDINATE_MM;
    if !run.scale.is_finite() || !(1e-6..1e6).contains(&cap) || cap == 1e-6
        || !within(run.origin.x) || !within(run.origin.y)
        || !run.angle.is_finite() || run.angle.abs() > std::f64::consts::TAU
        || !run.slant.is_finite() || run.slant.abs() > 1.0
    {
        return Err(invalid("run transform is outside B1"));
    }
    let mut glyph_end = 0usize;
    let mut text_end = if run.rtl { run.text.len() } else { 0 };
    for cluster in &run.clusters {
        let (g0, g1, t0, t1) = (
            cluster.glyphs.start as usize, cluster.glyphs.end as usize,
            cluster.text.start as usize, cluster.text.end as usize,
        );
        if g0 != glyph_end || g0 >= g1 || t0 >= t1 || (if run.rtl { t1 != text_end } else { t0 != text_end }) {
            return Err(invalid("clusters do not partition glyphs and logical text in B1 order"));
        }
        if g1 - g0 != 1 {
            return Err(invalid("multi-glyph cluster extraction is not supported by this text-plan increment"));
        }
        let logical = run.text.get(t0..t1).ok_or_else(|| invalid("cluster text is not whole UTF-8 characters"))?;
        if logical.len() > MAX_CLUSTER_BYTES || logical.encode_utf16().take(MAX_CLUSTER_UNITS + 1).count() > MAX_CLUSTER_UNITS {
            return Err(invalid("cluster exceeds the existing ToUnicode 512-byte destination contract"));
        }
        let glyph = run.glyphs.get(g0).ok_or_else(|| invalid("cluster glyph is outside the run"))?;
        if ![glyph.x, glyph.rise, glyph.advance].into_iter().all(within) {
            return Err(invalid("placed glyph is outside B1 coordinates"));
        }
        let intrinsic = font.glyphs.get(&glyph.id).ok_or_else(|| invalid("glyph is absent from the B1 font table"))?;
        if intrinsic.advance != intrinsic_advance(font, glyph.id)? {
            return Err(invalid("B1 intrinsic advance does not match the admitted hmtx"));
        }
        glyph_end = g1;
        text_end = if run.rtl { t0 } else { t1 };
    }
    if glyph_end != run.glyphs.len() || text_end != if run.rtl { 0 } else { run.text.len() } {
        return Err(invalid("clusters do not cover the whole run exactly once"));
    }
    Ok(run.glyphs.len())
}

/// Resolve an F0 run through the already validated Scene table, never by a
/// bundled number or Vec index. This increment still plans ONE font and all
/// its runs; mixed-font input is rejected by the existing plan_text checks.
/// Preparation is not proof of PDF embedding permission or trusted advances;
/// the existing owned admission/provenance boundary remains mandatory.
pub(super) fn plan_prepared_text<'a>(
    prepared: &solidcraft_geom::paper::Prepared<'a>,
    runs: &[&'a TextRun],
    max_plan_bytes: usize,
    max_subset_bytes: usize,
) -> Result<TextPlan<'a>> {
    let first = runs.first().copied().ok_or_else(|| invalid("empty allowance or unsupported run count"))?;
    let font = prepared.font(first).ok_or_else(|| invalid("run font ID is absent from the prepared scene"))?;
    plan_text(font, runs, max_plan_bytes, max_subset_bytes)
}

/// One bundled font and ALL its runs, in their existing paint order. Code keys
/// are sorted (original GID, literal text), so iteration/export repetition cannot
/// change their allocation. Code zero stays reserved for .notdef, not user text.
pub(super) fn plan_text<'a>(font: &'a Font, runs: &[&'a TextRun], max_plan_bytes: usize, max_subset_bytes: usize) -> Result<TextPlan<'a>> {
    let limit = max_plan_bytes.min(MAX_PLAN_BYTES);
    if limit == 0 || max_subset_bytes == 0 || runs.is_empty() || runs.len() > MAX_RUNS {
        return Err(invalid("empty allowance or unsupported run count"));
    }
    let descriptor = descriptor(font)?;
    let mut count = 0usize;
    let mut logical_bytes = 0usize;
    for run in runs {
        count = count.checked_add(check_run(font, run)?).filter(|n| *n <= MAX_GLYPHS)
            .ok_or_else(|| invalid("combined glyph count exceeds 16384"))?;
        logical_bytes = logical_bytes.checked_add(run.text.len()).filter(|n| *n <= MAX_LOGICAL_BYTES)
            .ok_or_else(|| invalid("combined logical text exceeds its allowance"))?;
    }
    total(&[
        bytes::<(u16, &str)>(count)?, bytes::<u16>(count)?, bytes::<CodeToGlyph<'_>>(count)?,
        bytes::<PlannedGlyph<'_>>(count)?, bytes::<PlannedRun<'_>>(runs.len())?,
    ], limit)?;
    let mut keys = reserve::<(u16, &str)>(count)?;
    let mut used = reserve::<u16>(count)?;
    let mut codes = reserve::<CodeToGlyph<'_>>(count)?;
    let mut planned_runs = reserve::<PlannedRun<'_>>(runs.len())?;
    let mut glyphs = reserve::<PlannedGlyph<'_>>(count)?;
    total(&[
        bytes::<(u16, &str)>(keys.capacity())?, bytes::<u16>(used.capacity())?, bytes::<CodeToGlyph<'_>>(codes.capacity())?,
        bytes::<PlannedGlyph<'_>>(glyphs.capacity())?, bytes::<PlannedRun<'_>>(planned_runs.capacity())?,
    ], limit)?;
    for run in runs {
        for cluster in &run.clusters {
            let glyph = run.glyphs.get(cluster.glyphs.start as usize).ok_or_else(|| invalid("checked glyph disappeared"))?;
            if glyph.id == 0 {
                return Err(invalid(".notdef cannot replace a missing text glyph"));
            }
            let logical = run.text.get(cluster.text.start as usize..cluster.text.end as usize).ok_or_else(|| invalid("checked text disappeared"))?;
            keys.push((glyph.id, logical));
        }
    }
    keys.sort_unstable();
    keys.dedup();
    for (gid, _) in &keys {
        if used.last() != Some(gid) {
            used.push(*gid);
        }
    }
    if !font.glyphs.keys().copied().eq(used.iter().copied()) {
        return Err(invalid("the font table is not exactly the glyphs of the supplied runs"));
    }
    // Reuse the real existing admission, composite closure and subset library.
    // There is no duplicate raw-font admission hash or copied font implementation.
    let subset = subset_font(&font.bytes, font.face, &used, max_subset_bytes)?;
    for (index, (gid, logical)) in keys.iter().enumerate() {
        let code = u16::try_from(index + 1).map_err(|_| invalid("text codes exceed u16"))?;
        let at = subset.mapping().binary_search_by_key(gid, |(old, _)| *old).map_err(|_| invalid("subset lost an original glyph"))?;
        let (_, subset_gid) = subset.mapping().get(at).ok_or_else(|| invalid("subset mapping is incomplete"))?;
        codes.push(CodeToGlyph { code, subset_gid: *subset_gid, logical });
    }
    for run in runs {
        let first = glyphs.len();
        let (sin, cos) = run.angle.sin_cos();
        for logical_index in 0..run.clusters.len() {
            let at = if run.rtl { run.clusters.len() - 1 - logical_index } else { logical_index };
            let cluster = run.clusters.get(at).ok_or_else(|| invalid("checked cluster disappeared"))?;
            let source = cluster.glyphs.start as usize;
            let glyph = run.glyphs.get(source).ok_or_else(|| invalid("checked glyph disappeared"))?;
            let logical = run.text.get(cluster.text.start as usize..cluster.text.end as usize).ok_or_else(|| invalid("checked text disappeared"))?;
            let key = keys.binary_search(&(glyph.id, logical)).map_err(|_| invalid("text code is absent"))?;
            let code = codes.get(key).ok_or_else(|| invalid("text code is outside its table"))?.code;
            let matrix_mm = [
                cos, sin, cos * run.slant - sin, sin * run.slant + cos,
                run.origin.x + cos * glyph.x - sin * glyph.rise,
                run.origin.y + sin * glyph.x + cos * glyph.rise,
            ];
            if !matrix_mm.iter().all(|n| n.is_finite()) {
                return Err(invalid("glyph transform is not finite"));
            }
            glyphs.push(PlannedGlyph {
                code, original_gid: glyph.id, source_glyph_index: source,
                intrinsic_advance_fu: intrinsic_advance(font, glyph.id)?, placed_advance_mm: glyph.advance,
                matrix_mm, logical,
            });
        }
        planned_runs.push(PlannedRun {
            glyphs: first..glyphs.len(), font_size_mm: run.scale * f64::from(font.units_per_em), rtl: run.rtl, text: &run.text,
        });
    }
    Ok(TextPlan { font, subset, descriptor, codes, runs: planned_runs, glyphs })
}
