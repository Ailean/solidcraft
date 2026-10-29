# Drawings code checkpoints — 29 October 2026

All links below refer directly to branches in **tangency-labs/solidcraft**. Existing PRs remain the coordination/review threads. These snapshots preserve unfinished work at Judit's request; they do not create new claims, transfer ownership or imply readiness for merge.

| Unit | Canonical branch | Existing discussion | Qualification |
| --- | --- | --- | --- |
| X1 / #5318 | [X1 checkpoint](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-x1-checkpoint-20261029) | #6184 | Exact existing `3fb2eef9` head; draft/conflicted, original-output failure remains. |
| X2 / #5319 | [X2 checkpoint](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-x2-checkpoint-20261029) | #8435 | Exact `fac00435`; preserve its failed wasm gate separately from later D8. |
| X3 / #5320 and integrated X4 subset | [D8 checkpoint](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-x3-checkpoint-20261029) | #8515 | Exact `d8e7a869`; Windows local full gate passed: 82 native binaries, 9603 passed, 0 failed, 69 existing ignored; strict Clippy, wasm, docs and offline web passed. This evidence applies only to this exact source. Draft/unmerged. |
| PDF / W22 P1/P3 + P2 original tests | [PDF P2 WIP](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-pdf-p2-wip-20261029) | #7626 and #4890 | `bf2a6fd5` preserves nine new P2 files on `506d4190`. 77 PDF test declarations, none executed on this new source. New dev dependency unresolved in Cargo.lock; production font/text integration pending. Parent history retains P1/P3. |
| X2/X3 command follow-ups | [Tests and connect-only stage A](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-dxf-command-followups-wip-20261029) | #5319, #5320 | `8eec0bf6`: four files, static review only. Parent D8 results do not qualify this source. |
| X2 conditional producer alternatives | [Owned-writer alternatives](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-dxf-owned-adapters-conditional-20261029) | #5319 comment6482559607 | `a211e61c`; NOT for integration before owner admission-policy decision. Unrun. |
| PDF prerequisite | [Original serial-runner proposal](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-pdf-serial-runner-proposal-20261029) | #5425 comment6415005697 | `e65a33d3`; isolated original #5494 delta, pending owner response; unapplied to PDF and unrun. |
| W38a | [Existing implementation](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-w38a-checkpoint-20261029) | #7608 | Exact `80e5bb34`; corrective five-file patch is preserved below, not applied to the PR or tested. |
| W38d | [Existing implementation](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-w38d-checkpoint-20261029) | #7064 | Exact `5741c5d2`; D7 acceptance proposals below are unexecuted; no full-unit completion claim. |
| W00b | [Existing documentation](https://github.com/tangency-labs/solidcraft/tree/codex/drawings-w00b-checkpoint-20261029) | #6163 | Exact `514ad79e`; validation route remains pending. |

## Unintegrated original sources in this branch

- [W10](W10/README.md): circular conversion helper, original tests, grid and Seg2 adapter proposals. Actual owner seam and integration remain pending.
- [W21](W21/README.md): five original SVG/PNG acceptance proposals and alternative patches against the identified owner source. Test-file handoff remains pending.
- [X4 / #5321](X4/README.md): full encoder, decoder, exact length quantizer and test-only bridge. Separate from the narrower adapter already in D8; do not overlay alternatives blindly.
- [W38a corrective patch](W38a/README.md): five-file corrective WIP against the exact published head.
- [W38d acceptance cases](W38d/README.md): original source-bound identity fixtures, not an implemented saved-fill pipeline.

These folders deliberately remain outside production module registration. Each README binds the actual source/base and distinguishes full-file copies, incremental patches and alternative implementations. Do not apply every alternative together. No compilation or runtime was performed for this preservation branch. Repository licensing, notices, original authorship and contributor scopes remain in force. Private operational launchers, grants, receipts, credentials, caches and binaries are excluded.
