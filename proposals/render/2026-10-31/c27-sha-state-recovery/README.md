# C27 recovery proposal: one active PNG/EXR SHA state

31 October 2026. **Source reviewed and adopted locally; not yet formatted,
compiled, tested, installed, ready or merged. Root and independent Kernel
source reviews are closed; ABI fit and execution remain unverified.**

The actual C27 combined build stopped with E0308 at its existing fixed metadata
assertion before any test body. This candidate is a narrow proposed repair,
**not evidence that E0308 is resolved or that the target ABI size is known**.

## Source change

The existing work scans PNG bytes before EXR bytes. The candidate retains one
active `Sha256` state, finalises the PNG output into
`Option<sha2::digest::Output<Sha256>>` (a 32-byte SHA256 digest, not an ABI-size
claim for the Option or surrounding structures), then uses a fresh active state
for EXR. It preserves the original chunk size, bounded advance count, byte order,
cancel/publication guards and payload/lease ownership. It adds no hidden heap
allocation and does not raise the 1,024-byte metadata allowance or either
existing size assertion. Those assertions remain compilation oracles.

One original regression is written in the existing host module:
`sequential_png_and_exr_hashing_preserves_independent_payload_digests`.
It derives expected PNG and EXR hashes independently over distinct whole buffers
larger than one advance; units 1 and 3 exercise transitions between and within
calls. It also covers absent/empty EXR, zero-unit and repeated completed steps.
The test is **UNRUN**. Existing digest, cancellation, late-publication and lease
regressions remain unchanged and need actual-source execution.

The proposed next selection is **983 cases, all UNRUN**: C27's 982 unexecuted
cases plus this one new regression. This is a planned selection, not an actual
inventory or a result from another run. No new compiler, runtime, native image,
physical GPU, full-CI or renderer-parity qualification follows from this patch.

## Receiver and continuation

Apply only after the [exact C27 failed-build 25-file composition](../cpu-every-pass-c27-source/README.md),
whose immediate host bytes hash to
`f775f4e458947b2c402fe97f10b320f7c7f4a9060cf502b5a69975e006eec71e`.
The candidate host hash is
`2c72c307ec49a94b100f4be32ab6a4af4be494199ff7ad5c87bb698e7f13ab4e`.
This single-file successor does not replace the other 24 source files and must
not be applied to arbitrary main or stacked blindly on the older nine-source
high-pass/UI snapshot.

`candidate/` contains exact RAW source, `source-bindings.json` is portable and
`delta-LF-review.patch` contains the full readable change, including removals.
Continue with one fresh serial cached validation and actual artifact/inventory
bindings before tests; source review and local adoption are already complete. Native
per-pass display, final/export quality and the default-change/preference process
remain separate unresolved obligations. Archive preservation is not production
readiness and changes neither production PR #8938 nor the installed app.
