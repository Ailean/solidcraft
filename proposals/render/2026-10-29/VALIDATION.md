# Validation boundaries

The published feature PR remains #8938 at `a077906234972dbbf11e80c54893f85ece97c8b7`. This checkpoint is source preservation, not ready/merged delivery.

- B1V3: all 516 nonignored render-library tests passed once in separate exact-name processes; all ten Interactive tests included. Strict render Clippy all-targets and default-feature render wasm compile check passed. Actual native executable SHA-256: `3a0a69b35f14326f9e954c3eec582347cbb72572109b46d6b14acb6b87e78f86`.
- Those results apply to the exact core source recorded below. They do not qualify the engine/UI adapter, later tests or pending parity proposals.
- C2 combined engine/UI build was cancelled by the resource guard: available memory 2139308032 bytes crossed the reserved 2147483648-byte floor. No build-finished event, no executable artifact, no compiler error diagnostic, no binary listing and no test body. This is neither compiler PASS nor demonstrated product failure. Only owned compiler work was stopped; the user's SolidCraft was preserved.
- Current snapshot additionally contains the independently reviewed error-recovery/dimensions addendum and comment cleanup after C2. It has not been compiled or executed. UI: 19 original Interactive cases written (7 initial +4 real-worker acceptance +8 recovery/dimensions); unsupported-host refusal branches are not CPU-runtime passes.
- Previous broad repository CI is incomplete after GPU-safety cancellation. Its partial results cannot be transferred to this snapshot. No global CI/GPU/native-window acceptance claim.
- GPU owner recommends both #6813 and #6850 land first, then fresh source/resource admission and a single bounded serial device test. Discrete-GPU historical measurements are not Pegasus iGPU qualification.
- Further combined compilation needs fresh admission; observed C2 global memory fall was about 3.421 GiB, not a measured compiler-only peak. Sole builder proposes 8 GiB available, preferably 10 GiB, while keeping the 2-GiB reserve. No flags/profile/cache clean/copy workaround or automatic retry.

Core RAW source hashes preserved in this snapshot:
- `crates/render/src/job/host.rs`: `7d2e248f8c7d6586f18d8b9840da9074514ed4672bc8a8845170a805b1bf9315`
- `crates/render/src/job/host_tests.rs`: `0febf5af52d7aa24f3b0586e230752a17b6c0e30b33d79810c8b7338a34c941a`
- `crates/render/src/job/interactive.rs`: `ccc8aaa5fc76038b39e8829f87b0888ccd92816567ea8b3d83778201364a4cce`
