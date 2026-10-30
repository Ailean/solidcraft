# Original acceptance declarations — NOT RUN

All 45 declarations are unrun on this changed candidate. Names below are source inventories, not test results. The 33 old test bodies are preserved byte-for-byte as the complete prefix of candidate dielectric_tests.rs; the 12 new declarations append to that existing file. No existing expectation/tolerance was modified.

## Twelve added cases

1. `direct_box_beer_uses_original_endpoints_and_multiplies_disjoint_rgb` — One/two disjoint index-matched 1 mm slabs: sigma=(ln2,ln4,0), expected RGB (1/2,1/4,1) and (1/4,1/16,1).
2. `direct_sheet_shared_diagonal_is_one_crossing_and_uses_virtual_beer_distance` — One two-triangle thin sheet/shared diagonal: one crossing and virtual-thickness Beer (1/2,1/4,1).
3. `direct_exterior_box_fresnel_is_not_a_refracted_or_infinite_bounce_slab` — Independent n=1.5 normal-incidence oracles: R=1/25; solid unbent two-interface transmission 576/625, incoherent sheet 12/13; no reflected-return claim for the solid.
4. `direct_connection_keeps_unselected_opaque_blockers_and_half_open_light_endpoint` — Unselected opaque blockers remain black; finite light-endpoint geometry and geometry beyond it are excluded by [min,max).
5. `direct_connection_refuses_coincident_different_bodies_instead_of_skipping_opaque` — Equal-depth selected and opaque different-body identities refuse; no success fallback.
6. `direct_connection_bounds_validate_identity_endpoints_overlap_and_crossing_budget` — Invalid 0/17 crossing limits, box face/interior endpoints/origin, name mismatch and overlapping selected boxes refuse; insufficient crossing budget gives WorkLimit and terminal cursor.
7. `direct_connection_spends_caller_work_and_cancellation_is_terminal_without_result` — Each step spends the same caller ledger; exhausted budget returns WorkLimit; cancellation precedes a new debit; errors make the cursor terminal.
8. `direct_connection_rejects_hostile_optics_before_any_dark_result` — NaN/infinite/negative/out-of-range absorption and NaN IOR are rejected before a result.
9. `direct_connection_accepts_all_eight_disjoint_boxes_and_preserves_reverse_rgb` — Eight disjoint boxes/sixteen boundaries in both directions: (1/256,1/65536,1); ninth selected override refuses.
10. `direct_connection_box_edge_is_refused_without_a_successful_fallback` — Certified box edge/corner ambiguity refuses without a successful fallback.
11. `direct_connection_setup_certification_cannot_escape_caller_work_limit` — Constructor certification spends its caller budget; one-unit budget cannot bypass setup work.
12. `direct_oblique_sheet_is_one_certified_plane_with_original_beer_angle` — Oblique sheet z=x/2: original index-matched virtual optical distance sqrt(5)/2 mm and Beer (2^(-sqrt(5)/2),4^(-sqrt(5)/2),1).

The channel comparison tolerance is the original authored fixed 1e-13, not fitted to execution. The cases derive their values from elementary Fresnel/Beer geometry; they do not use commercial reference images or external renderer output.

## Thirty-three preserved declarations

- `normal_interface_has_four_percent_reflection`
- `thirty_degree_snell_and_parallel_exit_are_reversible`
- `internal_grazing_ray_is_totally_reflected`
- `beer_lambert_uses_actual_millimetre_path`
- `hostile_optics_are_refused`
- `closed_box_certification_requires_complete_outward_boundary`
- `captured_occurrence_identity_and_unsupported_boundary_are_refused`
- `actual_png_closed_volume_absorption_uses_two_and_eight_mm`
- `actual_png_clear_volume_includes_multiple_fresnel_branches`
- `overlapping_volumes_are_refused_before_pixel_work`
- `cancelled_dielectric_job_cannot_publish_png`
- `runtime_dielectric_inputs_are_bounded_and_default_off`
- `oblique_parallel_slab_offset_scales_with_actual_thickness`
- `thin_sheet_is_certified_separately_and_duplicate_faces_refuse`
- `native_dielectric_job_uses_existing_host_and_retained_png_lease`
- `secondary_environment_is_explicit_and_primary_background_remains_display_srgb`
- `typed_runtime_roundtrip_preserves_default_empty_and_explicit_lists`
- `actual_oblique_png_selects_the_snell_shifted_colour_target`
- `thin_sheet_brewster_sums_polarizations_before_averaging`
- `thin_sheet_normal_incidence_keeps_the_lossless_slab_limit`
- `thin_sheet_matched_indices_keep_beer_lambert_transmission`
- `thin_sheet_zero_thickness_index_matched_grazing_is_transparent`
- `thin_sheet_positive_thickness_index_matched_grazing_refuses_without_tir`
- `thin_sheet_zero_thickness_grazing_still_validates_absorption`
- `thin_sheet_absorbing_brewster_keeps_separate_internal_returns`
- `actual_png_brewster_thin_sheet_matches_reflected_energy`
- `strict_box_parent_selection_is_order_independent_and_opt_in`
- `nesting_opt_in_still_refuses_touching_equal_and_partially_overlapping_boxes`
- `actual_nested_png_uses_parent_only_outside_the_inner_interval`
- `progressive_optics_keep_the_nested_certificate_opt_in_and_interactive_intent`
- `native_host_forwards_only_the_existing_nested_media_exception`
- `exact_face_excludes_own_zero_depth_and_retains_one_ulp_neighbour_on_every_axis_and_side`
- `exact_face_recovers_certified_plane_without_clamping_or_blessing_ties`
