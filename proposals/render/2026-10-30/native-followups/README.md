# Native Interactive follow-ups, 30 October 2026

Work-in-progress archive for #6201 / draft #8938. Production draft head remains a077906234972dbbf11e80c54893f85ece97c8b7. These follow-ups are applied locally but have **not been compiled or tested**.

## Patch sequence

1. `placement.patch`: move our preview card into current-frame measured free space, with the minimal earlier selection-bar composition call. Preserve the author's toolbar implementation and controls. Two original tests.
2. `02-waiting-after-placement.patch`: suppress Model Preview fallback while current CPU Interactive awaits pixels. Four original tests. `waiting-canvas.patch` is the preserved standalone alternative against the old base; do not apply both alternatives.
3. `03-coverage-after-combined.patch`: one additional original test isolates rejection of a live foreign Session while preserving the original context.
4. `04-formatter-tests.patch`: the actual authorized formatter delta, limited to three formatting hunks in the newly appended tests.

All seven new test declarations remain unrun. Together with nineteen existing Interactive regressions, the proposed affected selection is26 exact UI cases and24 positive CPU-supported markers. Static reviews found no blocker in this scope; they do not establish a passing build or runtime result.

Git converted line endings during local patch application: the initial raw-prefix assertion failed and is preserved. The original test prefix and all reviewed candidates matched exactly after LF normalization; no original expectation was changed. The later formatter delta is separately preserved above.

## Current validation state

C10 ran the formatter and formatter check successfully, then stopped at the unchanged7GiB admission gate **before Clippy**:7,493,988,352 bytes available versus7,516,192,768 required, a22,204,416-byte deficit. No Clippy, wasm check, compile, inventories or test bodies ran. All owned jobs drained and leases were released after independent verification. This is a resource stop, not a product failure or a validation pass. No automatic retry is authorized.

## Presentation tradeoff and pending follow-up

The waiting proposal keeps strict identity/invalidation guards and never reuses stale pixels as current, but temporarily leaves the model canvas empty. Source review found that picking/appearance input currently occurs before this presentation branch, so invisible geometry could still receive actions. A separate private follow-up will prevent geometry actions during that hidden wait while retaining camera navigation and visible controls. It is not yet applied or runtime-reproduced; do not present the current waiting patch as fully accepted UX.

The layout helper retains its existing16-obstacle cap and may omit the card in an insufficiently small/obstructed canvas. Native wide/narrow after-validation remains pending. Coordination is in the existing Nathan thread on #8000, comment6507877408. No changes to his toolbar implementation, panels, retained results/compare or lighting/environment gizmos are claimed.

See [the first native round](native-round-one.md) for the actual mouse-and-keyboard observations. Full repository CI and a rebuilt application's native after-acceptance remain pending. Old C7 results and the old application do not qualify these changed bytes.
