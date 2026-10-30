# Coherent preview, saved lighting and exposure - source checkpoint

This preserves the complete 61-path C19 source delta for recovery and review.
It is unformatted, uncompiled and unrun. It is not a production commit, a ready
PR, a completed issue, a GPU qualification or an application usability pass.
The production branch/PR is unchanged by this archive.

The patch is based on the exact C18 working source, not directly on the published
production head. `source-bindings.json` pins every before/after file, byte length
and new-file flag. The cumulative diff contains all changed code, including the
old lines. Apply only against those exact before hashes; do not replace someone
else's later file with a recovered snapshot. Unrelated source is unchanged.

## Included work

- Bounded persisted Raster directional lights. Missing/null preserves the
  existing studio; an explicit empty list is dark. Saving, editing, hiding and
  reopening preserve the authored rig. The existing backdrop command retains
  lighting, exposure and its one Undo step.
- One common exposure authority, finite -32 through +32 EV, for Raster and CPU
  as well as the existing GPU contract. Raster shades first, then applies EV
  before its existing transfer. The schematic legacy scalar path keeps its
  clipping behavior. This does not make Raster a physical transport renderer.
- The existing Model Preview responds to the common EV even when CPU or GPU is
  selected for the final render. A validated typed value reaches the product
  frame, uniform and cache key. Design frames retain zero exposure.
- Captured Interactive and Result images return before the Model exposure path;
  new tests require no second grading, no altered export bytes and stable paused
  image ownership after later draft edits.
- Paused-image picking uses the captured framing and pixel rectangle. Weak
  document/world identity prevents interaction with stale or unknown geometry,
  without creating a second evaluator or mesh producer.
- The existing Shot exposure slider supports the common range. Editing an
  inactive saved Shot preserves the active draft and camera. The positive
  slider fixtures explicitly activate the saved Shot they intend to edit.
- The original case-0079 optical fixture now supplies the exact pole camera it
  asserts. The normal Top command intentionally stays just short of the pole.
  This is a test setup correction, not a product camera/tolerance change.

The assembly is Core48, Model12, camera/scene compound, common Model exposure,
local Shot-fixture activation, then the CPU Shot range integration. It excludes
the separate strict-TIR prototype/UI, sampling-preset alternatives and unmerged
upstream device/path changes. Existing author and license notices remain intact.

## Validation status

There are 56 additional test declarations in these source files. Their names
are listed in `tests-NOT-RUN.txt`. They are not execution results. Two exercise
real offscreen GPU pixels and require separate device admission; do not count
an unsupported or skipped branch as acceptance.

The planned development validation combines the five affected library test
artifacts in one normal cached build, then runs affected/dependent cases with
required native CPU branches. That plan is not a test pass. The full repository
gate, a bound desktop build and human-style mouse/keyboard acceptance remain
required before production readiness. No previous pass transfers to this source.

C18's actual run stopped at 78 passed, 1 failed and 174 unrun cases. Its failure
was the original exact-pole fixture assertion before rendering. Those raw
results remain recorded in [the issue](https://github.com/tangency-labs/solidcraft/issues/6201).
The earlier HDR representation correction passed in that run; the newer
source changes here have not been executed.

## Remaining limits

This is not CPU/GPU feature parity or a claim of perfect convergence. Physical
glass/TIR integration, GPU-device acceptance, broader Raster approximations,
color-transfer follow-up and all other open issue items remain separate work.
The existing UI layout and gizmos are preserved. No new backend, timeout,
resolution budget, tolerance, ignored test or golden waiver is introduced.
