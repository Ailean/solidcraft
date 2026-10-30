# CPU display exposure: explicit expansion from ±16 to ±32

SOURCE REVIEWED; UNADOPTED, UNFORMATTED, UNCOMPILED AND UNRUN. This is an explicit new CPU capability, not a reinterpretation that the old ±16 negative tests were wrong.

The two active CPU guards (RaytraceOptions validation and lighting map_to_display) expand together to finite [-32,32]. Default 0, old valid-range arithmetic, radiance/film bounds, sampling/profile/depth, materials/optical policy, lighting/HDR, output mapping and memory/work admission remain unchanged. No clamping, reduced quality or implicit method/profile switch is added.

## Choose one exact nine-path delta

- standalone.patch adds only EV capability to the exact current selected-source bases in bindings.json; it does not depend on or install Strict TIR.
- tir-layer.patch is the same EV feature against the separately bound private TIR-policy/acceptance bases. Before this delta, the required order is deterministic TIR rebind → product policy → acceptance, with exact index/patch hashes recorded in bindings.json. Those three predecessor patches are deliberately UNPROVIDED IN THIS UNIT and NOT INSTALLED in current development source. This unit therefore does not archive a complete TIR implementation. Obtain/review/bind those prerequisites before using this alternative; do not substitute standalone whole files for them. The explanatory default-initializer snippet is already part of policy and must not be applied twice.

Never apply both EV alternatives. Each changes the same nine repo-relative paths, including one authored pending-format descriptor; no existing format number is assigned or generated file exported. Keep original patch bytes: several source files are CRLF and the engine test candidate has an original CRLF prefix with an LF append. RAW endpoint counts are explicit.

## Oracles and retained limits

Three existing negative-contract bodies are preserved verbatim under history/ with their original SHA256. Only their superseded exposure frontier changes: ±16.01/17 become valid within the new feature; nonfinite and outside[-32,32] values remain refused (including ±32.01/±33). Other invalid-input expectations, original names, and optical/TIR policies remain intact. No old test execution is relabeled.

Six new declarations cover exact rational lighting values, fixed/spatial/progressive output with one display gain, offline saved authority and real Check/Start capture/refusal. Their expected values precede execution; all six remain uncompiled/unrun. They are not rendered-image or GUI evidence.

Scope limits: generic FilmPngTask retains its unrelated ±16 contract; current CPU ImageTask uses another encoder. The narrow CPU phrase in docs/render-settings.md still requires an integration update from -16 through 16 to -32 through 32; that shared-document seam is not included here. Strict TIR selector, save/refusal and full Check/Start/Interactive product acceptance are not supplied by this EV delta. Full exact-source CI, native tests and separate GUI/GPU acceptance remain required; no passing gate or readiness is inferred.
