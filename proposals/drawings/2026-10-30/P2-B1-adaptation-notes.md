# Source-only P2/B1 adaptation notes

The owner-proposed B1 intrinsic-width contract is resolved; actual B1 code is not yet delivered. These notes inspect existing private P2 code and introduce no API or implementation. The DW policy at the end is still under review: ISO32000-1 Table117 makes DW optional with default1000; complete explicit W coverage of emitted CIDs may remove the need for a separate .notdef width. This does not excuse missing widths for any emitted code or an absent content-coverage check. See the [official PDF standard](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf). No implementation/test result is claimed.

# P2 adaptation delta to Cam's proposed B1 font records

Private design/source delta only. No new signature, public API, record implementation, P2 source edit or executable B1 provider is supplied. No compiler, formatter, Cargo, renderer, GUI, Git or GitHub write.

## Authority and exact inspected sources

Cam's [B1 records before code](https://github.com/tangency-labs/solidcraft/issues/4890#issuecomment-6517001980), amended by [the concrete advance-width answer](https://github.com/tangency-labs/solidcraft/issues/4890#issuecomment-6518550794). The width question is ANSWERED. These are owner-proposed records; Cam explicitly says no code has been built.

Current proposed resource fields relevant here:
- Font: id, bytes, face, postscript_name, num_glyphs, units_per_em, metrics, glyphs.
- Font.glyphs: BTreeMap<u16, FontGlyph>, keyed by ORIGINAL GID from the exact admitted face.
- FontGlyph: advance:u16 (intrinsic hmtx font units), outline:Vec<Vec<Seg2>>.
- TextRun: font table index, scale/origin/angle/slant, placed glyphs, logical text, clusters and rtl. PlacedGlyph.advance remains a layout result, not a PDF font width.

Pinned private implementations:
- font_objects.rs: private source checkpoint; exact SHA256 follows.
  SHA256 aef06369bad659706b1a30ebedfce1b2d8e89eaa4849e98d618bd3cf9ecb1eb1.
- subset_font.rs: private source checkpoint; exact SHA256 follows.
  SHA256 a9808063f2223fd9e86e208d55092b6b9e9a5ae7ba0eaba2dce5d557a7fd5f26.
- glyph_closure.rs: private source checkpoint; exact SHA256 follows.
  SHA256 a8d94c9996a5c718346e66d1983e6d2f2edb224286d3f45e92d731ab0013f1d8.

All references below bind those source bytes, not a future B1 revision.

## Exact emitter delta

| Current emitter site | Replacement responsibility after actual B1 delivery |
| --- | --- |
| lines155-200: MetricView fields/read scan subset head/hhea/hmtx/maxp; obtain unitsPerEm/numberOfHMetrics | Writer no longer reads a font program to obtain widths. Use the validated B1 Font.units_per_em and FontGlyph.advance; retain non-parser mapping/count/size guards separately. |
| lines203-214: MetricView.advance(subset_gid), including compressed-tail lookup | Resolve the matching ORIGINAL GID through SubsetFont.mapping, then use that original GID's B1 FontGlyph.advance. The resolver already owns the last-hmtx-entry rule. Scale by1000/units_per_em, keeping finite/range checks. Zero advance is valid. |
| line263: MetricView::read(input.subset) | Replace its metric-provider role with a validated same-resource bridge. Do not simply delete all validation hidden inside this call. |
| line280: per-code advance call during admission | Validate CID/subset-GID mapping and required original-GID B1 width presence before code-derived allocations; do not fall back to0 or infer a width from text/placed advance. |
| line298: widths.push((CID, metrics.advance(subset_gid))) | Same CID key; intrinsic width comes from B1 original-GID entry, through the validated subset mapping. Keep distinct CIDs sharing a GID, with identical intrinsic width but distinct logical mappings. |
| line315: default_width=metrics.advance(0) | Separate .notdef/default-width policy; B1's exact-used-glyph table does NOT promise GID0 when it isn't used in a run. See below. |
| lines124-153: part/word/dword/table | All production uses in this emitter belong to MetricView. After its remaining validation duties and default-width policy are resolved, this whole local SFNT byte-reader group can disappear from font_objects.rs. |

Keep the emission layer: Prepared.widths remains CID-indexed, and the existing W writer at lines346-348 emits each CID row. Type0/Identity-H, CIDToGIDMap, six reserved references, descriptor serialization, ToUnicode and local byte-bound preflight are separate responsibilities. No font re-layout or parsing is added to them.

## GID join and resource identity

Current SubsetFont.mapping stores (original_gid, subset_gid), ordered by original GID and dense by subset GID. Current CodeToGlyph rows contain only subset_gid. B1 Font.glyphs is keyed by original_gid. Therefore B1 glyphs[subset_gid] is WRONG after renumbering.

For each emitted CID: validate its subset_gid, obtain the corresponding original_gid from the checked mapping, require that original glyph's B1 entry, then use its advance. Preserve the original->subset relation for embedding and CIDToGIDMap. The B1 resource bytes/face/id must match the source supplied to subsetting; merely sharing a family string or a PostScript name is insufficient. Use checked lookups, bounded joins and existing caps, with no missing-entry fallback.

