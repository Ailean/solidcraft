use super::*;

fn triangle(z: f64) -> Triangle {
    Triangle::new([Vec3::new(0., 0., z), Vec3::new(2., 0., z), Vec3::new(0., 2., z)]).unwrap()
}
fn ray(origin: Vec3, direction: Vec3) -> Ray {
    Ray::new(origin, direction, 0., 100.).unwrap()
}
fn build(triangles: Vec<Triangle>, limits: BuildLimits, chunk: usize) -> Bvh {
    let cancel = CancelToken::default();
    let mut builder = BvhBuilder::new(triangles, limits, &cancel).unwrap();
    while !builder.step(chunk, &cancel).unwrap().complete {}
    builder.finish().unwrap()
}
fn closest(bvh: &Bvh, ray: &Ray) -> Option<Hit> {
    bvh.closest_hit(ray, &mut WorkBudget::new(MAX_WORK).unwrap(), &CancelToken::default()).unwrap()
}
fn brute(triangles: &[Triangle], ray: &Ray) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (primitive_index, triangle) in triangles.iter().enumerate() {
        if let Some(geometry) = triangle.intersect(ray)
            && best.is_none_or(|old| {
                geometry.distance < old.geometry.distance || (geometry.distance == old.geometry.distance && primitive_index < old.primitive_index)
            })
        {
            best = Some(Hit { primitive_index, geometry });
        }
    }
    best
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-12 * expected.abs().max(f64::MIN_POSITIVE), "{actual} != {expected}");
}

#[test]
fn ray_admission_normalizes_without_overflow_or_tiny_vector_cutoff() {
    for scale in [f64::from_bits(1), 1e-200, 1., 1e200, f64::MAX] {
        let r = ray(Vec3::ZERO, Vec3::new(scale, 0., 0.));
        assert_eq!(r.direction(), Vec3::X);
        assert_eq!(r.at(2.).unwrap(), Vec3::new(2., 0., 0.));
    }
    for direction in [Vec3::ZERO, Vec3::new(f64::NAN, 0., 1.), Vec3::new(0., f64::INFINITY, 1.)] {
        assert!(Ray::new(Vec3::ZERO, direction, 0., 1.).is_err());
    }
    for (min, max) in [(-1., 1.), (2., 1.), (0., f64::INFINITY), (f64::NAN, 1.), (0., MAX_DISTANCE_MM.next_up())] {
        assert!(Ray::new(Vec3::ZERO, Vec3::Z, min, max).is_err());
    }
    assert!(Ray::new(Vec3::new(MAX_COORDINATE_MM.next_up(), 0., 0.), Vec3::Z, 0., 1.).is_err());
    assert!(ray(Vec3::ZERO, Vec3::Z).at(f64::NAN).is_err());
}

#[test]
fn analytic_triangle_distance_barycentrics_normal_and_two_sides() {
    let t = triangle(0.);
    let hit = t.intersect(&ray(Vec3::new(0.5, 0.5, 2.), -Vec3::Z)).unwrap();
    assert_eq!(hit.distance, 2.);
    assert_eq!(hit.barycentric, [0.5, 0.25, 0.25]);
    assert_eq!(hit.geometric_normal, Vec3::Z);
    assert!(hit.front_face);
    let back = t.intersect(&ray(Vec3::new(0.5, 0.5, -2.), Vec3::Z)).unwrap();
    assert_eq!(back.distance, 2.);
    assert!(!back.front_face);
    let boundary = Ray::new(Vec3::new(0.5, 0.5, 2.), -Vec3::Z, 2., 2.).unwrap();
    assert!(t.intersect(&boundary).is_some());
    assert!(t.intersect(&Ray::new(boundary.origin(), boundary.direction(), 0., 1.99).unwrap()).is_none());
    assert!(t.intersect(&ray(Vec3::new(2., 2., 2.), -Vec3::Z)).is_none());
}

