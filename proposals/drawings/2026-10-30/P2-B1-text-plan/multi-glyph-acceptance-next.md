# Bounded successor: positive multi-glyph cluster acceptance

**Proposal only; no implementation, runtime, PDF or extraction result.** The current text plan correctly refuses this unsupported seam. Cam keeps ownership of parent content, global refs, `collect_layers` and many-page integration.

## Actual missing case

[B1a's real `Cluster`](https://github.com/tangency-labs/solidcraft/blob/8da9e9893d7dcf2c4b3584f6334b91352dc31dcb/crates/geom/src/paper/text.rs) associates a glyph range with a UTF-8 text-byte range. It explicitly represents both multiple characters in one glyph and multiple glyphs for a character with marks. [B1b's RTL/marks test](https://github.com/tangency-labs/solidcraft/blob/df3eff1ce9768c1d2cab3ed7cdb91471dd30e95f/crates/sketch/src/paper_text_tests.rs) checks mixed-direction runs, byte coverage and fewer clusters than characters. It does **not** assert that a particular cluster contains more than one glyph, and it does not produce or extract a PDF. The preserved P2 resources and new text-plan cases likewise have no successful multi-glyph page-content acceptance.

An original candidate fixture is `M \u{05E9}\u{05C1}\u{05B8}\u{05DD} 27` (Latin, Hebrew shin with two points, final mem, spaces and digits), through the actual bundled B1 resolver. Use a nonzero sheet origin, a quarter turn and its supported italic/slant style. Do not hand-invent glyph IDs or claim that this unrun string already produces the desired cluster.

First require that the actual returned scene passes `Scene::validate()`, then record a real returned cluster whose glyph range has length greater than one and whose exact logical byte slice includes the marked letter. Keep actual original GIDs, per-glyph x/rise/advance, intrinsic widths, run direction and logical slices. The candidate deliberately puts U+05C1 before U+05B8; compare that exact code-point sequence without Unicode normalization. If this candidate does not reach that seam, preserve the failed precondition and choose a justified fixture; do not skip or weaken the precondition.

## Success contract after an agreed content seam exists

1. All glyphs in that cluster retain the resolver's positions and are drawn exactly once. A mark's placement must survive origin, shear, rotation and the single sheet-to-point transform. A resource-only object chunk is insufficient evidence.
2. The exact logical cluster text replaces that cluster's combined visual content **once**, without repeating its whole string on every glyph, dropping marks, normalizing Unicode or absorbing the neighboring space/digit clusters.
3. A real independent PDF text consumer must extract the intended literal text once, with explicit checks for duplicate cluster strings and lost combining marks. Content/span structure and pixel/render evidence must independently guard over-wide replacement spans and missing/relocated visual glyphs. A raw metadata string or a single CMap assertion cannot establish this.
4. Per-cluster replacement is distinct from global reading order across mixed bidi runs. Qualify run ordering separately against the actual emitted document; this case alone does not prove all RTL extraction correct.
5. Preserve the current bounded refusal until the actual writer can enforce balanced marked-content scopes, exact glyph/text coverage, bounded encoded replacement text, local and aggregate output/work limits, and collision-free parent resources. The existing 512-byte ToUnicode destination policy must not be silently bypassed or changed by this proposal.

The current `CodeToGlyph` contract requires a nonempty logical string for each code. Assigning empty text to continuation marks is not an admitted shortcut; a new content policy must address this constraint explicitly.

One possible format route is a `Span` marked-content replacement, subject to actual writer support. The [PDF Association's specification errata, section 14.9.4](https://pdf-issues.pdfa.org/32000-2-2020/clause14.html#1494-replacement-text), describes ActualText on a Span marked-content sequence; its [tagged-PDF guide](https://pdfa.org/wp-content/uploads/2023/07/Tagged-PDF-Best-Practice-Guide.pdf) explains replacement text for otherwise unsuitable character content. These references establish a format option, **not** an implemented `pdf-writer` API, working integration, PDF/UA claim or reader result. No new production API or dependency is proposed here.

## Small compatible next unit

Prepare an original acceptance specification and source-bound fixture expectations for the actual B1 resolver, then give Cam an exact per-cluster coverage/position contract. Keep any prospective content adapter private and unregistered until the owner integrates it with the actual allocator and pages. Successful multi-glyph support requires changing the explicit refusal only together with the real content implementation and its executed acceptance, not by mapping the full cluster to each glyph.
