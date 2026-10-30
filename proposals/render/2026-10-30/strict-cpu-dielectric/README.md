# Strict CPU dielectric transport: source proposal

This archives a reviewed ten-path proposal against the exact pre-format C19
working-source binding. It remains unformatted, uncompiled, unrun and unadopted.
It is not part of the active C19 build, and no C19 result transfers to it.

The base source manifest is e70ee3bb. The later post-format C19 source binding
(1bb...) is a different receiver. A fresh exact rebind and source review are
required before integration; do not apply stale whole files or rely on a
normalized/fuzzy patch. Full before/after SHA256 values are in source-bindings.json.
These are working-source hashes, not Git commits. This delta is not a complete
standalone source tree.

## Policy and preserved intent

LegacyBounded remains the default and is omitted when serialized. StrictTir is
an explicit opt-in through the CPU Glass rendering control or
raytrace.dielectric_transport="strict_tir"; saved CPU job options preserve that
choice and forward it into the runtime request. Legacy remains selectable.
Changing policy does not clear unsupported authored material/effect intent or
reset exposure, samples, budgets, method, geometry or the captured image.

Strict keeps deterministic total-internal-reflection continuations on the real
spawn, BVH, cancellation, Beer/media and finite work paths. A positive non-TIR
continuation at the optical depth limit returns an explicit LimitExceeded rather
than a successful black residual. It does not erase resource limits or promise
unlimited convergence. Earlier previews, if any, remain provisional.

The existing ThinSheet/ClosedBox certificates, identity and nesting rules still
bound eligibility. Selected nonempty opaque reflections and other unsupported
transport combinations refuse; there is no implicit fallback to another policy.
This is not general or perfect glass support, GPU parity, arbitrary-geometry
certification or a physical-quality result.

The reviewed composition preserves C19 CPU exposure +/-32, Raster lights/EV,
Model/common exposure, view/camera and Shot changes. The original C19 image-test
prefix and all 18 new original test declarations remain intact. These are source
preservation statements, not execution results.

## Original regressions, not executed

The 18 new source names are listed in tests-NOT-RUN.txt. They cover policy/hostile
wire values, conservative refusals, certified continuation/debit/cancellation,
saved intent/UI lowering and public native capture. Two cases require real
NativeWorker and Raytrace availability plus their respective TRUE markers. An
unsupported return, cooperative substitute or marker merely present in source
cannot count as acceptance. Exact compilation and an actual completed result
remain required.

The IOR 3 rational-tail example retains its independent state/error assertions.
PNG 188 alone is not energy or convergence proof: the biased Legacy value and
ideal energy can quantize to the same byte. The configured unlit termination
fixture is not a general physical-absorber certification. No oracle, tolerance,
input or expected result is recalibrated by this archive.

## Apply one exact delta

Only combined.patch is included. It already contains production, acceptance and
all selected stages. Do not apply production.patch, acceptance.patch or stage
patches in addition to it; those are alternative assembly inputs. The copied
combined bytes and ten before/after file pairs are fixed in source-bindings.json.

Current C19 compilation does not compile these unadopted changes or add these 18
source names to its compiled inventory. Compiler/API/layout/feature/metadata,
wasm/full CI and separately admitted native/optical/UI validation remain pending.
Translation and RTL source review does not establish native linguistic acceptance.
Existing contributor and license notices remain in force; no external code,
asset, generated image, private control script or execution receipt is included.
Archiving this proposal authorizes neither source adoption nor a run.
