use super::*;

#[cfg(feature = "raytrace-cpu")]
mod camera_passes {
    use super::primary::PrimaryFilm;
    use super::*;
    use crate::job::CpuProgressiveOptions;

    fn primary() -> PrimaryFilm {
        let options = CpuProgressiveOptions { samples_per_pixel: 4, seed: 7 };
        let bytes = PrimaryFilm::required_bytes(2, 1, options, crate::job::CpuBudget::Compact).unwrap();
        let mut film = PrimaryFilm::new(2, 1, options, bytes, &CancelToken::default(), crate::job::CpuBudget::Compact).unwrap();
        assert!(!film.ready());
        let mut charged = 0;
        while !film.ready() {
            film.initialize_one(&CancelToken::default(), &mut || {
                charged += 1;
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(charged, 2);
        film
    }

    #[test]
    fn camera_film_keeps_linear_sums_coverage_and_uniform_absolute_passes() {
        let mut film = primary();
        let cancel = CancelToken::default();
        assert!(film.pixel(0).is_err());
        for ordinal in 0..4 {
            film.add(0, ordinal, [2.0, 4.0, 8.0], true, &cancel, &mut || Ok(())).unwrap();
            assert!(film.pixel(0).is_err(), "a partial pass cannot be resolved");
            assert!(film.add(0, ordinal, [0.0; 3], false, &cancel, &mut || Ok(())).is_err(), "duplicate pixel must not consume a sample");
            let covered = ordinal % 2 == 0;
            film.add(1, ordinal, if covered { [6.0; 3] } else { [0.0; 3] }, covered, &cancel, &mut || Ok(())).unwrap();
            assert_eq!(film.samples(), ordinal + 1);
        }
        assert_eq!(film.pixel(0).unwrap(), ([2.0, 4.0, 8.0], 4, 4));
        assert_eq!(film.pixel(1).unwrap(), ([3.0; 3], 2, 4));
        assert!(film.add(0, 4, [1.0; 3], true, &cancel, &mut || Ok(())).is_err());
    }

    #[test]
    fn camera_film_cancellation_invalid_radiance_and_work_failure_are_atomic() {
        let mut film = primary();
        let cancel = CancelToken::default();
        let observer = cancel.clone();
        assert_eq!(
            film.add(0, 0, [1.0; 3], true, &cancel, &mut || {
                observer.cancel();
                Ok(())
            })
            .unwrap_err()
            .code,
            ErrorCode::Cancelled
        );
        for (rgb, covered) in [([1.0; 3], false), ([f64::NAN; 3], true), ([-1.0; 3], true)] {
            assert!(film.add(0, 0, rgb, covered, &CancelToken::default(), &mut || Ok(())).is_err());
        }
        assert!(film.add(0, 0, [1.0; 3], true, &CancelToken::default(), &mut || Err(limit("work exhausted"))).is_err());
        film.add(0, 0, [4.0; 3], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        film.add(1, 0, [0.0; 3], false, &CancelToken::default(), &mut || Ok(())).unwrap();
        assert_eq!(film.pixel(0).unwrap(), ([4.0; 3], 1, 1));
        assert_eq!(film.pixel(1).unwrap(), ([0.0; 3], 0, 1));
    }
}

fn film(width: u32, height: u32, limit: u32) -> FilmAccumulator {
    let bytes = FilmAccumulator::required_bytes(width, height, limit).unwrap();
    FilmAccumulator::new(width, height, limit, bytes, &CancelToken::default(), &mut |_| Ok(())).unwrap()
}

fn add(film: &mut FilmAccumulator, start: usize, values: &[[f64; 3]]) {
    film.add_range(start, values, &CancelToken::default(), &mut |_| Ok(())).unwrap();
}

#[test]
fn constant_and_alternating_samples_have_analytic_statistics() {
    let mut film = film(2, 1, 4);
    for value in [0.0, 2.0, 0.0, 2.0] {
        add(&mut film, 0, &[[7.0, 3.0, 1.0], [value; 3]]);
    }
    assert_eq!(film.pixel_statistics(0).unwrap(), PixelStatistics { mean: [7.0, 3.0, 1.0], variance: [0.0; 3], count: 4 });
    let statistics = film.pixel_statistics(1).unwrap();
    assert_eq!(statistics.mean, [1.0; 3]);
    assert_eq!(statistics.variance, [4.0 / 3.0; 3]);
}

#[test]
fn complete_pass_refuses_empty_or_uneven_counts_and_keeps_linear_values() {
    let mut film = film(2, 1, 3);
    assert!(film.complete_pass(&CancelToken::default(), &mut |_| Ok(())).is_err());
    add(&mut film, 0, &[[20.0, 2.0, 0.0]]);
    assert!(film.complete_pass(&CancelToken::default(), &mut |_| Ok(())).is_err());
    add(&mut film, 1, &[[4.0, 8.0, 16.0]]);
    let pass = film.complete_pass(&CancelToken::default(), &mut |_| Ok(())).unwrap();
    assert_eq!(pass.samples_per_pixel(), 1);
    assert_eq!(pass.dimensions(), (2, 1));
    assert_eq!(pass.pixels().collect::<Vec<_>>(), vec![[20.0, 2.0, 0.0], [4.0, 8.0, 16.0]]);
}

#[test]
fn invalid_sample_at_end_leaves_entire_update_unchanged() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        let mut film = film(2, 1, 3);
        add(&mut film, 0, &[[1.0; 3], [2.0; 3]]);
        let before = film.pixels.clone();
        assert_eq!(film.add_range(0, &[[3.0; 3], [bad; 3]], &CancelToken::default(), &mut |_| Ok(())).unwrap_err().code, ErrorCode::InvalidRequest);
        assert_eq!(film.pixels, before);
    }
}

#[test]
fn sample_limit_and_numeric_overflow_refuse_atomically() {
    let mut limited = film(2, 1, 1);
    add(&mut limited, 1, &[[1.0; 3]]);
    let before = limited.pixels.clone();
    assert_eq!(limited.add_range(0, &[[1.0; 3]; 2], &CancelToken::default(), &mut |_| Ok(())).unwrap_err().code, ErrorCode::LimitExceeded);
    assert_eq!(limited.pixels, before);
    let mut extreme = film(2, 1, 3);
    add(&mut extreme, 0, &[[1.0; 3], [f64::MAX; 3]]);
    let before = extreme.pixels.clone();
    assert!(extreme.add_range(0, &[[2.0; 3], [0.0; 3]], &CancelToken::default(), &mut |_| Ok(())).is_err());
    assert_eq!(extreme.pixels, before);
}

#[test]
fn dimensions_bytes_and_actual_capacity_are_checked() {
    for (width, height, samples) in [(0, 1, 1), (1, 0, 1), (u32::MAX, u32::MAX, 1), (1, 1, 0), (1, 1, MAX_SAMPLES + 1)] {
        assert!(FilmAccumulator::required_bytes(width, height, samples).is_err());
    }
    assert!(FilmAccumulator::required_bytes(MAX_DIMENSION, MAX_DIMENSION, 1).is_err());
    assert!(bytes_for_capacity(usize::MAX).is_err());
    let bytes = FilmAccumulator::required_bytes(2, 2, 1).unwrap();
    assert!(FilmAccumulator::new(2, 2, 1, bytes - 1, &CancelToken::default(), &mut |_| Ok(())).is_err());
    let film = film(2, 2, 1);
    assert!(film.allocated_bytes().unwrap() <= bytes);
    assert_eq!(film.allocated_bytes().unwrap(), bytes_for_capacity(film.pixels.capacity()).unwrap());
    assert!(film.allocated_bytes().unwrap() >= (std::mem::size_of::<Pixel>() * (4 + MAX_UPDATE)) as u64);
}

#[test]
fn invalid_ranges_and_oversized_updates_preserve_state() {
    let mut film = film(2, 1, 1);
    for (start, values) in [(0, Vec::new()), (usize::MAX, vec![[1.0; 3]]), (2, vec![[1.0; 3]]), (0, vec![[1.0; 3]; MAX_UPDATE + 1])] {
        assert!(film.add_range(start, &values, &CancelToken::default(), &mut |_| Ok(())).is_err());
        assert!(film.pixels.iter().all(|p| *p == Pixel::default()));
    }
    assert!(film.pixel_statistics(2).is_err());
}

#[test]
fn incremental_partitions_preserve_bitwise_results_in_sample_order() {
    let mut whole = film(4, 1, 3);
    let mut partitioned = film(4, 1, 3);
    for values in [
        [[1.0; 3], [1e12; 3], [0.25; 3], [4.0; 3]],
        [[2.0; 3], [1e12 + 1.0; 3], [0.5; 3], [2.0; 3]],
        [[4.0; 3], [1e12 - 1.0; 3], [0.75; 3], [0.0; 3]],
    ] {
        add(&mut whole, 0, &values);
        add(&mut partitioned, 0, &values[..1]);
        add(&mut partitioned, 1, &values[1..3]);
        add(&mut partitioned, 3, &values[3..]);
    }
    assert_eq!(whole.pixels, partitioned.pixels);
    assert_eq!(whole.pixel_statistics(1).unwrap().variance, [1.0; 3]);
}

#[test]
fn cancellation_and_work_failure_preserve_all_pixels() {
    let mut film = film(2, 1, 3);
    let cancel = CancelToken::default();
    let observer = cancel.clone();
    assert_eq!(
        film.add_range(0, &[[1.0; 3]; 2], &cancel, &mut |_| {
            observer.cancel();
            Ok(())
        })
        .unwrap_err()
        .code,
        ErrorCode::Cancelled
    );
    assert!(film.pixels.iter().all(|p| *p == Pixel::default()));
    let mut charged = 0;
    assert_eq!(
        film.add_range(0, &[[1.0; 3]; 2], &CancelToken::default(), &mut |units| {
            charged += units;
            if charged > 2 { Err(limit("job work exhausted")) } else { Ok(()) }
        })
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(charged, 4);
    assert!(film.pixels.iter().all(|p| *p == Pixel::default()));
}

#[test]
fn construction_and_complete_pass_share_caller_work_and_cancel() {
    let bytes = FilmAccumulator::required_bytes(2, 1, 1).unwrap();
    let cancel = CancelToken::default();
    cancel.cancel();
    assert!(FilmAccumulator::new(2, 1, 1, bytes, &cancel, &mut |_| panic!("cancelled construction charged work")).is_err());
    let mut charged = 0;
    let mut film = FilmAccumulator::new(2, 1, 1, bytes, &CancelToken::default(), &mut |n| {
        charged += n;
        Ok(())
    })
    .unwrap();
    assert_eq!(charged, 2);
    add(&mut film, 0, &[[1.0; 3]; 2]);
    assert!(film.complete_pass(&CancelToken::default(), &mut |_| Err(limit("job exhausted"))).is_err());
    let pass = film
        .complete_pass(&CancelToken::default(), &mut |n| {
            charged += n;
            Ok(())
        })
        .unwrap();
    assert_eq!(charged, 4);
    assert_eq!(pass.samples_per_pixel(), 1);
}

#[test]
fn sample_counts_reach_4096_and_the_declared_cap_without_truncation() {
    for samples in [4096, MAX_SAMPLES] {
        let mut film = film(1, 1, samples);
        let cancel = CancelToken::default();
        let mut charged = 0u64;
        for _ in 0..samples {
            film.add_range(0, &[[7.0, 3.0, 1.0]], &cancel, &mut |n| {
                charged += n;
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(charged, u64::from(samples) * 2);
        let expected = PixelStatistics { mean: [7.0, 3.0, 1.0], variance: [0.0; 3], count: samples };
        assert_eq!(film.pixel_statistics(0).unwrap(), expected);
        assert_eq!(film.complete_pass(&cancel, &mut |_| Ok(())).unwrap().samples_per_pixel(), samples);
        assert_eq!(film.add_range(0, &[[1.0; 3]], &cancel, &mut |_| Ok(())).unwrap_err().code, ErrorCode::LimitExceeded);
        assert_eq!(film.pixel_statistics(0).unwrap(), expected);
    }
}

#[test]
fn construction_stops_on_exhausted_caller_work() {
    let bytes = FilmAccumulator::required_bytes(4, 1, 1).unwrap();
    let mut charged = 0u64;
    let result = FilmAccumulator::new(4, 1, 1, bytes, &CancelToken::default(), &mut |n| {
        charged += n;
        if charged > 2 { Err(limit("existing job budget exhausted")) } else { Ok(()) }
    });
    assert!(matches!(result, Err(JobError { code: ErrorCode::LimitExceeded, .. })));
    assert_eq!(charged, 3);
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn primary_film_large_profile_keeps_every_sample_and_cancels_before_mutation() {
    use crate::film::primary::PrimaryFilm;
    use crate::job::{CancelToken, CpuBudget, CpuProgressiveOptions};
    let options = CpuProgressiveOptions { samples_per_pixel: 512, seed: 7 };
    assert!(PrimaryFilm::required_bytes(1, 1, options, CpuBudget::Compact).is_err());
    let bytes = PrimaryFilm::required_bytes(1, 1, options, CpuBudget::Studio).unwrap();
    let cancel = CancelToken::default();
    let mut film = PrimaryFilm::new(1, 1, options, bytes, &cancel, CpuBudget::Studio).unwrap();
    film.initialize_one(&cancel, &mut || Ok(())).unwrap();
    for ordinal in 0..512 {
        film.add(0, ordinal, [1e12, 2.0, 0.0], true, &cancel, &mut || Ok(())).unwrap();
    }
    assert_eq!(film.pixel(0).unwrap(), ([1e12, 2.0, 0.0], 512, 512));
    cancel.cancel();
    assert!(film.add(0, 512, [0.0; 3], true, &cancel, &mut || Ok(())).is_err());
    assert_eq!(film.samples(), 512);
}

#[cfg(feature = "raytrace-cpu")]
mod partial_camera_passes {
    use super::primary::{PartialFrontier, PartialPixel, PrimaryFilm};
    use super::*;
    use crate::job::{CpuBudget, CpuProgressiveOptions};

    fn uninitialized(samples: u32, seed: u32) -> PrimaryFilm {
        let options = CpuProgressiveOptions { samples_per_pixel: samples, seed };
        let bytes = PrimaryFilm::required_bytes(4, 1, options, CpuBudget::Compact).unwrap();
        PrimaryFilm::new(4, 1, options, bytes, &CancelToken::default(), CpuBudget::Compact).unwrap()
    }

    fn ready(samples: u32, seed: u32) -> PrimaryFilm {
        let mut film = uninitialized(samples, seed);
        while !film.ready() {
            film.initialize_one(&CancelToken::default(), &mut || Ok(())).unwrap();
        }
        film
    }

    fn observe(film: &PrimaryFilm) -> (PartialFrontier, Vec<PartialPixel>) {
        (film.partial_frontier().unwrap(), (0..4).map(|index| film.partial_pixel(index).unwrap()).collect())
    }

    fn sampled(mean: [f64; 3], covered: u32, count: u32) -> PartialPixel {
        PartialPixel::Sampled { linear_premultiplied: mean, covered, count }
    }

    #[test]
    fn partial_frontier_distinguishes_first_prefix_miss_and_unobserved_pixels() {
        let mut film = ready(2, 7);
        assert_eq!(observe(&film), (PartialFrontier { completed_passes: 0, updated_prefix: 0, pixels: 4 }, vec![PartialPixel::Unknown; 4],));
        film.add(0, 0, [2.0, 0.0, 0.0], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        film.add(1, 0, [0.0; 3], false, &CancelToken::default(), &mut || Ok(())).unwrap();
        assert_eq!(
            observe(&film),
            (
                PartialFrontier { completed_passes: 0, updated_prefix: 2, pixels: 4 },
                vec![sampled([2.0, 0.0, 0.0], 1, 1), sampled([0.0; 3], 0, 1), PartialPixel::Unknown, PartialPixel::Unknown],
            )
        );
        assert!(film.pixel(0).is_err(), "legacy whole-pass reader still rejects the partial pass");
        assert!(film.pixel(1).is_err(), "a sampled miss does not imply a uniform pass");
    }

    #[test]
    fn partial_counts_and_means_follow_real_interior_and_full_pass_frontiers() {
        let mut film = ready(2, 0);
        let cancel = CancelToken::default();
        for (index, value, covered) in [(0, [2.0, 0.0, 0.0], true), (1, [0.0; 3], false), (2, [0.0, 0.0, 4.0], true), (3, [0.0; 3], false)] {
            film.add(index, 0, value, covered, &cancel, &mut || Ok(())).unwrap();
        }
        assert_eq!(film.partial_frontier().unwrap(), PartialFrontier { completed_passes: 1, updated_prefix: 0, pixels: 4 });
        assert_eq!(film.partial_pixel(3).unwrap(), sampled([0.0; 3], 0, 1));
        assert_eq!(film.pixel(0).unwrap(), ([2.0, 0.0, 0.0], 1, 1));

        film.add(0, 1, [4.0, 0.0, 0.0], true, &cancel, &mut || Ok(())).unwrap();
        film.add(1, 1, [0.0, 2.0, 0.0], true, &cancel, &mut || Ok(())).unwrap();
        assert_eq!(
            observe(&film),
            (
                PartialFrontier { completed_passes: 1, updated_prefix: 2, pixels: 4 },
                vec![sampled([3.0, 0.0, 0.0], 2, 2), sampled([0.0, 1.0, 0.0], 1, 2), sampled([0.0, 0.0, 4.0], 1, 1), sampled([0.0; 3], 0, 1)],
            )
        );
        assert!(film.pixel(0).is_err());

        film.add(2, 1, [0.0, 0.0, 8.0], true, &cancel, &mut || Ok(())).unwrap();
        assert_eq!(film.partial_frontier().unwrap().updated_prefix, 3);
        film.add(3, 1, [0.0; 3], false, &cancel, &mut || Ok(())).unwrap();
        assert_eq!(
            observe(&film),
            (
                PartialFrontier { completed_passes: 2, updated_prefix: 0, pixels: 4 },
                vec![sampled([3.0, 0.0, 0.0], 2, 2), sampled([0.0, 1.0, 0.0], 1, 2), sampled([0.0, 0.0, 6.0], 2, 2), sampled([0.0; 3], 0, 2)],
            )
        );
        assert_eq!(film.pixel(2).unwrap(), ([0.0, 0.0, 6.0], 2, 2));
        assert!(film.add(0, 2, [0.0; 3], false, &cancel, &mut || Ok(())).is_err());
        assert_eq!(film.partial_frontier().unwrap().completed_passes, 2);
    }

    #[test]
    fn partial_access_refuses_uninitialized_storage_and_invalid_indices() {
        let mut film = uninitialized(2, 7);
        assert!(film.partial_frontier().is_err());
        assert!(film.partial_pixel(0).is_err());
        film.initialize_one(&CancelToken::default(), &mut || Ok(())).unwrap();
        assert!(film.partial_frontier().is_err(), "allocated prefix is not an initialized film");
        assert!(film.partial_pixel(0).is_err());
        while !film.ready() {
            film.initialize_one(&CancelToken::default(), &mut || Ok(())).unwrap();
        }
        assert_eq!(film.partial_pixel(0).unwrap(), PartialPixel::Unknown);
        assert!(film.partial_pixel(4).is_err());
        assert!(film.partial_pixel(usize::MAX).is_err());
        assert_eq!(film.partial_frontier().unwrap().updated_prefix, 0);
    }

    #[test]
    fn failed_or_cancelled_updates_preserve_the_observed_partial_frontier() {
        let mut film = ready(2, 7);
        film.add(0, 0, [2.0, 0.0, 0.0], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        let before = observe(&film);
        for (index, ordinal, value, covered) in
            [(0, 0, [0.0; 3], false), (2, 0, [0.0; 3], false), (1, 1, [0.0; 3], false), (1, 0, [f64::NAN; 3], true), (1, 0, [1.0; 3], false)]
        {
            assert!(film.add(index, ordinal, value, covered, &CancelToken::default(), &mut || Ok(())).is_err());
            assert_eq!(observe(&film), before);
        }
        assert_eq!(
            film.add(1, 0, [4.0; 3], true, &CancelToken::default(), &mut || Err(limit("existing job exhausted"))).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(observe(&film), before);
        let cancel = CancelToken::default();
        let observer = cancel.clone();
        assert_eq!(
            film.add(1, 0, [4.0; 3], true, &cancel, &mut || {
                observer.cancel();
                Ok(())
            })
            .unwrap_err()
            .code,
            ErrorCode::Cancelled
        );
        assert_eq!(observe(&film), before);
        film.add(1, 0, [4.0; 3], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        assert_eq!(film.partial_frontier().unwrap().updated_prefix, 2, "failed updates did not consume the next ordinal");
    }

    #[test]
    fn reading_partial_pixels_preserves_default_sum_order_bits_and_charge_count() {
        for seed in [0, 7, u32::MAX] {
            let mut observed = ready(4, seed);
            let mut control = ready(4, seed);
            let mut observed_charges = 0;
            let mut control_charges = 0;
            for ordinal in 0..4 {
                let red = if ordinal == 0 { 1e12 } else { 2.0_f64.powi(-15) };
                for index in 0..4 {
                    let value = [red, red * 0.5, 2.0];
                    observed
                        .add(index, ordinal, value, true, &CancelToken::default(), &mut || {
                            observed_charges += 1;
                            Ok(())
                        })
                        .unwrap();
                    // These uncharged O(1) reads model only the accessor. A future
                    // snapshot caller still owes its own per-read/copy work charge.
                    let _ = observe(&observed);
                    control
                        .add(index, ordinal, value, true, &CancelToken::default(), &mut || {
                            control_charges += 1;
                            Ok(())
                        })
                        .unwrap();
                }
            }
            assert_eq!((observed_charges, control_charges), (16, 16));
            assert_eq!(observed.samples(), 4);
            for index in 0..4 {
                let (actual, covered, count) = observed.pixel(index).unwrap();
                let (expected, expected_covered, expected_count) = control.pixel(index).unwrap();
                assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
                assert_eq!(
                    actual.map(f64::to_bits),
                    [2.5e11_f64, 1.25e11_f64, 2.0_f64].map(f64::to_bits),
                    "ordered additions keep each sub-half-ULP contribution rounded at its original addition"
                );
                assert_eq!((covered, count), (expected_covered, expected_count));
                assert_eq!(observed.partial_pixel(index).unwrap(), sampled(actual, 4, 4));
            }
        }
    }
}

#[cfg(feature = "raytrace-cpu")]
mod workstation_primary_counts {
    use super::primary::{PartialPixel, PrimaryFilm};
    use super::*;
    use crate::job::{CpuBudget, CpuProgressiveOptions};

    fn primary(target: u32) -> PrimaryFilm {
        let options = CpuProgressiveOptions { samples_per_pixel: target, seed: 7 };
        let bytes = PrimaryFilm::required_bytes(1, 1, options, CpuBudget::Workstation).unwrap();
        let mut film = PrimaryFilm::new(1, 1, options, bytes, &CancelToken::default(), CpuBudget::Workstation).unwrap();
        film.initialize_one(&CancelToken::default(), &mut || Ok(())).unwrap();
        film
    }

    #[test]
    fn workstation_cap_is_explicit_and_other_profiles_and_defaults_stay_fixed() {
        assert_eq!(crate::job::MAX_CPU_SAMPLES_PER_PIXEL, 1_048_576);
        assert_eq!(CpuBudget::Workstation.limits().max_samples_per_pixel, 1_048_576);
        assert_eq!(CpuBudget::Studio.limits().max_samples_per_pixel, 1024);
        assert_eq!(CpuBudget::Compact.limits().max_samples_per_pixel, 64);
        assert_eq!(CpuBudget::default(), CpuBudget::Compact);
        assert_eq!(CpuProgressiveOptions::default(), CpuProgressiveOptions { samples_per_pixel: 16, seed: 0 });
        for samples_per_pixel in [0, 1_048_577, u32::MAX] {
            assert!(CpuBudget::Workstation.validate_samples(samples_per_pixel).is_err());
        }
        for (budget, cap) in [(CpuBudget::Compact, 64), (CpuBudget::Studio, 1024), (CpuBudget::Workstation, 1_048_576)] {
            budget.validate_samples(cap).unwrap();
            assert!(budget.validate_samples(cap + 1).is_err());
        }
        let large = CpuProgressiveOptions { samples_per_pixel: 1_048_576, seed: 7 };
        assert!(large.validate().is_err(), "the no-profile compatibility check must remain Compact");
        assert!(large.validate_image(1, 1).is_err());
    }

    #[test]
    fn high_sample_intent_keeps_camera_resolution_work_and_memory_admission() {
        let limits = CpuBudget::Workstation.limits();
        assert_eq!(limits.max_camera_samples, 68_719_476_736);
        assert_eq!(limits.max_dimension, 16_384);
        assert_eq!(limits.max_pixels, 67_108_864);
        assert_eq!(limits.max_progressive_pixels, 67_108_864);
        assert_eq!(limits.max_job_bytes, 12 * 1024 * 1024 * 1024);
        assert_eq!(limits.max_work_units, 16_000_000_000_000);
        assert_eq!(crate::job::NATIVE_CPU_POOL_BYTES, 16 * 1024 * 1024 * 1024);
        let million = Some(CpuProgressiveOptions { samples_per_pixel: 1_048_576, seed: 7 });
        CpuBudget::Workstation.validate_image(256, 256, million).unwrap();
        assert!(CpuBudget::Workstation.validate_image(257, 256, million).is_err());
        assert!(CpuBudget::Workstation.validate_image(u32::MAX, u32::MAX, million).is_err());
        // Pure admission boundaries, no large image or film allocation.
        for (width, height, samples) in [(1920, 1080, 33_140), (3840, 2160, 8_285), (7680, 4320, 2_071), (8192, 8192, 1024)] {
            CpuBudget::Workstation.validate_image(width, height, Some(CpuProgressiveOptions { samples_per_pixel: samples, seed: 7 })).unwrap();
            assert!(
                CpuBudget::Workstation
                    .validate_image(width, height, Some(CpuProgressiveOptions { samples_per_pixel: samples + 1, seed: 7 }))
                    .is_err()
            );
        }
        for budget in [CpuBudget::Studio, CpuBudget::Workstation] {
            assert_eq!(budget.validate_execution().is_err(), cfg!(target_arch = "wasm32") || usize::BITS < 64);
        }
        CpuBudget::Compact.validate_execution().unwrap();
    }

    #[test]
    fn one_pixel_million_sample_intent_does_not_allocate_a_sample_history() {
        let low = CpuProgressiveOptions { samples_per_pixel: 1, seed: 7 };
        let high = CpuProgressiveOptions { samples_per_pixel: 1_048_576, seed: 7 };
        let bytes = PrimaryFilm::required_bytes(1, 1, high, CpuBudget::Workstation).unwrap();
        assert_eq!(PrimaryFilm::required_bytes(1, 1, low, CpuBudget::Workstation).unwrap(), bytes);
        assert!(PrimaryFilm::new(1, 1, high, bytes - 1, &CancelToken::default(), CpuBudget::Workstation).is_err());
        for samples_per_pixel in [0, 1_048_577, u32::MAX] {
            assert!(PrimaryFilm::required_bytes(1, 1, CpuProgressiveOptions { samples_per_pixel, seed: 7 }, CpuBudget::Workstation).is_err());
        }
        assert!(PrimaryFilm::required_bytes(257, 256, high, CpuBudget::Workstation).is_err());
    }

    #[test]
    fn primary_resolve_keeps_exact_u32_counts_at_65535_65536_and_the_workstation_cap() {
        // Real film commits, one pixel only: no ray tracing, PNG, host or million-frame render.
        let mut film = primary(1_048_576);
        let cancel = CancelToken::default();
        let mut charged = 0u64;
        for ordinal in 0..1_048_576 {
            film.add(0, ordinal, [1.0, 2.0, 4.0], true, &cancel, &mut || {
                charged += 1;
                Ok(())
            })
            .unwrap();
            let count = ordinal + 1;
            if [4096, 65_535, 65_536, 1_048_576].contains(&count) {
                let (mean, covered, samples): ([f64; 3], u32, u32) = film.pixel(0).unwrap();
                assert_eq!(mean, [1.0, 2.0, 4.0]);
                assert_eq!((covered, samples), (count, count));
                assert_eq!(film.samples(), count);
                assert_eq!(charged, u64::from(count));
                assert_eq!(film.partial_pixel(0).unwrap(), PartialPixel::Sampled { linear_premultiplied: mean, covered: count, count });
            }
        }
        let before = film.pixel(0).unwrap();
        let frontier = film.partial_frontier().unwrap();
        for ordinal in [1_048_576, 1_048_577, u32::MAX] {
            assert!(film.add(0, ordinal, [0.0; 3], false, &cancel, &mut || panic!("rejected ordinal charged work")).is_err());
            assert_eq!(film.pixel(0).unwrap(), before);
            assert_eq!(film.partial_frontier().unwrap(), frontier);
        }
    }

    #[test]
    fn high_counts_keep_misses_in_the_mean_and_the_real_coverage_denominator() {
        let mut film = primary(65_537);
        let cancel = CancelToken::default();
        for ordinal in 0..65_537 {
            let covered = ordinal % 2 == 0;
            film.add(0, ordinal, if covered { [2.0, 4.0, 8.0] } else { [0.0; 3] }, covered, &cancel, &mut || Ok(())).unwrap();
            let count = ordinal + 1;
            if [65_535, 65_536, 65_537].contains(&count) {
                let hits = count.div_ceil(2);
                let expected = [2.0, 4.0, 8.0].map(|value| value * f64::from(hits) / f64::from(count));
                assert_eq!(film.pixel(0).unwrap(), (expected, hits, count));
                assert_eq!(film.partial_pixel(0).unwrap(), PartialPixel::Sampled { linear_premultiplied: expected, covered: hits, count });
            }
        }
    }

    #[test]
    fn high_count_cancellation_and_work_failure_preserve_the_exact_committed_film() {
        let mut film = primary(65_537);
        for ordinal in 0..65_536 {
            film.add(0, ordinal, [1.0, 2.0, 4.0], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        }
        let before = film.pixel(0).unwrap();
        let frontier = film.partial_frontier().unwrap();
        let cancelled = CancelToken::default();
        cancelled.cancel();
        assert_eq!(
            film.add(0, 65_536, [3.0; 3], true, &cancelled, &mut || panic!("cancelled input charged work")).unwrap_err().code,
            ErrorCode::Cancelled
        );
        let cancel = CancelToken::default();
        let observer = cancel.clone();
        let mut charged = 0;
        assert_eq!(
            film.add(0, 65_536, [3.0; 3], true, &cancel, &mut || {
                charged += 1;
                observer.cancel();
                Ok(())
            })
            .unwrap_err()
            .code,
            ErrorCode::Cancelled
        );
        assert_eq!(charged, 1);
        assert!(film.add(0, 65_536, [3.0; 3], true, &CancelToken::default(), &mut || Err(limit("same cumulative job exhausted"))).is_err());
        assert_eq!(film.pixel(0).unwrap(), before);
        assert_eq!(film.partial_frontier().unwrap(), frontier);
        film.add(0, 65_536, [1.0, 2.0, 4.0], true, &CancelToken::default(), &mut || Ok(())).unwrap();
        assert_eq!(film.pixel(0).unwrap(), ([1.0, 2.0, 4.0], 65_537, 65_537));
    }

    #[test]
    fn changing_only_the_target_keeps_old_4096_prefix_bits_and_ordinals() {
        let mut old_target = primary(4096);
        let mut high_target = primary(1_048_576);
        let cancel = CancelToken::default();
        for ordinal in 0..4096 {
            let covered = ordinal % 3 != 0;
            let sample = if covered { [0.25, 2.0, 8.0] } else { [0.0; 3] };
            old_target.add(0, ordinal, sample, covered, &cancel, &mut || Ok(())).unwrap();
            high_target.add(0, ordinal, sample, covered, &cancel, &mut || Ok(())).unwrap();
            let (old_mean, old_covered, old_count) = old_target.pixel(0).unwrap();
            let (high_mean, high_covered, high_count) = high_target.pixel(0).unwrap();
            assert_eq!(old_mean.map(f64::to_bits), high_mean.map(f64::to_bits));
            assert_eq!((old_covered, old_count), (high_covered, high_count));
            assert_eq!(old_target.partial_frontier().unwrap(), high_target.partial_frontier().unwrap());
        }
    }
}
