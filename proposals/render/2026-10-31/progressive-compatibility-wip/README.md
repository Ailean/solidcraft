# Progressive CPU: preserved compatibility WIP and isolated fixture fix

31 October 2026. Source preservation for #6201; **local curation, not a production
source change or completed integration**. Four WIP subpackages retain 34 RAW
candidates, and a fifth package preserves the exact earlier compiled C28 host
reference: 35 source files total, with per-file RAW bindings and LF patches.
They are deliberately separate; this archive supplies no combined source.

## Actual earlier C28 result, separate from these proposals

The C28 source build succeeded with five library artifacts and ten inventories.
Of 983 selected cases, **28 passed, one failed and 954 were not run**; two actual
CPU-supported branch markers were observed before the stop. The failed case 976
panicked because its separate UI test context had no ui-semibold font binding.
It did not reach a failing SPP/cap assertion. This is the earlier exact C28 result,
not a pass for the changed source packages below.

The isolated one-line font fixture correction is preserved in fontfix and remains
UNRUN. It uses the actual bundled-font bootstrap already used by app and Harness,
with every original assertion intact. It is not a change to product typography.

## Independent packages and required composition

- [core](core/README.md): 26 source candidates for one serializable runtime cadence
  authority, strict command handling, sparse legacy producer/host behavior and
  eight written tests. Requires ui and prefs.
- [ui](ui/README.md): five source candidates for captured preference identity and
  copy-only Interactive sampling, plus six neutral constructors and five tests.
- [prefs](prefs/README.md): two candidates, including the locale v2 successor; two
  independent true-default bools and one preference-contract test.
- [fontfix](fontfix/README.md): one isolated existing-test fixture correction.
- [current-c28-host](current-c28-host/README.md): the exact formatted host from the
  earlier C28 build, separate from the 34 uncompiled WIP candidates. Its recovery
  preimage differs by exactly one LF; this file is not a complete build snapshot.

All compatibility proposals are **NOT ADOPTED, UNCOMPILED and UNRUN**. Their 14
new tests and the corrected existing test are not results. Exact later composition
must preserve both overlapping render_workspace_tests.rs deltas; neither full
candidate may replace the other. No hypothetical precedence is implied.

The source has independent static Kernel and UI reviews. No demonstrated P1/P2
functional defect was reported in those bounded reviews. One known P3 remains:
the earlier typed-job controls list at core/candidate/crates/engine/src/cmd/render.rs
line 127 omits cpu_progressive_every_pass although later documentation and the
parser admit it. The source bytes are preserved without silently fixing it.

Actual Record layout/ABI and sufficient fixed admission accounting are pending.
An unchanged size assertion is not a measured layout or runtime pass. Resource
budgets must not be raised, and no allocation hidden, to satisfy these checks.

No native application acceptance, full CI, renderer parity, first-launch policy
qualification, readiness or merge is claimed. BIN/native validation for the
earlier source remains a separate activity and is not blocked or replaced here.

## How to review

Each candidate is byte-identical to its closed owner packet; no formatter or
production edit was run by the curator. source-bindings.json in each directory
pins every RAW preimage and candidate. delta-LF-review.patch normalizes line endings
for reading; candidate bytes remain authoritative for mixed-EOL files. Do not apply
the archive to arbitrary main or qualify it using historical test results.

Only public-safe source, portable bindings, review patches and these notes are
included. Private ledgers, scripts, grants, control receipts, logs, binaries and
local paths are excluded.
