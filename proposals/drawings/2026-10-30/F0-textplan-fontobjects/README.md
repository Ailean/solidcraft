# P2 F0 font-plan boundary — source preservation, v3

**UNFORMATTED, UNCOMPILED, UNRUN, UNREGISTERED.** This packet preserves two owned product proposals and original acceptance fragments. It is not an integrated branch, production font support, a completed PDF, a passing gate or a merge claim. Earlier archives remain available; none is overwritten.

## Actual source and lineage

The current F0 contract is pinned to Cam's B1a [#9512 at 16c9e911](https://github.com/tangency-labs/solidcraft/pull/9512) and B1b [#9513 at a46d46e9](https://github.com/tangency-labs/solidcraft/pull/9513): `TextRun.font` names `Font.id`, and `paper_text` returns the unique used `Vec<Font>`. The numeric `Font.bundled` field is gone. `Font.embed` belongs to F2 and is **not** invented here. Full immutable heads, blobs, byte hashes and per-file baselines are in [source-bindings.json](source-bindings.json).

The predecessor [width/Refs composition](https://github.com/tangency-labs/solidcraft/tree/5f127aceb09f04dcfe6e4dc1d0253a292787dffa/proposals/drawings/2026-10-30/P2-width-Refs-composition) remains bound to its older records. This successor carries the F0 and checked-source boundary work. The v3 source manifest is `1b6f64a24513cb069efd76708fd8788dd29ff09da802ea662c1a60da2bbc24ed`; its product candidates remain byte-identical to v2 (`6795b37c`).

## DW decision since this archived source

Cam has now [resolved the DW contract](https://github.com/tangency-labs/solidcraft/pull/7626#issuecomment-6534292604): explicit `/W` coverage for **every emitted CID**, without relying on `/DW`; B1 adds no GID-0 width. The embedded font program's `.notdef` glyph is a separate embedding responsibility.

The exact v3 product below predates that decision and **still contains its old absent-GID-0 refusal**. It is not the implementation of the now-resolved contract. A separate v4 successor is being prepared; no product bytes are changed in this archival payload and no new public question is asked. The old refusal is preserved as history, not presented as an unanswered owner decision.

## What the proposed boundary actually checks

`TextPlan::font_input` can expose a borrowed `FontPlanView` only after successful plan construction. Its font, immutable factory-produced subset, descriptor and codes are private fields with existing read-only getters. The production font input accepts that view plus the subset tag and local output allowance. Raw parts and component emission remain module-private; descendant component tests use explicit private aliases rather than a new proof constructor. This prevents a safe sibling caller from simply pairing arbitrary Font and Subset records at that entry. It does not certify facts the plan never checks.

The existing exact regular-font policy remains. The proposed path compares the actual bytes and face with the fresh builtin `font_data(None)` result, using the existing catalog path before installed-font/filesystem/cache lookup. The original B1 bytes-plus-face identity, original-GID intrinsic-advance check and single subset→closure→RAW admission chain remain. The font-object helper removes its redundant identity rehash; this is **not** removal of the original identity/admission checks or permission to add another parser/RAW admission. `/W` still uses checked B1 original-GID advances, distinct from placed advances. No font lookup fallback, new asset, new dependency or broadened embedding policy is supplied.

The view attests only those performed checks. It does not attest arbitrary outlines, historic layout provenance, licensing/distribution rights, all runs in a document, global budgets or successful future PDF emission. The mutable-advance poison fixture edits the Font **before** `Scene.prepare`; structural preparation succeeds and planning must refuse it. No mutation of an already-live immutable Prepared borrow or executed success is claimed.

## Acceptance and the narrow v3 correction

There are **46 ordinary written declarations** in this proposed subset: 24 inline helper cases, 13 retained B1 cases, 4 F0 wrapper cases and 5 production-boundary cases. The [four negative compile obligations](acceptance/negative-compile-obligations.rs) are separate intentional invalid snippets, not ordinary module code. Their expected diagnostic classes are not observed compiler results. No positive case has been relabelled as compile-fail.

The sole v2 source finding was six positive-test lines still reading removed raw input fields. V3 changes exactly eight reads on those six lines to the existing `source.font()`, `source.descriptor()` and `source.codes()` getters. The descriptor getter borrows a successful plan's nonoptional descriptor; this does not remove the missing-descriptor refusal from the raw component boundary. Test names, order, classification, assertions, literal text/numbers, seeds, tolerances, budgets and fixtures are preserved.

The independent focal closure (`335195903fca30866b1dbef23417b191439bee063e6bf35487858151c40c1fcf`) closes **only that source accessor finding**. Product bytes and product patch alternatives are unchanged from v2. No formatting, compiler, ordinary/negative-test execution, PDF generation/readback or full-feature qualification follows from this closure. The worker's sealed-before-review manifest remains historical; no private report is republished here.

Earlier Cam/P1/many-page, Refs and other P2 acceptance remain in the predecessor archives and must survive a future assembly. The count 46 is this proposal's components, not the whole project or an executable inventory.

## Choose one route; do not overlay alternatives

- `candidate/` contains the final two proposed files.
- [Product canonical delta](deltas/product-from-canonical.patch) starts from `before/canonical/`, the owned predecessor's per-file bytes.
- [Product alternative delta](deltas/product-from-alternative-partial.patch) starts from the earlier partial text plan in `before/alternative-partial/` and the same canonical font-object baseline. These routes produce the same two candidates; they are alternatives, not sequential patches.
- [Retained-case cumulative delta](deltas/acceptance-from-owned-appendix.patch) starts from `before/acceptance/owned-b1-cases.rs`. Its logical filename deliberately identifies an **owned appendix**, not Cam's complete `pdf_tests.rs`.
- [Accessor-only delta](deltas/acceptance-accessor-fix-from-v2.patch) starts from `before/acceptance/v2-retained-b1-cases.rs`. It is the alternative route to the same final retained fragment. The wrapper and boundary fragments append unchanged when normally integrating into the area's existing test file. Keep negative obligations separate.

All paths are portable source artifacts. These mixed component baselines are not one validated Git head, and no parent registration or application is supplied. Preserve earlier test suffixes and contributor authorship when composing the eventual source.

## Still pending

- **Implementing the resolved DW contract** remains a separate successor task. This exact v3 still reaches its historical missing-GID-0 refusal; full emitted-CID `/W` coverage without a `/DW` fallback is not implemented or qualified by these files. No default width, unused glyph or new B1 field is invented, and no text-emission success is claimed.
- Cam's actual #9647 parent/page/content/resource builder is untouched. Normal modules, all-runs-per-font grouping, aggregate budgets, font/page resources and actual text emission remain open.
- Multi-glyph ActualText and real RTL/text extraction remain unsupported/unqualified. Source-only positioning is not reader evidence.
- F2 embedding policy/rights, coherent dependency and Cargo.lock integration, current-source compiler/native/wasm/strict/full gate, actual old-versus-new PDF byte comparison and PDF rendering/readback remain unrun.

Changes a default: no. Original Worker #1/Engineer-team source and cases retain the existing Cam, P2 and font contributors' authorship/notices. No font binary, private machine data, launcher, log, grant or control file is included.
