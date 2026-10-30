# C20 reviewed composition: adopted working source, unrun

This is the exact 16-path source delta adopted into the working checkout as C20.
It contains two new paths and 19 new Rust test declarations. Adoption here means
source bytes only: C20 has not been formatted, compiled or tested. It is not a
production commit, a completed issue, a full-CI result or native/GPU readiness.
Publishing this archive performs no source application or execution.

## Exact provenance

- Post-format C19 before-source manifest SHA256:
  `1bbda5d67a58d1732abd8998ca70eda7486de3b82b2dc7c0c501fb3ae023f480`.
- C20 composition manifest SHA256:
  `eb19e1fb3002b610f8d103137af13e998b941ee82ba1e3cb9ea92ee342961894`.
- Adopted C20 working-source manifest SHA256 (3,580 members):
  `eb6ec43c4fe6862dcf27d9832e8724abf05b3edcdc945f6a8c3e17f35b76d116`.
- Independent final source review SHA256:
  `57b2ec67e923db22be13acc7a28b2abd1334d3c77b99a0872c6c94d782dac7a0`.
- Independent RAW replay audit SHA256:
  `31a85aac6e71efe0cf0e012bd4daa69691f6311fc945be67ffd260cce2bb546f`.

These are artifact/source hashes, not Git commits or test results. The portable
bindings pin each before/after file. Apply only against those exact before
hashes; the delta is not a standalone copy of the whole source tree. The earlier
Strict TIR and U17 archives were separate proposals on different receivers.
Their presence did not establish adoption or execution; do not apply them again
on top of this combined C20 delta.

## Included source

C20 combines the exact post-format Strict CPU dielectric and U17 rebinds, two
connected-UI documentation edits, two test-module imports and one final unread
local time-store removal. It preserves the reviewed C19 CPU exposure +/-32,
Raster fields, common Model exposure, view/camera and Shot source contracts.
Original test bodies and oracles remain intact apart from that disclosed
non-observable store removal; no expectations were adapted to renderer output.

Legacy dielectric behavior remains default/omitted; Strict is explicit opt-in
with existing certificates, finite work/cancellation and honest error limits.
There is no general/perfect-glass, convergence or GPU-parity promise. PNG 188 in
the IOR 3 rational witness is not energy proof; distinct energies can quantize
to that same byte.

U17 fixes the narrow explicit-light product sRGB branch. Some(lights), rather
than Frame.product, selects that shader branch; actual ordinary Design producers
use None. The global historical decoder, implicit path and PBR source remain
unchanged. Corrected explicit EV0 pixels deliberately change under its bug
contract. Source preservation is not an executed before/after image comparison.

## Acceptance still pending

The 19 source names are listed in tests-NOT-RUN.txt. Eighteen belong to the TIR
proposal. The remaining U17 test proposes 96 real GPU renders within one test,
not 96 process cases; it is excluded from the native development selection.
All 19 remain uncompiled and unrun for C20.

The proposed development counts of 349 cases and 60 required TRUE branches are
not compiled inventory or observed results. A future exact-source build and
metadata/case selection must establish the actual cases. A marker in source,
an unsupported/optional branch or any prior C19 result cannot count as acceptance.
Shader/layout/API checks, strict GPU, optical/native/UI acceptance, wasm and full
repository CI remain separate. Existing author and license notices remain in
force; no private host/control/log receipt or external asset is exported.
