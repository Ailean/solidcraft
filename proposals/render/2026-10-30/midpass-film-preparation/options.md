# CPU visible progress inside a pass — two bounded options

Design/source inventory only. NO implementation, test body, numerical probe, build, app, shared mutation, Git or GitHub operation. Glass packets remain untouched. Source bindings are in source-bindings.json. Follow Engineer review before any code.

Recommendation: Option A as the smaller first increment, with a reduced bounded preview envelope; Option B only if direct tile updates/first-pixel responsiveness justify its additional commit and acknowledgement contract. Both require actual film-derived data. Neither raising preview_count nor reissuing the old texture is sufficient.

## Verified current seams

- film.rs29 maps film::primary to film_primary.rs. PrimaryFilm stores sum RGB and covered per pixel, global samples and next_pixel (film_primary.rs17). add validates scanline index/absolute ordinal, charges and checks cancel before the one mutation, then advances the prefix. pixel() at125 refuses a partial or empty pass.
- raytrace/image.rs723 finish_sample commits film.add, clears PendingPixel and increments self.pixel. This is the safe completed-pixel snapshot seam. No partial ray/sample contribution may become a film sample.
- image.rs878 camera jitter and secondary samplers use seed, logical final pixel and film.samples. No output sequence/timing enters keys; preserve that.
- image.rs1449 resolves full request dimensions;1531 starts resolution only at final or power-of-two COMPLETE passes.1503 returns Preview then resets self.pixel=0. A midpass path must never execute that reset; it must save/restore the exact completed-pixel cursor.
- host.rs WorkPoll/ImagePoll/PendingPng/PreviewInfo carry only one uniform samples_per_pixel;835 checks powers of two from sequence. RenderPreview contains PNG and a transferred Reservation; Reservation::split at113 is one-way and Drop releases to the global pool, not back to the running task.
- image admission226 and CpuProgressiveOptions.preview_count mod.rs659 prepay every possible retained full PNG. This is valid for the finite existing sequence, not for a streaming sequence.
- UI render_workspace_interactive.rs298 requires spp>=1 and preview dimensions equal request; general render_workspace.rs1325 also requires request-sized frames and labels uniform SPP. Both need explicit partial/reduced provenance, not weakened dimension checks.
- Interactive Snapshot::run engine/render_interactive.rs228 already passes captured scene/request/options/environment to a distinct job; render/job/interactive.rs12 fits its request to1024. A final Workstation request can still be16384 on an edge and67,108,864 pixels. A preview envelope must not overwrite that final request/FrameInfo/export.

## Common invariants and proposed policy boundary

Keep PrimaryFilm.pixel strict for legacy complete-pass/final consumers. Add a separate proposed partial snapshot view; do not change old tests' partial-pass refusal. Existing default whole-pass checkpoint behavior and its exact PNG/work/sequence tests stay unchanged. An internal ephemeral presentation policy selecting the new behavior must be explicitly routed by Engineer through host construction/submission; there is no such policy API today. Do not infer it from JobId or add a saved/public request flag casually. If the behavior is intended for manual progressive jobs too, review that contract separately instead of silently changing the legacy observer.

A snapshot is captured only immediately after a successfully committed pixel with pending.is_none(). Keep the existing self.pixel, film.samples/next_pixel and every sampler ordinal unchanged. Resolving/encoding pauses integration over an immutable captured frontier; resume the SAME next pixel, never replay completed updates or discard a pending ray. At the full-pass boundary the existing cursor-N/film-next0 transition is special and remains distinct from a partial prefix.

For P final pixels, s completed passes and prefix k (0<=k<P):
n(i)=s+1 when i<k, else s; c(i)=film's committed covered count; L(i)=sum(i)/n(i) only if n(i)>0.
No new final-film sample counter is needed. A fresh descriptor records s,k,P and source dimensions, allowing counts for every source pixel. n=0 is UNSAMPLED, not a zero-radiance hit, opaque black, or a measured camera miss. n>0,c=0 is a real miss and retains the original display-sRGB backplate. c/n remains physical coverage; tone/exposure/composite occur once exactly as composite_spatial for sampled data.

