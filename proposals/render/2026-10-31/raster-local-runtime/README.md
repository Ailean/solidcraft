# Raster local lighting — source proposal

Source-only review snapshot: 34 existing repository paths, their exact before/candidate bytes, and one combined patch. This proposal adds bounded transient point/spot and rectangular schematic Raster lighting and an exact-coincidence zero-contribution policy for local Model sources. Saved local-light authoring, shadows, finite rectangle integration and physical/material parity are not claimed.

Base source-manifest SHA256: de1a7c3ae6720e89ba25e92d3eab86673ddef37d8dcc57a675716c6bd8b04b59 (C21). This includes the two separately adopted test compilation repairs; the patch preserves them. bindings.json pins every included source pair and the delta.

The v3 rebind preserves the previously reviewed v2 feature changes and 37 new source test bodies. Its four changed receivers retain existing formatting and test repairs; it introduces no further feature or oracle change relative to v2. Source preservation/replay does not establish compilation or execution.

No compilation, formatting, tests, application/GPU acceptance or CI was performed for this proposal. It is not adopted into the active C21 validation. Re-pin the actual receiver after any later formatting/source change and rebind exact hunks before integration; do not overwrite a newer file with these whole snapshots. Preserve existing contributions and authorship.

`delta.patch` is the sole patch offered here. It starts from the exact before/ state and yields candidate/. Do not apply it successively with the standalone numeric or rebind alternatives from another packet. Independent text checks replayed forward/reverse exactly; no fuzzy matching or line-ending normalization was used.

The new physical GPU test `view::glass_tests::raster_local_model_exact_coincidence_is_black_then_positive_source_repaints_both_formats` must be excluded from native-only validation. Its unsupported-adapter return is not a GPU pass. All 37 new source cases remain unrun on this proposal.

This package contains only source snapshots, this README, bindings and the delta. It contains no private coordination notes, run receipts, compiler logs, review reports or temporary files. Existing project licensing and attribution apply; no external asset, dependency, renderer code or material dataset is introduced.