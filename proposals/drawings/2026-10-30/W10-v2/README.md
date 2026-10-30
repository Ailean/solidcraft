# W10 arc converter v2 - unintegrated source checkpoint

This archives original work following Cam's contract decision in #4890, comment6518014281. It is **not applied, compiled, tested, registered as a production API, or ready to merge**. Independent mathematical/source review is pending. Historical v1 remains preserved in the existing W10 archive.

`cubics.rs` and `cubics-v2-and-tests.patch` provide the same-units converter and thirteen new analytical tests appended to the existing geometry test file. Signed sweeps through a half turn, finite positive radii, representability/certificate refusals and caps are explicit. No production module hook or writer adapter is included. See DERIVATION-v2.md and manifest.json for the source argument, limits and actual receiver pins.

`pdf-off-grid-two-tests.patch` is a SEPARATE proposal against the pinned PDF PR receiver in manifest.json, not against the geometry receiver. The two ordinary-circle fixtures came from Cam's simulation. The real walk/Piece::seg/export tests have not run; neither a reproduced writer defect nor successful output is claimed. Pair them with the actual production correction/adapter before a gate. Do not mix their source bases or add failing tests to main.

The writer must compose the converter certificate with the actual displacement of every rounded or replaced control. A forced walked endpoint is not automatically within a half-step diagonal of the stored endpoint; that proof and the adapter are still pending. No new public API, consumer adoption, font/dependency/asset change or claim of prior validation is made.

Separately, Cam has supplied B1's intrinsic glyph widths alongside outlines in FontGlyph (comment6518550794). This resolves the record-level PDF width question, but the author says no B1 implementation exists yet. P2 must bind to actual delivered code, keeping intrinsic widths separate from placed glyph advances. Engineer acknowledged this in comment6519182623.

Source checkpoint only. Runtime, focused tests, lint/wasm, repository gates and final integration remain pending under the single-builder protocol.