New publications need a monotonic sequence independent of SPP, job+visual revision/generation, separate preview dimensions, captured frontier and sampling/reduction provenance. UI must show nonuniform counts/range/coverage honestly. A reduced snapshot cannot masquerade as final dimensions or a uniform1SPP frame. No export/final ownership transfer, old-generation adoption or reuse of a stale payload under a new sequence.

## Option A — bounded reduced whole-PNG snapshot of a partial final film

Smallest change to delivery: retain immutable independently decodable PNG checkpoints, but read a partial film into a bounded display grid, max edge1024 (existing Interactive ceiling), preserving final request and full film.

Use a deterministic PROPOSED representative-source mapping for each preview pixel (u,v):
x=floor(u*W/w), y=floor(v*H/h), source_index=y*W+x.
This is nearest representative sampling of committed final pixels, not a box average, new camera render, or full-resolution convergence statement. Top-left representatives allow the first committed source pixel to produce actual visible content. The mapping is fixed by captured dimensions and never affects trace pixel ids/seeds. A centered mapping is possible but delays first-row visibility; choose one explicitly before implementation.

Resolve at most M=w*h actual source pixels, not all P pixels. Each representative carries its actual n,c; n is derived from s,k and mapping, c requires a bounded u16 sidecar if exposed per preview pixel. The sidecar allocation is billed outside the fixed1024-byte FrameInfo metadata allowance. Unsampled cells use an explicit mask/transparent provisional presentation plus an unknown canvas treatment, never a fake sample/backplate claim.
An unsampled visual placeholder is not the request's transparent_background setting and cannot become exportable output.

At a safe committed frontier with changed represented data and an available publication credit, pause and resolve O(M), encode at preview dimensions, hash incrementally, publish, then resume the exact final-film cursor. Reuse a bounded prefix of existing admitted RGBA scratch if safe; otherwise bill separate4M scratch. Do not allocate a second full final film or change camera rays. Coalesce dirty frontiers while publication is unavailable; do not encode an image the host cannot retain.

Pros: independent snapshots tolerate skipped frames; uses existing PNG decoder/upload path after explicit metadata changes; no tile acknowledgement/mosaic state. Keeps final accumulation byte order intact.
Costs/limits: O(M) resolve, PNG encode, hash, decode and full preview upload for each new snapshot; representative aliasing; no fresh picture until geometry initialization and at least a useful committed pixel exist. It cannot guarantee a wall-time frame rate for a single expensive in-flight ray. Real cadence must be measured; no sleep/fake timer can establish it.

Required paths:
film_primary.rs: new read-only partial accessor/frontier without relaxing pixel();
raytrace/progressive.rs: snapshot/resume frontier;
raytrace/image.rs: safe trigger, bounded resolve/encoder dimensions and cursor preservation, same work counter;
job/host.rs + job/mod.rs: partial descriptor, sequence and resource-credit admission/publication;
job/encoding.rs: existing dimensional encoder reused, not relabelled;
engine/render_interactive.rs and its actual submission route: proposed ephemeral policy, captured revision;
UI render_workspace_interactive.rs and render_workspace.rs: explicit source/preview dimensions, partial sample/unknown state; normal final decoder/export unchanged.
Tests remain in film_tests.rs, primary_aa_tests.rs, host_tests.rs and existing engine/UI test files under their current owners.

## Option B — bounded reduced preview grid, dirty tile patches from committed samples

Retain the SAME final-film row-major loop. After each committed final pixel sample, update one cell in a separate bounded preview grid (max edge1024); cells correspond to integer source-pixel rectangles. This is preview-only state, not another renderer and not tile-first tracing.

Each display cell stores sum RGB, aggregate committed camera-attempt count N and aggregate primary-covered count C over its source footprint. L=sum/N is a weighted mean of committed attempts; C/N is its coverage. N=0 remains explicitly UNSAMPLED. N is not SPP: source pixels in a partially advanced footprint can have s or s+1 samples. The snapshot also records frontier s,k and footprint, so that source per-pixel counts remain derivable. Partial-footprint/weighted-preview provenance must be visible; no claim that it equals an equal-spatial-weight final downsample during an incomplete pass.

