# Native live-preview acceptance — mouse and keyboard

Status: PLANNED, NOT EXECUTED. Required by Judit on 30 October 2026. This plan is not application evidence and closes no acceptance checkbox.

## Admission and provenance

- Finish C7 and independently verify actual process/Job drain before assigning any application build or runtime. One heavy job at a time; reuse the existing build cache.
- Record the exact source, actual production binary path/SHA, feature set, version, build result and native launch. A library-test binary is not the application, and the installed release does not acquire local fixes automatically.
- Keep installed-release reproduction and locally rebuilt-candidate acceptance separate. Failed checks or incomplete full CI remain visible.
- Observe existing windows first. Preserve user documents, profiles and applications. Use an isolated owned profile only through supported documented launch options, and original synthetic geometry in a dedicated acceptance file. No invented flags, forced graphics backend or silent renderer fallback.
- All product actions use visible mouse clicks/drags and occasional keyboard through Windows Computer Use. No internal product command, MCP endpoint, scripted geometry generation or hidden state mutation substitutes for the user's journey.

## Evidence for every case

Record build and scene, visible starting state, one real action, response/progressive states, stable resulting image or explicit error, screenshot references and PASS/FAIL/BLOCKED/NOT RUN. Observe before each action; use current window/screenshot bindings. No fixed coordinates or control labels are assumed before seeing the application.

Capture only the selected application and owned synthetic scene. Privacy-review screenshots before public upload. Keep file-picker paths, private designs, tokens and unrelated applications out of public evidence. Exported files may be inspected after real UI export to verify dimensions and identity; this is not an internal renderer invocation.

## Journeys

1. Open Render with Interactive off. Locate and activate the real preview control. Observe a visible Updating/Refining/Target reached sequence and a provisional image; record any misleading label or no-op.
2. Change a body appearance and then roughness/reflective response where available. Verify the subject changes without manually starting a final render. Undo/redo should refresh the current intent without invalid target reuse.
3. Change exposure up and back. Verify automatic response and a return to the baseline within the method's documented limits. Distinguish unsupported/refused settings from a silent unchanged picture.
4. Change background/environment and HDR rotation/intensity using the actual controls. Verify both illumination and background policy. Record missing assets, stale pictures and explicit refusals separately.
5. Add/edit/mute a light or apply an existing studio, and restore it. Include the area/LED path as a known pending capability, not a presumed implemented feature. Preserve Nateaux's controls/gizmos and owner scopes.
6. Toggle cast shadows and supported optical/effect controls. Each supported edit must update the visible request; unsupported ones must explain why. Do not mislabel a fixed Model Preview as the traced result.
7. Orbit/zoom and Fit using the real viewport. Verify obsolete frames never overwrite the new camera; check tall and wide output aspects. Fit must show the intended subject/frame.
8. Change final quality/resolution. Verify the provisional preview remains bounded and clearly identified; final settings must not be silently changed to preview values. Aspect changes should follow the current visible frame.
9. Pause; make two edits; wait; resume. Paused must stay paused, then resume the newest intent. Toggle off/on and change workspace/document to check old-frame rejection and ownership.
10. Make a supported continuous gesture. Observe feedback during the gesture and automatic refinement when it settles. Record measured response on Pegasus rather than claiming a universal latency.
11. Trigger one reversible input/asset error through the UI. Confirm a specific visible error, no retry loop or stale success, and recovery after a genuine correction or explicit resume.
12. Start a final CPU render from the UI. Verify its priority over Interactive, progressive feedback, retained result identity and separation of provisional versus final pixels. Export through the native picker into the owned evidence folder; verify it is the chosen final result.
13. Save the owned test design/setup, close/reopen only that owned file and verify intended persistence versus transient preview state. Never discard or overwrite a user design.
14. Repeat key activation/edit/pause/export journeys at normal and narrow window sizes, with keyboard focus. Record clipped controls, hidden primary actions and buttons that act on the wrong editor.

## Follow-up and publication

- Add reproduced failures and evidence to existing #6201, preserving historical findings below the active list. Link current owner PRs and avoid duplicate implementations.
- Explain assumptions affecting another contributor in their own issue/PR, politely and with a brief reason. Nateaux retains panels, result comparison and lighting/environment gizmos.
- Group coherent fixes with disjoint file ownership and original regression tests. Validate affected/dependent code serially, then the repository's actual final gate; do not equate static review, focused tests or draft archives with full CI or installed-app acceptance.
- Preserve every meaningful code increment and its precise validation status in tangency-labs/solidcraft, including clearly labelled draft/WIP source when qualification is incomplete.