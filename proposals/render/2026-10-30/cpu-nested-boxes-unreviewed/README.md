> Preserved unreviewed WIP at Judit's request. NOT APPLIED, NOT COMPILED, NOT RUN. Rebind against the actual receiver; do not overwrite the newer Area/LED tests. This is not an accepted fix or production registration.

# CPU nested certified boxes — unreviewed V1

Status: NOT COMPILED, NOT FORMATTED, NOT RUN. Independent Kernel/source review pending. Not merge-ready or Interactive parity evidence.
Engineer decision: reuse existing JobOptions.nested_media only. No RaytraceOptions flag, saved format, GPU code, third-party code, assets, kernel or SolidCore changes.

## Exact inputs and scope

Receiver: the privately pinned source candidate; current shared source has subsequently received the Area/LED increment.
The private manifest pins raw before/candidate SHA256 for six files, with exact base captures and original CRLF/LF conventions. Shared source was not edited.
render-proposal.patch changes five render files; engine-forwarding-proposal.patch is a separate dependent engine consumer proposal. Both must be reviewed together for command-level support.
verify_packet.py only generates/consumes text diffs and hashes. It does not import/run Rust or product code.

## Three actual routes traced before implementation

1. Final CPU direct: ImageHost.submit_with_options -> Record.options -> WorkTask::new. Existing code retained options in Record but dropped them at WorkTask::new (host.rs698). Proposal forwards JobOptions to ImageTask::new_with_environment_and_nested. Existing public constructors still pass false.
2. Final CPU command: jobs.rs Start had deny_unknown_fields without nested_media; Prepared always returned default options (line701). M2 adds the same existing runtime nested_media input, uses host.admit(scene,request,options) for truthful preflight, and preserves options in Prepared/existing submission. No new flag in RaytraceOptions or saved schema.
3. CPU progressive/Interactive: RaytraceOptions.validate mod.rs895 refuses all dielectrics with progressive, and cpu_request sets progressive. These restrictions remain byte-unchanged. Interactive Snapshot already captures params Value; its Prepared already holds JobOptions and Snapshot::run forwards them at render_interactive.rs228. M2 preserves an accepted existing nested_media value through Start/prepare; no extra snapshot envelope is needed. However Interactive dielectric requests still fail the explicit progressive contract. UI CPU runtime binding policy also currently rejects base.dielectrics. Therefore this packet enables only bounded FINAL CPU nesting, not progressive/Interactive glass. It does not quietly strip optics or fall back.

## Production contract

Every box still passes the existing exact12triangle outward axis-aligned certificate before parent selection. parents() only consumes that certified set. Strict containment is sufficient for this box-only subset, not arbitrary mesh bounds.
At most8 overrides bound hierarchy work to64 comparisons, no allocations. Parent table is fixed [Option<usize>;8] owned by existing OpticalTask. Existing sizeof OpticalTask admission term automatically includes its storage; no field is added to ImageTask's fixed host slot.
For each box, select the smallest strictly containing parent independently of override/body ordering. Partial overlap, equality and touching face/edge/corner refuse. Sheet-volume exclusions, edge/corner ambiguity, clearcoat/other incompatibilities and camera/near-plane-inside refusals remain in place.
A branch stores its existing current medium identity. A fixed immutable parent table is enough for certified strict containment: entry must match the selected box's parent, transmission enters box; matching exit restores parent. Reflection retains current medium. No new mutable stack or global framework is needed.
IORs at each boundary are actual current/parent materials. Existing Fresnel/Snell/radiance eta-squared and current-medium geometric Beer segment evaluation are retained.
Existing MAX_BOUNDARIES8/MAX_PATHS511, cumulative WorkBudget, cancellation and bounded preview tail remain unchanged. This is not arbitrary-depth/lossless exact transport: unresolved boundary continuation retains the existing black-tail policy. Near-coincident representable gaps/spawn exclusion remain a numerical review/qualification obligation; strict f64 inequality alone is not runtime evidence.

Host exception: mask ONLY nested_media to false then compare every other JobOptions field to exact default. Raster+nesting, GPU source transmission/resources, and unsupported CPU option combinations still refuse. No blanket JobOptions bypass.

## Eight appended original tests, all UNRUN

dielectric_tests.rs: original26-test byte prefix23697 preserved, now31 (+5).
- Strict parent order/immediate choice with three certified-box records; nesting remains opt-in.
- Equal, partial and face/edge/corner-touching boxes refuse.
- Actual final PNG uses outer2mm+inner2mm and original Beer oracle (exp(-.8),exp(-1.2),1), independently mapped Reinhard+sRGB -> [151,132,188,255], both body orders.
- Progressive/Interactive dielectric refusal remains.
- Native host preflight rejects defaults/other nondefault controls and raster nesting; actual submit_with_options must complete the same expected image and retain nested flag.

image_tests.rs: original36-test byte prefix65020 preserved, now39 (+3).
- First transmitted path observes four actual boundaries and parent restoration: outerentry .96/2.25; innerentry factor27/49; innerexit factor256/147; finalexit2.16. RGB comparisons1e-12. These factors derive from air1/outer1.5/inner2 normal-incidence Fresnel and radiance eta², not sampled output.
- Actual nested continuation is held to one work unit, then separately cancellation or total WorkLimit must prevent completion.
- Parent allocation remains billed; malformed box and inside camera still refuse.

Existing assertions/numbers are untouched. No test outcome or compile success is claimed.
M2 currently has source-route review, not a new engine integration test; command snapshot regression coverage is an explicit follow-up for integration, never inferred from the direct native-host test.

## Handoff

Main patch SHA256 152d0b043f1998bc18235a868ef5fc6aec82ce2c6c4ef47c3a091e05a46d4bb6.
M2 SHA256 9e5f9fa3a422caa85035aa39088ef68497cc57f0146185b4b0426bc345caab02.
Next: Engineer/Kernel independently review hierarchy, branch identity/IOR/Beer, finite-depth policy, real API compatibility and oracles; coordinate M2 jobs.rs ownership/engine coverage. Only Performance may validate exact admitted source with a fresh grant. No shared application/publication occurred.
