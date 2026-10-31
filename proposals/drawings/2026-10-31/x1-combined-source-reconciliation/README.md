# X1 combined source reconciliation: eleven paths

**Source-only, unformatted, uncompiled and unrun. Cargo.lock unresolved; whole-workspace API compatibility unqualified. Not a standalone validated main tree, a whole-PR rebase, resolved mergeability or readiness.**

This archive combines nine original-X1 prerequisite candidates with the exact immutable caller/test pair from the [two-file parent archive](../x1-main-reconciliation/README.md). It is an equivalent combined alternative, not a sequential follow-up patch to apply after that parent. Apply the sole [combined-source.patch](combined-source.patch) once to the exact frozen-main before source; do not apply the parent deltas again. Before/candidate snapshots are for inspection and replay, not additional patches.

Frozen main is `8e9e2700275a363ac63ee5178aca7e33fac06722`. Original X1 is head `3fb2eef92dce4bafe9cfb3f68a8334985d4d83ee` over merge base `0312808b92d4523894d2af57c99401c65886e24d`; its reviewed caller successor is archive `28d90fae50dc9b060434a393305b0c93dff0444a`. Only the legitimate original-X1 source delta is reconciled. The historical 459-path divergent comparison was not imported. [Portable bindings](source-bindings.json) distinguish Git commit identifiers from RAW file SHA256 values and record every before/candidate state.

## Source supplied and absent-file status

The eleven paths consist of the immutable parent `sheet.rs`/`sheet_dxf_tests.rs`, plus doc sheet conversion, engine manifest, geom lib/profile/tests, IO lib/writer and two added IO path files. Nine modified files have literal before snapshots. `crates/io/src/dxf_out/path.rs` and `crates/io/tests/dxf_paths.rs` were absent at frozen main: their zero-byte before snapshots are explicitly marked `existed=false` and `placeholder_only=true`, never evidence that empty source files existed. The patch creates both through `/dev/null` and `-0,0`; their candidate bytes are exactly original X1.

Five modified prerequisite paths preserve unique original contexts; four independently commute with nineteen foreign hunks. Two export conflicts are resolved only at their existing lines: geom retains `Similarity2` while adding original `EllipseArc`/`conic_ellipse`; IO uses the original X1 `DxfPath`/`DxfPathStyle`/`DxfVertex`/`path_shapes` export migration, preserving current surrounding code. The entire document candidate equals original X1, including its directed-turn certificate. Existing `Out::num` is unchanged; no X4 number policy is introduced.

## Preserved contracts and public API qualification

Original strict generic recognizer/extraction bodies and all 47 test bodies added by original X1 remain literal. Five common tests differ from old X1 because frozen main already changed them; this reconciliation preserves those current-main bodies exactly. The parent pair remains byte-identical, including 53 original expected output tuples and its 18-test prefix plus seven caller cases. There are 117 source declarations in the nine prerequisites and 25 in the parent: 142 total, including inherited and conditional tests. This is not 142 new or executed tests.

The caller still admits style and every original piece through the shared bounded path converter before its already agreed local CIRCLE merge. Generic strictness, original limits/refusals, fixed pin expectations and the producer turn helper are not relaxed or removed. The original five-entity trimmed-cap acceptance remains required.

The public removal of `loop_shapes`/`seg_shape` is inherited from original X1; it is a real API change. Captured internal test calls use local test-only wrappers. No dangling consumer was demonstrated in captured source, but the callgraph is partial and does not prove complete frozen-main public-API or exhaustive-enum consumer coverage. Actual affected compilation and suites must qualify that compatibility. This archive supplies no new fallback or speculative adapter.

## Remaining dependency and execution work

`Cargo.lock` is deliberately not a candidate or part of the combined patch. The engine manifest adds the original workspace `sha2` dev-dependency; an existing root `sha2 = "0.10"` and lock package `0.10.9` do not resolve the changed engine dependency by themselves. Actual resolver execution, lock-delta inspection and exact-source pinning remain required before `--locked` or readiness claims.

Then qualify the same coherent candidate with the original seven sheet regressions, seven caller boundary/admission cases, unchanged 53 pins, affected doc/geom/IO/sheet/view/API suites, strict checks, wasm and full repository CI, plus actual observable output. No old PASS transfers. No formatter, compiler, test/oracle, app/GPU, merge or readiness result is claimed here. Kernel, Cam's view receiver/current-view migration, X2/X3/X4, A8/paper/picture/multipage work and other contributors' windows remain outside the authored changes.

Existing caller policy references: [Cam's decision](https://github.com/tangency-labs/solidcraft/pull/6184#issuecomment-6522562149) and [prior ACK/source review](https://github.com/tangency-labs/solidcraft/pull/6184#issuecomment-6522733382). No new compatibility choice is introduced.

Original X1 source is credited to the 0xlillith team in PR6184. Worker1 prepared source composition and the two explicit export merges; existing SolidCraft contributors' foreign source stays preserved. Existing MIT OR Apache-2.0 licensing and attribution remain intact. No external CAD code/assets or private control/proof files are bundled.
