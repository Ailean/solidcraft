# G01 native control map — C20 source review

**SOURCE_ONLY / NATIVE_NOT_RUN.** This map accompanies [G01-CASE.md](G01-CASE.md); it changes no G01 specimen or assertion. PNG navigation incorporates the separately documented correction in [G01-ROUTE-CORRECTION.md](G01-ROUTE-CORRECTION.md).

The reviewed source has production UI paths for CPU, Compact, progressive48SPP,640x480, CPU seed81, primary1, EV0 and opaquePNG. This is source evidence, not native acceptance. No requested numeric value needs changing on source evidence.

G01 is opaque and does not require Strict. An additional policy-selection observation can use **Continue reflections**, which stores StrictTir; there is no literal Strict button. Selecting it on G01 does not exercise glass, refraction or TIR.

## Production navigation and controls

Line references below name repository-relative source paths at the C20 review snapshot. Portable file hashes are in source-bindings.json. The C20 archive is a composed source delta, not a shipping application or a full root source tree; see README.md. All native actions remain NOT_RUN.

| Requested setting/action | Actual native control route and available value | Source evidence; limitation |
| --- | --- | --- |
| Output / Sampling navigation | Render browser: Output, then Sampling child. Inspector is scrollable; expand the required sections. | render_ui/scene.rs:310-337; render_workspace_inspector.rs:298-340,781-795,971-988. These are production routes, not the cfg(test) Page::All form. |
| CPU final rendering | Output > Renderer > **CPU ray tracing**. Final work starts explicitly with **Render shot** or ribbon Render. | render_workspace.rs:973-996; inspector:797-808,950-958; render_workspace_ribbon.rs:146-155. Selecting CPU is not admission or successful execution. |
| Compact | Sampling > CPU ray tracing > **Compact** segmented budget choice. | C20 render_workspace_cpu.rs:39-60,112-114. Other choices: Studio, Workstation. Do not raise the budget. |
| Progressive | Same section > **CPU progressive**, rather than CPU fixed. | C20 cpu:116-130. This is the final CPU job mode, separate from Interactive preview. |
| 8 SPP for A; 48 SPP for B/C | Same section > numeric **samples per pixel** field, integer 1..4096, step 1. Enter 8 or 48 and commit the field. | C20 cpu:136-147. Compact additionally caps SPP at 64. There is no 48-SPP quality preset; do not use the GPU samples control, which has a different path. CPU Draft/Balanced/High choices are disabled in inspector:1171-1189. |
| Strict policy | Same section > **Glass rendering** > **Continue reflections**. Spanish labels: Render de vidrio > Continuar reflejos. Alternative: Standard / Estándar. | C20 cpu:99-109; pending/cpu-dielectric-transport.json. The second choice stores StrictTir; it does not itself prove glass rendering. |
| 640 x 480 | Output > Image > **Size**, two numeric fields. Expand Advanced and turn **Lock aspect ratio** off before entering both dimensions; commit and reread both. | inspector:810-837,897-912,1229-1247. CPU numeric range 1..16384 is subject to actual CPU limits. Presets do not include 640 x 480; Custom uses the fields. Do not choose Use Largest/HD/4K as a substitute. |
| Primary sample count 1 | Choosing CPU progressive sets primary_samples to 1 automatically. | C20 cpu:77-85; workspace:999-1020. The AA control is disabled in progressive mode; do not require manually operating a disabled 1x control. |
| CPU seed 81 | Sampling > CPU ray tracing > numeric **CPU sampling seed**, range 0..u32::MAX, step 1. | C20 cpu:141-144. Use this CPU field, not GPU seed. |
| EV 0 | Active shot's exposure control; retain the existing 0 EV, or set its displayed value to 0 and commit. | render_workspace_shot.rs:415-439; ui_kit/fields.rs:644-678. Real slider with visible numeric editor, range -32..32 EV. Verify active shot first; merely selecting a browser shot is not activation. Exact keyboard interaction still needs native observation. |
| Opaque PNG | Output > Image > PNG; Transparent background unchecked. | inspector:877-912; render_workspace_studio.rs:416-435. CPU transparency checkbox can be disabled when already false, which is correct for opaque PNG. Do not require enabling it. |
| Ordinary lighting / shadows | Retain the specimen's studio setup. CPU Lighting offers Neutral, Key/Rim, Warm/Cool presets and a Shadows checkbox. | render_workspace_light.rs:407-443. Follow any visible preset confirmation; no RPC replacement. No environment assets or extra optical workload are needed for G01. |
| Check | Output > Check > Check now, or ribbon Check. | inspector:1039-1048; workspace.rs:358-391. Real host admission remains authoritative. |
| View progressive Result | After completed A, while idle, use Output > View image to select Result before starting B, and again before C if needed. | inspector:950-968; ribbon:253-254. While busy the Output Render section returns at its progress pill before View image; do not rely on that button during the job. Auto-open preference may differ, so record it rather than assuming it. |
| Cancel B | Existing ribbon Cancel or active job progress pill Cancel. | workspace.rs:358-372; render_workspace.rs:2397-2418. This is final-job cancellation; Interactive pause is not a substitute. Timing/responsiveness remain untested. |
| Save setup / export image | Existing Shots Save shot dialog; completed-image export via Destination / result actions. | render_workspace_shots.rs:78,125-149; C20 render_workspace_setups.rs:68-69,177; inspector:915-930; workspace:1662 onward. Saving/activating a setup can replace settings, so reread Output/Sampling afterward. Save/export persistence remain untested. |

