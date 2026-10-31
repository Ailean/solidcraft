# Render validation evidence at handoff — 31 October 2026

This is a public-safe summary of completed local C28 validation and the subsequent
normal desktop build. It preserves actual outcomes; no command was replayed to
prepare this note. **The complete gate failed/incomplete, not ready or merged.**

## Exact source and artifacts

- Checkout HEAD: `a077906234972dbbf11e80c54893f85ece97c8b7`.
- The actual locally modified, post-format source had 3,583 files and manifest
  SHA-256 `d38a8d946b6653864ad8918d4da662725bd24ca01b2e5aa2d3b38b47b1a7e163`.
  HEAD alone does not contain these newer uncommitted changes. Reconstruct the
  matching archived source and its per-file bindings, not arbitrary current main.
- C28's own five library-test artifacts were built and qualified from Cargo JSON;
  all ten all-tests/ignored inventories ran before the test selection.
- The separately built normal desktop `solidcraft.exe` had SHA-256
  `869b08ffadb758f01cbceeea06bbac1c01e6f77abd896172182c86a8703e60c2`.
  This was an ordinary BIN artifact, not a library test executable or copied cache
  binary. The build itself did not launch the app or qualify a native journey.

Both builds used the normal `ci` profile: `opt_level="1"`,
`debuginfo="line-tables-only"`, `debug_assertions=false`, `overflow_checks=true`.
Library artifacts had `test=true`; the ordinary desktop BIN had `test=false`.
The BIN's actual Cargo features were `bundled-render-library`, `default`, and
`raytrace-cpu`. Cargo recorded package version 0.26.1; this is not a new release.
Builds were serial with the existing cache, `CARGO_BUILD_JOBS=1`,
`RUST_TEST_THREADS=2` and Rayon unset. No clean, copied binaries or profile override
is claimed. Host paths and private control data are omitted here.

## Actual commands and results

C28 combined library-test build — exit **0**:

```sh
cargo test -p solidcraft-engine -p solidcraft-ui-egui -p solidcraft-render -p solidcraft-gpu -p solidcraft-doc --features solidcraft-engine/raytrace-cpu,solidcraft-engine/bundled-render-library --profile ci --lib --no-run --message-format=json --locked --offline
```

The exact selected native test process was invoked with its qualified test name
and `--exact --test-threads=2 --nocapture`. Execution stopped at the first failure:
**28 passed, 1 failed, 954 selected cases UNRUN** out of 983. The two required
CPU-supported markers reached so far were true; none was false. This is partial
coverage, not 983 passing tests or all 60 marker obligations satisfied.

New case 983, `job::host::sequential_png_and_exr_hashing_preserves_independent_payload_digests`,
passed in its own process (exit **0**):

```text
test job::host::sequential_png_and_exr_hashing_preserves_independent_payload_digests ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 761 filtered out; finished in 0.00s
```

Case 976 failed (exit **101**):
`render_workspace::tests::cpu_progressive_editor_preserves_high_intent_without_resizing_or_changing_profile_seed_or_mode`.
Its original raw stderr contained this unchanged panic message:

```text
FontFamily::Name("ui-semibold") is not bound to any fonts
```

The location was the dependency-relative `epaint-0.36.2/src/text/fonts.rs:1025:41`;
only the absolute machine/user path and process identity have been omitted.
The raw failed stdout reported:

```text
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3611 filtered out; finished in 0.06s
```

Source diagnosis: this new UI test made a standalone `egui::Context::default()`
without installing the real named fonts used by the segmented control. This is
not evidence that the production app lacks its fonts, and the log did not show a
failed pass-count, seed, profile or resolution assertion. The minimal proposed
fixture correction adds `crate::fonts::setup_bundled_fonts(&ctx);` to that fresh
context. **The original assertions and failure are preserved. The correction is
archived separately, UNADOPTED and UNRUN; no corrected-case pass is claimed.**

Subsequent normal desktop build on the same frozen source — exit **0**:

```sh
cargo build -p solidcraft --bin solidcraft --profile ci --message-format=json --locked --offline
```

Its own Cargo JSON recorded `build-finished` success and the ordinary BIN artifact
above. It executed no test or inventory and cannot turn C28's failed test gate
into a pass. Any later mouse/keyboard observations require their own evidence;
this note adds no native acceptance result.

## Still unrun or incomplete

- The remaining **954** selected native cases and remaining marker obligations.
- All three planned postchecks: strict affected Clippy (`-D warnings`),
  default-feature wasm check, and repository `lists` — **UNRUN**.
- Full `cargo xtask ci`, complete repository/full-CI readiness and merge gates.
- Current detailed-scene every-pass native acceptance, glass, cancellation,
  final/export checks and physical GPU acceptance are not established by this
  build/test summary. Historical observations or source review do not close them.
- Separate compatibility/preferences, font-fixture and Result-overlay proposals
  remain uncomposed WIP unless a later exact source and result explicitly says
  otherwise. No previous PASS transfers to those changed files.

The normal builds and bounded test outcomes are useful progress, but do not prove
feature parity, perfect convergence, release quality or a completely fixed Render
workspace. Continue from preserved failures and exact source bindings.
