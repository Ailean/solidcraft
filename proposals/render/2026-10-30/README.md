# Render: current reviewable source and qualification boundaries

30 October2026. These are canonical source checkpoints requested by Judit. Production modules on this archive branch remain unchanged. Source/math review, written tests and a running validation are different from an observed runtime pass or ready PR.

| Packet | Current meaning |
| --- | --- |
| [Complete53-path C12 candidate](c12-complete-source/README.md) | One patch against draft base a0779062; core corrections, Interactive capture/cache/lifecycle, waiting/input/layout, help translations, GPU reasons and Area/LED. Applied locally and frozen for current C12. Do not compose this with the earlier incremental patches. |
| [Actual-worker Area/LED E2E tests](cpu-progressive-area-led/E2E.md) | Included in the complete candidate. Two original native-host/Snapshot/PNG/cache/stale-final-priority cases; C12 built successfully but stopped on selection before test bodies; C13 tests-only pending. |
| [Nested certified-box glass V2](cpu-nested-boxes-v2-source-review/README.md) and [current cumulative rebind](cpu-nested-boxes-v2-source-review/CURRENT-REBIND.md) | Independent numerical and Engine source review,15 new Render/Engine declarations; NOT APPLIED/COMPILED/RUN. V1 remains blocked. General/progressive/Interactive glass and physical-library routing remain open. |
| [EV/HDR real-worker additions](ev-hdr-e2e/README.md) | Separate2-test proposal on the213-test C12 prefix; independently source-reviewed, unrun and not part of C12's53-case selection. |
| [Explicit output-envelope helpers](output-envelope-helpers/README.md) | Seven-helper V2/13 original tests, independently source-reviewed; no CPU host/raw-linear/work/publication enablement. Unapplied/uncompiled/unrun. |
| [Genuine midpass progress foundation](midpass-film-preparation/README.md) | Option A selected. A1 partial-film accessor +5 originals reviewed; still unintegrated, expected unused API until a REAL A2 producer exists. No lint allowances/dummy consumers. Host/producer/UI implementation is being prepared privately under disjoint ownership. |
| [Raster perspective candidate](parity-raster-main66bd/README.md) | Source-reviewed against its actual newer main API, not this older frozen candidate. Original perspective/legacy controls, no compiler/runtime claim or permission to overlay it blindly. |
| [Native observations](native-followups/native-round-one.md) and [remaining EV/HDR/final journeys](native-followups/ev-hdr-final-acceptance-proposal.md) | First older-binary round was partial. Fresh current-source app build and mouse/keyboard acceptance remain pending; source tests do not close native UX. |

## New source packets and original detailed fixture

- [C13 pinned-action correction](c13-pinned-actions/README.md): independently source-reviewed, unrun; original failing assertion retained.
- [Actual A1/A2 partial producer and host](partial-stream-source-v1/README.md): written source and original tests, unapplied/uncompiled/unrun; combined review and UI receiver still pending.
- [Original19-body long-render fixture and synthetic HDR](original-long-scene/README.md): privacy-reviewed source subset, attributed; no native creation/render/quality result.

## Current validation

C12 is terminal: formatter/check, repository lists, strict affected Clippy, default-feature wasm and the combined engine/UI/Render native library-test build exited0. Six inventories also exited0. Selection stopped before any test body because two names used the wrong module prefix (`job::tests` instead of `job::runtime_request_roundtrip_tests`). **Zero test bodies and zero CPU markers ran; this is a preparation error, not a product failure or53 passes.** Independent closure verified the13 exited command roots/empty owned Jobs, source/config/toolchain, the actual three CargoJSON artifacts and physical lease cleanup.

C13 is terminal, stopped without retry on a **real UI assertion failure**. Six fresh inventories succeeded; two exact native cases executed: **1 passed,1 failed;51 were not run**. The failing case retained both expected positive CPU markers and exited101: at760×700, Pause was outside the Interactive card clip (card y327–381; button y380–408). This is not a skipped CPU branch or parser failure. No Cargo/build ran. All eight command roots ended and owned Jobs drained; source and artifacts remained unchanged. The original test/assertions and failure evidence are preserved. An isolated layout fix is being prepared and independently reviewed before any new validation grant. Current-source native application and long-scene quality acceptance still remain pending.

