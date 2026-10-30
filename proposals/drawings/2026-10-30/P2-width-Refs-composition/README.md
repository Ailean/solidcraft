# P2 additive composition: B1 widths + actual #9647 references

SOURCE ONLY — UNFORMATTED, UNCOMPILED, UNRUN, UNREGISTERED. This is a new private composed successor. The width 9379, Refs 8ebe and compatibility b68 packets remain immutable. No Cam parent function, shared source, index, Git/GitHub, dependency/lock, module registration, build or runtime was changed.

## What was composed

The B1 width-provider `font_objects.rs` is the starting point. The exact 28-line Refs reservation adapter is added to it; removing that exact adapter reproduces the entire width-provider candidate byte for byte. We did **not** replace the helper with the older full Refs candidate and restore production MetricView. Production widths still use the actual `source_font`, map subset GIDs back to original GIDs, and read `FontGlyph.advance / units_per_em`. The old SFNT reader remains only inside the test module as its unchanged independent oracle. Source/face identity checks, missing-width errors and the strict missing-GID 0 DW refusal remain unchanged.

The added private reservation function uses Cam's actual `Refs` type/`next()` six times, validates with the existing helper policy, and commits its staging cursor only after all six IDs are admissible. No second persistent allocator, fixed 6+layers arithmetic, duplicated cap, font-tag policy or parent call site is added.

The text-plan consumer is the exact width-successor copy, whose sole change from b68 is `source_font: self.font` in `FontObjectInput`. Its actual B1 positions, intrinsic/placed distinction and compatibility checks are not edited here.

## All written acceptance preserved

- Helper: all 11 earlier test bodies and the 13 original width-provider cases are byte-identical to the width successor: 24 declarations.
- Existing `pdf_tests.rs`: Cam's RAW 28-case source plus the exact 8 Refs cases form the unchanged prefix. The original 6 P2 appendix cases and original 13 compatibility-plan cases then append unchanged: 55 declarations.
- The remaining unchanged P2 helper modules contain 35 further declarations. A later normally registered assembly retaining those modules therefore has 114 declarations in these components. This is source accounting, **not** a compiler inventory or executed test count.

No original test, assertion, expected value, tolerance, ignore or seed was changed. Separate appendix copies show exact provenance; do not append the 13 width cases again, since they already live in the helper. The old standalone helpers/packet endpoints must not overwrite this composed source during adoption.

## Baselines and owner window

Actual B1a/B1b sources remain 9512`8da9e989` / 9513`df3eff1c`. Cam's actual many-page source is 9647`ce505658`, sole parent 506d4190, whose Refs allocates catalog/tree, page/content pairs, Info, then shared layers. The canonical old P2 helper is bf2a6fd5. Exact source hashes and blobs are in `source-bindings.json`.

The cumulative patch is deliberately bound per file: helper against canonical P2, consumer against the b68 typed plan, and tests against actual Cam 9647. These are not yet one integrated Git base. The separate deltas show adding Refs to the width helper and adding widths to the old Refs helper. They are alternatives for inspecting the same final helper, not sequential patches to apply twice. The tests delta from the Refs candidate preserves its complete RAW prefix and appends older owned cases.

Cam's `paper_pdf`, `paper_pdf_pages`, `Refs`, `collect_layers`, page resources and content remain untouched; their pinned copy is read-only reference evidence. The adapter is not wired into his builder. Parent font tags/resources, aggregate limits, actual emitted-CID coverage, prepared/grid text preservation and real page content are still integration work.

## Explicit blockers retained

1. **Default width:** our existing owner question 6532370208 is still pending at the recorded live read. The actual B1 table contains only placed glyphs and normally lacks unused GID 0. Font-object preparation still refuses that case; no fallback, extra B1 field, unused glyph insertion or new DW policy was invented. No duplicate public question was posted.
2. **Trusted font records:** bytes/face identity does not attest arbitrary JSON/mutated B1 advances. Production use needs the real trusted B1 producer/rehydration boundary. No new provenance API or second production hmtx authority is invented here.
3. **Font/text emission:** no composed parent resource/content/allocator registration or full content/CID proof exists. Multi-glyph ActualText remains explicitly unsupported by the preserved typed plan; layout/RTL/extraction are not qualified.
4. **Old one-page bytes:** source tracing supports unchanged one-page inputs/write order, but actual 506d-versus-ce505 output comparison remains unrun. Cam's wrapper-versus-builder test compares two routes to the new implementation, not a frozen old-source output. Source/text identity is not PDF-byte acceptance.
5. **Execution:** actual source/toolchain/dependency-lock admission, independent composed review, sole-builder compiler/native/wasm/strict/repository gate, PDF-reader/render/extraction and final merge checks remain pending. No earlier pass or resource grant transfers to this source.

Changes a default: no. Original team adapter/cases preserve existing B1, P2 and Cam authorship. No new asset, dependency, external implementation or commercial CAD data is included.

## Archive status

This README records proposal preparation. The subsequent independent source
composition review found no additional actionable composition defect. Publishing
these files preserves the code; it does not satisfy the dependency, trust,
registration, emitted-font or execution gates listed above. No complete checkout
or single integrated Git base is supplied by this three-path patch.
