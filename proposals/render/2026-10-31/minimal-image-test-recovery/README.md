# Minimal image test recovery

This is a one-file, two-line test-only increment over the exact C20 post-format source. It repairs the two carried Render test call sites diagnosed by that failed compilation. The candidate has been adopted as C21 source; it has not been formatted, compiled or tested. No successful recovery build, test PASS, full CI, GPU, desktop or production readiness is claimed.

## Exact source

- Existing repository file: `crates/render/src/job/image_tests.rs`.
- [Before snapshot](before/crates/render/src/job/image_tests.rs): `e4aef1c05dbd600ecb3ea780e514f637acfce99a3738aff8f4bf9c3a918ddd96`.
- [Candidate snapshot](candidate/crates/render/src/job/image_tests.rs): `3b209558bd7363fd41c039f034701141d30fe43ef3764a5cc42f48820bb44922`.
- [Delta](delta.patch): `17c35716ad66c62841ebd0f0775e67184211e3b1296fb51f59749e14dd07c167`.
- [Portable bindings](source-bindings.json) record the exact source inventory hashes and source-review pin. Inventory SHA256 values are not Git commits or test results.

Apply `delta.patch` once to the exact before file. The before/candidate snapshots support inspection and byte replay; they are not extra patches. This increment requires the existing C20 project and is not a standalone application or complete source reconstruction.

## Two existing cases, same oracles

1. `registered_executor_geometry_admission_excludes_the_cpu_raster_estimate`: line 288 adds the SAME `&r` already passed to `admit_source` as the second argument required by `admit_raster(Source, &ImageRequest)`. The authored 2048 x 2048 request, 384 MiB refusal, geometry-only accounting and host refusal assertions remain unchanged.
2. `explicit_raster_request_rejects_capacity_conflicts_and_foreign_integrators`: line 550 replaces `super::Integrator::Raytrace` with `crate::job::Integrator::Raytrace`. The test is a child of `job::image`; the enum is defined in `job`. The same foreign-integrator `InvalidRequest` assertion remains unchanged.

There are 23 original test declarations and 0 new declarations. These are the only two carried body exceptions. All assertions, constants, pixel/resource thresholds, fixture/camera inputs, comments, helpers and other bytes remain intact. Reversing the two unique contextual edits restores every original RAW byte. All other 3579 files in the bound 3580-file C21 source inventory, including production and UI/cache code, remain unchanged by this increment. Larger Raster/shadow proposals are separate and excluded.

## Validation status

The original C20 compilation exited 101 with `E0061` at 288 and `E0433` at 550, with `build-finished success=false`; zero cases executed. This evidence remains a failure. The corrected candidate is source-reviewed only. All 349 proposed native cases and 60 required TRUE markers remain UNRUN for C21; the 23 declarations in this file are not 23 executed tests. Compilation, actual inventories, native execution, physical GPU/desktop acceptance and full CI remain separate validation work.

Existing SolidCraft contributor code and original authored test oracles are preserved byte-for-byte except for the two disclosed call-site expressions. These source snapshots retain the repository's MIT OR Apache-2.0 licensing and attribution; this archive does not change authorship or relicense pre-existing material.
