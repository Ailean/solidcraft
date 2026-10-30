# U17 source test-module label correction

The earlier source-only tests-NOT-RUN wrapper incorrectly named the new test
under `view::tests`. Its actual source registration is
`view::glass_tests::explicit_product_dark_channels_follow_one_srgb_transfer_in_both_targets`.
The current wrapper corrects that label only; the earlier commit remains in history.

No Rust/WGSL/test body, oracle, selection, controller, grant or source manifest
changes. The C20 caller already excludes the correct complete real-GPU case.
This is not a compiler inventory or execution result: the pixel test and its
96 proposed renders remain UNRUN and require separate real GPU acceptance.
