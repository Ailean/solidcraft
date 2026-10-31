# CPU progressive: high-count film and Interactive controls

31 October 2026. Reviewable source preservation for #6201. **Not integrated,
compiled, tested, installed, ready or merged. Do not apply this checkpoint alone.**

## What is written and source-reviewed

- Workstation's explicit CPU pass ceiling becomes **1,048,576**. Compact remains
  64 and Studio 1,024; defaults, pixel counts, pixel-times-sample admission,
  ordinary work limits and memory budgets remain unchanged. At the unchanged
  Workstation camera-sample ceiling, one million passes is admitted only up to
  65,536 pixels. This does not promise all maximum settings simultaneously.
- PrimaryFilm resolves exact coverage and sample counts as u32, removing the
  65,535 representation boundary while preserving radiance sums and division.
- Interactive retains an explicitly selected pass target and seed. A fixed
  request retains the prior 16-pass, seed-zero preview choice and preview fit.
  The controls and PNG-bound current/target labels use the canonical high cap;
  they do not silently resize an image or select another resource profile.
- Seven original film regressions and six request/UI regressions are written,
  **all UNRUN**. High-count film coverage uses real one-pixel commits, including
  65,535, 65,536 and 1,048,576, without a million-image render. UI test source
  includes real cooperative CPU PNG routes; source is not execution evidence.

Independent source review checked the three film/budget files and six UI/request
files, RAW bindings and patch reconstruction. All 19 prior film tests and the
284 prior workspace-test RAW prefix are preserved. The first UI packet had a
new u32/u64 assertion mismatch; this checkpoint contains the reviewed one-line
expected-value type correction, without changing the production DTO or oracle.

## Dependency still being implemented

The producer/host unit that publishes every complete pass, widens its coverage
sidecars, preserves historical area-light sample keys and reuses bounded image
slots is **not included here**. These consumers require that exact adapter;
copying these nine files into the old host is not a supported or compiled batch.
Consecutive per-pass delivery, detailed-scene speed/quality, external observer
retention diagnostics and current desktop acceptance therefore remain pending.

The final implementation must preserve cancellation, visual invalidation,
provisional-versus-final/export authority and Nateaux's UI. More samples alone
do not establish physical correctness or remove the other open Render defects.

## Contents and use

`candidate/` preserves the nine RAW candidate files. `source-bindings.json` pins
every required immediate before/after file, with null before for the new locale
entry. Verify these bytes before composing. `delta-LF-review.patch` is a readable
LF review diff; RAW candidates and hashes are authoritative for mixed EOL files.
Do not apply this to an arbitrary newer checkout or bless old tests as passing.

The production draft #8938 remains at a077906234972dbbf11e80c54893f85ece97c8b7.
This archive changes neither that head nor the installed application. The next
step is exact producer/host composition and independent review, one serial cached
normal build, the affected tests/full repository gate, then actual mouse/keyboard
acceptance showing images for passes 1, 2, 3 and 4 while rendering continues.
