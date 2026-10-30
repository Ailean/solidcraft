# Explicit product sRGB transfer - source proposal

This preserves one reviewed two-file proposal against the exact C19 working
source. It is unformatted, uncompiled, unrun and unadopted. Archiving it grants
no source application or execution and is not a completed issue or ready PR.

Apply the patch only to the exact before-file hashes in source-bindings.json.
Those are working-source SHA256 bindings, not Git commits. This packet contains
the two-file delta; it is not a standalone copy of the whole C19 source tree.

## Narrow correction

The explicit-light legacy material branch historically left the low sRGB albedo
segment undecoded. It also sent encoded output through that historical decoder
on sRGB attachments. The proposal reuses the existing exact product decoder for
ambient and directional albedo, applies exposure once in linear light and one
Reinhard map, then returns linear RGB to an sRGB attachment or encoded RGB to
Unorm. Alpha is unchanged. Corrected EV0 pixels deliberately change under this
bug contract; no unchanged defective near-black baseline is promised.

The branch is selected by Frame.product_lights=Some(lights), not by Frame.product.
Actual ordinary Design producers use None; a manually inconsistent Design frame
with Some(lights) also enters the changed branch. No independent product flag is
added. The global historical decoder, normal Design/implicit-light source path,
implicit EV0 bypass and PBR shader source remain unchanged. This is source scope,
not an executed Design preservation comparison. No CPU/Raster backend, UI,
camera, material schema or optical recipe is changed.

## Original test, not executed

One new test in the existing GPU view test module proposes 96 renders: two RGBA8
formats, two SSAA settings, plain and index-matched PBR Lambert controls, ambient
and directional sources, two authored RGB triples, and EV -8/0/+1. Each render is
64 x 64. The expected byte uses the independent geom sRGB transfer, linear EV and
one Reinhard; its fixed tolerance is one 8-bit code. No expected value or tolerance
comes from a renderer calibration. The PBR control is a new analytic fixture,
not a reduction of an authored production glass recipe.

The test uses the real Renderer/Frame/GpuScene/readback route. Strict GPU
acceptance must require SOLIDCRAFT_REQUIRE_GPU before process start, an actual
completed test result and the final supported=true marker. An optional
supported=false/no-adapter return is not qualification. Adapter availability does
not establish physical GPU, driver or display quality.

All 96 renders remain unexecuted. Coverage excludes fractional alpha/OIT, BGRA,
float attachments, edge quality, general material/lighting parity and native
visual acceptance. Combined viewport-plus-PBR shader validation, compilation,
real before/after and Design preservation captures, strict GPU execution, full
repository CI and normal maintainer review remain separate requirements. No
previous C19 pass transfers to this proposal.

The patch is copied without modification. Existing SolidCraft author and license
notices remain in force. No external implementation, asset or generated image is
included; the transfer regression is original project test code.
