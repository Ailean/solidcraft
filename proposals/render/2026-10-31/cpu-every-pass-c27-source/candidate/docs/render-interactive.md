# CPU Interactive preview

The workspace adapter connects `job::interactive` to the Render canvas and the
Interactive card. Select **CPU ray tracing** in Output and use **On** to enable
the provisional native CPU preview. Entering Render or changing stages does not
enable it or start a final image. The registered `render.interactive` command
uses explicit on, pause, resume and off actions.

Interactive requires an available native worker host supporting CPU ray tracing;
it is not the web/cooperative Raster path or a GPU path-tracing preview. An
unsupported method, host or request reports a reason rather than changing
methods or discarding settings. The final renderer's settings, document format
and export registry remain separate.

## Workspace behavior

After On, committed camera, appearance, visibility, lighting and request edits
invalidate the current preview automatically. Capture groups edits after 200 ms
without a newer change; this is a scheduling rule, not a promised display
latency. The card exposes Updating, Refining preview, Sample target reached,
Pause/Resume and Off. A completed Interactive image is still labelled
Provisional preview; reaching its target does not make it a final result.

Pause cancels owned preview work and may retain accepted pixels as a reference.
Edits mark that reference Out of date. Resume uses the latest input identity.
An appearance or lighting edit alone does not prohibit repeated aligned
selection/material editing. Geometry targets require the accepted image's
camera, authored aspect, visibility, evaluated world and placement metadata to
match the current view. A changed camera, hidden body or changed geometry must
not authorize picking or Paint from stale pixels, including after a navigation
button is released. Unknown source pixels also cannot claim geometry targets.
A missing alignment certificate fails closed; painting a retained image does
not certify its geometry.

The image is fitted without stretching; painting and geometry input share its
actual pixel rectangle. Empty letterbox space is not a geometry target.
Navigation and release/Escape handling remain available when geometry input is
blocked. While an enabled preview awaits its current image, the canvas reserves
that route rather than painting the fast Model preview as current CPU pixels.

Off, leaving Render or changing documents disables/cancels the owned preview
context. A manual final temporarily suspends preview capture; it does not
export an Interactive image or cancel the final job. Result has canvas priority.
A failed capture waits for a relevant edit or explicit Resume instead of
starting an unbounded retry loop.

## Preview envelope

In the coordinated source proposal, `cpu_request` clones an immutable final
request and preserves its camera, background, shadows, exposure, lighting,
resources and transport. If that captured request has explicit CPU progressive
intent, its target and seed are retained; they are not replaced by the current
live draft or silently reduced to 16/0. Only an absent progressive intent uses
the existing fallback of 16 SPP and seed 0. One primary and one selected
reflection attempt are used per camera sample. The original final request stays
untouched.

Preview dimensions remain bounded to 1024 pixels along the longest edge,
rounded to the nearest pixel without enlargement. Integer aspect rounding is at
most half a pixel (or one pixel for an extreme aspect). Preserving a high target
does not remove the selected profile's pixel, total camera-sample, geometry,
traversal or memory intersections. Transparent output, another integrator or
unsupported progressive transport returns an error instead of dropping effects.
These changes still require integrated validation and native observation.
Rectangle/LED direct-light inputs, their emitter-sample count and area-pattern
seed survive preview lowering unchanged. Only camera/reflection sampling uses
the preview envelope; dielectric intent is retained for capture and transport
certification, while unsupported progressive combinations, including anisotropy,
still fail explicitly. This is not a promise of general glass support.
See [CPU progressive sampling](render-cpu-progressive.md) and
[CPU resource profiles](render-cpu-resource-profiles.md) for the separate final
request and admission contracts.

The coordinated retention diagnostic is read from the actual current job handle.
A successful status supplies `Some(count)`, including `Some(0)`; an absent handle
or failed status lookup supplies no count (`null` in the inspection reply), not
a fabricated zero. Lookup errors are not included; other reported errors are independent. A nonzero count
reports skipped snapshot boundaries caused by externally retained slots, not
missing samples or a count of physical canvas repaints; see
[CPU progressive publication](render-cpu-progressive.md#preview-publication-and-final-export).

## Exposure and image ownership

The saved setup's common exposure is finite and bounded to -32..32 EV.
With Interactive Off, Model uses that exposure in its fast product preview for
all final-method choices: CPU ray tracing, GPU path tracing and Raster. Editing
it does not require Start or enable the selected final executor. This Model
path still requires its viewport device and retains its current environment
restriction (rotation 0, intensity 1); it is not a promise of Model/final pixel
parity or of every final effect being represented.

Interactive receives exposure through its captured CPU request. Retained Result
and Interactive PNGs return through their own image-display routes before the
live Model exposure is applied. Editing the current draft must not grade those
captured pixels a second time. An explicit Effects/regrade action is a separate
operation. Design's viewport does not inherit product exposure. See
[Scene and Shot editors](render-shot-scene.md) for active versus selected setup
semantics and the saved Exposure control.

## Adapter contract

- Allocate a distinct transient job session for each document's Interactive
  controller. Never recreate a controller with the same session while callbacks
  from its previous instance can arrive. Do not register its jobs in the engine's
  final/export registry. Reaching its captured sample target still produces
  provisional content, not an exportable final.
- Increment `VisualRevision.visual_revision` whenever camera, draft, material,
  visibility, lighting, HDR/resource, method or output intent changes. Include
  changes that leave the document revision unchanged. Capture this counter and
  `scene_generation` atomically with the full request and scene. This core does
  not infer that identity from a setup name or a partial hash.
- Explicit activation calls `enable`; edits call `edit`. Supply monotonic elapsed
  `Duration` values. `capture` issues one ticket after 200 ms without a newer edit.
  Do not evaluate geometry or read files every frame just to compare identity.
- Recheck `accepts_capture(ticket)` before expensive capture/admission and after
  worker preparation. This avoids preparing an already obsolete request; it does
  not replace the mandatory identity checks in `attach` after admission.
- Capture through the existing authoritative engine boundary, prepare the CPU
  request, then admit/submit through the installed CPU host using
  `CaptureTicket::job_id()`. Pass the resulting handle to `attach`
  with the original ticket. Delayed jobs are cancelled, including a wrong serial
  or generation in our dedicated preview session. A foreign session is rejected
  without cancelling its work, even if its handle arrived through a misrouted
  callback. Final jobs must keep their distinct engine session.
  Report a failed capture through `capture_failed`; failure waits for an edit or
  explicit resume, rather than retrying every frame.
- Read only `current()` to publish checkpoint pixels. The existing host provides
  immutable previews, sample counts, sequence and final target bytes. Their job
  and revision must remain attached to the displayed image. A copied handle or
  retained texture is not permission to display it as current after an edit.
- Pause may retain an immutable checkpoint in the adapter, explicitly labelled
  paused and stale after edits. It cancels work. Resume uses the latest revision.
  Leave Render or change documents with `disable`; Drop also cancels owned work.
- Before a manual final starts, call `set_final_active(true)`. This cancels only
  Interactive work and prevents new capture tickets. Call it with false after the
  final finishes; an enabled, unpaused controller then groups the latest edits
  again. No method here cancels a final job or exports an image.

## Validation scope

These routes describe the current source contract, including the connected UI;
they are not a statement that native visual acceptance or GPU qualification has
passed on this revision. Core/headless regression coverage does not replace a
supported native build and admitted runtime checks. Pending integration gates,
Model/final numeric parity and the installed application's reported shadow
defect are not declared resolved by this document.
