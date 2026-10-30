# Render #6201 — evidence milestones after C17

The consolidated U01–U17 list still has **17 open items; none fully resolved end to end**. Partial code exists for13, including private source-only per-method sampling-preset proposals. GPU G01–G04 sit under U04; G05 is U17. Historical lists are not counted twice.

Each bar contains five independent evidence milestones: diagnosis, code, current-candidate tests, in-app acceptance, resolution. Equal widths are **not percentages of remaining effort, time, feature coverage or an ETA**. A documented finding does not necessarily prove its cause. Hatching means partial scope. Earlier in-app improvements do not qualify the changed candidate. Tests are hatched only where exact C17 cases actually passed; no item has a complete current automated gate. U02 also contains the failed HDR assertion.

## Actual validation

C17 compiled the current engine, UI and render test artifacts and completed six inventories. It ran69 exact native test cases: **68 passed,1 failed,184 remain unrun** from253 selected. Nine required CPU-supported markers were true; none were false. The selected run stopped at its first failure and was not retried. The failed new UI HDR assertion compares a32-byte array to a64-character hexadecimal string; independent source review identifies matching digest content. Its correction remains unrun and does not erase the failure. The completed cases cover partial progressive optics, material routing, perspective projection, original Pause interaction and automatic Result behavior. No full item is thereby closed.

Strict affected Clippy, default wasm, lists and full CI were not reached. The current candidate has no bound desktop mouse/keyboard acceptance or physical GPU acceptance. Root independently verified the terminal logs, artifacts, source and drained jobs before releasing the reservation. Earlier C15 fixture compile failure and C16 pre-Cargo RAM stop remain historical, separate evidence.

There is no defensible completion date or overall percentage yet. The critical path is correcting the test representation and finishing current-source automated validation, then a bound desktop build and real mouse/keyboard tests; remaining CPU shadow/optical quality, Raster effects and physical GPU issues still need implementation and acceptance. Source-only successor proposals remain separate from C17. Multilingual search is deferred.

Source: [issue6201](https://github.com/tangency-labs/solidcraft/issues/6201), exact C17 native results and independent terminal/source review. Editable row-level evidence and limits are in `render-progress-c17-20261030.json`.