#[test]
fn tiny_and_huge_geometry_have_no_fixed_world_epsilon() {
    for scale in [1e-120, 1e-12, 1., 1e11] {
        let t = Triangle::new([Vec3::ZERO, Vec3::X * (2. * scale), Vec3::Y * (2. * scale)]).unwrap();
        let r = Ray::new(Vec3::new(0.5 * scale, 0.5 * scale, 2. * scale), -Vec3::Z, 0., 4. * scale).unwrap();
        let hit = t.intersect(&r).unwrap();
        close(hit.distance, 2. * scale);
        close(hit.barycentric[0], 0.5);
        let bvh = build(vec![t], BuildLimits::default(), 1);
        assert_eq!(closest(&bvh, &r).unwrap().geometry, hit);
    }
}

#[test]
fn shared_edges_vertices_and_equal_distance_use_lowest_input_ordinal() {
    let a = Triangle::new([Vec3::ZERO, Vec3::X, Vec3::Y]).unwrap();
    let b = Triangle::new([Vec3::new(1., 1., 0.), Vec3::Y, Vec3::X]).unwrap();
    for triangles in [vec![a, b], vec![b, a], vec![a, a]] {
        let bvh = build(triangles.clone(), BuildLimits { leaf_size: 1, ..Default::default() }, 1);
        for point in [Vec3::new(0.5, 0.5, 1.), Vec3::new(1., 0., 1.)] {
            let r = ray(point, -Vec3::Z);
            assert_eq!(closest(&bvh, &r), brute(&triangles, &r));
            assert_eq!(closest(&bvh, &r).unwrap().primitive_index, 0);
        }
    }
}

#[test]
fn degenerate_parallel_coplanar_and_grazing_cases_are_explicit() {
    for vertices in [[Vec3::ZERO; 3], [Vec3::ZERO, Vec3::X, Vec3::X * 2.]] {
        let t = Triangle::new(vertices).unwrap();
        assert!(t.geometric_normal().is_none());
        assert!(t.intersect(&ray(Vec3::new(0., 0., 1.), -Vec3::Z)).is_none());
    }
    let t = triangle(0.);
    assert!(t.intersect(&ray(Vec3::new(0.5, 0.5, 1.), Vec3::X)).is_none());
    assert!(t.intersect(&ray(Vec3::new(0.5, 0.5, 0.), Vec3::X)).is_none());
    let hit = t.intersect(&ray(Vec3::new(-1., 0.25, 1e-9), Vec3::new(1., 0., -1e-9))).unwrap();
    close(hit.distance, 1.);
    assert!(t.intersect(&ray(Vec3::new(0.5, 0.5, 0.), Vec3::Z)).is_some(), "closed t=0 is a caller policy, not silently offset");
}

#[test]
fn slab_intervals_handle_signed_zero_boundaries_and_almost_parallel_components() {
    let b = Bounds::new(Vec3::new(1., 1., 1.), Vec3::new(2., 2., 2.)).unwrap();
    assert_eq!(b.intersect(&ray(Vec3::new(1., 1., 0.), Vec3::new(-0., 0., 1.))), Some([1., 2.]));
    assert!(b.intersect(&ray(Vec3::new(0., 1., 0.), Vec3::Z)).is_none());
    assert_eq!(b.intersect(&ray(Vec3::new(1.5, 1.5, 0.), Vec3::new(f64::from_bits(1), 0., 1.))), Some([1., 2.]));
    assert!(b.intersect(&ray(Vec3::new(0., 1.5, 0.), Vec3::new(f64::from_bits(1), 0., 1.))).is_none());
    assert!(Bounds::new(Vec3::X, Vec3::ZERO).is_err());
    assert!(Bounds::new(Vec3::new(f64::NAN, 0., 0.), Vec3::X).is_err());
}

