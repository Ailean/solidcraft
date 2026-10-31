# Native CPU render acceptance: G01 and G02

**Source-only acceptance documentation. All native observations remain NOT_RUN.** This package supplies original CAD recipes and mouse/keyboard procedures, not a constructed application, model, screenshot, render, executable or test result. It adds G02 and documents the existing G01 controls; it does not replace or modify the original 24 acceptance cases.

| Document | Purpose | Native status |
| --- | --- | --- |
| [G01-CASE.md](G01-CASE.md) | Opaque 19-body specimen: long final CPU progression, cancellation, edit and fresh completed export | NOT_RUN |
| [G01-CONTROL-MAP.md](G01-CONTROL-MAP.md) | Actual production routes for 640×480, Compact, 48 SPP, CPU seed81 and Result visibility | NOT_RUN; source paths found |
| [G01-ROUTE-CORRECTION.md](G01-ROUTE-CORRECTION.md) | PNG and transparency are in Output > Image; completed-image export is in Destination | Source navigation correction only |
| [G02-CASE.md](G02-CASE.md) | Original editable two-box glass comparator: optical cues, absorption, preview refresh and export | NOT_RUN; source limitations identified |
| [source-bindings.json](source-bindings.json) | Repository-relative paths, exact source SHA-256 identities and immutable archive links | Source binding only |

## Version and source status

The installed-application baseline reported for this preparation is **0.26.1**. This documentation task did not inspect or launch that application, change its version or install a newer build. A preference selecting a newer release is not proof that the installed or currently running binary changed. The source archive is not a shipping version. Before native acceptance, record the actual binary version and bind it to the reviewed source and installed CPU capability; do not infer support from preferences or these documents.

The reviewed source includes unmerged Render work. Its checkout base was `a077906234972dbbf11e80c54893f85ece97c8b7`, but that base alone does **not** contain the newer Strict source. Do not link the root file at that base as evidence for Strict. The [C20 composed source archive](https://github.com/tangency-labs/solidcraft/blob/f346faa7859ce9e6ccc123ff8189d48f60e2720a/proposals/render/2026-10-30/c20-reviewed-composition/README.md) and portable per-file hashes identify the added source. An archive composition must be reconstructed against its matching before hashes; it is not a full source checkout or an applied production commit.

Useful immutable source anchors:

- [Standard and Continue reflections public labels](https://github.com/tangency-labs/solidcraft/blob/f346faa7859ce9e6ccc123ff8189d48f60e2720a/proposals/render/2026-10-30/c20-reviewed-composition/c20-source.patch#L6).
- [Native Glass rendering policy controls](https://github.com/tangency-labs/solidcraft/blob/f346faa7859ce9e6ccc123ff8189d48f60e2720a/proposals/render/2026-10-30/c20-reviewed-composition/c20-source.patch#L950).
- [Strict positive-continuation branch-limit error](https://github.com/tangency-labs/solidcraft/blob/f346faa7859ce9e6ccc123ff8189d48f60e2720a/proposals/render/2026-10-30/c20-reviewed-composition/c20-source.patch#L467).

These anchors refer to lines in the **archived patch**, not lines in a production root file. Full reviewed-source identities are listed separately; unchanged files are not necessarily repeated inside that 16-path delta. G01's map records its C20 source review, while G02 records the subsequent exact source snapshot. Neither a source review nor any unrelated build/test result closes native acceptance.

## G02 obligations stay separate

| Observation | Required interpretation |
| --- | --- |
| C: certified box glass at IOR1.50 with Continue reflections | Must finish the refracted image. An error/refusal is FAIL or BLOCKED with the actual error preserved; never Strict/feature PASS. |
| Strict branch-limit concern | Concrete source-derived risk, not a reproduced native failure or proof that every view fails. No changed oracle, increased cap/depth or repeated attempt to obtain a pass. |
| A and B references; D/E/F under Standard | Separate obligations. Standard results do not rescue C, close Strict or prove perfect convergence. The policy switch is explicit. |
| Index-matched A | Control only. It does not substitute for C's normal IOR1.50 glass. |
| Absorption edit/reversal | Require actual current subject-pixel refresh, then a new completed original export. A label or swatch is insufficient. |
| CAD edit → glass refresh | Revision-bound assignments have an explicit revalidation step. Manual recovery is separate from an automatic-refresh claim. |
| CPU optical persistence | BLOCKED_UI on reviewed source: the optical Save control requires GPU, and temporary bindings are not portable saved setup inputs. Saved editable CAD geometry is distinct. |
| Lighting | Neutral studio, ordinary Shadows off from the outset; transmitted shadows unsupported/off. Same setting retained across comparisons. |
| Supported specimen | Two disjoint, world-axis-aligned boxes, subject to actual geometry certification. No sphere/general-mesh substitute. |

G01 keeps its original three distinct provisional-image and in-flight cancellation requirements. If a render finishes too quickly, retain NOT_EXERCISED_STREAM or NOT_EXERCISED_CANCEL; do not increase caps, add delays or relax the assertions. G01 remains an opaque case and cannot prove glass/TIR simply by selecting Continue reflections.

Public scope is limited to these acceptance documents and portable source bindings. There are no bundled full source snapshots, private reviews, host/process information, control files, logs or runtime evidence. Publication does not authorize application launch, consume a test grant or establish release readiness.
