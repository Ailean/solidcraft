# Scene overview and Shot editor

The Scene overview replaces the old Scene page. Entering Render with no selection,
clearing the selection, or selecting nothing opens Scene. Selecting a saved shot
opens its Shot editor; the other editors retain their existing adapters during the
selection-based Inspector migration. Browser selection alone does not activate
that setup. Clicking its thumbnail in the Shots strip switches the active setup;
the active highlight identifies the draft used by Model and the next render.

Scene follows E1: Stages (Look, Light, Frame, Output), Ready to render, Reminders.
Each stage row summarises current state and navigates through the view commands.
Look opens the object/appearance stage, Light selects Environment, Frame selects
the current saved shot, and Output selects Output. Check now and Render shot use
the same request and command adapters as the stage bar. Open Check opens the
existing diagnostics editor because this branch has no Check selection kind.
The check summary distinguishes unchecked, current and stale results. Renderer
setup progress, failures, unavailable hosts and offline reasons remain visible.
All shots shows the saved-shot count and stays faded with the unavailable queue
reason; it never silently starts an individual render.

Shot follows E7: Name; changed-camera banner and Update/Revert/duplicate/remove;
Camera; Lens (GPU); Exposure; Frame. Camera contains projection, FOV, distance in
subject-size units, target XYZ, yaw/pitch/roll, Frame subject and Look at selection.
Camera controls save through render.settings.set and apply through render.camera.set;
navigation alone leaves the camera changed banner until Update shot or Revert.
Remove always uses the shared confirmation and checks for stale document state.

Lens values are saved to the selected shot's GPU job while retaining its other
options. Depth of field needs GPU path tracing and perspective; aperture and focus
distance remain visible when disabled. Orthographic FOV stays faded with its
reason. Changing to orthographic while saved depth of field is enabled
is rejected by the existing settings validation; disable depth of field first.
Exposure is enabled for Raster, CPU and GPU setups and edits the selected saved
shot's common finite -32..32 EV value. A slider drag commits on release as one
undoable settings command. Editing the active shot also updates its live exposure
draft; editing an inactive selected shot leaves the active setup's draft and
camera unchanged. Selecting a different final method does not reset exposure.
With Interactive Off, the fast Model preview uses common exposure for all three
methods, without starting a render. Interactive applies it through its captured
CPU request; retained Interactive and Result pixels are not graded again when
the live draft changes. Model and final renderers need not produce identical
pixels. Design's viewport remains independent. See
[CPU Interactive preview](render-interactive.md) for ownership, support and
paused-input rules.

Frame retains aspect presets and transient composition guides. A preset changes
the selected shot's saved dimensions, not just a crop: for example, 16:9 authors
1920 × 1080 and 4:5 authors 864 × 1080. Current method/profile validation can
refuse a preset; it does not silently lower dimensions or samples. Custom follows
the saved dimensions. The aspect selector/guides are transient view state, so
undoing a saved size edit and selecting Custom are distinct operations. The
preview fits its painted frame without stretching and uses those same pixel
bounds for geometry input. Size is read-only in Shot; Edit in Output selects
Output, the home of image dimensions.

These two editors use the shared workspace header without the old setup form,
page switcher or pinned job footer. Explicit native-studio resource reviews remain
in the library card, keeping Scene limited to orientation and readiness. The
shared section_method component supplies method badges, with a gallery sample.
New copy is in assets/locales/pending/render-shot-scene.json for en, ja, zh, de, ko.

The Shot Target uses the shared number_fields row cells with equal widths. The
Projection row uses the kit's base label column to leave room for both full
projection labels, and the distance label is shortened to Distance. Layout
regression coverage checks every editor widget at 350 and 320 points of content
width, including disabled controls, equal Target cells and untruncated labels.

The source includes headless egui and focused render regression cases for these
contracts. Their presence does not claim a completed native visual review,
GPU-device verification or Model/final pixel parity on the revision being
reviewed. The connected Interactive/common-exposure behavior still requires
qualification against the exact supported native build.
