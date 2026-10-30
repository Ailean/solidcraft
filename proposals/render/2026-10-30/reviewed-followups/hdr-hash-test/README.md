# HDR digest test representation correction

SOURCE REVIEWED; APPLIED TO THE C18 DEVELOPMENT SOURCE. Current execution results are tracked separately in [#6201](https://github.com/tangency-labs/solidcraft/issues/6201). This archive contains a test-only patch, not a passing test or release.

The existing native CPU test compared status source_sha256 (a serialized 32-byte array) directly with request expected_sha256 (the canonical lowercase 64-character HdrId string). The correction normalizes those distinct contracts independently into exactly 32 bytes and checks both against SHA256 of the original immutable HDR fixture bytes. No production serializer, rendering behavior, input fixture, pixel threshold, timeout or later assertion changes.

Missing/null/wrong-type fields, wrong array length, invalid byte values and noncanonical hex remain hard failures. The original C17 assertion failure remains historical evidence; correcting its representation comparison does not establish that its later image/file-change assertions pass.

Apply fix.patch only to the exact bound C17 post-format UI source. The same candidate has subsequently been adopted in C18 source; do not apply it twice. bindings.json records both source snapshots and RAW endpoints. Existing test count is unchanged. Exact-source compilation and test results belong to separately bound runs; this archive does not establish a passing gate, full repository CI or separate native GUI/GPU acceptance.