Update staging must be atomic with film.add: validate both new states and required work before either mutation, then commit without an intervening fallible/cancel boundary. Never commit film, fail preview work, and then replay the sample to repair the side grid. Each sum uses checked bounds; count type must cover the existing profile's total camera-sample cap, not u16 SPP.

Publish a bounded dirty rectangle/tile (e.g. a proposed32x32 cell cap) with exact rect, sequence/base, source frontier, actual N/C sidecar, encoding and job/revision identity. Copy/resolve O(T) committed display cells, not P; UI uploads that subrectangle to a bounded texture. Final film/request/PNG stay untouched. A raw tile is not a PNG or a final FrameInfo; use a distinct typed provisional payload, not a forged png_sha256.

Delivery MUST handle a slow UI: a delta may apply only to the exact acknowledged base/generation. Keep bounded dirty state and coalesce cells; require an explicit applied-sequence acknowledgement or a bounded full-grid resync if a delta was missed. One latest patch slot without ack/resync loses updates and is invalid. These APIs do not currently exist. No unbounded patch queue, history of tile images, or full-frame clone per patch.

Pros: new content can update a cell from each actual final-pixel commit; small resolve/upload latency and no repeated full PNG. Better fit when the first pass itself is slow.
Costs: additional preview-grid sums/counts and charged work per commit, stricter dual-commit invariants, separate payload/UI texture-update path, acknowledgement/resync protocol and dirty-cell bookkeeping. This is broader than Option A; keep it confined to the same film/host/UI adapter rather than introduce a rendering framework.

Dependencies: all common paths above; replace PNG preview-only delivery with a typed raw-patch path in host/WorkPoll and UI, leave encoding.rs's final PNG logic untouched. film_primary.rs/add and image.rs/finish_sample require staged commit review; progressive.rs owns bounded grid/dirty state. No BVH, BSDF, sampler, kernel or saved-format change.

## Cost at the actual large envelopes

Current limits are numerical admission bounds, not qualified performance results:
Workstation max edge16384 / max pixels67,108,864; max job12GiB. Primary Pixel layout is target-dependent, commonly32bytes (verify target), so this film alone is about2GiB; final RGBA256MiB.
Current encoded bound image.rs229 is8P+65,536: at67,108,864 it is512MiB+64KiB for EACH full-size PNG capacity, before metadata. A complete snapshot also resolves67million pixels and scans/encodes/hashes proportional to that size. Repeating this midpass is not a bounded low-latency display solution.

Both options bound DISPLAY pixels M<=1,048,576 independently. Option A encoded capacity <=8MiB+64KiB plus up to2MiB covered-count sidecar and4MiB working RGBA (or already paid scratch). A16384x4096 final would display at1024x256, not secretly render/export at that resolution. No O(P) downsample scan is hidden: A reads representative indices only.
Option B's grid with3f64 sums+2u64 counts is nominal40M bytes plus checked actual layout/dirty state, at most about40MiB for M=1,048,576. A32x32 patch is at most1024 display cells; include RGBA, N/C arrays, headers and staging in the same admission. These are estimates to verify in source/target, not benchmarked gains.

## Retained copies and backpressure: necessary for BOTH options

Proposed fixed K=2 independently retainable preview allocations plus ONE bounded in-flight encode/copy slot, all paid by the existing job/host lease and existing profile ceiling. K is a review choice, not a new public budget. Cloned Arcs of one payload do not count as another PNG allocation.
Admission must use this peak and actual allocated capacities, not preview_count times all future frames. Do not lower admission thresholds or pretend immutable external payloads disappeared.
Reservation::split alone cannot recycle publication capacity: last-drop releases globally, while the task's original reservation remains depleted. A small per-job lease-credit return/merge seam on the SAME Budget identity is needed; it is not currently implemented. No mutable buffer is reused while an external Arc exists. On terminal cleanup free credits release, externally held payloads retain their actual lease until last-drop; final PNG owns its independent unchanged lease.
If K payloads remain externally held, do not resolve/encode/copy another checkpoint. Continue tracing, coalesce dirty progress and publish the newest actual data when a credit returns. Never block final completion waiting for an uncooperative consumer. Show the last payload as older data with its original provenance, not as a freshly rendered texture.

