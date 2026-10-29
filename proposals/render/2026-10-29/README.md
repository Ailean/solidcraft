# Render work checkpoint — 29 October 2026

Source-preservation checkpoint requested by Judit. Parent `a077906234972dbbf11e80c54893f85ece97c8b7`; production baseline remains unchanged on this checkpoint branch. **Nothing here is a ready-to-merge or feature-complete claim.**

## Integrated local candidate

`source/` holds all 20 changed files as exact RAW snapshots. `interactive-working-changes.patch` is their reviewable delta against the parent, including the three new files. It contains the corrected CPU lifecycle, immutable engine capture/identity, strict Interactive actions and nine-language text, cached binding proof, UI/provisional pixels, lazy GPU setup policy and original tests. The current local shared checkout already contains these changes: do not apply the patch there again.

For a clean continuation, read current repository/area instructions and coordination first, preserve existing WIP, agree the one shared checkout/sole builder, verify the exact base/preimages, and apply only the intended delta. `source/` is a backup, not permission to overwrite another contributor's newer files. Check `MANIFEST.json` hashes. The source uses existing dependencies and the unchanged parent Cargo.lock/config.

Read [VALIDATION.md](VALIDATION.md) before citing results. The combined candidate is uncompiled/unrun; a memory cancellation is not a code failure or a pass. No installed application was replaced. All local source remains preserved too.

## Pending separate units

| Proposal | State / dependency |
|---|---|
| CPU area/LED progressive | Independent source review complete; original 7 tests plus 1 positive core case written, unrun. Existing 25-catalog CPU help must be updated through the locale workflow before readiness. Keep Hiro's budget/profile work separate. |
| Raster perspective view direction | Independent mathematical/source review complete; CamWight and cfaulkingham confirmed it can proceed as its own small PR on main. Preserve their tile/clear and pixel-work admission changes. Three original tests unrun. |
| GPU session diagnostics | Metadata-only proposal; no backend/driver/recovery override or GPU qualification. |
| GPU light contract/tests | Point/Spot and multiple directionals need actual transport. Preserve true delta directional vs finite Sun; payload/ABI/caps are proposals, not existing APIs. Owner question pending in #6281. |
| Shadow baseline | Test-only, five new cases preserving four originals. All unrun; screenshot cause unproved. |
| Retired shadow production patch | **DO NOT APPLY.** Concrete admitted counterexample found in independent review. |

Apply only the chosen proposal once; alternatives are not cumulative feature support. No skipped tests, hidden fallback, relaxed assertion or blessed golden establishes completion.

## Public coordination

- [Current parent work plan and acceptance checklist](https://github.com/tangency-labs/solidcraft/issues/6201)
- [Existing draft core PR](https://github.com/tangency-labs/solidcraft/pull/8938)
- [Nathan-Dane: Interactive domain available](https://github.com/tangency-labs/solidcraft/pull/8000#issuecomment-6503761456)
- [Concrete adapter placement/ownership note](https://github.com/tangency-labs/solidcraft/pull/8000#issuecomment-6507877408)
- [GPU compiler prerequisites and measured limits](https://github.com/tangency-labs/solidcraft/pull/6850#issuecomment-6507173501)
- [Raster owner coordination](https://github.com/tangency-labs/solidcraft/pull/6483#issuecomment-6508034498)
- [GPU light-type/lowering owner question](https://github.com/tangency-labs/solidcraft/issues/6281#issuecomment-6508177026)

Drawings/SolidCore and historical runtime grants remain stopped; this checkpoint revives none of them. Preserve author ownership and existing panels/results/gizmos. Final repository gates and native mouse-only acceptance remain required before claiming delivery.