#[test]
fn builder_steps_are_bounded_and_results_do_not_depend_on_chunk_size() {
    let triangles = (0..31).map(|i| triangle(f64::from(i))).collect::<Vec<_>>();
    let cancel = CancelToken::default();
    let limits = BuildLimits { leaf_size: 1, ..Default::default() };
    let mut builder = BvhBuilder::new(triangles.clone(), limits, &cancel).unwrap();
    let mut used = 0;
    loop {
        let progress = builder.step(1, &cancel).unwrap();
        assert!(progress.work_used - used <= 1);
        used = progress.work_used;
        if progress.complete {
            break;
        }
    }
    let a = builder.finish().unwrap();
    let b = build(triangles.clone(), limits, MAX_BUILD_STEP);
    assert_eq!(a.node_count(), b.node_count());
    assert!(a.node_count() < 2 * triangles.len());
    assert!(a.retained_bytes().unwrap() <= limits.max_memory_bytes);
    for z in [0.5, 5.5, 50.] {
        let r = ray(Vec3::new(0.5, 0.5, z), -Vec3::Z);
        assert_eq!(closest(&a, &r), brute(&triangles, &r));
        assert_eq!(closest(&a, &r), closest(&b, &r));
    }
}

#[test]
fn coincident_centroids_and_depth_cap_terminate_without_recursion() {
    let triangles = vec![triangle(0.); 129];
    for depth in [1, MAX_DEPTH] {
        let bvh = build(triangles.clone(), BuildLimits { max_depth: depth, leaf_size: 1, ..Default::default() }, 3);
        let r = ray(Vec3::new(0.5, 0.5, 1.), -Vec3::Z);
        assert_eq!(closest(&bvh, &r), brute(&triangles, &r));
        assert_eq!(closest(&bvh, &r).unwrap().primitive_index, 0);
    }
}

#[test]
fn empty_scene_and_incomplete_build_are_distinct() {
    let empty = build(Vec::new(), BuildLimits::default(), 1);
    assert_eq!(empty.primitive_count(), 0);
    assert_eq!(empty.node_count(), 0);
    assert!(closest(&empty, &ray(Vec3::ZERO, Vec3::Z)).is_none());
    let builder = BvhBuilder::new(vec![triangle(0.)], BuildLimits::default(), &CancelToken::default()).unwrap();
    assert!(matches!(builder.finish(), Err(TraceError::BuildIncomplete)));
}

#[test]
fn work_limit_is_an_error_not_a_partial_hit_and_can_span_many_rays() {
    let bvh = build(vec![triangle(0.)], BuildLimits::default(), 1);
    let cancel = CancelToken::default();
    let r = ray(Vec3::new(0.5, 0.5, 1.), -Vec3::Z);
    assert_eq!(bvh.closest_hit(&r, &mut WorkBudget::new(1).unwrap(), &cancel), Err(TraceError::WorkLimit));
    let mut work = WorkBudget::new(2).unwrap();
    assert!(bvh.any_hit(&r, &mut work, &cancel).unwrap());
    assert_eq!(work.used(), 2);
    assert_eq!(work.remaining(), 0);
    assert_eq!(bvh.any_hit(&r, &mut work, &cancel), Err(TraceError::WorkLimit));
    let mut builder = BvhBuilder::new(vec![triangle(0.); 20], BuildLimits { max_work: 1, ..Default::default() }, &cancel).unwrap();
    assert_eq!(builder.step(2, &cancel), Err(TraceError::WorkLimit));
}

#[test]
fn cancellation_covers_admission_mid_build_and_both_traversals() {
    let cancel = CancelToken::default();
    let mut builder = BvhBuilder::new(vec![triangle(0.); 20], BuildLimits::default(), &cancel).unwrap();
    builder.step(2, &cancel).unwrap();
    cancel.cancel();
    assert_eq!(builder.step(1, &cancel), Err(TraceError::Cancelled));
    assert!(matches!(BvhBuilder::new(vec![triangle(0.)], BuildLimits::default(), &cancel), Err(TraceError::Cancelled)));
    let bvh = build(vec![triangle(0.)], BuildLimits::default(), 1);
    let r = ray(Vec3::new(0.5, 0.5, 1.), -Vec3::Z);
    assert_eq!(bvh.closest_hit(&r, &mut WorkBudget::new(10).unwrap(), &cancel), Err(TraceError::Cancelled));
    assert_eq!(bvh.any_hit(&r, &mut WorkBudget::new(10).unwrap(), &cancel), Err(TraceError::Cancelled));
}

