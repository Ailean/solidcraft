# Scoped validation result, 30 October 2026

Production files on this source-archive branch remain unchanged.

## C7: completed and independently checked

- Formatter and format check passed with no source changes.
- Strict all-target engine/UI Clippy with the CPU feature passed.
- Default-feature engine/UI wasm compilation check passed with three warnings; it was not warning-free and did not execute wasm.
- The combined native engine/UI library-test build passed. Both current CargoJSON artifacts were checked against actual package/source/manifest/features and typed ci profiles before execution.
- Both complete/ignored inventories passed. All22selected cases passed once each in separate serial processes:10engine and12UI,14positiveCPU markers and0false. No physicalGPU case was executed or counted as a pass.
- All31executed phases exited0 and recorded owned Job active_after0. Final process/lease closure and all3569source hashes, HEAD/index and both executable hashes were independently rechecked.

Actual native library-test executable SHA256:

- Engine: `82d6cb306c582f5e9ae7a0ec50b0abe776ff0803d3fe9431dca41f8a80a5e8a3` (bundled-render-library,default,raytrace-cpu).
- UI: `89c31de5b7dd73ff7390ec684388bf04b77c81645f234471cdd23183fb804eab` (no crate features).

This is bounded affected/dependent validation, **not full repository CI, a production application build, GPU qualification or installed-app acceptance**. Native tests use the ci profile; no alternate fast compiler output is substituted.

## What the source correction does

The two engine snapshots and patch resolve eleven test-scaffolding diagnostics: four fixture helpers propagate errors via Result and actual test bodies unwrap at their assertion boundary; the session-identity test uses a struct initializer. The earlier UI helper Result correction is also preserved. No original assertion, renderer behavior, lint allowance, work limit or expected image was weakened.

C3's earlier81selected passes and the scoped516-case renderer result remain exact-source historical evidence. C4/C5 stopped before Cargo for RAM admission. C6 passed admission and stopped on the engine test-helper diagnostics; no later C6 stage ran. Original failure evidence is retained.

## Next: actual application, then mouse/keyboard acceptance

C7 built library tests, not the application. A source-bound production `solidcraft` binary build is being prepared with the existing cache. The adjacent native mouse/keyboard acceptance plan is still **PLANNED, NOT RUN**. It will distinguish the installed release from the rebuilt candidate, use an owned synthetic design, preserve user documents, and drive real controls rather than internal product commands.

PhysicalGPU acceptance, full repository CI and native-application acceptance remain open. CPUArea/LED, locale and Raster proposals in this archive remain conditional/unrun; preservation is not their production integration.