All initialization/read/copy/resolve/encode/hash work and cancellation use the SAME existing cumulative WorkBudget. No new work budget, reset, uncharged scan, off-thread helper renderer or side pool. Extra snapshots have real cost: cannot honestly promise the same historical total used units or unconditional completion at an already exhausted cap. A reviewed deterministic production trigger (independent of worker quantum/seed) and finite presentation-work scheduling/coalescing are required. Backpressure changes presentation work, never sample ordinals. If the requirement were identical historical work consumption, arbitrary additional presentations are impossible unless replacing existing presentation work; raise this explicitly rather than hide cost.
Legacy mode keeps its old work/quantum/power-of-two tests unchanged. New-policy tests must separate invariant trace results from honestly charged optional presentation work and require LimitExceeded before partial mutation/publication. No invented wall-clock cadence acceptance; later native measurements include resolve/encode/hash/decode/upload.

## Original oracles to propose, not execute

1. Four-pixel raw-film frontier: after first two commits, s0/k2 -> n[1,1,0,0], c[1,0,0,0]; pixel0 radiance[2,0,0], pixel1 real miss, pixels2/3 explicitly UNSAMPLED. After one pass plus two commits, n[2,2,1,1]. With p0 first[2,0,0], second[4,0,0], its mean is[3,0,0], covered2. With p1 firstmiss, second[0,2,0], its premultiplied mean is[0,1,0], covered1/2. No division by zero or black-unobserved fiction.
2. Snapshot/resume at k=1 and k=P-1 and full-pass sentinel: exact next (pixel,ordinal) sequence equals no-partial-preview control, including non-power-of-two target5, seeds0/123, Area1/4/16/64 and HDR/reflection existing fixtures. Compare final PNG bytes and film sum f64 bits, not just visual tolerance. No call-size/timing key enters variates.
3. Existing camera_film_keeps_linear_sums_coverage_and_uniform_absolute_passes still rejects legacy pixel() during a partial pass. New accessor accepts exactly the observed frontier, not arbitrary/trusted input counters.
4. Sampling/coverage display: N>0,C0 preserves exact backplate regardless exposure; unknownN0 is separately flagged. Mixed coverage computes physical C/N and tones once, never averages encoded RGB. A representative mapping must match its chosen source index. B aggregate box of4 committed attempts [2,0,0],miss,[0,2,0],miss yields sum[2,2,0],N4,C2,premultiplied mean[.5,.5,0],coverage.5.
5. K=2 held external payloads: third publication performs zero new buffer allocation/encode, no new sequence; tracing continues. Release one -> next published payload carries the LATEST real frontier, not queued old progress. Cancel/replace during resolve/hash/publication cannot attach a late preview; old external immutable payload stays billed. Final completes while credits are full, independent final-frame lease. Check actual capacities/pool return and last-drop behavior.
6. B only: apply exact base then patch yields known mosaic; missing/duplicate/out-of-order/generation-mismatched patch refuses or performs bounded explicit resync. N/C and dirty-cell versions cannot be silently lost by replacing a latest slot.
7. Resource/cadence acceptance: a high-size request keeps final16384x4096 while preview<=1024edge; instrument bounded read/copy/encoder/upload counts to show no hidden P scan in A/B previews. A deterministic safe-pixel barrier can show a new payload before pass completion without sleeps; this proves midpass publication, not FPS. Later native timed evidence is separate and must include actual preparation and worst pixel stalls.

## Decision requested from Engineer, not user permission

Select A or B, confirm transient routing/default compatibility, preview-side count/provenance fields and the K-credit lifecycle, and define the new-mode presentation work/trigger contract. Then assign disjoint film/host/UI ownership with existing Area/LED/glass preserved. No implementation is authorized by this report. No timer, deadline, app control or automatic successor was created.
