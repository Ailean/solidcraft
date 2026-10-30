# W21 NoPictures export acceptance — source-only continuation

The original three-test increment is preserved here for the agreed contribution to [#7591](https://github.com/tangency-labs/solidcraft/pull/7591), under [Cam’s test-file handoff](https://github.com/tangency-labs/solidcraft/issues/4890#issuecomment-6503678820). This is a source archive, not the stacked implementation PR or completed W21 delivery.

Receiver checked 30 October 2026: `5ddde8e23a86c530f2b4d2395980e3f7b88048ae`, `crates/io/src/paper/png_tests.rs` blob `39e4bc69c53f4cefe1f6b4cba8a23ec29af93502`. Only this existing test file is affected. Its complete original bytes and 14 tests stay intact. The two original NoPictures parity tests are unchanged; one new determinism test reuses each Prepared across both real writers, reversing call order on repeats. The separate three picture proposals are excluded. No Scene, Prepared, Grid, exporter implementation or dependency changes.

The absolute tests check preplaced 50/25 mm paper lengths and 0.2 mm stroke widths at 254 DPI, plus outward rounding and partial edges on a 100.08 × 80.08 mm sheet. Expected values come from authored paper geometry. Repeatability compares complete encoded SVG/PNG bytes and reports, separately from those geometry assertions. This does not test model projection, rasterize SVG, or qualify pictures, dashes, text, clipping or all of W21.

Independent source review found no actionable blocker within this scope. Patch reconstruction and byte-prefix preservation were checked. **Three new / seventeen total test declarations; not compiled or run.** No result from D8 or the render build qualifies this source. The agreed stacked draft, exact-source sole-builder registration/execution and applicable repository checks remain pending. Existing contributor ownership and the single build queue are preserved.

Patch SHA-256: `48c3d33ca4a225f7f8a1b33d90accb62ebc8976e829d20538adaad56f0176540`.
Expected candidate-file SHA-256: `893c9d6a62b192dae8b5cb1fb7415e920d3afb3a8f9a6056cb3c8d57ae13d833`.
