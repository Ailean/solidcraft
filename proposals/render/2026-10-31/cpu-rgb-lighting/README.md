# CPU directional lighting: RGB transmission prerequisite

Source-only two-file proposal. Adds an internal sibling of the existing boolean visibility shading method, accepting completed per-light RGB transmission. Finite channels in[0,1] are required; invalid input and callback errors refuse without clamp/fallback. Light eligibility, index/order, directions, BRDF, intensity/cosine and accumulated-radiance validation are preserved. Original incident contribution is multiplied by RGB transmission exactly once. No exposure or tone mapping is added.

Before/candidate files and exact delta are bound in bindings.json. The complete previous lighting file is unchanged except for the inserted method; all8 prior tests remain byte-identical as a prefix, plus7 new tests. All15 are UNRUN. Exact source comparison/replay is not compilation, typecheck, lint, numerical execution, runtime or native acceptance. The internal method has no productive caller yet and may cause dead_code warnings; no suppressions added.

Independent Lambert constants and binary-preservation cases accompany invalid-channel/error/eligibility/index tests. The radiance-bound test intentionally constructs a private hostile light intensity outside public admission; it does not change or claim public acceptance of that intensity.

No ImageTask, colored-shadow feature flag, layout/resource accounting, command/UI/saved format, dependency or asset is included. This alone does not implement functioning colored shadows and is not a release or launch grant. Re-pin the actual receiver before composing; retain unrelated later changes. OWNER and runtime consumer require separate integration and validation; earlier runtime results do not qualify this source.

Original contribution under repository MIT OR Apache-2.0 terms. No third-party code or assets added.