#[test]
fn memory_counts_input_capacity_and_hostile_limits_refuse_before_own_allocations() {
    let defaults = BuildLimits::default();
    let normal = BvhBuilder::required_bytes(1, 1, defaults).unwrap();
    assert!(BvhBuilder::required_bytes(1, 100, defaults).unwrap() > normal);
    assert_eq!(BvhBuilder::required_bytes(1, usize::MAX, defaults), Err(TraceError::MemoryLimit));
    assert_eq!(BvhBuilder::required_bytes(1, 1, BuildLimits { max_memory_bytes: 1, ..defaults }), Err(TraceError::MemoryLimit));
    for limits in [
        BuildLimits { max_depth: MAX_DEPTH + 1, ..defaults },
        BuildLimits { leaf_size: 0, ..defaults },
        BuildLimits { max_triangles: MAX_TRIANGLES + 1, ..defaults },
        BuildLimits { max_memory_bytes: MAX_MEMORY_BYTES + 1, ..defaults },
        BuildLimits { max_work: crate::job::MAX_CPU_WORK + 1, ..defaults },
    ] {
        assert!(BvhBuilder::required_bytes(1, 1, limits).is_err());
    }
    assert!(WorkBudget::new(0).is_err());
    let mut builder = BvhBuilder::new(vec![triangle(0.)], defaults, &CancelToken::default()).unwrap();
    assert!(builder.step(0, &CancelToken::default()).is_err());
    assert!(builder.step(MAX_BUILD_STEP + 1, &CancelToken::default()).is_err());
}

struct Seed(u64);
impl Seed {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn scalar(&mut self) -> f64 {
        (self.next() >> 11) as f64 / ((1_u64 << 53) as f64)
    }
    fn point(&mut self) -> Vec3 {
        Vec3::new(self.scalar() * 20. - 10., self.scalar() * 20. - 10., self.scalar() * 20. - 10.)
    }
}

#[test]
fn seeded_bvh_vs_brute_force_closest_and_any_hit() {
    let mut seed = Seed(0x1368_0001);
    for _ in 0..16 {
        let triangles = (0..64).map(|_| Triangle::new([seed.point(), seed.point(), seed.point()]).unwrap()).collect::<Vec<_>>();
        let bvh = build(triangles.clone(), BuildLimits { leaf_size: 2, ..Default::default() }, 7);
        for _ in 0..128 {
            let r = ray(seed.point(), seed.point());
            let expected = brute(&triangles, &r);
            assert_eq!(closest(&bvh, &r), expected);
            let any = bvh.any_hit(&r, &mut WorkBudget::new(MAX_WORK).unwrap(), &CancelToken::default()).unwrap();
            assert_eq!(any, expected.is_some());
        }
    }
}

#[test]
fn seeded_hostile_float_admission_never_publishes_nonfinite_geometry() {
    let mut seed = Seed(0x1368_badd);
    for _ in 0..2048 {
        let value = f64::from_bits(seed.next());
        let p = Vec3::new(value, 0., 0.);
        let accepted = value.is_finite() && value.abs() <= MAX_COORDINATE_MM;
        assert_eq!(Ray::new(p, Vec3::Z, 0., 1.).is_ok(), accepted);
        assert_eq!(Triangle::new([p; 3]).is_ok(), accepted);
    }
}

