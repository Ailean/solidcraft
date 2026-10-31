# Completed Result overlay — uncomposed source WIP

31 October 2026. Preservation for #6201 before a machine handoff.
**Unformatted, uncompiled, functionally unreviewed and UNRUN. Not adopted,
installed, ready or merged. Do not apply these whole files alone.**

The author's proposed UI-only change adds an early return from the Interactive
card overlay while the completed Result view is selected. Its intended behavior
is to leave the retained Result image and its copy/export/close actions visible,
then restore the normal Interactive controls after returning to Model.

Two original RAW candidate files are preserved, together with their exact RAW
before files and a full readable diff. The production edit is a three-line guard.
One newly appended existing-area UI regression covers Result/Model controls and
retained image identity. It uses a real cooperative Raster final only as input
to the presentation test; this is not CPU progressive admission, streaming or GPU
acceptance. The test has not been executed and is not claimed to pass.

## Composition warning

The required receiver is the author's frozen d38a source, pinned by the exact
per-file hashes in `source-bindings.json`; it is not arbitrary current main.
This packet is **not composed** with the separately preserved case-976 bundled-font
fixture correction, compatibility changes or canonical-constructor work. The
full workspace-test candidate still contains the original case-976 fixture.
Blindly copying it over a newer receiver can lose those separate changes.
Preserve both corrections when composing the small diffs, then review and run
the resulting exact candidate. No earlier C28 result transfers to this overlay.

`before/` and `candidate/` are authoritative RAW bytes, including existing line
endings. `delta-LF-review.patch` includes additions and removals for review;
`source-bindings.json` supplies portable before/after byte counts and SHA-256.
Private scripts, controls, receipts, logs and machine paths are deliberately
excluded. This source archive does not modify the production PR or application.

Next work: independent functional review, conflict-aware composition preserving
the font and compatibility fixes, affected tests under the single builder,
required repository validation and actual mouse/keyboard before/after acceptance.
