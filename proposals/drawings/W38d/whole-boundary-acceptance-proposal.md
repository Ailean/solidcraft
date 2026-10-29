# W38d D7: original whole-boundary acceptance matrix

Private report to Engineer. Source review and proposed tests only: no product run,
checkout edit, dependency adoption or executed acceptance is claimed.

## Exact source bindings

- R16 W38a: `0xlillith/solidcraft` commit
  `d3b7cfcf6fc00107ce8d90c17a619a1f3901f38d`.
- D6 W38d: `0xlillith/solidcraft` commit
  `5741c5d205f637ecdd09db09cacd6210492f2d7b`.
- The two branches have NOT been composed by this review. The live W38a checkout
  remains another execution's locked source. Public named blobs were inspected;
  the current uncommitted engine batch is outside this report.

Relevant blob identities:

| File | Git blob SHA |
| --- | --- |
| R16 `crates/doc/src/document.rs` | `9d87ee1c845b9ce02df4da1b696a85e3d40bfc17` |
| R16 `crates/sketch/src/profiles.rs` | `553e9c04361f6234ee56ae5fbd21f87efd554fff` |
| R16 `crates/doc/src/drawing/sketch.rs` | `86edcc7cf3b8b1b758dc1345365b29500be41d21` |
| D6 `crates/geom/src/hatch.rs` | `8e753148c9322fac4db5c8492b8689b916a80fb6` |
| D6 `crates/geom/src/hatch/generate.rs` | `114664cf732f06a64537cae2d3ee0586f153e108` |
| D6 `crates/geom/src/hatch/budget.rs` | `243bed689af7f6f7c569790483e4e1389d004f60` |

## Existing APIs: reuse the strict matcher

`WholeProfileBoundary` already exists in `document.rs:100`: `outer: Vec<String>`
and `holes: Vec<Vec<String>>`. `Profile::whole_curve_boundaries()` in
`sketch/profiles.rs:42` returns certified complete-source identities. It is not
equivalent to `outer_curves`/`hole_curves`: its private certificates are absent
for partial/intersected/incomplete graph coverage.

There is ALREADY a strict matcher in `doc/profile_refs.rs`, not a missing
algorithm to independently reimplement:

- `whole_boundary(Profile)` builds `WholeProfileBoundary` from the certificate.
- `validate_whole` requires canonical sorted, unique, disjoint curve IDs. Limits
  are 128 selected profiles, 128 holes/profile, 512 IDs/boundary, 20,000 total
  IDs, and at most 512 bytes/ID.
- `whole_profile_indices` compares outer AND all hole lists exactly, refusing a
  missing or ambiguous complete identity.
- `profile_selection_indices(..., ProfileSel::WholeBoundaries { profiles })`
  uses it. Even the lenient feature wrapper routes this variant to that exact
  matcher; it does not use the ordinary curve-loop fallback.
- `profile_by_curves` explicitly ignores holes (`profiles.rs:1076-1083`) and
  must NOT be the fill identity resolver.

Actual sheet entry is `Document::solve_sheet_sketch(drawing, sketch)` or
`Drawing::solve_sheet_sketch(&Document, sketch)`. `SolvedSheetSketch` has
`drawing`, `sheet`, `id`, `sketch`, `report`; it does NOT have `profiles`.
Use `solidcraft_sketch::find_profiles(&solved.sketch)` and factor the strict
whole-boundary matcher to accept `&[Profile]`. Do not manufacture a model
feature/SolvedSketch merely to call the existing wrapper.

Missing APIs/data: no `SketchFill`, saved fill collection, fill resolver, last-good
fill cache, fill command or sheet fill-scene adapter exists in the inspected
R16 drawing record. Names/statuses below describe test requirements, not APIs
that callers can use today. A later persisted record needs its own pending
drawing-format change. Derived region/cache data must not accidentally become
an authored saved field; its eventual ownership needs explicit design.

## Common original fixture

Use drawing D=100, sheet S=110, sheet-sketch K=120; source id=140 when expressions
are used. These are distinct nonzero drawing-object ids. A future fill F=130
would also be distinct; F is a proposed record id, not an existing field.

Build exact axis-aligned line loops, each with distinct point identities and
the following curve identities. All coordinates are paper mm:

- Outer O: (0,0), (40,0), (40,20), (0,20), IDs `oa,ob,oc,od`.
- Hole H1: rectangle (8,4)-(12,8), IDs `h1a,h1b,h1c,h1d`.
- Hole H2: rectangle (24,10)-(30,14), IDs `h2a,h2b,h2c,h2d`.
- Requested canonical boundary B: outer `[oa,ob,oc,od]`; holes
  `[[h1a,h1b,h1c,h1d],[h2a,h2b,h2c,h2d]]`.

Area oracle is `40*20 - 4*4 - 6*4 = 760 mm2`. Resolve the surrounding region,
not either hole's own island. An optional independent D6 downstream oracle is
one continuous horizontal family, angle=0, origin=(0,0.5), delta_x=0,
delta_y=2, empty dashes. It has ten member lines y=0.5,2.5,...,18.5.
H1 splits two members and H2 splits two others: exactly 14 output strokes,
total length `10*40 - 2*4 - 2*6 = 380 mm`. No tangent/boundary coincidences
are needed. These expectations come from integer arithmetic, not generator
output or refreshed goldens.

## Six discriminating fixtures

### 1. Complete identity survives order changes, not noncanonical saved input

