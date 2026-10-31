# CPU glass editor to directional transmitted-shadow request

Reviewed source checkpoint for #6201, now included in the seven-path C23 candidate. It is unformatted, uncompiled and unrun; it is not an installed-app fix, a passing GPU/native/full-CI result, or general feature parity. The exact receiver was C22 post-format source manifest9b193a92875783101d2511342eb4e4781abeb2f149187179f8900931fbd06e3f. The full adopted C23 source manifest is55bcda795f1c124465330f9156ad6894501ac1e184203fe3e2994d1052eee45b (3,582members); production PR #8938 still has its original head.

## User-visible request behavior

For active, current whole-body glass bindings with explicit CPU permission, uniform transmitted-shadow choices now reach the existing directional RGB producer: all ON derives directional_shadow_transmission=true in the submitted wire; all OFF preserves false/omission. The authored editor-global default remains false. The current producer has one global switch, so mixed ON/OFF or OFF/ON refuses explicitly instead of dropping an OFF assignment. An incoming conflicting global true also refuses. Direct manual authoring of that global View field remains subject to the earlier CPU composition validation; this bridge does not promise a new global-authoring route.

Global shadows, environment/nesting restrictions, all currentness/duplicate/stable-identity/visibility/permission checks, ThinSheet/ClosedBox boundaries and real producer certification/admission/resource/work/cancellation gates remain in force. Unsupported coat/maps/subsurface remain refused. The UI commits only a completely validated request; no failed composition mutates the authored design or request payload. No CPU permission, boundary, backend, preset, depth or cap is silently changed.

The admitted effect is straight directional RGB attenuation through the currently certified, finite supported geometry, not bent refracted shadows, caustics, arbitrary closed meshes, environment transport or per-body shadow-mask parity. Unselected geometry still blocks according to the producer. New localized help states this partial boundary.

## Original acceptance and remaining validation

The existing277 test bodies in render_workspace_tests.rs are byte-preserved, with seven new original cases. One explicitly authorized old expectation in the existing CPU native-style harness is changed because it asserted that CPU transmitted shadows were always unsupported: the SAME fixture, IOR1, policy and controls now require the correct typed/wire flag and real render.preflight success. Its prior function is preserved privately, and all other bytes/refusals remain unchanged. This is a deliberate new-feature boundary, not adapting an image oracle or weakening a threshold.

New cases cover two-body uniform selections across fixed/progressive and transport policies, both mixed orders, real incompatible environment/global/nesting/light combinations, stale/duplicate/unsupported-boundary/no-permission guards, inactive bindings, default/conflicting wire behavior, atomic refusal and the three new messages in nine locales. All seven cases and the changed existing acceptance remain UNRUN. Source inspection and RAW replay found no actionable blocker; compilation, actual inventories/tests, formatter/lists/localization checks and native mouse/keyboard before-and-after images are still required.

The next serial development run combines this unit with the reviewed quality-button and minimal TIR test-call changes, uses one normal full-five build with cache retained, then qualifies its own artifacts/inventories. No complete issue row is closed. Default persisted formats, dependencies, command APIs, Nateaux's layout/results/gizmos and physical GPU acceptance are outside this source change.
