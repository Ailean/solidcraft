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
