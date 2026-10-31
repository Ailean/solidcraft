# Exact earlier C28 formatted host, separate from compatibility WIP

This is the actual compiled C28 host source (RAW SHA-256
9dbf55511147641a8d7d7c3b4bf444ae8b8ece154aab49db4b57cb208347b4a8),
retained as the before file in the sealed compatibility core packet. It belongs
to the C28 d38a exact-source baseline. The formatter removed exactly one LF
from the prior recovery source 2c72c307; every other byte is unchanged.

Earlier C28 built five library artifacts and completed ten inventories. Selected
cases: **28 PASS / 1 font-fixture panic / 954 UNRUN**, with two actual
CPU-supported true markers before stopping. Postchecks were not executed; no
native application acceptance, full CI or readiness is claimed. This single file
is not the whole source binding for that earlier build.

The failure was case 976's unregistered ui-semibold test font, not a failing
SPP/cap assertion. Its before test-file SHA is 28ddafe4; that exact source is
already preserved in the [earlier source archive](../../cpu-every-pass-c27-source/candidate/crates/ui-egui/src/render_workspace_tests.rs).
The separate fontfix candidate 9a3e58d8 is UNRUN and must not inherit a pass here.

Do not confuse this file with core's compatibility host candidate 89f28961,
which remains NOT ADOPTED, UNCOMPILED and UNRUN. No compatibility code is applied
by this reference package.
