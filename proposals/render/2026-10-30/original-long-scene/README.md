# Original detailed CPU render fixture - source preparation only

This is the privacy-reviewed original source subset for a longer, inspectable render. Nothing here has been executed in SolidCraft, saved as a native design, rendered or qualified. It is not a .sc file and must not be opened as one.

The fixture specification describes19 separate editable primitives: four PBR spheres, floor/backwall, steps, thin fins/gaps and a post/bridge assembly. The asymmetric32x16 HDR is original synthetic content, not a downloaded library/user asset. The nominal geometry checks are independent analytical expectations, not observed product measurements. No artificial delay or sleeps are used to make a render appear progressive.

- geometry.commands.json is an editable automated-test recipe in the source command format, not GUI acceptance evidence or authorization to run it.
- fixture-spec.json is the numerical geometry/camera/material/light reference.
- raster-comparison.request.json is a geometry/framing/silhouette comparison; Raster does not thereby reproduce the CPU HDR/area-light transport.
- optional-glass-probe.commands.json describes negative-probe geometry. It does not prove physical glass, progressive-glass support or routing through a material library.
- static-checks.json records static checks only.

CPU/GPU long-render requests and portable-save recipes with machine-local HDR paths are intentionally excluded from this public subset. It is not a complete executable test package. Any future automated run needs its own exact current-source/executable/fixture binding and serial resource admission. Native acceptance must create and manipulate the scene through mouse and keyboard; missing GUI controls remain a reported gap, not a reason to substitute engine.script, MFC, CLI or control endpoints. Never replay this recipe to claim a human-interface journey.

Actual future acceptance must separately establish progressive pixels, unknown versus sampled regions, cancellation, captured visual identity, material/light response, retained final/export, save/reopen and effect-specific final image quality. Completion or higher SPP alone proves none of those. Preserve the numerical expectations when the app disagrees and retain raw failed evidence.

See the active tracker: https://github.com/tangency-labs/solidcraft/issues/6201
