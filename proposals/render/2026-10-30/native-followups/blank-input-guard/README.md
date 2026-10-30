# Waiting-canvas input guard and original coverage

Source-only follow-up to the first native CPU Interactive round. **Applied locally, unformatted, uncompiled and unrun.** Neither this archive nor the current production draft head qualifies these fixes.

Apply `proposal.patch` after the ordered placement/waiting/coverage/formatter patches in the parent directory. Then apply `02-immediate-release-and-selection.patch` once. The latter appends two tests; it does not replace or weaken existing tests. Do not apply standalone waiting alternatives together.

The guard suppresses invisible geometry hover, selection, painting, sampling, drops and context actions while no current CPU pixels are displayed. It retains camera navigation, visible controls, Escape and drag-release cleanup. It observes pending visual changes after navigation and before picking, in addition to the existing final pre-paint observation. That extra observation's cost has not been measured. Paused/accepted images retain their existing routes; this is not blanket disabling of the Render canvas.

Seven original guard cases cover waiting versus Off, paint/sample, drag cleanup, accepted and paused positive controls, navigation, immediate setting-edit/release with no intervening frame, and preservation of existing selection under plain/Shift/Ctrl clicks. They reuse actual accepted CPU snapshots and require positive native CPU branch markers. The complete test source now has208 declarations:194 original plus14 new layout/waiting/input cases. These are declarations, not executed passes.

The independent reviews found no static blocker in the reviewed guard/addendum. Current source is pinned privately; original line-ending mismatch evidence is retained (LF content exact, RAW prefix not exact after Git conversion). C10 ran formatter/check only, then stopped before Clippy at RAM admission. C11 is a disabled proposal for36 selected UI cases and33 positive CPU markers, strict checks and fresh combined engine/UI artifacts. It has not run. Full CI, a new application build, actual mouse/keyboard acceptance and physical GPU validation remain open.

Production draft #8938 remains at a077906234972dbbf11e80c54893f85ece97c8b7. Nathan's assignment-toolbar implementation, panels/results and lighting/environment gizmos are preserved. First-Studio creation semantics remain with the existing owner question; no change is assumed from silence.
