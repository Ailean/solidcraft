# First native CPU Interactive round: partial acceptance

Observed on the actual locally built application from the frozen pre-follow-up candidate, executable SHA256 `04557a8ccd9fa51dc9e0c69dc0f18663546c2791b27abedb63f8c89cc93eb6a2`. This is not a claim about the installed release or the later patches.

Engineer used real mouse/keyboard input in an isolated owned profile: created and saved a20mm cube, entered Render, switched CPU raster to CPU ray tracing, and explicitly enabled Interactive. No product/MCP/scenario commands were used for the acceptance.

- Raster correctly refused Interactive activation with a visible reason. CPU reached Updating, Refining and16/16SPP provisional output, separate from the final result/export slot.
- Applying ABS Blue automatically refreshed the CPU image without starting a final render.
- Changing roughness to0.89 refreshed the request/image, but this cube lacks a useful highlight: visual roughness fidelity is not qualified.
- Applying Warm and cool lighting automatically refreshed the image. Applying the preset also created a Studio saved setup/shot; whether that is intended remains an owner clarification, not a confirmed defect.
- Pause retained the completed provisional image. Editing Cast shadows while paused retained Paused and displayed Out of date. Resume entered Updating and reached16/16SPP again. No receiving ground was present, so cast-shadow fidelity is not qualified.
- Mouse orbit changed the view, entered Updating, and reached16/16SPP again.
- Confirmed presentation defects: our status/actions card collided with the selected-body toolbar; during edits the ray-traced image alternated with the differently lit Model Preview before returning. Legacy preview tooltips also described the old lighting-only behavior.
- Ctrl+S opened command search in one attempt; Save via the visible button succeeded. Input delivery/product shortcut cause remains unverified, so no shortcut defect is asserted.

The synthetic design was saved and the owned app closed normally (exit0). Independent process/source/artifact/lease checks completed. Lifecycle completion does not prove visual correctness.

**Not tested in this round:** exposure; HDR/environment recovery; final rendering/export; cold reopening; narrow-window interaction; physical GPU rendering. No full acceptance, full CI, latency guarantee or GPU parity claim is made. Original unedited app screenshots and private evidence are retained locally; private dialogs, paths, profile/control records and tokens are excluded from this public note.
