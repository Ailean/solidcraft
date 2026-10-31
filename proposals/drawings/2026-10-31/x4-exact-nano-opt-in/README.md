# X4 exact-nanometre lengths: explicit opt-in DXF writer proposal

This archive preserves a source-reviewed, two-path proposal on literal D8 receiver commit `d8e7a8690a30e44a58255e5dd345881ef092a93a` from `0xlillith/solidcraft` PR #8515. It is not a patch against current main, an adopted default, or a tested integration. Existing source notices and authorship are retained verbatim; curation does not transfer authorship or introduce external assets.

The existing free-function writers still choose the Legacy policy: their existing twelve-decimal formatting remains, together with the existing precise bulge/vertex exceptions. The 53 original flat/view consumer byte pins are outside these two changed paths and remain unchanged and UNRUN; their fixtures and runtime results are not reproduced here.

The two new inherent methods explicitly opt in on `DxfFile<DxfShape>`:

- `write_dxf_exact_nano_lengths()`
- `write_dxf_exact_nano_lengths_with_text_bounds(provider)`

D8's `crates/io/src/lib.rs` publicly reexports both receiver types despite its private `dxf_out` module. The parent `dxf_out.rs` reexports the public `file_model::DxfFile` and defines public `DxfShape`; `file_model.rs` declares `pub struct DxfFile<G>`. The candidate therefore attaches the methods to the source-exported `solidcraft_io::DxfFile<solidcraft_io::DxfShape>` without a lib.rs edit. This is source visibility evidence, not a compiled API claim. The exact Git blobs and RAW SHA256 pins are recorded in `source-bindings.json`.

The owner's [resolved tick policy](https://github.com/tangency-labs/solidcraft/issues/5321#issuecomment-6503679437) concerns the exact value stored in binary64, rounded to nanometre ticks (1e-9 mm) with ties away from zero. The already archived quantizer is copied RAW unchanged. Its original proposal-stage header is preserved; this two-path candidate supplies the explicit writer hook. Length fields are selected at their actual call sites, not inferred from DXF group numbers. Dimensionless angles, normals, ratios, knots and weights keep the existing twelve-decimal path; bulges keep the existing round-trip spelling.

The opt-in preflight builds a bounded quantized shadow, validates positive/distinct geometry and derived bounds, and emits lengths/header through the same policy. The text-bounds provider receives the emitted shadow on this opt-in route. The source geometry is not changed. Nonfinite or out-of-range lengths (outside inclusive +/-9000000 mm), collapsed geometry, allocation failures and byte-budget failures return errors through the existing failure channel. Work, output and handle caps remain in place; this is source reasoning, not a measured memory guarantee.

The original twelve-decimal admission still runs before the added nano checks. The opt-in is consequently conservative: it does not promise to accept every geometry that would be valid on a nano grid alone. Bounds remain sampled/control-polygon and f64-derived approximations, not new analytic or certified conservative extents. `mapped-callsite-inventory.json` records the length/dimensionless boundaries and limitations.

The 47 original inline cases, their full bodies/assertions and the existing test prefix are preserved. Ten writer cases are inserted before the module's closing brace, so the complete final test module is larger and is not RAW-identical to the original complete module. The helper's five cases are copied unchanged. All 10 writer cases, five helper cases and 53 external consumer pins are UNRUN. No cases, including the 47 retained inline cases, were executed for this packet. No Rust parser, compiler, formatter, targeted tests, native/wasm acceptance, full CI or output-byte execution is claimed. Static source review is not PASS, readiness or main compatibility.

`candidate/` contains the exact two after files. `two-path-alternative.patch` is the sole combined delta from literal D8, including the `/dev/null` addition of `length_decimal.rs`. It replays forward to these exact candidate bytes and reverses to D8 plus helper absence. Apply the combined patch once, OR use the candidate files / earlier split deltas; these are alternatives and must not be applied sequentially or twice. Check the exact before hashes and absent helper before using it. Other D8 prerequisites are not bundled; reconcile and rebind against actual main separately, without assuming an X1 overlay.

Owner coordination about this explicit opt-in proposal, any integration/default migration, an exact receiver rebind, and the applicable execution gates remain pending. This archive does not change a shared checkout or open a competing PR.

Portable source/review pins: source packet manifest `1735ecf69800212dc89e17170893143723f129fb1859a33f5ea33898cbb27135`; namespace supplement `bb641e5949cc371316b3db2d35b15655bffd7586831aa33f06488d503db5cef8`; independent source review `20a0682acb71807ee666d2ed59c596a8363aa263b9024429154d191f8e450d08`; namespace review `0cb7dc0c1863663e777ffb1383729ee9ea86d26ac36b8335978740131b7b4b54`. These SHA256 values identify artifacts, not Git commits or runtime results.
