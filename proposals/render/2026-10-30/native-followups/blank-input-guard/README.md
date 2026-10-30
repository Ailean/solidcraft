# Hidden-wait geometry input guard - unapplied prototype

Source-only follow-up to the blank waiting-canvas proposal. Not applied, formatted, compiled, tested or accepted in the app. C10 stopped for resource admission before Clippy; no product failure motivated these changes.

This patch observes current identity after shared navigation and before geometry input, suppresses hidden geometry targets while waiting, and retains the final pre-paint observation. It preserves the original assignment implementation and visible panel/toolbar controls. Five original input tests are included; existing test assertions remain intact. Independent review found no static blocker, but a direct drag starting on actual accepted CPU pixels with a setting change immediately before release is an additional coverage case still being prepared. Picking/painting/native after-acceptance and the cost of the extra observation are unqualified.

Apply only after the exact post-format base documented in REVIEW.md is rebound; do not apply over a different private/source base. See the existing Nathan coordination note in #8000 comment6507877408. This is continuity material, not a production commit or readiness claim.