#[test]
fn protected_ledger_keeps_ordinary_profiles_and_requires_the_explicit_new_envelope() {
    use crate::job::{CpuBudget, MAX_CPU_WORK, MAX_PROTECTED_CPU_WORK, ProtectedPreviewPlan};
    assert_eq!(MAX_CPU_WORK, 16_000_000_000_000);
    assert_eq!(MAX_PROTECTED_CPU_WORK, 31_462_813_335_678);
    assert!(WorkBudget::new(MAX_CPU_WORK + 1).is_err());
    assert!(WorkBudget::new(MAX_PROTECTED_CPU_WORK).is_err());
    for budget in CpuBudget::ALL {
        let plan = ProtectedPreviewPlan::new(1024, 1024, 2, budget).unwrap();
        let work = WorkBudget::new_protected(plan).unwrap();
        let report = work.protected_report().unwrap();
        assert_eq!(report.ordinary_limit, budget.limits().max_work_units);
        assert_eq!(report.presentation_limit, 958_464_130);
        assert_eq!(report.aggregate_limit, report.ordinary_limit + report.presentation_limit);
        assert_eq!(work.remaining(), report.ordinary_limit);
        assert_eq!(work.ordinary_remaining(), report.ordinary_limit);
        assert_eq!(work.presentation_remaining(), report.presentation_limit);
        assert_eq!(work.aggregate_remaining(), report.aggregate_limit);
        assert_eq!((report.ordinary_used, report.presentation_used, work.used()), (0, 0, 0));
    }
    let max = ProtectedPreviewPlan::new(1024, 1024, 2, CpuBudget::Workstation).unwrap();
    assert_eq!(max.aggregate_limit().unwrap(), MAX_CPU_WORK + 958_464_130);
    assert!(WorkBudget::new(100).unwrap().protected_report().is_none());
    assert!(ProtectedPreviewPlan::new(u32::MAX, u32::MAX, u32::MAX, CpuBudget::Workstation).is_err());
}

#[test]
fn protected_ledger_exact_ordinary_and_aggregate_boundaries_do_not_steal_or_refund() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(1, 1, 2, CpuBudget::Compact).unwrap();
    let cost = plan.snapshot_work().unwrap();
    let cancel = CancelToken::default();
    let mut work = WorkBudget::new_protected_for_test(plan, 7).unwrap();
    work.charge(6, &cancel).unwrap();
    assert!(work.charge_presentation(1, &cancel).unwrap());
    let before = work.protected_report().unwrap();
    assert_eq!((before.ordinary_used, before.presentation_used), (6, cost));
    assert_eq!(work.remaining(), 1, "presentation did not consume the last ordinary tick");
    assert_eq!(work.presentation_remaining(), 0);
    assert_eq!(work.aggregate_remaining(), 1);
    assert_eq!(work.charge(2, &cancel), Err(TraceError::WorkLimit));
    assert_eq!(work.protected_report().unwrap(), before);
    work.tick(&cancel).unwrap();
    let complete = work.protected_report().unwrap();
    assert_eq!((complete.ordinary_used, complete.presentation_used, work.used()), (7, cost, 7 + cost));
    assert_eq!(work.aggregate_remaining(), 0);
    assert_eq!(work.charge(u64::MAX, &cancel), Err(TraceError::WorkLimit));
    assert!(!work.charge_presentation(1, &cancel).unwrap());
    assert_eq!(work.protected_report().unwrap(), complete);
}

