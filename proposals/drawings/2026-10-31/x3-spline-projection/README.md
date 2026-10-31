# Conditional X3 SPLINE acceptance

Source-only proposal for SolidCraft; one existing product path, `crates/io/src/dxf.rs`. The added adapter lives exclusively inside its existing `#[cfg(test)]` module. There is no new production helper, import routing or test registration outside that module.

## Provenance and application

The receiver is #8515 at `d8e7a8690a30e44a58255e5dd345881ef092a93a`. The supplied **before** source composes that receiver with **four exact, unedited dxf.rs hunks by rmorgans from #9356** at `2edca81fef00da8e8cd43fbc77ad1dfebff350ee`, based on `f9146cdd5887af90b0999c8dbd8f4fb8ae0f7f18`. Those owner hunks cover the legacy SPLINE arm, Nurbs affine handling, the writer match arm and the owner's original test. They are prerequisites, not changes authored by this proposal.

The candidate adds only our conditional adapter, fixture builder and seven acceptance tests to that composed source. The 84 prior inline test bodies and the entire composed production prefix are preserved. TEXT/INSERT and rmorgans' legacy SPLINE fallback are unchanged.

Apply `own-conditional-appendix.patch` only to source identical to `before/crates/io/src/dxf.rs`; its output is `candidate/crates/io/src/dxf.rs`. All three are literal RAW bytes. No full alternative patch is included: the complete before source plus the owned patch already reconstructs the candidate without duplicating owner changes. Reconcile the exact prerequisites before applying to another receiver.

## Preserve, defer and reject

- **Preserve:** admitted degree/control coordinates, nonuniform clamped knots and unequal rational weights pass to the real `Geom2::bspline`. Units become millimetres once in the typed reader; positive uniform copy transforms only control positions. The owner's even-knot/equal-weight editable-spline canonicalization remains, without promising authored representation bits survive it.
- **Defer:** production projection/routing, TEXT/MTEXT anchor/raw spelling semantics, INSERT/block semantics, original flag provenance and whole-import/expansion budgets. The current typed reader admits only its supported clamped SPLINE subset; it rejects closed/periodic/linear/fit flags and unclamped inputs. `DxfShape` alone cannot prove the original flags.
- **Reject:** factory `Option::None` becomes an explicit `IoError`; other shapes explicitly refuse. No clamping, fitting, resampling, extra unit factor, guessed flags, silent drop or legacy fallback is introduced.

Seven original **UNRUN** tests cover nonuniform quadratic spans with independent analytic points; the owner's even/equal-weight editable path; a weighted quarter circle with inch-to-mm import and scale/translation; unsupported source flags/fit data; malformed/unclamped controls and counts; ten invalid typed inputs producing explicit constructor errors; and non-SPLINE refusal. Real reader, model-copy, factory and public evaluator APIs are used.

## Integration conditions

#9356 was open/draft/unmerged at the pinned metadata capture. Besides its four dxf.rs hunks, five other owner paths remain integration dependencies and are not supplied or edited here:

- `crates/io/src/sketch2d.rs`: factory and Nurbs representation.
- `crates/sketch/src/lib.rs`: sketch representation/public exports.
- `crates/sketch/src/curves.rs`: evaluator.
- `crates/engine/src/cmd/sketch_import.rs`: engine conversion.
- `crates/io/src/svg.rs`: exhaustive geometry handling.

This isolated geometry acceptance proposal does not complete X3 or integrate #9356. Source preservation and byte reconstruction establish no compiler, test, runtime or mergeability result. No formatting, compilation or tests were run for this proposal. Owner-stack reconciliation, independent integration review and compiler/test qualification remain required.

`source-bindings.json` records exact pinned source references and RAW identities. `publication-manifest.json` lists only the files intended for publication.
