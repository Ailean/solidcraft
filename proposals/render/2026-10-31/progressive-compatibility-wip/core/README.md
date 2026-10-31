# Canonical request and producer compatibility WIP

26 exact RAW candidates from the sealed W3 proposal. **NOT ADOPTED, UNCOMPILED,
UNRUN; not a complete workspace.** Eight new tests are written, not executed.
The sole runtime authority is ImageRequest.cpu_progressive_every_pass, with the
same strict top-level Start control: omitted/true keeps every-pass publication,
false survives serialization and restores the earlier sparse policy. Saved jobs
retain their own sampling authority. No skipped nested alias is introduced.

The UI package supplies six required constructor updates and capture wiring;
prefs supplies two independent switches. Source reviews are not runtime proof.
The fixed Record embeds ImageRequest: actual target layout/ABI and sufficient
fixed admission accounting remain unqualified. Existing task/frame/metadata
assertions were preserved; their preservation is not a compiled PASS.

Known P3: candidate crates/engine/src/cmd/render.rs:127 has an earlier saved-job
control list omitting cpu_progressive_every_pass; later parameter documentation
and the parser admit it. Correct that sentence during coordinated integration.
No demonstrated P1/P2 source defect was reported by the independent review.
