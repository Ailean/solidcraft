# CPU progressive camera sampling

The limits below describe the compatibility **Compact** profile. Explicit Studio and Workstation envelopes are documented in [CPU resource profiles](render-cpu-resource-profiles.md); their implementation batch still requires consolidated validation.

The every-pass cadence and bounded snapshot policy below describe a composed
source proposal awaiting integration and validation, not a new native display or
CI result. The earlier published policy used powers-of-two checkpoints. This is
an intentional requested behavior change; its default-change classification and
preference handling remain unresolved before production readiness. This document
does not settle that process question.

CPU progressive rendering repeatedly traces fresh camera samples into a bounded
linear-light film. Each complete nonfinal pass, 1 through the requested count
minus one, is a checkpoint boundary. Normal consumption publishes each boundary;
the diagnosed external-retention exception is defined below. The exact requested
count produces one final PNG, including non-powers of two such as 3 or 5. A
one-sample job has no nonfinal complete-pass checkpoint. Optional first-pass
partial-coverage feedback is separate from a uniform checkpoint. Publication
does not guarantee that a slower client physically repaints every image.

This mode uses the native `cpu-raytrace` executor and its existing direct-light
and selected-reflection transport. It does not add diffuse global illumination,
caustics, adaptive convergence, a denoiser, or general glass path tracing.

## Request and saved intent

The CPU option is nested inside `raytrace`. The top-level `progressive: true`
request continues to select the registered GPU path executor.

```json
{
  "integrator": "raytrace",
  "settings": { "width": 512, "height": 512 },
  "raytrace": {
    "progressive": { "samples_per_pixel": 32, "seed": 81 },
    "primary_samples": 1,
    "exposure_ev": 0,
    "shadows": true
  }
}
```

Both `render.preflight` and `render.start` use this request. Missing or null
`raytrace.progressive` preserves the existing fixed 1/4/16 spatial AA route.
An explicit empty progressive object defaults to 16 samples and seed 0. The
seed is an unsigned 32-bit integer. Unknown fields are rejected.

The corresponding portable setup is:

```json
{
  "name": "CPU film",
  "set": {
    "width": 512,
    "height": 512,
    "exposure_ev": 1,
    "job": {
      "method": "cpu_raytrace",
      "options": {
        "progressive": { "samples_per_pixel": 32, "seed": 81 },
        "primary_samples": 1
      }
    }
  }
}
```

This is a `render.settings.set` request. Exposure is stored once in the setup;
normalization transfers it into the CPU request. Film storage, checkpoint PNGs,
job identities, cancellation state and host reservations are never serialized
into the design. Saving validates pixel and camera-sample bounds even when no
render host is installed.

## Supported combinations and limits

| Control | CPU progressive contract |
| --- | --- |
| Dimensions | Each side 1..4096; at most 1,048,576 pixels |
| Samples per pixel | 1..64 |
| Total camera samples | At most 16,777,216 (`width × height × samples_per_pixel`) |
| Fixed spatial AA | `primary_samples` must be 1 |
| Selected mirror / isotropic rough reflection | Existing bounded depth; `reflections.samples` must be 1 when bodies are selected |
| Directional / point / spot sources | Existing CPU light contracts and limits |
| Constant / captured HDR environment | Existing CPU contracts; HDR plus rough reflections still requires certified fully metallic selected bodies and effective faces |
| Surface emission / isotropic surface effects | Existing CPU appearance contracts; emission is visible radiance |
| Area / LED sources | Existing bounded rectangle direct-light transport; continuous LED recipes lower to the same sources |
| Dielectric overrides | Smooth `thin_sheet` / `closed_box` intent retained; capture certifies geometry, identity and supported combinations |
| Anisotropic face overrides | Explicitly rejected in progressive mode; use fixed sampling |
| Output | Opaque PNG with the existing display background |

An incompatible request fails; it does not remove settings or switch methods.
The fixed CPU route retains its existing area, dielectric and anisotropy support.
Numerically valid dielectric selectors also survive progressive and Interactive
lowering for capture-time certification. Successful lowering is not scene or job
admission: existing boundary tessellation, captured identity, opacity, camera,
medium and transport-policy limits still apply. The same dielectric body cannot
also select opaque reflection, anisotropy, surface emission or nonidentity surface
effects. Unsupported combinations remain errors; no general glass path is added.
Progressive area/LED uses at most four shared rectangle slots, with
`area_samples` of 1/4/16/64 (default 16) and the existing combined light caps.
Each film pass uses the existing area-power normalization and cumulative work
budget; increasing emitter samples does not multiply emitted power. The area
pattern uses `raytrace.seed`, separately from the camera-jitter seed
`raytrace.progressive.seed`. One emitter sample still uses its centre. This is
direct lighting, with no MIS or diffuse GI; general glass and anisotropic
progressive transport remain unsupported. Host-pool and profile limits are unchanged.
At 1024 × 1024 the camera-sample cap allows at most 16 samples per pixel; at
512 × 512 it allows 64. Requests are also subject to the selected CPU resource profile's geometry,
job-memory and shared-host pool limits; see [CPU resource profiles](render-cpu-resource-profiles.md). Traversal,
shading, film updates and ordinary output work retain the selected profile's
cumulative work ceiling: Compact 1,000,000,000, Studio 1,000,000,000,000 and
Workstation 16,000,000,000,000. The count is not reset at a pass boundary.
Presentation has a separate finite precharge for the bounded early feedback and
nonfinal complete-pass opportunities; increasing the target is not an unbounded
codec/work grant. The camera-sample limit is a cold upper bound, not a promise
that every scene will finish below cumulative work or host-memory limits.

