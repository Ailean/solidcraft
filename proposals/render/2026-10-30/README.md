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