Composite components and .notdef are added by subsetting closure but may not appear in TextRun. They need embedded glyph programs, not invented scene FontGlyph entries or Unicode text. Require B1 widths for emitted glyphs; do not demand the entire closure appear in B1's exactly-used table. This distinction also prevents a false 'missing width' refusal for non-emitted components.

## Do not remove these guards with MetricView

MetricView::read currently mixes font parsing with ordered/dense mapping, original-GID0, subset-size and count guards (lines164-177). Keep those invariants using the immutable subset artifact/factory; do not treat removal of metric parsing as permission to trust an arbitrary mapping or caller-constructed record.

subset_font.rs production lines47-90 already owns glyph_closure, remapping, actual subsetter call and check_subset_program, returning immutable bytes/mapping. This is a subset-production/validation stage, not the PDF resource emitter.

glyph_closure.rs check_subset_program (lines485-528) still checks the returned program, including maxp/count, required tables, hhea.numberOfHMetrics and hmtx structural length. KEEP that structural safety check; a B1 advance table does not prove the subsetter returned a valid program. Independent test-side hmtx read-back also remains legitimate acceptance evidence. Neither should be confused with the emitter deriving W by parsing hmtx.

The private emitter's repeated maxp parse can move out only because the actual factory already validates the subset's count; preserve its trust/immutability boundary. Its hard-coded2048 read becomes provider units_per_em, but that does NOT automatically broaden the fixed-font admission. Current resource policy still admits the exact Liberation Sans bytes/face and TrueType FontFile2; arbitrary fonts, variable instances, CFF and installed fonts remain unsupported until separately implemented/reviewed.

## .notdef and DW: actual remaining integration decision

The old emitter always reads subset GID0 to write its default width, even with no code rows. The new B1 table contains exactly glyphs used by runs; it may contain neither original GID0 nor composite-only glyphs. It is incorrect to index a nonexistent B1 glyph0 or silently supply width0.

Before deleting the last metric-reader use, the emitter must choose an explicit default-width strategy with real content/CID coverage, OR consume separately admitted .notdef metrics from the appropriate resource/subsetting stage. The present B1 proposal does not itself provide that extra value. This is an identified private integration dependency, not a new public request or a reason to reopen Cam's answered intrinsic-width question. No option, new record field or public function is invented here.

## Metrics and actual code still required

FontGlyph.advance resolves the width authority, but no compiled B1 scene/resolver/builder exists in the inspected handoff. Actual adoption depends on:
- final code/revision and validated resource lifetime: same bytes/face, stable id, bounded fields and font-unit values that cannot drift from the admitted resource;
- the resolver's real hmtx-tail implementation and tests, including zero-width glyphs; missing glyphs refuse instead of using sketch's replacement;
- final validated Font/TextRun/Cluster types and caps; logical/visual order and per-run glyph identity, without writer re-layout;
- mapping Font.metrics from font units into the existing1000-unit DescriptorMetrics, with units_per_em checked before division, explicit flags/angle units/StemV semantics and unchanged finite/bound checks;
- actual bundled-only resolver provenance/licences and supported static TrueType face; no system/path fallback;
- the subset mapping/closure bridge and explicit DW decision above;
- P2 page resource registration, globally reserved object refs/tags, two-byte content codes, positioned glyph content, ToUnicode/ActualText policy and aggregate budgets.

The current emitter's W checks do not qualify any of these absent consumers. The new B1 proposal also does not turn our old test-only P2 module registration into production PDF text support.

## Original acceptance to retain when the real bridge is implemented

1. Same fixed source widths: original GIDs2/3 have advance569 and GID0 has1536 at2048units/em. Keep literal expected W values569000/2048 and750; do not derive them from emitter output or placed advances.
2. Original/subset/CID separation: the existing fixture maps originals0,2,3 to subsets0,1,2. A CID selecting subset2 must look up ORIGINAL3 in B1, not key2 by coincidence. Add distinct source advances in a separate synthetic bridge fixture so a wrong-key join cannot pass merely because both old widths are569.
3. Two CIDs using the same original GID with different logical clusters retain equal W and distinct logical extraction; layout advances can deliberately differ without changing W.
4. Advance0 remains0; missing original-GID entry, wrong font identity/face, zero units_per_em and invalid mapping refuse before partial resource emission.
5. Preserve subset closure/structural checks, CID ordering, local byte bounds, sparse-code map bounds and original ToUnicode serializer rejection cases.
6. For the provider's real code, independently verify compressed hmtx-tail widths and all admitted fonts against their exact byte/face source. A source document is not an executed provider check.
7. Exercise the chosen .notdef/default-width policy with an empty run set and with a scene that never uses GID0; do not force extra glyphs into the B1 exactly-used table.
8. Actual positioned page content plus independent extraction/rendering must qualify the final PDF separately. Metadata Unicode or a resource-only six-object Chunk is not searchable drawing text acceptance.

## Handoff

The removable metric reader is now located precisely, and the same-font intrinsic-width dataflow is settled. Keep this as a delta against the pinned private emitter until actual B1 code and the remaining private default-width/bridge decisions are present. No source/caps/assertions/dependencies/licences changed. W10v2 and both PDF reproducer proposals remain immutable and unrun.
