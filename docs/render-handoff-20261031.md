# SolidCraft Render — portable shutdown handoff, 31 October 2026

Work is stopped at Judit's shutdown request. Pegasus is not required to remain
online. The native application closed normally; no heavy process, reservation
or new runtime grant remains. Preserve incomplete results exactly; this is a
resumable WIP handoff, not a ready/merge or full-CI announcement.

## Start from the actual source

- [Exact current source commit `a09ecc2ec6178a05132ce911ec97a9ddcca18963`](https://github.com/tangency-labs/solidcraft/tree/a09ecc2ec6178a05132ce911ec97a9ddcca18963) on
  `codex/render-progressive-emergency-20261031`. Its real parent is
  `a077906234972dbbf11e80c54893f85ece97c8b7`; authored history is retained.
- Actual source tree: `d287e8326eb8cb8146fd460c18eae27c283d977d`.
  The checkpoint carries 117 tracked WIP modifications plus 17 public new files:
  **134 exact source/test/shader/locale/documentation files**. It was compared to
  the real base tree; it is not a proposal-file archive pretending to be source.
- Current full source binding:
  `d38a8d946b6653864ad8918d4da662725bd24ca01b2e5aa2d3b38b47b1a7e163`.
- The normal desktop executable was independently qualified locally, SHA-256
  `869b08ffadb758f01cbceeea06bbac1c01e6f77abd896172182c86a8703e60c2`.
  This identifies the tested executable; no executable is uploaded or full CI
  inferred. The installed release and production draft head are unchanged.

## Preserved proposals and application evidence

[Immutable proposal archive and continuation index](https://github.com/tangency-labs/solidcraft/blob/545191528d75bad98760f0ae537d499fb13cdf15/proposals/render/2026-10-30/README.md) at
`545191528d75bad98760f0ae537d499fb13cdf15` contains separate unadopted source packages and later
owner overlays. [Native partial-acceptance report](https://github.com/0xlillith/solidcraft/releases/download/material-finishes-demo-20261015/render-d38a-20261031-native-addendum.md) and the
[existing owned release](https://github.com/0xlillith/solidcraft/releases/tag/material-finishes-demo-20261015) preserve **13 assets**, all downloaded again
and verified byte for byte: eight JPEG screenshots with correct .jpg aliases,
the completed PNG, two editable native Sample Plate files, the report and hashes.
The two native-design snapshots are byte-identical (36,879 bytes), not proof that
all transient renderer controls persist or that cold reopen was tested.

[Earlier Drawings checkpoint index](https://github.com/tangency-labs/solidcraft/blob/codex/drawings-all-code-checkpoint-20261029/proposals/drawings/README.md) preserves the separate
Drawings work. Do not mix its partial alternatives into the Render candidate.

## What was actually observed

A built-in one-body Sample Plate was used through mouse and keyboard, without
MFC/control commands. Actual images—not just counters—were inspected.

- One job, 2:5:5, requested **3840×2160 / Workstation / 1024 passes**. Its
  provisional images were recorded at **1 → 2 → 3 → 4** complete passes. It was
  **canceled after recording passes 1–4**; neither exact cancellation at pass 4
  nor completion of all 1024 passes at 4K is claimed.
- A **682×511 / Compact / 48-pass** baseline completed (job 2:1:1); 37/48 was
  observed while refining. Interactive also showed 24/48; a longer target1024
  run showed pass53, and Pause held pass82. Resume restarted refinement according
  to its documented contract; that restart is not listed as a new regression.
- Exposure **0 → 0.2 → 2 EV** automatically invalidated/restarted and visibly
  brightened Interactive without pressing Render.
- Cancellation restored the earlier completed 682×511 image. Actual GUI PNG
  export retained that image rather than the canceled 4K provisional frame.
  Verified dimensions: 682×511; SHA-256
  `bf2a76186c7c445fd086bd9984dff154b8215e24451345545d180215bf80d6bf`.
- The editor accepted **1,048,576** passes without changing 682×511; launch was
  refused by the existing admission budget. No million-pass completion is
  claimed. Compact64, Studio1024 and pixel/sample/work limits remain binding.

These are useful partial native results. **Original 19-body G01 and glass G02
were NOT RUN.** Detailed transport/lighting quality, cold reopen, physical GPU,
CPU/Raster/GPU parity and complete issue acceptance remain open.

## Validation and unadopted work

[Exact build commands, artifact profile and original failed-test excerpt](render-validation-evidence-20261031.md)
are preserved alongside this handoff.

C28 built and qualified five native-library artifacts and ten inventories.
Its 983-case selection stopped at **28 PASS / 1 FAIL / 954 UNRUN**, with two
actual CPU-supported true markers. Case976 failed because its separate test
context did not register ui-semibold; it did not fail an SPP/cap assertion.
Strict affected Clippy, wasm, remaining postchecks and full CI remain incomplete.
Native images do not retrospectively pass that test. All **17 U-rows are open**.

The one-line font fixture correction is separately source-reviewed, **not adopted
and UNRUN**. Compatibility core/UI/preferences and their **14 new tests** remain
**NOT ADOPTED, UNCOMPILED and UNRUN**. Record fixed layout/ABI/admission accounting
and the P3 early parameter-list omission still require work. Source review and
archiving are not runtime evidence.

### Safe overlay order for the next tool

1. Start from actual commit a09ecc2e and read its current AGENTS, attribution,
   notice, contribution and area documentation. Freeze a fresh source binding.
2. Use the immutable archive's per-file RAW preimages and dependency notes.
   Keep the actual compiled source separate from all future overlays; do not
   apply every candidate directory or assume a newer folder is authoritative.
3. Compose the font correction as its **one-line delta** on the matching workspace
   tests. UI capture and font-fix full-file candidates share before28ddafe4;
   copying either whole candidate over the other loses work.
4. The preference UI uses the locale-only v2 successor. Capture needs the sealed
   canonical core field, six neutral constructors and consistent serialization.
   Later owner overlays require their own exact matching before/review; they are
   not adopted by this handoff. Resolve actual overlap before compiling.
5. Rebind the real composed source and host on the next machine. Use one serial
   builder, cached/incremental where available, original size/resource guards,
   focused old/new cases and the actual repository gate. No old grant or timer
   authorizes a retry. Do not raise budgets or weaken assertions to get a pass.
6. Run original G01/G02 and remaining native journeys on the resulting qualified
   desktop build. Continue #6201 in its existing thread and keep #8938 draft until
   its real gate and maintainer process are satisfied.

Engineer owns public coordination. Preserve Nateaux's UI work and other owners'
scopes. This handoff introduces no new claim, competing PR, runtime permission,
production merge, waiver or promise of perfect physical rendering.
