# X1 two-file reconciliation on frozen main

**Source-only, unformatted, uncompiled and unrun. Not standalone main, a whole-PR rebase, resolved mergeability or readiness.**

This archive composes the two owned X1 caller/test files onto frozen main `8e9e2700275a363ac63ee5178aca7e33fac06722`. The original X1 head is `3fb2eef92dce4bafe9cfb3f68a8334985d4d83ee` over merge base `0312808b92d4523894d2af57c99401c65886e24d`; the exact existing caller successor is archive `28d90fae50dc9b060434a393305b0c93dff0444a`. No new compatibility choice or generic tolerance is introduced.

## Payload and application

[Portable bindings](source-bindings.json) list both exact before/candidate snapshots and RAW deltas. Apply each delta once to its matching exact before file. The paths are disjoint; snapshots are for inspection and replay, not additional patches. Only `crates/engine/src/cmd/sheet.rs` and `crates/engine/src/cmd/sheet_dxf_tests.rs` are supplied. Source SHA256 values identify bytes; the four commit identifiers above identify Git history.

Independent RAW replay reconstructed the old-head-plus-successor source, transplanted six unique owned hunks and restored the originals with inverse deltas. Twelve foreign sheet-code hunks commute over the successor to the exact candidate. The foreign main payload is preserved. This integrity evidence is not runtime validation.

## Preserved behavior and acceptance

The shared path converter still validates style, every original piece and its 200000-piece cap before the flat-pattern-only CIRCLE merge. The historical caller retains the first support, strictly-less-than `1e-9` mm center/radius differences, common sweep sign and strictly-less-than `1e-9` rad difference from one turn. Generic recognition stays strict; JSON retains the historical caller helper. Layers/order/bends/options/writer behavior is outside the new compatibility scope.

All 53 original expected name/length/SHA256 tuples remain literal. The full 18-test original-head prefix and exact seven-case successor appendix are retained: 25 declarations, all unrun for this candidate. The pre-existing OUTLINE assertion's migration to `path_shapes` is already in original X1, not a new expectation adjustment in this reconciliation. The original trimmed-cap five-entity acceptance and producer turn helper remain required; their removal is not part of this unit.

## Required integration remains separate

The captured legitimate merge-base-to-X1 comparison lists 12 paths. The historical 459-path divergent comparison was not imported. Only two paths are reconciled here; the other ten original X1 integration/acceptance paths still require preservation-aware reconciliation:

- `Cargo.lock`
- `crates/doc/src/sheet_convert.rs`
- `crates/engine/Cargo.toml`
- `crates/geom/src/lib.rs`
- `crates/geom/src/profile.rs`
- `crates/geom/src/tests.rs`
- `crates/io/src/dxf_out.rs`
- `crates/io/src/dxf_out/path.rs`
- `crates/io/src/lib.rs`
- `crates/io/tests/dxf_paths.rs`

Frozen main lacks the required X1 path exports/converter, original recognizers, engine test dependency and producer turn helper at the referenced receivers. No prerequisite is silently installed, substituted or removed by this archive. Do not apply these two snapshots alone and call the project buildable. Cam's view receiver, X2/X3/X4, paper/picture/multipage work and other contributors' windows remain outside this source unit.

## Policy references and status

- [Caller decision](https://github.com/tangency-labs/solidcraft/pull/6184#issuecomment-6522562149).
- [Existing ACK/source-review reference](https://github.com/tangency-labs/solidcraft/pull/6184#issuecomment-6522733382).
- [Original plan](https://github.com/tangency-labs/solidcraft/issues/860#issuecomment-6234779815).

The caller policy is already agreed; this archive reuses the reviewed successor and introduces no new owner choice. Tracker text is separate and is not included or posted here. Existing source-bound historical results do not transfer. A later coherent candidate must run the original seven regressions, seven caller cases and 53 pins, affected suites, strict checks, wasm and full repository CI, with actual observable-output evidence. No compiler/test/oracle, GUI/GPU, full-CI, merge or readiness success follows from source replay.

Original X1 source is credited to the 0xlillith team in PR6184; CamWight supplied the existing caller-policy reference, and Worker1 prepared this exact two-path composition. Other SolidCraft contributors' frozen-main source remains preserved. Existing repository MIT OR Apache-2.0 licensing and attribution remain intact; this archive does not relicense prior work or bundle external CAD code/assets.
