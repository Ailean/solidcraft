# P2 F0 explicit widths — source-only v4 successor

**UNFORMATTED, UNCOMPILED, UNRUN, UNREGISTERED.**
This implements Cam's resolved width policy in our owned font-resource helper
only. It is not a completed PDF/text pipeline, registered integration, runtime
result or ready PR. The [v3 source archive](../F0-textplan-fontobjects/README.md)
at `45286e048983246ae1c7debf4e01e102762b2783` remains unchanged.

## Resolved policy and actual change

[Cam's decision on #7626](https://github.com/tangency-labs/solidcraft/pull/7626#issuecomment-6534292604)
requires explicit `/W` for every emitted CID, without relying on `/DW` or
inventing a B1 `.notdef` width. The embedded program's glyph zero remains a
separate embedding obligation. This follows the existing answer; review and
validation remain required.

Only `crates/io/src/paper/pdf/font_objects.rs` changes:
`393ed32f…` to `2c1aaa6f…`. It removes the default-width lookup/scalar,
propagation and explicit `default_width(...)` writer call. Its existing checked
per-code original-GID join and `/W` loop remain. The companion `text_plan.rs`
is byte-identical `eddb5120…`; no parser, source admission or public B1 field
is invented.

Every code in the **immutable opaque declared-code table** requires a real
checked B1 `FontGlyph.advance` and original `unitsPerEm`; the existing finite
width conversion is 1000*advance/unitsPerEm. Placed glyph offsets/advances remain
separate. Each declared code emits one explicit `/W` row, including a real zero
width. Missing original widths refuse. Sparse holes, an unused CID0 map placeholder
or an empty declared set do not invent widths.

Original0→subset0 and embedded FontFile2 program bytes remain unchanged. An
explicitly declared synthetic CID0 must still map to subset0 **and** have its own
actual source width; merely having a program `.notdef` cannot supply it.
Omitting `/DW` does not authorize undeclared codes or treat a reader's implicit
fallback as coverage. Future real page content must emit only planned codes and
separately prove every emitted CID is in the declared table.

## Exact source and routes

[Portable bindings](source-bindings.json) pin actual B1a
`16c9e9119214a3595b1303e2f1116cd067986c38`, B1b
`a46d46e95a7f5c7680ae7440d3c9989846947128`, actual source blobs/byte hashes and
the owned helper endpoints. These are exact source pins, not an assertion that
all upstream PRs are merged or that this is one qualified Git head.

- [Product delta from exact v3](deltas/product-from-v3.patch) replays against
  [before helper](before/crates/io/src/paper/pdf/font_objects.rs) to the
  [candidate helper](candidate/crates/io/src/paper/pdf/font_objects.rs).
- [Unchanged companion](candidate/crates/io/src/paper/pdf/text_plan.rs) is included
  for source continuity; its parent/producer boundary is not newly changed.
- [Acceptance delta](deltas/acceptance-owner-policy-from-v3.patch) replays the
  [old five-case appendix](before/acceptance/new-production-boundary-cases.rs)
  to the [new appendix](acceptance/new-production-boundary-cases.rs). Its logical
  path is an **owned appendix**, not Cam's complete `pdf_tests.rs`.
- Retained B1, F0 wrapper and negative fragments are unchanged copies; do not append
  them again if already composed. Normal integration belongs in the area's
  existing test file, with negative snippets kept separate. Earlier v3 alternative
  product patches are not additional layers on top of this v4 increment.

The opaque FontPlanView/getters, exact trusted builtin bytes/face checks,
original B1 identity/intrinsic-hmtx validation and single subset/closure/RAW
admission stay. Finite/metadata/descriptor/code/order/map/UTF/Ref/tag/budget
guards remain. `Font.embed`/F2, font substitution, assets, dependencies and
embedding rights are not supplied or broadened.

## Original acceptance and scope of review

[Acceptance inventory](acceptance-NOT-RUN.md): **49 ordinary source declarations
and four separate negative compile obligations, all UNRUN**.
Nineteen old helper test bodies are byte-identical; four DW-related helper bodies
are explicitly superseded by the owner decision, and the existing all-u16 case
is strengthened to inspect all 65,536 rows while retaining one-extra refusal.
Three new helper cases cover sparse extremes, missing explicit CID0 width and
the real pinned F0 producer's AV resource path. The five-case boundary appendix
keeps four old bodies and updates only the owner-authorized missing-GID0 case.
The 13 original B1, four F0 wrapper and four negative fragments remain unchanged.

Independent focal source review found no actionable defect in this owner-authorized
delta. Separate text/hash replay reconstructed both declared endpoints exactly.
This does not establish formatting, Rust typechecking, compilation, negative-test
diagnostics, produced PDF pages, actual reader/native results, full CI or readiness.

## Remaining integration

Cam's #9647 parent/content/Refs/global allocator window is untouched. Normal
modules/resources/text emission, every real emitted-code coverage, all-runs
grouping and aggregate bounds remain pending. Multi-glyph ActualText/extraction,
real RTL/reader evidence, coherent dependency/Cargo.lock integration, current
compiler/native/wasm/strict/full gates, actual PDF rendering/readback and old-byte
acceptance remain unrun. Future F2 and embedding/package licence compliance are
separate. The earlier owner decision is resolved; these pipeline gates are not.

Changes a default: no. Original Worker #1/Engineer-team proposal and analytic
cases preserve existing Cam/P2/font contributor authorship and notices under
MIT OR Apache-2.0. No font binary, asset, private path, full private proof,
controller, launcher, receipt or machine log is included.
