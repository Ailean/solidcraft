# U04: proposed integration of the original GPU owner patches

Source proposal only. Not adopted into the application and not included in C23. No Rust, Naga, GPU or native validation was run for this proposal. It does not establish the cause of, or a fix for, HostUnavailable.

The three files under `candidate/crates/gpu/src/path/` preserve the receiver's existing CPU/Raster admission and Windows viewport guards and incorporate twelve original hunks by **plexi32**:

- [PR #6813](https://github.com/tangency-labs/solidcraft/pull/6813), head `a7b769c676e1fed71003d57adb1e193e5ce9e55a`: two Rust implementation hunks and the original `path_pipelines_compile_one_at_a_time` test. This PR was **unmerged in the captured response**; the head is a donor reference, not a merge claim.
- [PR #6850](https://github.com/tangency-labs/solidcraft/pull/6850), head `f76bd351d74753674d42f05f17fab2d9aecffa84`, actual merge `44a9f9ee42a85404cd4eb08cf95b37d3de5ac872`: eight WGSL hunks and the original `main_inlines_few_bvh_walks` test. The captured merge patches match the owner patches.

Only the original hunks were applied, each at a unique old context. No donor file was substituted wholesale and neither original test expectation was adapted. Unaffected receiver bytes, Raster explicit-light/exposure rejection, CPU contracts, Windows backend guard and `cache: None` remain intact. No dependency or backend/driver/fallback changes are proposed.

`source-bindings.json` records the original and candidate RAW identities, relative paths and frozen receiver manifest. It binds this proposal to three files from `9b193a92875783101d2511342eb4e4781abeb2f149187179f8900931fbd06e3f`; it does not qualify any later receiver. The subsequent C23 seven-path composition did not overlap these three paths, but a future adoption still requires fresh source binding and review.

`reversible-RAW-source-patch.json` records exact old/new bytes as base64, relative paths, stage offsets, stage hashes and donor provenance for each of the twelve edits. Apply in listed order only against the exact bound before bytes; reverse in reverse order against the recorded stages. Reversal was checked as a bytes-only operation and reconstructs the originals exactly. This file is source data, not an executable installer.

`candidate-LF-review.patch` is an **alternative for review only**, with CRLF normalized to LF. LF equality is not RAW equality. Actual candidates preserve context and unaffected bytes, with added lines using the local EOL. The candidate shader equals the actual #6850 merge shader only after EOL normalization.

The #6813 mutex covers shader/pipeline creation in that process, not device discovery, request_device, warm-up/deferred compilation, other pipelines or other processes. It guarantees neither memory admission nor cache persistence. #6850 changes the grouping of floating-point lighting operations; no bitwise, image or convergence equivalence is claimed. The owner tests check synchronization and shader call structure, not GPU-driver behavior or physical output.

All Rust/Naga/GPU/native checks remain **UNRUN for this candidate**. Source review and byte reversal do not constitute compilation, full CI, GPU acceptance or readiness. Integration and subsequent validation remain subject to the owner's coordination, current upstream state and separate resource/backend gates; no competing PR or claim is created by this archive.
