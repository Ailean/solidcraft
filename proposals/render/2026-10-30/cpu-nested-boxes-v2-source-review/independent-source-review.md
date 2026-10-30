# CPU nested boxes V2 — independent source review

Disposition: no new actionable source or mathematical blocker found within the accepted ClosedBox-only contract. V1's specific F1 launch-offset mechanism is closed by V2 at source/proof level. This is NOT a compile/test PASS, runtime closure, integration approval, full optical parity or merge readiness. V1 remains blocked and immutable. No Cargo, formatter/compiler, numerical probe, product/test execution, shared edit, Git or GitHub action was performed.

Bound inputs: README5ff008f3c3e1dd6e8febdc9a86ff511a2cf8c863e60c3ac2d9300aeb5c761c0f; manifest95c20c8ff74a0cf0b963c11d5b257ceeac34ebd7d83d934851aa6cb6341e95ad; complete patch1110cb5d84144c396e8d1842f3e716c260efd3691b9a23f2e8b689eb611ac70f; V1 delta1283979d6fd95431a6afff5db21546fba000516cd0bbe88226149d2eaa2be950. Contract reviewed:6b41fcce5dfff48197affd100551cff2a82224dc2327d5b34224ca8cf0a314bf. source-receipt.json records independent hashes, raw prefixes and exact reconstruction of both patch forms by an independent text consumer. Use one form against its own exact base, never both.

## Contract implementation

Candidate dielectric.rs:293-398 supplies ExactBoxFace/exact_box_face. It recovers exactly one constant vertex axis, matches that plane to the selected certified min/max, requires the outward axis normal to agree with the actual triangle normal, and verifies original vertex corner membership. This consumes already-certified twelve-triangle boxes; it does not turn arbitrary bounds into a certificate.

Only the face-axis position coordinate is replaced. Tangential coordinates stay unchanged, finite and strictly within the face. Boundary equality and outside-face reconstruction refuse rather than clamp. Internal triangulation diagonals remain admitted. The incoming axis component must be nonzero and its plane depth must be finite inside the original interval. Equal independently computed depths to another box face refuse regardless of a rounded interior reconstruction or BVH ordinal tie. This covers the contract's explicit representable tie policy, not every real-arithmetic near-corner classification.

ExactBoxFace::spawn constructs the actual normalized Ray at that same point with least-positive min and then checks its actual face-axis sign. For entering=true, reflection requires the exterior sign and transmission the interior sign; for entering=false (an exit), those sides reverse. The boolean logic implements those four cases correctly. Tangent/wrong-side continuations refuse. Total internal reflection continues to have no transmitted branch.

Image integration at image.rs:1319-1333 is guarded by OpticalTask.nested AND the selected Certificate::ClosedBox. shared.position is corrected before current-medium attenuation. Both branch segment_start fields and their new Ray.origin use that same corrected point (1426,1435). Reflection retains the old current medium; transmission and parent restoration/IOR are unchanged from reviewed V1. No new public API or mutable medium stack is introduced.

The F1 proof now applies directly: current own-face plane numerator is c-c=0 and rejected by positive min; Inner's distinct one-ULP plane remains a positive numerator rather than lying behind a shifted origin. No arbitrary separation epsilon, normal-plane predicate, next-up origin displacement or expectation relaxation was added.

## Preserved scope and bounds

With nested_media=false, exact-face recovery is bypassed and both branches call the existing shared.spawn with the unmodified reconstructed point. Legacy constructors still pass false. Host code is byte-identical to V1. ThinSheet and unselected opaque hits cannot acquire an ExactBoxFace. prepared.rs/global spawn, shadow launch, GPU and the progressive dielectric refusal are outside this delta.

The extra boolean lives in the existing OpticalTask allocation; the checked size_of::<OpticalTask>() admission term automatically accounts for its actual layout. Constant three-axis/three-vertex checks add bounded work within one charged interface continuation. Existing selector/traversal cumulative work/cancel checks, maximum eight boundaries,511 paths and fixed pending stack remain. No measured latency, allocation or cancellation outcome is asserted.

The primary camera/near-plane-outside guard remains. Eight-boundary black tail still limits deep paths, including five nested volumes requiring ten direct crossings. Tangential reconstruction, near-corner intersection robustness and arbitrary meshes remain outside the certified launch/depth claim; V2 does not certify them by accepting a positive gap.

## Original test proposals

Independent raw-byte checks confirm complete original and V1 prefixes: dielectric tests26→31→33; image tests36→39→42. Five V2 tests are appended without modifying previous assertions. All thirteen combined V1/V2 added tests are UNCOMPILED and UNRUN.

- Own-zero exclusion/one-ULP neighbor exercises all three axes, both outward signs and incoming entry/exit. It verifies actual Triangle queries and actual normalized spawned rays; this helper fixture does not substitute for the whole task test.
- Exact-plane recovery explicitly accepts a face diagonal and rejects edge/corner/outside/NaN tangential points, bad certificate/normal, incident tangent, outgoing tangent/wrong sign and independently equal incoming face depths.
- The actual F1 task checks four visited media/interfaces and exact physical z endpoints, from both approaches, with one-ULP and separated controls. IOR1/zero absorption makes throughput1 and unit-environment display188 independently consistent. Requiring all four events prevents a matching final PNG from masking a skipped interface.
- The reflected child route checks both approaches. Its prefactor is outer entry*(27/49)*(1/49); first red/green Beer exponents include one Outer mm plus two Inner mm. A second two-mm Inner segment and factor256/147 precede restoration of Outer. Those are the independently derived factors from the stated fixture. It observes actual queued reflected transport, not a stub. A dedicated nested TIR execution case is not added; no claim that it was run is made.
- The opt-in comparison requires the old inward offset for false and exact origin=segment_start for true on an actual first entry. Existing V1 negative/camera/work/Beer/IOR proposals remain intact.

## Dependencies and handoff

This review covers the render V2 packet and its delta, not the separate M2 engine addendum230f1204. Command help/Session tests must be reviewed and qualified as that separate dependency; direct host tests cannot prove them.

The five recorded shared before files matched at receipt read, but that is not an overlay grant. In particular current job/mod.rs has hash03b8cb130fd848a64d30048ecdbfea785e9144827336eb8859473293076bc7ee, different from the earlier V1 review support snapshot; its currently read progressive dielectric guard and nested default remain present. Preserve other owners/AreaLED and rebind all actual final source before integration/execution. Current required document hashes match the versions consulted in this session; AGENTS.md was read again in full. No private plan contents were invented.

Next: Engineer decides integration after remaining dependency reviews, then Performance alone validates freshly pinned actual source under a new grant. Keep source-only F1 closure separate from an observed regression pass. No shared or public action was taken by this reviewer.
