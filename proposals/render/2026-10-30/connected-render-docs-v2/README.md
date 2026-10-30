# Connected Render documentation v2 - source proposal

This preserves two reviewed documentation edits for the connected Render UI.
They remain unadopted and carry no compiler, runtime, GPU or visual acceptance.
A C19 build bound to the adopted frozen source does not include these unadopted
documentation edits; its results cannot qualify their described behavior.

The Interactive text explains captured-image ownership, paused framing/input
limits, common exposure and the existing support/admission boundaries. The Shot
text distinguishes selecting a saved shot from activating it, covers saved
exposure and protects the active draft/camera when an inactive shot is edited.
These are source-contract descriptions, not a general feature-parity claim.

The corrected transport wording retains dielectric intent for capture and
transport certification. Unsupported progressive combinations, including
anisotropy, still refuse explicitly. It does not promise general glass support
or erase the existing final-render and resource-profile limits.

## Exact patch choice

This packet includes only the cumulative C19-RAW patch 31e5651f. Apply it once to
the exact 14cf4b47/d5df48d1 before-file hashes in source-bindings.json; its results
are f98decbc/3258f084. Before/after files use CRLF. The patch has LF headers and
preserves each payload line's original terminator; do not normalize its bytes.
The original Adapter block remains preserved as RAW bytes.

The separate 5b46c565 bridge from superseded v1 LF endpoints is not included.
It is an alternative for a different exact before state, not a follow-on patch.
Never apply the cumulative and that bridge sequentially. The v1 history and its
normalized-replay defect remain preserved; no v1 RAW success is claimed.

These are source hashes, not Git commits. The two-file delta is not a standalone
copy of C19. Existing documentation structure, authorship and license notices
remain in force. No code, new test, asset or generated result is included.
The docs await separate adoption and exact-source native qualification; this
archive does not authorize either.
