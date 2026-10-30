# P2/B1 typed text-plan increment

**SOURCE ONLY — UNFORMATTED, UNCOMPILED, UNRUN, UNREGISTERED.** This is preserved WIP for [PDF #7626](https://github.com/tangency-labs/solidcraft/pull/7626), not finished PDF text support or a ready-for-review implementation. No user-visible default changes.

The patch adds `crates/io/src/paper/pdf/text_plan.rs` and thirteen original cases in the existing `pdf_tests.rs`. It consumes the actual B1 records from [#9512 at `8da9e989`](https://github.com/tangency-labs/solidcraft/pull/9512) and the actual resolver contract from [#9513 at `df3eff1c`](https://github.com/tangency-labs/solidcraft/pull/9513). Immutable heads, file blobs, byte hashes and dependency pins are in [source-bindings.json](source-bindings.json). These are separate draft branches; their composition has not been compiled.

## Included behavior

The typed plan keeps intrinsic `FontGlyph.advance` in font units separate from placed x/rise/advance in sheet millimetres. It preserves rotation, slant, source glyph positions and explicit RTL information without reshaping. Logical aliases of one GID have distinct PDF codes; repeated identical keys reuse a code. B1's bytes-plus-face identity is checked separately from the admitted font's raw-byte identity. The exact supported regular Liberation Sans face and descriptor remain unchanged.

Only one glyph per logical cluster is currently supported. Several characters carried by one glyph retain their literal string; a cluster with several glyphs refuses explicitly. Complete-cluster ToUnicode bounds remain intact. No text is duplicated across every glyph, silently split, normalized or claimed to be extractable from a real PDF. Local plan/subset allowances are explicit; they do not establish whole-process memory bounds.

The original thirteen written cases cover these distinctions, positions and RTL order, mapping repeatability, space-without-outline, identity/metrics/partition/finite checks, local storage limits, and byte-envelope refusal before digest work. Independent source review closed the admission-order finding in the preserved source; this is not type-checking or executed acceptance evidence.

## Compatibility scaffold, not the final B1 width migration

The six existing P2 helpers exactly match the published [P2 checkpoint `bf2a6fd5`](https://github.com/tangency-labs/solidcraft/tree/bf2a6fd567b03e1d25dcb66624e719a0cec109fc/crates/io/src/paper/pdf). This increment reuses their legacy `MetricView` and checks original `hmtx` provenance. It **does not implement** the final design described in the earlier [P2/B1 adaptation notes](../P2-B1-adaptation-notes.md): the resource emitter still has to consume validated B1 intrinsic widths, preserve mapping/subset safety, and remove its local metric parser. The old note's statement that B1 code was not yet delivered is historical; the actual B1 heads are now pinned here. The owner-set intrinsic-width authority remains B1, not placed advances.

The `.notdef`/`DW` policy and complete width coverage of emitted CIDs are still integration decisions. This packet neither invents B1 fields nor claims those decisions are settled. The next positive multi-glyph acceptance gap is described separately in [multi-glyph-acceptance-next.md](multi-glyph-acceptance-next.md).

## Applying the preserved work later

`text-plan-and-original-tests.patch` is bound to the **25-case P1** `pdf_tests.rs` at `506d419084a1cd6463c3117ea34d15f805a6ce37`, whose entire raw prefix is unchanged. The already published P2 checkpoint has **52 additional cases**: six are an unchanged suffix in its current 31-case `pdf_tests.rs`; 46 are in the existing helper modules. Do not replace the current test file with this older-base endpoint or lose any helper cases: retain that six-case suffix and compose the helper and the exact [original thirteen-case suffix](original-tests.rs) into the selected current source. The expected names are in [expected-tests-NOT-RUN.txt](expected-tests-NOT-RUN.txt). Ninety total declarations would result from retaining all 25 + 52 + 13 cases; that arithmetic is not an execution result.

Cam owns the many-page `paper_pdf`/`collect_layers`, content and common object-allocator window under [the agreed split](https://github.com/tangency-labs/solidcraft/pull/7626#issuecomment-6530265408). This increment supplies no parent module registration, font references, page resources, fixed ref numbers, subset tags, text-content emission or edits to that window. No shared writer is reserved by this archived increment.

Before production adoption: select and bind the actual composed B1/P2/many-page source; finish the width-provider and content seams; resolve the existing subsetter dependency and inspect the real lock delta; integrate normal module registration; preserve original tests; then use the sole builder for affected native/wasm/strict checks and the actual repository gate. Independent PDF rendering, reader extraction, multiple-page/font/layer and xref acceptance remain required. No old pass transfers to this source.

Original adapter and acceptance cases: Engineer team for 0xlillith. Referenced B1 and PDF code retains its existing authorship; no external implementation, private evidence, font binary or new asset is included.
