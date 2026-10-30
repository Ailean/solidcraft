# Render #6201 — evidence milestone snapshot

The current consolidated list has **17 items (U01–U17), none fully resolved end to end** in this snapshot. GPU G01–G04 are detailed under U04 and G05 under U17; the historical UX/R/AP lists are not counted twice.

Each horizontal bar contains five independent milestones. **Their equal widths are not percentages of time, effort or feature coverage.** A filled diagnosis means a documented native or source finding, not necessarily a proven root cause. Hatched code means partial implementation or source-only work, including other contributors. Hatched in-app means an improvement observed on a specific earlier route; it does not qualify the changed candidate or the whole item. Gray means the stated milestone has not been demonstrated for the consolidated item. Tests refers specifically to the current combined candidate's automated gate.

The C16 attempt stopped at RAM admission before formatter/Cargo/tests (253 selected cases unrun). This is a resource stop, not a product or compiler failure. The earlier C15 four-E0599 fixture compiler failure remains separate and preserved. The fixture correction and automatic Result preference are published as source-only WIP in the adjacent C16 checkpoint. Full CI, current native acceptance, physical GPU output and broad optical parity remain required.

No reliable completion date or overall remaining-effort percentage can be inferred from these bars. The critical path is resource admission, current-source validation, a bound desktop build with mouse/keyboard acceptance, and the remaining CPU/Raster/GPU capabilities and regressions. A bar closes only when its full scope is implemented, tested, accepted and integrated. Multilingual search remains explicitly deferred.

Source of status: the current OP in [#6201](https://github.com/tangency-labs/solidcraft/issues/6201), the actual C16 resource-stop evidence, and current source/PR observations. This is an editable evidence snapshot, not a delivery promise. Exact per-row data and limitations are in `render-progress-20261030.json`.