## Sampling, linear film and display

Camera jitter is a deterministic counter sequence keyed by the u32 seed, pixel
index and absolute sample ordinal. Reflection and HDR-direct variates use
separate domains of the same counter. Target sample count, worker quantum and
checkpoint cadence are not keys. Consequently, for an unchanged captured
request and scene, a checkpoint at N samples has exactly the same sample prefix
and PNG bytes as a job whose final target is N.

Each pixel stores three f64 sums and a primary coverage count. Only complete,
uniform passes can be resolved. Camera misses add zero scene radiance and zero
coverage. The resolve computes the mean scene radiance, applies exposure and
the existing tone curve once to covered radiance, composites the display-linear
backplate by coverage, and encodes sRGB once. The background is not illuminated
or exposed. Averaging already tone-mapped sample pixels is not equivalent and
is not used.

`film::primary::PrimaryFilm` is a compact camera-specific accumulator alongside
the existing statistical `FilmAccumulator`. It has incremental initialization
and requires the exact next scanline pixel and absolute ordinal on every update.
The existing general `FilmAccumulator`/borrowed `FilmPngTask` APIs are retained;
the owned image job uses the shared `PngEncoding` pipeline without a
self-referential borrowed task or a second independent pool.

## Preview publication and final export

`JobStatus.preview`, when present, contains:

```text
PreviewInfo {
    frame_sequence: u32,
    samples_per_pixel: u32,
    frame: FrameInfo
}
```

`JobHandle::preview()` returns an `Arc<RenderPreview>` whose `png()` is the
immutable encoded snapshot. The first checkpoint has sequence 1. The job's
`JobId { session, serial, generation }` and captured `scene_generation` remain
unchanged across every pass. `frame_sequence` counts actual publications, not
samples or skipped boundaries. Clients cache previews by `(JobId, frame_sequence)`
and may retain an older snapshot without mutating it. For example, publications
SPP1/sequence1 and SPP2/sequence2 can be followed by a diagnosed retained-slot
skip at SPP3, then SPP4/sequence3 when a slot returns. The producer still processes
each real pass in order; this exception does not license a fabricated producer
jump from pass1 to pass3.

The runtime `JobStatus.cpu_preview_retention_skips: u32` counts nonfinal uniform
boundaries whose snapshot could not be published because both reusable slots
were externally retained. Retire an obsolete host-owned snapshot and retry the
credit before diagnosing that condition. Count that boundary once, not each
poll or a later retry of the same boundary. Closed pools, in-flight buffers,
unrequested feedback or an exhausted optional feedback quota are not themselves
evidence of external retention. Cancellation is checked before the diagnostic
update. This counter is runtime metadata, never saved
intent, and does not count frames physically displayed by a client.

The preview slot is separate from `JobStatus.frame`/`JobHandle::frame()`. A
preview cannot be exported through `render.export`, cannot become the session's
last completed image, and does not release the running task or film. The final
PNG is published only after its digest is complete; it clears the preview slot
and is the only new exportable result. `render.export` writes those exact PNG
bytes and does not render or re-encode them.

Cancellation clears the preview slot immediately. Failure and completion also
clear it. Publication checks the cancellation token while holding the same
record lock used for cancellation, including after the final digest chunk.
The worker cannot publish a late checkpoint into a cancelled or replaced job.
An externally retained preview remains an immutable snapshot of its original
job, but cannot replace a previous successfully completed final frame.

## Allocation and retained snapshots

Admission reserves the working film, existing RGBA/encoder/final-PNG capacity,
and at most two reusable immutable checkpoint slots with their metadata. It no
longer reserves a separate PNG for every possible pass. Protected feedback uses
its existing two partial-snapshot slots. Actual film, sidecar and PNG capacities
remain checked against admitted capacities after fallible allocation; there is
no full-film clone or growing production list of checkpoint PNGs.

A published snapshot retains a lease for its admitted slot. Replacing or clearing
the host's `Arc` cannot mutate externally held bytes or release their charge.
When the last consumer releases a snapshot, its credit can be reused while the
job is active. Terminal cleanup closes free credits; externally retained leases
stay billed until their last release. Existing working codec and final-image
buffers remain accounted separately.

If both slots are externally pinned, the diagnosed boundary produces no new
snapshot. The renderer continues sampling and final work without waiting for an
acknowledgement, failing the final merely because of retention, allocating an
extra slot or inventing a frame. Returning a credit permits a later real pass to
publish again. Final-only export, cancellation and captured identity are unchanged.
Even without pinned consumers, a latest-image client can coalesce observations
when it repaints more slowly than the worker; no latency or unconditional
physical-display guarantee follows from every-pass publication.

## Regression coverage

The source includes regressions for deterministic prefixes and worker quanta,
fresh primary rays, non-power-of-two targets, actual HDR/rough-reflection PNGs,
linear averaging before tone mapping, coverage and unexposed backgrounds,
bounded/cancelled film mutations, and the single cumulative work budget.

Host tests manually advance the worker to the digest/publication boundary. They
hold multiple preview `Arc`s through replacement, cancellation and completion,
check allocation identity and exact pool release, and reject an invalid
checkpoint sequence. Document tests cover offline limits and saved round trips.
Engine tests cover Check/Start equality, cold rejection before HDR capture,
session ownership, exact final export, and retention of the previous completed
image after a progressive job is cancelled. Execution evidence belongs to the
integrated CI receipts; this source handoff does not claim those gates passed.
