# Direct visibility owner: source-only successor

This two-file source proposal succeeds the direct-visibility foundation. It is not a standalone patch for the current application checkout. Apply only against the exact foundation before hashes in bindings.json, then compose later changes with review.

The internal owner stores the captured scene and options by value together with a private reference-free cursor state. Its shared getters, explicit begin and incremental step preserve the original borrowed public API. No exported state/resume interface can rebind certificates to another capture. Active begin refuses replacement; successful or failed completion remains terminal until an explicit successful begin. Work remains charged to the caller's cumulative budget. Future consumers must retain their single budget and cancellation token.

Both original files are included under before/ and proposed files under candidate/; delta.patch contains exactly those two file changes. Forty-five existing tests remain byte-identical as a prefix; seven owner continuity/state/budget regressions are appended. All52 tests are UNRUN for this candidate. Source inspection and exact patch replay do not establish compilation, borrow checking, lint compliance, runtime correctness or native acceptance. The internal owner has no productive caller yet and may cause dead_code warnings; no lint suppressions were added.

No ImageTask, lighting mode, feature flag, command, UI, saved format, dependency or asset is added here. This proposal alone does not enable colored shadows. Integration must separately account inline storage and temporaries, preserve admission and task-wide work/cancellation, compose subsequent source changes and pass appropriate validation. It is not an application release or execution authorization.

Original contribution under the repository's MIT OR Apache-2.0 terms. No third-party assets or code added.
