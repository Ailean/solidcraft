# Case 976 isolated font-fixture correction

One RAW candidate adds only crate::fonts::setup_bundled_fonts(&ctx) after the
new test's separate Context::default(), before it paints the existing UI kit.
Removing that line recovers the entire captured before file byte for byte.
All assertions, pass targets, seed and mode values are unchanged.

The sealed C28 failure was an unbound ui-semibold font in this separate test
context. Actual app and Harness already register the bundled fonts. This is not
a demonstrated product cap/transport defect, nor an SPP assertion result.
The correction itself is **UNRUN**; its target test must be executed again on an
authorized exact composition. No product font default or new framework changes.

**Do not overlay this full candidate onto ui.** Both test-file candidates start
from the same before 28ddafe4. This file lacks ui's five new tests and four neutral
constructor insertions. Preserve the one-line correction during later exact
delta composition; there is no precomposed source in this archive.