## Smallest preparation clarification, with G01 assertions preserved

1. In the disposable native profile and the specified native specimen, choose CPU / Compact / progressive. With 8 SPP selected, set 640 x 480 using unlocked Size fields; set CPU seed 81, retain EV 0 and opaque PNG. If checking the additional Strict policy, select Continue reflections consistently for A/B/C. Reread settings after any saved-shot activation. No UI automation, MCP/RPC or foreign fixture is substituted for native mouse/keyboard acceptance.
2. Complete and export A at 8 SPP as already required. Select Result through the existing View image control while idle. Enter 48 in the real CPU SPP numeric field, commit, and explicitly start B once. Observe the actual partial image and responsiveness, then Cancel before completion. Do not infer final-job cancellation from Interactive pause.
3. Only after B is terminal, perform the original Tall extrusion/color edit. Reselect Result while idle if necessary; reread 48 SPP and all output settings, then explicitly start C once. Preserve the original requirement for three distinct visible provisional subject crops with timestamps and SPP/sequence/coverage evidence, followed by the completed 640 x 480 export. Counts or labels alone do not establish image updates.

This clarifies control routes only. It does not change the 19-body specimen, edit, fixed camera/output, three-update requirement, cancellation requirement, completed-image inspection, admission rules or no-retry rule. If completion precedes cancellation or three observable updates, retain NOT_EXERCISED_CANCEL / NOT_EXERCISED_STREAM. Do not increase caps/workload, add artificial delay or weaken assertions.

At 640 x 480 x 48, the request contains 307,200 pixels and 14,745,600 camera samples. Compact source limits remain 64 SPP, 1,048,576 progressive pixels, 16,777,216 camera samples, 262,144 triangles, 128 MiB geometry, 384 MiB job and 1e9 work units (render/src/job/cpu_budget.rs:51-61). Arithmetic is not host admission: actual geometry, allocation and work checks still govern.

## Native outcome still required

All control rows are SOURCE_UI_PATH_FOUND / NATIVE_NOT_RUN. No source-level requested-value NOTEXERCISABLE was found. The nonexistent 48-SPP preset and literal Strict button are not acceptance prerequisites; their actual controls are described above. If the eventual bound native binary lacks or cannot commit a necessary control, record NOTEXERCISABLE / BLOCKED_UI with the visible evidence. Do not substitute RPC, 32/64 SPP, a higher budget or a relaxed acceptance assertion. Any later binary/source changes require rebinding this map before native acceptance.

The original 24 cases and G01 retain their original statuses. This control map grants no native pass, CPU/GPU parity, optical-quality result, perfect-image claim or permission to launch the application. Build qualification and later desktop acceptance remain separate gates.
