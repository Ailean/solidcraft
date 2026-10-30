# Raster local studio helper v2: independent future proposal

This unit is separate from adopted C20 and is not included in its source
manifest, build proposal or test selection. No request, Model or image consumer
is installed by this archive. It remains unformatted, uncompiled, unrun and
unadopted; source review is not numerical, allocation or pixel acceptance.

The candidate adds a standalone shade_schematic helper over the existing opaque
BRDF: directional lights, shared point/spot slots and a rectangle-centre proxy.
It returns relative linear response, not exposure/tone mapping/display output.
It does not promise finite emitter footprints, shadows, calibrated units,
transmission or Model/Raster/CPU/GPU parity. Its exact-coincidence-zero policy
still differs from the current Model fallback; a coupled consumer/shader design
and separate acceptance are required before claiming parity.

Valid inputs use borrowed slices without new containers or retained allocations.
Invalid DTOs may allocate the existing validator's bounded error-message String.
The caller must still admit real capacities/copies/memory, charge bounded work
and preserve cancellation; slice counts grant no resource waiver.

There are 18 new original numeric test declarations and eight preserved prior
cases: all 26 remain UNRUN. The independent v2 closure changes only qualified
allocation wording; function bodies and test oracles stay byte-identical to v1.
No expected value is recalibrated by this archive.

## Exact standalone source and delta

- Before studio.rs SHA256:
  `39eec7cc41207f966994b980a2cec11661a89f7e7a019608616f584261764d26`.
- Candidate studio.rs SHA256:
  `006790b27c2013ee2f05d4b21f04b8db3572b97796774aa657b8d69423846729`.
- Full cumulative studio.patch SHA256:
  `c5ebd47d075ad3aa4bd9a0d129b72987a461d6b993e5c640c1e2b955a45c6199`.
- Focal source-closure SHA256:
  `4c641435891350dd2e4b9b70485ee4c8cf6103ee3457cd8fe1df2704d547f359`.

The base is the exact post-format C19 studio source, retained unchanged by the
C20 delta. These are source/artifact hashes, not Git commits. studio.rs is the
complete candidate for review; studio.patch is the alternative exact delta from
the pinned before file. Do not apply the private wording bridge after the full
cumulative patch. No consumer registration, UI enablement, controller or runtime
operation is provided. Existing contributor and license notices remain in force.
