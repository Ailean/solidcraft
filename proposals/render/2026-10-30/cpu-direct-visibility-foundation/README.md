# CPU direct-visibility foundation — source proposal

**Unformatted, uncompiled, unrun, unregistered; no image consumer is installed.**
This two-file increment preserves an original bounded CPU light-connection component
for [Render issue #6201](https://github.com/tangency-labs/solidcraft/issues/6201).
It is not an image-shadow repair, a new product default or a CPU/GPU parity result.

## Source and integration boundary

The exact receiver is frozen C20 source-manifest
`eb6ec43c4fe6862dcf27d9832e8724abf05b3edcdc945f6a8c3e17f35b76d116`.
[Portable bindings](source-bindings.json) pin both before/candidate files and the
actual surrounding API sources. The [C20 archive](../c20-reviewed-composition/README.md)
provides its source lineage; the published draft PR HEAD alone is not this receiver.

Only `crates/render/src/raytrace/dielectric.rs` and its **existing**
`dielectric_tests.rs` change. Production is one insertion before the unchanged
test-module router. Tests only append; every prior byte, including all 33 old
declarations and expectations, remains intact. [delta.patch](delta.patch) replays
exactly against the two [before files](before/crates/render/src/raytrace/).
The [candidate files](candidate/crates/render/src/raytrace/) and two addition
fragments preserve identical authored bytes, not newly formatted alternatives.

No image/job/studio/UI/command/schema/serialization/appearance/default/registry or
renderer-selection consumer changes. This packet is separate from C20 and from
the existing Strict-TIR camera-transport proposal. It must not be overlaid on an
arbitrary later formatter output or treated as a complete glass implementation.

## Actual API and optical scope

`DirectVisibility::new(&ProductScene, &RaytraceOptions, Ray, crossing_limit,
&mut WorkBudget, &CancelToken)` creates a borrowed cursor.
`step(&mut self, work, cancel) -> Result<Option<Vec3>>` returns `None` for
pending actual work and `Some(rgb)` only for a completed connection.
Success or any step error makes the cursor terminal.

This is **one straight exterior-to-exterior direct-light approximation**.
Certified thin sheets reuse the existing incoherent polarized-sheet transmission
closure. Certified disjoint axis-aligned boxes use reciprocal external-angle
Fresnel at both unbent interfaces, multiplied by Beer absorption over the original
entry/exit segment. The solid connection includes no reflected returns, bending,
focusing or caustics. Both endpoints must be outside admitted boxes; no partial
in-medium endpoint or extra eta factor is inferred. Existing camera/refraction/TIR
code and its old oracles remain unchanged.

At normal incidence with n=1.5, R=((n-1)/(n+1))^2=1/25.
The straight solid's factor is (1-R)^2=576/625.
The unchanged zero-absorption incoherent sheet instead gives (1-R)/(1+R)=12/13.
These are distinct contracts. With sigma=(ln2,ln4,0), a one-mm index-matched slab
gives (1/2,1/4,1), two slabs multiply to (1/4,1/16,1), and eight to
(1/256,1/65536,1). For sheet plane z=x/2 at unit virtual thickness and index match,
the optical distance is sqrt(5)/2 mm. These are original analytic expectations,
not numerical execution results.

## Geometry, cost and error bounds

- At most eight selected bodies; exact captured body ordinal **and** name are
  certified. Thin sheets have one/two consistently wound planar triangles,
  boxes twelve complementary outward corner triangles, and certification admits
  at most 36 positions. Unsupported meshes never receive a fabricated certificate.
- Crossing limit is explicit 1..=16. Selected nested/touching/overlapping boxes,
  selected sheets conservatively touching/overlapping a box, box edge/corner
  ambiguity, and origin/start/end on or inside a box refuse.
- Actual scan interval is [min,max): geometry at a finite light endpoint is
  excluded. Original origin and normalized direction stay literal; only min
  advances to next_up(hit.distance). Co-depth **different** bodies refuse.
  A crossed certified sheet's complete body is masked once, preventing its
  triangulation diagonal from being counted twice.
- Unselected geometry is explicitly opaque. Only real opaque blocking or
  mathematically zero transmission completes black. Pending traversal,
  cancellation, ambiguous/malformed input, a missed box exit and exhaustion never
  become a white/black fallback result.
- The existing BVH API lacks the all-ties/exclusion seam needed here. This
  foundation scans actual borrowed source incrementally, one triangle intersection
  or body transition per step, then a separately debited bounded optical stage.
  Cost is O((crossings+1)*(triangles+bodies)), **not** accelerated traversal.
  Existing caps are 8,000,000 triangles per pass and 262,144 bodies.
- Constructor validation/certification/pair checks and every step spend the
  **same caller-owned cumulative WorkBudget**; the caller must not replace/reset it.
  No new work ledger or credit is created. Cancellation checks precede new debits.
  Work/crossing exhaustion returns existing `TraceError::WorkLimit`;
  cancellation returns `Cancelled`; input/identity ambiguity returns errors.
- Fixed arrays/scalars and borrowed scene/options create no new containers during
  valid setup/steps. The existing invalid-options validator may allocate its
  bounded error String. No compiled size, heap/stack/RSS or performance measurement
  was made. Future admission must account actual metadata, retained capacities
  and copies instead of assuming this prototype fits.

The unchanged validator also bounds finite IOR to 1..=3, absorption per mm to
0..=100 and sheet thickness to 0..=10,000 mm; closed-box thickness must be zero.
The existing global Ray/WorkBudget limits still apply. This does not increase
output resolution, sample limits or host resources.

## Evidence and remaining work

[Original acceptance inventory](acceptance-cases-NOT-RUN.md): 33 preserved plus
12 new declarations, **all unrun on this candidate**, with fixed original channel
tolerance 1e-13. Static independent review found no actionable defect in the
delivered component. A separate text/hash replay reconstructed both candidates
exactly. Neither establishes Rust typechecking, Clippy, compilation, a numerical
test result, current native/GPU acceptance, full CI or readiness.

Before product enablement: rebind the actual receiver/APIs, integrate a real
`PreparedScene.source` borrowing consumer, admit its true memory/cumulative work,
preserve cancellation and structured errors, and schedule progressive image work.
Then run original component and affected/dependent validation plus real images
and native interaction. General mesh glass, nested direct volumes, refracted
light paths, rough transmission, caustics and demonstrated GPU parity remain
outside this component.

Original SolidCraft contributor source and analytic tests; **MIT OR Apache-2.0**.
No new dependency, third-party asset or commercial renderer reference is added.
Authorship and original assertions are preserved.
