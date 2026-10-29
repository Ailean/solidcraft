# W38a five-file corrective source — preserved WIP

This patch is against PR #7608 head80e5bb347051f2e3972c4cc30ebb87d67fc0e7e2. It preserves our uncommitted corrective source at the user's explicit request. NOT APPLIED TO THE PUBLIC PR, NOT COMPILED, NOT RUN; no ready or merge claim. W38a is supporting preserved work, not an expansion of the current delivery priority.

It covers paper-aware creator parameters and compound redo isolation, including bounded scratch provenance/MAX_UNDO/checkpoint corrections. Independent source review covered the five files; it is not runtime qualification. Preserve the original failed/cancelled validation history in #7608. Rebind to the real receiver before applying; do not overwrite newer work or infer an exclusive file/build grant from this backup.

The source.patch is the payload; no private receipts, process logs, worktree metadata, credentials, assets or binaries are included. Patch SHA256: cb1041d896e635d1df12adb87b656abea6b64004f39a0cb98e9dd3483545b1b8.