#[test]
fn protected_milestones_coalesce_without_fabricating_accepted_snapshots_or_spent_quota() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(4, 1, 5, CpuBudget::Compact).unwrap();
    let mut work = WorkBudget::new_protected(plan).unwrap();
    let initial = work.protected_report().unwrap();
    assert!(!work.charge_presentation(0, &CancelToken::default()).unwrap());
    assert_eq!(work.protected_report().unwrap(), initial);
    assert!(work.charge_presentation(19, &CancelToken::default()).unwrap());
    let coalesced = work.protected_report().unwrap();
    assert_eq!((coalesced.accepted_snapshots, coalesced.next_milestone_index, coalesced.quota_limit), (1, 7, 7));
    assert_eq!(coalesced.presentation_used, plan.snapshot_work().unwrap());
    assert!(coalesced.presentation_used < coalesced.presentation_limit);
    assert!(coalesced.milestones_complete);
    assert!(!coalesced.quota_exhausted);
    for progress in [19, 20, u64::MAX] {
        assert!(!work.protected_ready(progress).unwrap());
        assert!(!work.charge_presentation(progress, &CancelToken::default()).unwrap());
        assert_eq!(work.protected_report().unwrap(), coalesced);
    }
}

#[test]
fn protected_zero_and_one_milestone_policies_are_real_boundaries() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let cancel = CancelToken::default();
    let zero = ProtectedPreviewPlan::new(1, 1, 1, CpuBudget::Compact).unwrap();
    let mut work = WorkBudget::new_protected(zero).unwrap();
    let report = work.protected_report().unwrap();
    assert_eq!((report.quota_limit, report.presentation_limit), (0, 0));
    assert_eq!(report.aggregate_limit, report.ordinary_limit);
    assert!(report.quota_exhausted && report.milestones_complete);
    assert!(!work.charge_presentation(0, &cancel).unwrap());
    assert!(zero.milestone(0).is_err());
    let one = ProtectedPreviewPlan::new(1, 1, 2, CpuBudget::Compact).unwrap();
    assert_eq!(one.quota_limit(), 1);
    assert_eq!(one.milestone(0).unwrap(), 1);
    assert!(one.milestone(1).is_err());
    let mut work = WorkBudget::new_protected(one).unwrap();
    assert!(!work.charge_presentation(0, &cancel).unwrap());
    assert!(work.charge_presentation(1, &cancel).unwrap());
    assert!(work.protected_report().unwrap().quota_exhausted);
}

#[test]
fn protected_quota_exhaustion_leaves_the_entire_ordinary_allowance_available() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(32, 32, 64, CpuBudget::Compact).unwrap();
    let cancel = CancelToken::default();
    let mut work = WorkBudget::new_protected_for_test(plan, 17).unwrap();
    for index in 0..127 {
        assert!(work.charge_presentation(plan.milestone(index).unwrap(), &cancel).unwrap());
    }
    let exhausted = work.protected_report().unwrap();
    assert_eq!((exhausted.accepted_snapshots, exhausted.next_milestone_index), (127, 127));
    assert_eq!(exhausted.presentation_used, exhausted.presentation_limit);
    assert!(exhausted.quota_exhausted && exhausted.milestones_complete);
    assert_eq!(work.ordinary_remaining(), 17);
    assert!(!work.charge_presentation(plan.total_commits() - 1, &cancel).unwrap());
    work.charge(17, &cancel).unwrap();
    let final_report = work.protected_report().unwrap();
    assert_eq!(work.used(), final_report.aggregate_limit);
    assert_eq!(final_report.ordinary_used, 17);
}

#[test]
fn protected_cancel_before_debit_is_atomic_and_after_debit_is_never_refunded() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(4, 1, 5, CpuBudget::Compact).unwrap();
    let mut work = WorkBudget::new_protected(plan).unwrap();
    let before = work.protected_report().unwrap();
    let cancelled = CancelToken::default();
    cancelled.cancel();
    assert_eq!(work.charge_presentation(1, &cancelled), Err(TraceError::Cancelled));
    assert_eq!(work.protected_report().unwrap(), before);
    let cancel = CancelToken::default();
    assert!(work.charge_presentation(1, &cancel).unwrap());
    let paid = work.protected_report().unwrap();
    cancel.cancel();
    assert_eq!(work.tick(&cancel), Err(TraceError::Cancelled));
    assert_eq!(work.charge_presentation(2, &cancel), Err(TraceError::Cancelled));
    assert_eq!(work.protected_report().unwrap(), paid);
    assert_eq!(work.used(), plan.snapshot_work().unwrap());
}