The real i18n gate invokes a cached release CLI-registry build internally. Its exterior exit0 does not independently prove that registry's success because a helper can swallow the error. That evidence limit is preserved. A uniquely identified lingering compiler auxiliary was explicitly disposed after its command exited; that administrative action is not a product/test pass. No control receipts, tokens or machine logs are public here.

CPU final output should cover GPU-supported effects, including real glass, and coherent maximum-resolution admission/finishing/export. Current16SPP Interactive remains a responsive provisional default, not the final quality ceiling. Long renders need genuine intermediate pixels, truthful unknown/sample provenance and bounded retained images. Raster is explicitly schematic. More samples/completion alone do not prove clean, correct or converged output.

The [active tracker and acceptance plan](https://github.com/tangency-labs/solidcraft/issues/6201) records dependencies/owners and preserves historical results. Existing Nathan-Dane panels/Results/Studio/gizmos and other contributors' GPU/compiler/color work retain their ownership.

<details>
<summary>Earlier source-preservation snapshot (historical; current table above supersedes statuses)</summary>

# Render follow-up source preservation, 30 October 2026

These are unqualified work-in-progress snapshots and narrow patches, preserved at Judit's request. Production files on this archive branch remain unchanged. The earlier [complete source and pending parity packet](../2026-10-29/README.md) remains intact. This directory does not register feature modules or claim a ready delivery.

- `source/`: exact current two UI sources after the test-only Result correction. Other combined source remains as in the earlier checkpoint.
- `test-current-result.patch`: preserves controller errors rather than unwrapping in a test helper; six callers retain their original Some/None assertions. Applied locally; not yet revalidated.
- `cpuarea-existing-help-25-catalogs.patch`: conditional on the pending CPU Area/LED progressive implementation. Removes only the existing translated Area-light refusal and its list separators. No new translated claims, IDs, caps or defaults. UNAPPLIED and UNRUN. Do not apply before the corresponding behavior lands.
- `raster-reciprocal-w-append.patch`: append-only original regression for the pending weighted-scalar perspective fix from the earlier packet. Preserves all existing tests. UNAPPLIED and UNRUN.

## Original Raster oracle

With eye (0,-10,0), clip w=(2,8,4) and sample (2.5,2.5), screen barycentrics (1/4,1/2,1/4) give perspective weights (1/2,1/4,1/4). Independently intersecting E+t*(-3/8,1,3/8) with 4x-y-4z=-6 gives t=4 and world=(-3/2,-6,3/2). Normal (3,1,0)/sqrt(10) separates the correct view from affine and fixed-back alternatives. Closed-form dielectric specular with F0=.04, roughness=1 gives radiance .014412012452814404; Reinhard plus sRGB rounds to byte32. The test expects [32,32,32,255]; the two wrong-view controls predict zero. This is an analytical prediction and static source review, not an executed baseline/candidate result. It observes view direction, not distance along the same ray.

## Validation status

C3 built the combined native engine/UI test executables and passed 81 selected cases (33 engine,48 UI), including 17 positive CPU markers. Strict Clippy then failed on the old test-helper unwrap; wasm was not started. The test-only Result change preserved here was applied after that execution and still awaits its affected tests and strict checks. Subsequent C4/C5 attempts stopped at RAM admission BEFORE formatter/Cargo/test execution. Neither is a product failure or a pass. No GPU, native GUI acceptance or full repository CI is claimed.

The original 516-case core result remains bound to its exact unchanged core sources. Native GUI acceptance and actual GPU dependencies remain separate. Pending proposals must be integrated narrowly against real current source, reviewed and validated before production publication.


## Latest engine correction and native acceptance plan

[Current validation status](STATUS.md) supersedes the earlier snapshot above. The [engine test-helper Result correction](engine-test-helper-result.patch) and exact engine sources are now preserved here. [Native mouse/keyboard acceptance](native-mouse-keyboard-acceptance.md) is planned after validation and an actual application build; it has not been executed.

</details>

<!-- protected-combined-source-20261030 -->
## Protected CPU progressive-feedback source

[Complete27-file source checkpoint](protected-combined-source/README.md), including31 new unrun producer/host regressions and the protected receiver over the previous reviewed A2 foundation. Exact text composition only: unapplied, unformatted, uncompiled and unrun; cross-module review pending. Manual-final streaming, general/progressive glass, library routing, finishing and native long-scene cadence/quality remain open. This cumulative patch is an alternative to overlapping earlier increments, never an extra layer on top of them.

<!-- next-render-source-20261030-v1 -->
## Next Render source packets

[Source-only continuation](next-render-source/README.md) preserves progressive certified optics, the exact-index straight-path correction and original camera-TIR tests; explicit CPU material routing and its progressive/caption deltas; manual-final feedback V1 plus mandatory V2 corrections; the receiver-bound Raster perspective fix; and separate Photographic test-contract/cursor-commit corrections. Before/candidate hashes and dependencies distinguish cumulative alternatives from incremental deltas. All new packets remain unapplied, unformatted, uncompiled and unrun. Actual C14 is separately36PASS/1FAIL/108unrun after a successful three-library build and original Pause regression; it is not full CI or current-source native/GPU acceptance.

<!-- actual19-reviewed-source-20261030 -->
## Exact19-path adopted Render composition

[Actual19 cumulative source](actual19-reviewed-source/README.md) preserves the independently source-reviewed V2 that ROOT adopted by exact RAW copy, with3571 members checked. It combines manual-final feedback, progressive certified optics and both W2 suffixes, CPU material routing/bridge/caption, UI252 with its explicit supported-feature/DTO-stage corrections, both Photographic fixes and Raster perspective/directional-core source. Reconstruct the exact previous C14 post-format base; this is an alternative to overlapping increments, never another layer on top of them. LF hunks and eight CRLF endpoints are explicitly bound. New66 declarations and planned242-case/49-marker selection are **unrun**; formatting/build/actual gates/native acceptance are still pending. Historical C14 remains36PASS/1FAIL/108unrun and no pass transfers.

<!-- c16-defaults-auto-result-source-20261030 -->
## C16 source: fixture defaults and Result navigation

[C16 exact five-file source](c16-defaults-auto-result/README.md) is independently source-reviewed and adopted by RAW copy; the adjacent12-file C15 formatting bridge makes the Actual19→C15 post-format→C16 lineage explicit. It fixes the four invalid new-fixture calls and adds preference-compliant Result navigation after accepted GUI Start, retaining old behavior with `render_open_result_on_start=false`. Four new declarations remain unrun. C15 stays an actual four-E0599 build failure with0 test bodies/242 cases unrun; no build/test/native PASS transfers. Manifests record exact endpoint hashes/EOL and cumulative alternatives; no private execution files are published.

<!-- render-progress-evidence-20261030T1813 -->
## Progress by consolidated issue

[Current U01-U17 evidence-milestone chart after C17](progress-c17/README.md): 17 open items, partial code in13 and earlier partial native improvements in8; none fully resolved end to end. C17 ran69 cases:68 passed,1 failed,184 remain unrun; no full current gate or desktop/GPU acceptance. These are milestones, not effort percentages or an ETA. [Earlier pre-C17 snapshot](progress/README.md) is preserved as history.

<!-- next-render-source-units-20261030 -->
## Separate reviewed source units

[Camera/framing and Raster directional source units](next-source-units/README.md) preserve reviewed, unrun code without adopting or combining it. Camera/framing has3 original UI cases on C16 and leaves geometry/placement/visibility identity open. Raster preserves its33-file C15 archival base plus mandatory DTO-path, empty-capacity and borrowed-JSON fixes,14 new tests and explicit current UI/W2 rebind obligations. No source review or historical C15/C16 outcome is a runtime PASS; the later W2 core-policy packet is excluded until delivered/reviewed.

## Reviewed HDR, sampling and CPU exposure follow-ups (30 October)

Additive source proposals; prior checkpoint sections remain unchanged. This append was prepared against observed archive head 9001a87ea4b6c4473055dd690b31435a77a79a33 on codex/render-all-code-checkpoint-20261029. Recheck the actual branch head/index before integrating it.

- [HDR digest test representation correction](reviewed-followups/hdr-hash-test/README.md): exact test-only patch adopted in C18 development source, execution tracked separately in #6201; no C17 failure is relabeled.
- [Sampling presets and future persistence correction](reviewed-followups/sampling-presets/README.md): choose legacy OR the ordered PR8000 sampling/persistence alternative, never both; unadopted and unrun.
- [CPU display exposure±32 alternatives](reviewed-followups/cpu-exposure-32/README.md): exact standalone OR TIR-layer EV delta, six unrun new tests and retained original negative-contract source bodies. The TIR prerequisite chain is referenced, unprovided here and not installed; this is not a complete TIR code archive.

See the [unit index](reviewed-followups/README.md) and each portable bindings file. These source archives do not establish full CI, native GUI/GPU acceptance or readiness.

## Coherent preview and exposure source checkpoint

[C19 source, bindings and unrun original tests](coherent-preview-exposure/README.md)
preserve the full reviewed-source assembly for saved Raster lights, common
Model/CPU/Raster exposure, captured-image boundaries and paused framing.
This archive is not adoption, a test pass or production readiness.

## Explicit product sRGB source proposal

[Two-file shader proposal and unrun original GPU test](explicit-product-srgb/README.md)
preserve the narrow explicit-light color-transfer correction against exact C19
source. One test proposes 96 renders; none has run. Ordinary Design/implicit
preservation is source-scoped, and Some(lights) selects the changed branch.
This archive does not authorize adoption or execution.

## Connected Render documentation v2 source proposal

[Two reviewed CRLF documentation deltas](connected-render-docs-v2/README.md)
record the Interactive/Shot source contracts and corrected bounded dielectric
wording. Only the cumulative patch from exact C19 RAW endpoints is included.
The docs are unadopted; C19 build results and the separate U17 proposal do not
qualify these edits or authorize runtime/adoption.

## Strict CPU dielectric source proposal

[Ten-path strict-TIR proposal and 18 unrun original tests](strict-cpu-dielectric/README.md)
preserve default/omitted Legacy behavior and explicit bounded Strict intent.
This pre-format C19 delta requires a separate post-format receiver rebind; the
current C19 build does not include or qualify it. Only the combined patch is
archived, with no adoption, runtime, energy-convergence or GPU-parity claim.

## C20 reviewed working-source composition

[Exact C20 delta, portable bindings and 19 unrun declarations](c20-reviewed-composition/README.md)
record the reviewed 16-path composition adopted as working source only. No
compiled inventory, runtime, full-CI or readiness result is implied. Earlier
Strict TIR and U17 proposal archives do not substitute for this exact receiver.

## Raster local studio v2: separate future unit

[Standalone candidate and delta](raster-local-studio-v2/README.md) preserve the
source-closed helper with 18 new numeric cases, all unrun. It has no consumer
and is not part of C20 or its development selection.

## U17 test-module label correction

[Source wrapper correction](explicit-product-srgb/test-module-label-correction.md) fixes `view::tests` to the actual `view::glass_tests` registration. No source, case selection or runtime result changes; the real GPU case remains unrun and excluded from C20 native development acceptance.

## CPU direct-visibility foundation: separate source proposal

[Two-file candidate, exact delta and original cases](cpu-direct-visibility-foundation/README.md)
preserve a bounded incremental straight exterior-light connection through certified
sheets/disjoint boxes. The 33 old plus 12 new declarations are all unrun on this
candidate. There is no image consumer or C20 adoption; brute-scan cost, refusal
domain, cumulative work/memory admission and missing real-image acceptance remain
explicit. Source review and replay are not compilation, full CI or GPU parity.

## 2026-10-31: minimal carried image-test recovery

[Minimal image test recovery](../2026-10-31/minimal-image-test-recovery/README.md) archives two exact call-site repairs in the existing Render image tests, both RAW snapshots, the unchanged delta and portable bindings. 23 original declarations / 0 new; all original oracles remain. C20 compiler failure is preserved; C21 is source-adopted only, with 0 executed cases and the 349-case/60-marker selection still UNRUN. No PASS/full-CI/GPU/desktop readiness is inferred.

## 2026-10-31: Raster local runtime consumer — source only

[Bounded Raster local-light consumer](../2026-10-31/raster-local-runtime/README.md) preserves exact before/candidate source for 34 existing paths, a single combined delta and portable bindings. It connects transient point/spot/rectangle schematic lights through request admission and execution, with bounded resource accounting and corrected weighted-work progress. Four receivers retain the current formatting and two test-compilation repairs. The 37 new cases remain unrun; the actual GPU Model case is excluded from native-only validation. This proposal is outside C21 and supplies no saved-light authoring, shadows, physical/material parity, CI or installed-app acceptance.

## 2026-10-31: CPU direct-visibility owner — source-only successor

[Captured owner and incremental cursor](../2026-10-31/cpu-direct-visibility-owner/README.md) follows the earlier direct-visibility foundation: an opaque inline owner keeps its scene/options by value and cannot resume a certified cursor against a different caller-supplied capture. The existing borrowed API and 45 original test bodies remain; seven new continuity, terminal and cumulative-work cases are unrun. This two-file successor has no image consumer and is outside C21; it is not standalone on the current checkout or proof of colored shadows, layout, lint, borrow-checking or native quality.

## 2026-10-31: original native CPU acceptance cases G01/G02 — not run

[Mouse/keyboard acceptance procedures](../2026-10-31/native-g01-g02-acceptance/README.md) preserve G01's original 19-body progressive/cancel/export requirement and add the distinct two-box physical-glass G02 case. G02 requires an actual completed IOR1.50 Strict image; a preserved error cannot pass it, and Standard observations remain separate. Actual CAD certification, optical-edit pixel refresh, stale-body recovery and CPU optical persistence are explicit obligations/limitations. Source-derived risks are labelled; all native observations are NOT_RUN. These are editable CAD recipes and procedures, with no bundled models, screenshots, binaries, full sources or runtime results. They do not replace the original 24 cases or constitute an execution grant.

## 2026-10-31: CPU RGB light-transmission shading — source-only prerequisite

[RGB direct-light shading helper](../2026-10-31/cpu-rgb-lighting/README.md) preserves the existing boolean lighting API and eight original test bodies, adding a separate per-light RGB-transmission method and seven analytic/boundary cases. The complete 15-case source set is unrun. It applies finite admitted channel values once without exposure or tone-map changes; invalid values/errors are refused. There is no ImageTask consumer, mode flag, UI, saved-format or render result, and this helper is outside C21. The owned-cursor and RGB proposals remain separate prerequisites for a later coherently reviewed consumer.

## 2026-10-31: physical-glass library to existing editor — source proposal

[Explicit physical-recipe staging](../2026-10-31/physical-glass-library-staging/README.md) adds a bounded action from the six typed starter glass entries to the existing whole-body optical editor. It preserves scalar assignment refusals, authored Closed/Thin choices, current target identity and existing permissions; staging is not Applied, CPU opt-in, certification or persistence. Four paths include one new pending-locale file. The 267 existing test bodies remain and ten original source cases are unrun. No new panel, renderer switch, GPU-guard removal, TIR convergence change or native acceptance is claimed. The new locale's absent before-state is explicit, with no fabricated empty before file.

## 2026-10-31: Interactive dielectric lowering contract

[Three-file test-contract alignment](../2026-10-31/interactive-dielectric-lowering-contract/README.md) preserves the exact valid glass payload with full typed local-retention checks, all other original negatives and new invalid-thickness/IOR controls, plus precise capability documentation. Same 8 declarations / 0 new and 1 disclosed oracle change; full fixture remains inadmissible. Source-only/unformatted/uncompiled/unrun, not a product fix or C21 PASS. Sole delta, before/candidate RAW snapshots and portable bindings are included.

## CPU directional RGB image consumer — source-only proposal, 31 October

[Twelve-file source packet and limitations](../2026-10-31/cpu-directional-rgb-consumer/README.md).
The complete API consumer connects captured direct visibility and RGB shading to bounded CPU image continuations, preserving legacy mode by default. Sixteen new tests are written but UNRUN. All before/candidate sources, unchanged RGB references and the exact delta are included; mixed ancestry and Raster composition requirements are explicit. This is a source archive, not an adopted complete workspace, UI feature-parity claim, test pass or production readiness result.

## C22 composed and adopted source — uncompiled and unrun, 31 October

[Coherent 43-path aggregate increment and source bindings](../2026-10-31/c22-composed-source/README.md). All seven reviewed units are preserved, including the original numeric oracles and two explicit neutral schema fillers. One new locale; 89 new test declarations UNRUN. A single current locked/offline metadata capture passed without source/lock changes; this is not compilation or test evidence. Staged development validation, desktop/native/GPU acceptance and full repository gates remain pending; production PR head is unchanged.

### 31 October — C23 quality budget controls and minimal TIR test-call adapter

[Three-path source and exact delta](../2026-10-31/c23-three-path-source/README.md). Existing GPU review/acknowledgement/currentness/save preserved;74old inspector tests and original TIR oracles retained,7newUI tests UNRUN. No universal CPU/Raster quality presets, compilation, native/GPU/full-CI or readiness claim.

### 31 October — CPU glass directional-shadow UI bridge

[Four-path source and request-boundary documentation](../2026-10-31/cpu-directional-shadow-ui/README.md). UniformON/OFF reaches the existing producer; mixed assignments refuse explicitly. Original277cases retained, one old unsupported-feature frontier deliberately updated,7newcasesUNRUN. Included in C23source only, not native/image/GPU/full-CI qualification.

- [U04 original GPU owner patches: proposed source integration](../2026-10-31/u04-owner-integrated-source/README.md) — Twelve literal hunks from plexi32's #6813 and #6850, original tests and RAW bindings. Not adopted; Rust/Naga/GPU/native UNRUN.

## C24 existing quality-test macro expression (2026-10-31)

[One-file checkpoint, exact two-parentheses delta and source bindings](../2026-10-31/c24-two-parentheses/README.md). Immediate base is C23 post-format source, not the earlier unformatted public candidate. No production change or new tests; C24 resource admission stopped before combined Cargo, with 953 cases and 60 required marker contracts UNRUN. No gate/readiness PASS inferred.

## CPU progressive high-pass film and Interactive controls — 31 October

[Nine source candidates and exact bindings](../2026-10-31/cpu-high-pass-ui-source/README.md)
preserve the source-reviewed 1,048,576 Workstation ceiling, u32 film resolve and
explicit Interactive target/seed controls. Thirteen new tests are written and
UNRUN. **Producer/host per-pass and u32 adapters are still pending; not adoptable
alone, compiled, installed, ready or merged.** Existing budget limits remain.

## CPU every-pass composition after C27 — 31 October

[Twenty-five exact post-format source files and portable bindings](../2026-10-31/cpu-every-pass-c27-source/README.md)
preserve the complete CPU proposal and its actual failed-build source. C27
format/check exited 0; combined compilation stopped on the existing 1,024-byte
metadata-size assertion (E0308). All 982 selected cases are UNRUN; the private
layout repair, full CI and native each-pass acceptance remain pending. This
supersedes the earlier nine-file snapshot for continuation, not production readiness.


## C27 metadata recovery: single active SHA state — 31 October

[One exact host-source successor and bindings](../2026-10-31/c27-sha-state-recovery/README.md)
propose serial PNG/EXR hashing with one active SHA state and the completed PNG
digest. The 1,024-byte assertion remains unchanged. Kernel review and actual
compilation are pending at curation; the planned 983 cases are all UNRUN.
Apply only on the exact C27 failed-source composition, not arbitrary main.

The single-host recovery has now been independently source-reviewed and adopted locally. Compilation, all 983 planned cases and native each-pass image acceptance remain pending.

## Progressive CPU compatibility WIP, isolated fixture and exact C28 host — 31 October

[Separated source packages and exact bindings](../2026-10-31/progressive-compatibility-wip/README.md)
preserve 34 uncompiled WIP candidates plus the actual earlier formatted C28 host
(35 source files). C28: 28 PASS / 1 font-fixture FAIL / 954 UNRUN, two actual
CPU-supported markers; no postchecks/native acceptance. The corrected test is
UNRUN. Compatibility remains NOT ADOPTED, UNCOMPILED and UNRUN; fixed Record
ABI/accounting and one P3 parameter-list omission remain pending. No full CI or
readiness claim, no implied composition of overlapping UI and font-fix files.
