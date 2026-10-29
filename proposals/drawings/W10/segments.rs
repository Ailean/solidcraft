//! Private proposed Seg2 adapter; not an existing or connected upstream API.
//! Geometry is already in local paper millimetres. No placement, grid snapping,
//! endpoint replacement, closure repair, style or writer operation occurs.
//! Line/Cubic fields are copied bit for bit if finite, including signed zero;
//! this is not Scene admission and does not impose its coordinate/collapse rules.
//! Arcs inherit circular_arc's stricter admission and conservative mm bound.
//! Conics and higher-degree Beziers are explicitly unsupported.

use super::{MAX_ARC_CUBICS, circular_arc};
use crate::paper::Invalid;
use crate::Seg2;

type Result<T> = std::result::Result<T, Invalid>;

/// Caller ceilings, independently required to be in1..=64; never clamped.
/// Work counts one unit per copied segment and the worst-case number of trial
/// cubic slots across a helper call (1+...+helper_cap), even on early success.
/// Fixed per-slot interval recurrences are inherited from the frozen helper.
/// Input validation/iteration is separately bounded by input, also <=64.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SegmentCaps {
    pub(crate) input: usize,
    pub(crate) output: usize,
    pub(crate) work: usize,
}

/// Return only Lines/Cubics, preserving input sequence and arc sweep direction.
/// Each arc independently satisfies error_mm in local mm; this is a maximum
/// geometric displacement bound, not a sum over paths or a writer error budget.
/// No partial output is returned if any field, cap, allocation or arc refuses.
/// Conservative worst-case work charging may refuse otherwise convertible data.
pub(crate) fn segments(source: &[Seg2], error_mm: f64, caps: SegmentCaps) -> Result<Vec<Seg2>> {
    if [caps.input, caps.output, caps.work].into_iter().any(|cap| !(1..=MAX_ARC_CUBICS).contains(&cap)) {
        return Err(Invalid("segment candidate: every caller cap must be from1 to64"));
    }
    if source.len() > caps.input {
        return Err(Invalid("segment candidate: input cap exceeded"));
    }
    if !error_mm.is_finite() || error_mm <= 0.0 {
        return Err(Invalid("segment candidate: finite positive mm error required"));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(caps.output)
        .map_err(|_| Invalid("segment candidate: cannot allocate bounded output"))?;
    let mut work = 0;
    for segment in source {
        let remaining_output = caps.output.checked_sub(output.len())
            .ok_or(Invalid("segment candidate: output accounting exceeded"))?;
        let remaining_work = caps.work.checked_sub(work)
            .ok_or(Invalid("segment candidate: work accounting exceeded"))?;
        if remaining_output == 0 || remaining_work == 0 {
            return Err(Invalid("segment candidate: output or work cap exhausted"));
        }
        match *segment {
            Seg2::Line { a, b } => {
                if !a.is_finite() || !b.is_finite() {
                    return Err(Invalid("segment candidate: nonfinite line coordinate"));
                }
                output.push(*segment);
                work += 1;
            }
            Seg2::Cubic { p0, p1, p2, p3 } => {
                if ![p0, p1, p2, p3].into_iter().all(|point| point.is_finite()) {
                    return Err(Invalid("segment candidate: nonfinite cubic coordinate"));
                }
                output.push(*segment);
                work += 1;
            }
            Seg2::Arc { .. } => {
                // Each candidate k can attempt k(k+1)/2 trial cubic slots.
                // Select an internal budget within the unmodified caller caps.
                // k<=64 makes these integer calculations bounded and exact.
                let mut helper_cap = 0;
                let mut charge = 0;
                for candidate in 1..=MAX_ARC_CUBICS {
                    let trial_slots = candidate * (candidate + 1) / 2;
                    if candidate <= remaining_output && trial_slots <= remaining_work {
                        helper_cap = candidate;
                        charge = trial_slots;
                    }
                }
                if helper_cap == 0 {
                    return Err(Invalid("segment candidate: no certified helper work budget"));
                }
                let converted = circular_arc(segment, error_mm, helper_cap)?;
                if converted.pieces.is_empty() || converted.pieces.len() > helper_cap
                    || !converted.error_bound_mm.is_finite() || converted.error_bound_mm > error_mm
                    || converted.pieces.iter().any(|piece| !matches!(piece, Seg2::Cubic { .. }))
                {
                    return Err(Invalid("segment candidate: unexpected helper result"));
                }
                // Reserved output capacity covers every push; no hidden extend allocation.
                for piece in converted.pieces {
                    output.push(piece);
                }
                work += charge;
            }
            Seg2::Conic { .. } | Seg2::Bezier { .. } => {
                return Err(Invalid("not supported yet: conic or higher-degree Bezier segment conversion"));
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_BEZIER_CTRL, Vec2};
    use std::f64::consts::{FRAC_PI_2, PI};

    fn caps() -> SegmentCaps {
        SegmentCaps { input: 64, output: 64, work: 64 }
    }

    fn arc(sweep: f64) -> Seg2 {
        Seg2::Arc { center: Vec2::ZERO, radius: 1.0, start: 0.0, sweep }
    }

    fn bits(segment: &Seg2) -> Vec<u64> {
        let points = match *segment {
            Seg2::Line { a, b } => vec![a, b],
            Seg2::Cubic { p0, p1, p2, p3 } => vec![p0, p1, p2, p3],
            _ => panic!("test expects a line or cubic"),
        };
        points.into_iter().flat_map(|point| [point.x.to_bits(), point.y.to_bits()]).collect()
    }

    #[test]
    fn finite_passthrough_is_bit_exact_including_signed_zero_and_large_coordinates() {
        let source = [
            Seg2::Line { a: Vec2::new(-0.0, f64::MAX), b: Vec2::new(0.0, -f64::MAX) },
            Seg2::Cubic {
                p0: Vec2::new(-0.0, 0.0), p1: Vec2::new(f64::from_bits(1), -0.0),
                p2: Vec2::new(-f64::MAX, f64::MAX), p3: Vec2::new(0.0, -0.0),
            },
            Seg2::Line { a: Vec2::ZERO, b: Vec2::ZERO },
        ];
        let before: Vec<_> = source.iter().map(bits).collect();
        let result = segments(&source, 1e-4, caps()).unwrap();
        assert_eq!(result.len(), source.len());
        assert_eq!(result.iter().map(bits).collect::<Vec<_>>(), before);
        assert_eq!(source.iter().map(bits).collect::<Vec<_>>(), before);
    }

    #[test]
    fn mixed_input_keeps_order_and_inputs_without_applying_a_placement() {
        let line = Seg2::Line { a: Vec2::new(20.0, 30.0), b: Vec2::new(21.0, 30.0) };
        let cubic = Seg2::Cubic {
            p0: Vec2::new(40.0, 0.0), p1: Vec2::new(41.0, 1.0),
            p2: Vec2::new(42.0, 1.0), p3: Vec2::new(43.0, 0.0),
        };
        let source = [line, arc(-FRAC_PI_2), cubic, line];
        let before = source;
        let result = segments(&source, 1e-4, caps()).unwrap();
        assert!(result.len() > source.len() && result.len() <= 64);
        assert_eq!(bits(result.first().unwrap()), bits(&line));
        let tail = result.get(result.len() - 2..).unwrap();
        assert_eq!(bits(tail.first().unwrap()), bits(&cubic));
        assert_eq!(bits(tail.last().unwrap()), bits(&line));
        for piece in result.iter().skip(1).take(result.len() - 3) {
            assert!(matches!(piece, Seg2::Cubic { .. }));
        }
        assert_eq!(source, before);
    }

    #[test]
    fn positive_and_negative_sweeps_keep_independent_endpoint_and_tangent_direction() {
        for sign in [-1.0, 1.0] {
            let result = segments(&[arc(sign * FRAC_PI_2)], 1e-4, caps()).unwrap();
            assert!((result.first().unwrap().start() - Vec2::new(1.0, 0.0)).len() < 1e-10);
            assert!((result.last().unwrap().end() - Vec2::new(0.0, sign)).len() < 1e-10);
            let Seg2::Cubic { p0, p1, .. } = *result.first().unwrap() else { panic!("cubic required") };
            let tangent = p1 - p0;
            assert!(sign * (p0.x * tangent.y - p0.y * tangent.x) > 0.0);
            for pair in result.windows(2) {
                assert_eq!(pair[0].end(), pair[1].start());
            }
        }
    }

    #[test]
    fn all_caller_caps_are_validated_without_clamping() {
        for bad in [0, 65, usize::MAX] {
            for limit in [
                SegmentCaps { input: bad, ..caps() },
                SegmentCaps { output: bad, ..caps() },
                SegmentCaps { work: bad, ..caps() },
            ] {
                assert!(segments(&[], 1e-4, limit).is_err());
            }
        }
        let line = Seg2::Line { a: Vec2::ZERO, b: Vec2::new(1.0, 0.0) };
        assert!(segments(&[line, line], 1e-4, SegmentCaps { input: 1, ..caps() }).is_err());
        assert!(segments(&[line, line], 1e-4, SegmentCaps { output: 1, ..caps() }).is_err());
        assert!(segments(&[line, line], 1e-4, SegmentCaps { work: 1, ..caps() }).is_err());
        assert_eq!(segments(&[line; 64], 1e-4, caps()).unwrap().len(), 64);
        assert!(segments(&[line; 65], 1e-4, caps()).is_err());
        assert!(segments(&[], 1e-4, caps()).unwrap().is_empty());
    }

    #[test]
    fn arc_output_and_global_trial_work_exhaustion_refuse() {
        let quarter = arc(FRAC_PI_2);
        assert!(segments(&[quarter], 1e-4, SegmentCaps { output: 1, ..caps() }).is_err());
        // At work2 only helper_cap1 is permitted, and this quarter needs more.
        assert!(segments(&[quarter], 1e-4, SegmentCaps { work: 2, ..caps() }).is_err());
        // Two one-cubic calls cost1 each; a third exceeds the global work cap.
        assert_eq!(segments(&[quarter; 2], 4e-4, SegmentCaps { work: 2, ..caps() }).unwrap().len(), 2);
        assert!(segments(&[quarter; 3], 4e-4, SegmentCaps { work: 2, ..caps() }).is_err());
    }

    #[test]
    fn every_passthrough_coordinate_refuses_nan_or_infinity() {
        for hostile in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for slot in 0..8 {
                let mut values = [1.0; 8];
                values[slot] = hostile;
                let points = [
                    Vec2::new(values[0], values[1]), Vec2::new(values[2], values[3]),
                    Vec2::new(values[4], values[5]), Vec2::new(values[6], values[7]),
                ];
                let cubic = Seg2::Cubic { p0: points[0], p1: points[1], p2: points[2], p3: points[3] };
                assert!(segments(&[cubic], 1e-4, caps()).is_err());
                if slot < 4 {
                    let line = Seg2::Line { a: points[0], b: points[1] };
                    assert!(segments(&[line], 1e-4, caps()).is_err());
                }
            }
        }
    }

    #[test]
    fn unsupported_and_hostile_arcs_refuse_instead_of_returning_a_partial_prefix() {
        let line = Seg2::Line { a: Vec2::ZERO, b: Vec2::new(1.0, 0.0) };
        for refused in [
            Seg2::Conic { a: Vec2::ZERO, apex: Vec2::new(1.0, 1.0), b: Vec2::new(2.0, 0.0), w: 1.0 },
            Seg2::Bezier { n: 4, p: [Vec2::ZERO; MAX_BEZIER_CTRL] },
            Seg2::Arc { center: Vec2::ZERO, radius: 0.0, start: 0.0, sweep: FRAC_PI_2 },
            Seg2::Arc { center: Vec2::new(f64::NAN, 0.0), radius: 1.0, start: 0.0, sweep: FRAC_PI_2 },
            arc(PI),
        ] {
            assert!(segments(&[line, refused, line], 1e-4, caps()).is_err());
        }
        for error in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(segments(&[line], error, caps()).is_err());
        }
        assert!(segments(&[arc(FRAC_PI_2)], f64::from_bits(1), caps()).is_err());
    }
}
