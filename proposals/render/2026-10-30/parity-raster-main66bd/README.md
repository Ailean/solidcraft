# Raster joint packet V2 — receiver correction

Status: source-only candidate; independent review pending; NOT COMPILED, NOT RUN.
V1 patch 7f5ada4e0ea6d30472848a4475f154665390e08730c2ac6ac18065d1de7e87a4 is BLOCKED by Kernel F1/F2. Its immutable files are preserved and must not be marked accepted.

Base: tangency-labs/solidcraft main 66bd64cc5ebb8f20c63ffc61e154d101f54c71cb.
Scope: crates/render/src/job/image.rs and image_tests.rs; private copies only.

## Corrected receiver access

Pinned source declares Projection::project(&ProductMesh, usize, Option<&FaceAppearance>) at line294.
ImageTask::project(Draw) at line610 obtains the body and look, then delegates to self.projection.
The oracle uses ImageTask::project(Draw::default()), selecting its existing sole body and triangle.
Dimensions and background are read through task.projection.sw/sh/back.

V1-to-V2 changes exactly four test lines (378,382,386,448). No numbers, assertions, expected raw pixel32, negative controls, geometry, or production code changed. The complete current-main12-test prefix and all four additions remain. The other three added equivalence tests use no moved task fields or project receiver calls.

## Artifacts and verification

weighted-scalar-and-reciprocal-w-v2.patch is the complete two-file main-bound proposal.
v1-to-v2-receiver-only.patch records only the correction against immutable V1.
before/ and candidate/ contain exact patch inputs and outputs.
A text-only unified-hunk consumer checked every old context line and hunk count and reconstructed both candidates exactly. No git apply, compiler, formatter, test or runtime was invoked. receiver-access-review.json records the receiver review; verification.json records exact reconstruction and changed lines.
Production candidate SHA256: b834a630d0b5f28f28a93eb01e62583293d53e25bd8853114291187a3dc7b839.
Test candidate SHA256: b69c2001599f3f654857472ef314405cf2463a7164090106e4317bce3022b268.
Combined patch SHA256: ea14cfb540d05e54aad9ecfc732298476e6310c70102f213cb74c6102f5551ca.
Correction patch SHA256: 7e55349cc93fc864bb88ed69680f8fff572beb48afdedf2a5d46efb7cbf37c6e.

## Handoff

Engineer owns integration/public coordination; Kernel re-review is the next step.
Performance remains sole builder under a fresh exact-source grant. Shared checkout is untouched and its older preimage must not be used for this main-bound patch. No Git/GitHub writes or public messages were made.
Keep #5728 admission tests and #6483 ownership. Do not claim this packet compiles, passes CI, demonstrates runtime/GPU parity, or is merge-ready.

Independent focal review closed V1 receiver errors in this V2. V1 remains rejected; all numerical expectations are unchanged. No baseline failure or candidate pass has been executed.
