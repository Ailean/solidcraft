# Private addendum: no geometry actions on the blank Interactive canvas

2026-10-30. PRIVATE / UNAPPLIED / UNFORMATTED / UNCOMPILED / UNRUN. Root's concrete UX decision is the reason for this addendum; it is not a response to a failed product test. C10 stopped for RAM before Clippy, with formatter/check0 only and no compilation/tests. All earlier individual/combined/coverage/runbook v1 packets remain immutable. Shared source stays postfmt-frozen.

## Proposed change and precise ordering

Only render_workspace.rs::show production code changes. The existing result-view early return is untouched.

In the normal model branch:
1. Keep environment::ring_input and the exact shared navigation::handle(...NavCaps::RENDER) call.
2. Immediately AFTER navigation::handle has applied this frame's camera motion, call interactive::before_paint (refresh with dispatch=false). This observes the actual session/scene/camera/request before a geometry candidate is resolved.
3. Read awaiting_current_frame from that freshly observed state. While true, filter the canvas hover before pick_cached, pass no candidate and inside=false to assign::input, skip geometric tooltip/selection and skip canvas body context-menu opening.
4. Keep the original final before_paint before drawing. A visible action on a current/paused image can change the document/request in this same frame; the final observation must see it, clear current pixels or mark paused pixels Out of date as appropriate.
5. Preserve paint and the waiting-canvas draw branch unchanged. Neither observation dispatches Snapshot. A callback may be drained or an admitted cache updated under the existing refresh policy; this is not a new capture path.

Checking the previous frame's phase before navigation is insufficient: the camera or settings may have changed while the old UI cache/phase still says TargetReached. Removing the final observation would miss a legitimate paint commit on a visible image. This patch intentionally retains both checkpoints.

## Blocked versus preserved interactions

Only while awaiting_current_frame is true:
- No display-target pick_cached raycast, hover target or geometric tooltip.
- No ordinary/add/toggle canvas selection, including accidental deselection by clicking the blank canvas.
- No canvas appearance paint click or appearance pick-up sampling.
- No drag-over geometry preview/hover and no appearance drop commit onto hidden geometry.
- No new body context menu opened from hidden canvas geometry.

assign::input is still called with no candidate and inside=false. Its existing lifecycle handles Escape and clears drag hover/preview on movement/release, disposing the release rather than deferring a hidden target until an image returns. Painting/sampling tools need not be forcibly disarmed just because an update is in progress; their visible controls and Escape remain usable.

Preserved: shared camera navigation, visible environment ring, panel/ribbon/library actions, existing selected-body toolbar and existing menus opened by those visible surfaces. A library action explicitly targeting an already selected body is allowed; it is not a pick on hidden geometry. No edits to assign.rs, render_ui/mod.rs, Nathan's files/Area IDs/actions/layout, navigation.rs or interactive.rs. Do not globally stop appearance tools, clear selection or close panel menus.

The gate is the existing narrowly defined awaiting_current_frame predicate. Off, explicitly Paused with an admitted image, error/fallback, non-CPU, result and final-suspended routes keep their existing interaction policy. Current admitted CPU pixels remain interactive. A queued final or result does not become a new blank-canvas gate.

Navigation retains its own existing pivot/zoom/empty-space raycasts through navigation::handle: these are camera semantics, not selection/appearance/context-menu targets. This packet does not change presets or NavCaps. If all geometry queries during navigation are also to be prohibited, that is a separate navigation-policy decision, not implied by this local action guard.

A final refresh can receive current pixels after an earlier waiting=true guard; input remains conservatively suppressed for that one frame. No claim is made about what the human saw before a newly admitted frame was first presented; the guard uses the current refreshed render state of this frame.

## Five original tests proposed, not run

The append goes into the existing render_workspace_tests.rs. Three private test helpers construct a real small box through the existing fixture, fit its actual scene, locate a genuine face by projection/pick, and feed real egui PointerMoved/press/release/modifier/Escape/wheel events into the production show function. They create no fake frame/controller/renderer, add no product/test observation API, and do not call an injected version of the interaction function.

1. Waiting blocks hover, plain/Shift/Ctrl selection and right-click context opening. The SAME actual hit becomes selectable and opens its body menu with Off. No CAD document/history mutation or capture dispatch during waiting.
2. Real painting and sampling tools cannot act on the hidden target. Tool look/mode and document/history remain intact; Escape still cancels. Off permits the same actual sample/paint click to work.
3. A real Off drag has a genuine preview over geometry. Enabling Interactive and moving/releasing over that same face while blank clears preview/hover and does not commit or defer a document edit.
4. A real accepted 16-SPP CPU job is painted via the existing actual Snapshot/PNG acceptance helper. Current and explicitly Paused images both permit canvas selection and paint. Active paint invalidates its old cache by the final observation; paused paint preserves the admitted image and marks it stale/Out of date. No show capture dispatch.
5. Start from real accepted pixels and a real primary press, change a supported visible setting before release while the UI still holds the old cache, then release. Selection must stay empty: refresh must precede hit resolution, not merely final drawing. A real wheel event while waiting must still move the camera, leave CAD intact and create no pending Snapshot.

Positive Off cases intentionally use the logical no-GPU fixture; they prove that our gate does not suppress those real input targets, not that a GPU ModelPreview rendered. The visible-image positives use actual accepted CPU pixels and existing export-isolation checks. The five tests are native CPU availability guarded; require cpu_supported=true to qualify each intended branch. Cases with two variants require both native paths to reach assertions, not refusal/early return.

exact-test-inventory.txt also retains relevant existing final/foreign-context/error/late-camera and full navigation/appearance regression tests. No existing declaration or assertion is modified. The postfmt prefix contains 201 tests; proposal contains 206. This is written coverage intent, not execution evidence.

## Scope, cost and handoff

The extra non-dispatch refresh repeats current observation/request validation on active frames, and may drain/cache once earlier. Existing pixel-key deduplication avoids a second decode for the same frame. No new mesh extraction, worker, timer, file IO, memory admission policy or cache/admission relaxation is introduced by this patch. CPU/UI cost has NOT been measured; repeated request lowering is an explicit performance risk to check rather than call free. Off/no-context returns through the existing cheap refresh path. Final/result remains its old single early path.

Base is the current native-read postfmt frozen source, not the old pre-format private combined candidate. bindings.json records normalized UTF-8/LF+final-newline source/candidate pins and their provenance. The prior source/patch hashes are not silently reused for the formatted tests. No direct raw filesystem hash, formatter, git apply/check, compile/test, app or GUI run was performed.

For adoption: Root must authorize a source window, repin this exact base, transplant only the two hunks/append and preserve every prior original/W3/W1/addendum test. The sole builder then records a new freeze/hash set and performs fresh authorized validation. Do not copy whole donor files over current source. No resource retry is authorized here.

gui-runbook-extension.md is an additional FUTURE path for the invisible-canvas decision; the earlier runbook remains unmodified. All runtime cases stay NOT_RUN until Root authorizes an actual validated candidate. If C10/resource state remains stopped, wait; do not adapt expectations or product code to that stop.