Initial B resolves uniquely to area760 and the 14/380 hatch oracle. Reorder
the stored Sketch curve vector, reverse traversal of each original line loop,
and swap the geometric discovery order of H1/H2 without changing IDs or point
geometry. Recompute profiles: B still resolves uniquely; geometry/area and
hatch remain equal. This catches reliance on profile index or traversal order.

By contrast, a request that stores B's outer or holes in noncanonical order
is invalid input under the CURRENT `validate_whole`; it is not silently sorted
on damaged-file read and is not a Lost attachment. Creation may use the
certified canonical boundary returned by `whole_curve_boundaries()`.

### 2. Delete one hole: explicit Lost, never an outer-only refill

Delete H1's four curves and retain O/H2 exactly. The new surrounding area is
776, but saved B must resolve as missing/Lost, not to this larger region.
Adding an equal-area unrelated hole elsewhere also must not recover B.
Keep the authored B unchanged and return no replacement region. If a later
runtime last-good cache exists, it must retain the previous area760 region;
the pure resolver must not invent persistence for that cache.

This fixture discriminates directly against `profile_by_curves`: that helper
would still match O. It also rejects an area-only or point-inside fallback.

### 3. Replace equal geometry with different identities: still Lost

Replace H1 by an identical rectangle with new curve IDs `r1a,r1b,r1c,r1d`.
Coordinates, area760 and visible hatch geometry remain equal to fixture1,
but B is missing/Lost. Restore the ORIGINAL H1 IDs and remove the replacements:
B resolves again. Never rewrite B to r1 IDs or treat coordinate equality as
identity. Test both delete+recreate in a transaction and a later restoration.

### 4. Wrong owner, malformed identity and ambiguous certified candidates

Make drawing D=200, sheet S=210 with its own K=120 and local curve IDs equal to
fixture1, but outer width60. A request owned by D=100 must use D100/K120 only;
passing the other drawing as an owner must not redirect the original fill.
Resolve each drawing explicitly with the real `solve_sheet_sketch(D,K)` seam
and assert returned drawing/sheet/id before boundary matching. Missing owner
is an owner error, not a geometric search across the document.

Within one sketch, repeating a curve ID is invalid at W38a payload validation.
Within B, repeated outer IDs, an ID reused between outer and a hole, duplicate
hole lists, or an empty boundary are invalid at `validate_whole`, not accepted
as sets. Separately, a private matcher-level defensive test may clone one
certified Profile in the candidate slice. Two exact matches are ambiguous;
the resolver must report Lost/ambiguous or an explicit ambiguity error, never
choose first or nearest. This synthetic slice is NOT a valid serialized Sketch.

### 5. Read-only source update changes geometry, preserves identity

Give the two right outer points an existing W38a paper-length expression based
on source Width. Width40 yields fixture1. Change Width to60 by the model's own
parameter edit, then resolve the drawing: exact B still resolves, outer area
becomes1200 and filled area1160. The same family still gives14 strokes, total
length `10*60-20 = 580 mm`. IDs/B stay unchanged. Resolution must not modify
the source's parameter values/history or authored sketch expressions: compare
the source snapshot immediately BEFORE versus AFTER resolution of each state.

Remove/rename Width: real `solve_sheet_sketch` must produce its unknown-source
parameter error before profiles are adopted. This is a source/evaluation error,
not permission to resolve stale points or select a different source. Preserve
the previous authored B/expression and any runtime last-good region. Drawing
display units and an unrelated view's scale must not rescale these paper values.

### 6. Explicit identity and generation budgets, no partial replacement

At the strict matcher boundary, construct a request with129 hole lists, and a
separate boundary with513 IDs. Both must fail existing admission limits before
candidate matching; keep original B and last-good state unchanged. An input
with repeated IDs is invalid rather than a cheap way to fit a budget. The
production default limits must not be raised for these tests.

Then use the VALID resolved fixture1 with one continuous family at angle0,
origin(0,0), delta_y=0.000001, delta_x0. The 20mm span asks for about20 million
candidate members, above D6's10,000 ceiling; the public `hatch::generate` must
return `candidate line budget exhausted`, not a truncated Vec. Identity is
still Resolved; this failure is a generation/resource error, not Lost. No
new partial marks may replace the old display/cache. D6's other default caps
remain input pieces8192, work2,000,000, events65,536, dash steps200,000,
output marks40,000. Their existing tests remain intact.

## Minimal seam and implementation boundary

Factor the existing strict `profile_refs` matcher over `&[Profile]`, preserving
the existing model selection wrapper's behavior and tests. Keep malformed
selection admission separate from missing/ambiguous match classification so
the new sheet caller need not parse DocError text. A private match result
can distinguish Unique(index), Missing and Ambiguous; this is a proposal,
not an existing API or a demand for a new public enum.

The sheet adapter resolves the exact drawing/sketch, computes current profiles,
uses their complete-source certificates, and obtains the matched Region2.
Save/restore of authored boundaries belongs to the later fill record/command
transaction, through Session::edit_drawing. Retaining the last-good resolved
region belongs to an explicitly owned runtime result. Those responsibilities
must not be hidden inside a function that only compares IDs.

Keep general Curve/WholeProfileBoundary semantics unchanged, do not broaden
profile_by_curves, and do not add another sketch store or solver. Reuse the
same six fixtures for eventual saved records/undo/scene integration once those
APIs actually exist; record-only tests cannot claim that later acceptance.
