# Native Interactive preview follow-ups, 30 October 2026

These are separate proposed patches against the frozen local M2/M3 candidate based on production draft head a077906234972dbbf11e80c54893f85ece97c8b7. They are archived for review and continuity, not applied to production or qualified by the earlier candidate's tests. Both are unformatted, uncompiled and unrun.

## Placement

Native mouse-and-keyboard testing found the provisional preview status/actions overlapping the selected-body toolbar. placement.patch moves only our card into measured current-frame free space. A minimal composition-order hook draws the existing selection bar before measuring it; its implementation, controls and gizmos remain unchanged. Two original tests cover wide/narrow placement and real selection/Pause/Off hit targets. Extremely obstructed canvases and the shared helper's 16-obstacle bound remain explicit limitations. Coordination with the panel owner and native after-validation remain required.

## Waiting canvas

During supported edits the current candidate drops the invalidated ray-traced image, displays Model Preview with different lighting, then replaces it with the new provisional image. waiting-canvas.patch preserves all image identity/invalidation guards and suppresses that fallback only while an active current CPU Interactive context awaits its pixels. It does not claim an image was painted and does not reuse stale pixels. The tradeoff is a temporarily empty model canvas during updating; continuous edits may prolong that wait. Four original tests cover actual show routing, accepted edits/new captures, paused out-of-date pixels, and camera/document/late-callback isolation.

## Integration and validation

Apply placement first, then re-pin and transplant the waiting hunks and append its tests. Do not overwrite whole files or apply the overlapping test append blindly. Preserve both individual patches and all original assertions. An independent review and a combined private candidate are pending. The shared checkout remains frozen during native acceptance. One builder will validate the combined exact source after the app closes and its runtime is drained; native before/after checks, the repository gate and publication readiness are still pending. No physical GPU acceptance or full CI result is claimed.

Related: #6201 and draft #8938.
