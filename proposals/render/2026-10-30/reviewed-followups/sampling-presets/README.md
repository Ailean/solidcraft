# Sampling presets: two exclusive frontend alternatives

SOURCE REVIEWED; UNADOPTED, UNFORMATTED, UNCOMPILED AND UNRUN. Choose exactly one frontend route. These are sampling shortcuts, not a promise of global image quality, convergence, speed or capacity.

| Existing mode | Draft | Balanced | High | Sole changed field |
|---|---:|---:|---:|---|
| CPU fixed |1|4|16| primary_samples |
| CPU progressive |4|16|64| existing progressive.samples_per_pixel |
| GPU path |16|64|256| samples |

Method, resource profile, dimensions, camera, seed, depth, optical policy, materials, lights/HDR, effects and adaptive intent remain authored. CPU options/current-dimension budget and GPU adaptive constraints are validated before mutation; incompatible values are refused rather than repaired. Real Check/Start remain the full scene/resource/admission authority. Raster has fixed final SS2 and no adjustable AA field, so its three presets remain disabled with a reason. Custom keeps existing Sampling navigation. Selection reflects the existing scalar; no preset is applied automatically on load.

## Patch order and bases

- Legacy C16 inspector: apply legacy.patch only. Existing explicit dirty-draft/save/shot-confirmation semantics remain. Seven declarations append to the literal 74-test prefix (81 total).
- Future PR8000 frontend at head 4ccbb633121628e5d0f3222bd2ab5d1d0e07c6ea: apply future-pr8000-sampling.patch, then future-pr8000-persistence.patch. The correction is mandatory: it gives the stage bar its existing persist_editor transaction/rollback while the contextual Sampling inspector retains its own single outer transaction. A missing numerical draft disables preset mutation; Custom stays reachable. Successful transactions clear obsolete library quality advisory, refusals restore the complete prior View. The literal 66-test base gains eight sampling and three caller/persistence declarations (77 total).

The future route requires the separately bound parity backend and its actual frontend architecture. Do not copy Output files into the legacy checkout, apply both alternatives, or apply the common new pending locale twice. It is included once in the first patch of either route. Exact RAW bases/endpoints and EOL conventions are in bindings.json.

The existing library apply_quality projection remains intact and distinct; these presets do not invoke its dimensions/depth/seed/denoise projection. All original test assertions/oracles remain literal preserved prefixes. New caller tests are egui/source acceptance declarations, not native window evidence. Every added test remains uncompiled/unrun; no full CI, GUI/GPU acceptance or readiness is established.