#[test]
fn protected_private_test_boundary_is_chosen_before_work_and_never_changes_the_production_plan() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(4, 1, 5, CpuBudget::Compact).unwrap();
    assert!(WorkBudget::new_protected_for_test(plan, 0).is_err());
    assert!(WorkBudget::new_protected_for_test(plan, plan.ordinary_limit() + 1).is_err());
    let mut first = WorkBudget::new_protected(plan).unwrap();
    first.charge_presentation(19, &CancelToken::default()).unwrap();
    let paid = first.protected_report().unwrap();
    // A distinct admitted job starts a new ledger; it does not restore/transfer
    // any state in the first job. Repeated report reads do not reset either one.
    let second = WorkBudget::new_protected(plan).unwrap();
    for _ in 0..4 {
        assert_eq!(first.protected_report().unwrap(), paid);
    }
    assert_eq!(second.protected_report().unwrap().presentation_used, 0);
    assert_eq!(first.protected_report().unwrap(), paid);
}

#[test]
fn protected_reached_milestones_match_literal_small_and_first_pass_boundaries() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    for (pixels, samples, cases) in [
        (1, 1, vec![(0, 0), (1, 0)]),
        (1, 7, vec![(0, 0), (1, 1), (5, 5), (6, 6), (7, 6)]),
        (2, 4, vec![(0, 0), (1, 1), (2, 2), (3, 2), (4, 3), (5, 3), (6, 4), (7, 4), (8, 4)]),
        (3, 4, vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 3), (5, 3), (6, 4), (8, 4), (9, 5), (11, 5), (12, 5)]),
        (65, 3, vec![(0, 0), (63, 63), (64, 64), (65, 65), (129, 65), (130, 66), (194, 66), (195, 66)]),
        (66, 3, vec![(0, 0), (64, 63), (65, 64), (66, 65), (131, 65), (132, 66), (197, 66), (198, 66)]),
        (67, 3, vec![(0, 0), (65, 63), (66, 64), (67, 65), (133, 65), (134, 66), (200, 66), (201, 66)]),
    ] {
        let plan = ProtectedPreviewPlan::new(pixels, 1, samples, CpuBudget::Compact).unwrap();
        for (committed, expected) in cases {
            assert_eq!(plan.reached_milestones(committed).unwrap(), expected, "P={pixels} S={samples} c={committed}");
        }
        assert!(plan.reached_milestones(plan.total_commits() + 1).is_err());
        assert!(plan.reached_milestones(u64::MAX).is_err());
    }
}

#[test]
fn protected_large_delayed_frontier_coalesces_once_without_allocating_a_film_or_spending_ordinary_work() {
    use crate::job::{CpuBudget, ProtectedPreviewPlan};
    let plan = ProtectedPreviewPlan::new(1, 1, 1_048_576, CpuBudget::Workstation).unwrap();
    assert_eq!(plan.quota_limit(), 1_048_575);
    assert_eq!(plan.reached_milestones(65_536).unwrap(), 65_536);
    assert_eq!(plan.reached_milestones(1_048_575).unwrap(), 1_048_575);
    assert_eq!(plan.reached_milestones(1_048_576).unwrap(), 1_048_575);
    let mut work = WorkBudget::new_protected_for_test(plan, 17).unwrap();
    assert!(work.charge_presentation(1_048_575, &CancelToken::default()).unwrap());
    let report = work.protected_report().unwrap();
    assert_eq!((report.accepted_snapshots, report.next_milestone_index), (1, 1_048_575));
    assert_eq!(report.presentation_used, 65_552);
    assert_eq!(report.ordinary_used, 0);
    assert_eq!(work.ordinary_remaining(), 17);
    assert!(report.milestones_complete && !report.quota_exhausted);
}
