//! PRIVATE CANDIDATE: circular arcs to cubic pieces, in paper millimetres.
//!
//! Internal proposed signature, not an agreed paper API or a connected writer.
//! The construction is the midpoint/tangent circular cubic used by CamWight's
//! original P1 PDF implementation (MIT OR Apache-2.0). The enclosure code here
//! is original; no third-party implementation is imported.
//!
//! Mathematical context: A. Riskus, Information Technology and Control 35(4),
//! 2006, section3, doi:10.5755/j01.itc.35.4.11812.
//! See the private derivation for the radial polynomial and rounding argument.
//!
//! At beta=abs(sweep)/(4*n), the ideal cubic has radial error at most
//! r*(2/27)*sin(beta)^6/cos(beta)^2. This candidate uses the larger bound
//! r*(2/27)*beta^6/(1-beta^2), so choosing pieces needs no transcendental call.
//! Control points use outward-rounded Taylor enclosures, including explicit
//! Lagrange remainders; their L1 uncertainty bounds the emitted cubic's
//! positional perturbation by Bernstein convexity. No platform trig accuracy
//! assumption is used. Scalar operations require Rust's ordinary IEEE754
//! binary64 round-to-nearest semantics, not a fast-math/flush-to-zero rewrite.
//!
//! This bound excludes later writer-grid snapping and endpoint replacement.
//! A consumer must reserve and qualify those separately. No adapter is supplied.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::paper::{Invalid, MAX_COORDINATE_MM};
use crate::{Seg2, Vec2};

type Result<T> = std::result::Result<T, Invalid>;

/// The inherited P1 per-arc ceiling; caller limits may only tighten it.
pub(crate) const MAX_ARC_CUBICS: usize = 64;

/// A private candidate result, not a serialized record or a prepared scene.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ArcCubics {
    pub(crate) pieces: Vec<Seg2>,
    /// Geometric approximation plus stored-control-point uncertainty, in mm.
    /// Not a writer's gridding/physical-print error or an achieved API promise.
    pub(crate) error_bound_mm: f64,
}

/// Converts one admitted circular arc, with a caller-supplied absolute error
/// allowance in mm. Every output curve is a cubic. More than a quarter turn,
/// starts outside [-PI,PI], non-arcs and hostile fields are explicitly refused.
/// No silent angle normalization, grid policy or input mutation occurs.
pub(crate) fn circular_arc(arc: &Seg2, error_mm: f64, max_pieces: usize) -> Result<ArcCubics> {
    let Seg2::Arc { center, radius, start, sweep } = *arc else {
        return Err(Invalid("cubic arc candidate: only a circular arc is supported"));
    };
    arc.check(MAX_COORDINATE_MM).map_err(Invalid)?;
    if !error_mm.is_finite() || error_mm <= 0.0 {
        return Err(Invalid("cubic arc candidate: the mm error allowance is finite and positive"));
    }
    if !(1..=MAX_ARC_CUBICS).contains(&max_pieces) {
        return Err(Invalid("cubic arc candidate: the piece cap is from1 to64"));
    }
    if !start.is_finite() || start.abs() > PI || !sweep.is_finite() || sweep == 0.0 || sweep.abs() > FRAC_PI_2 {
        return Err(Invalid("cubic arc candidate: start in [-PI,PI], nonzero sweep at most a quarter turn"));
    }

    for n in 1..=max_pieces {
        let h = Enclosure::point(sweep).div(Enclosure::point(n as f64))?;
        let ideal_error = radial_bound(radius, h)?;
        if ideal_error > error_mm {
            continue;
        }
        let mut pieces = Vec::new();
        pieces.try_reserve_exact(n).map_err(|_| Invalid("cubic arc candidate: cannot allocate bounded pieces"))?;
        let mut error_bound_mm: f64 = 0.0;
        for i in 0..n {
            // The same expression evaluates each shared end once per neighbour,
            // yielding bit-identical stored endpoints, with no endpoint snap.
            let angle = |j: usize| {
                Enclosure::point(start).add(
                    Enclosure::point(sweep).mul(Enclosure::point(j as f64).div(Enclosure::point(n as f64))?)?,
                )
            };
            let (cubic, control_error) = make_cubic(center, radius, angle(i)?, angle(i + 1)?, h)?;
            let total = Enclosure::point(ideal_error).add(Enclosure::point(control_error))?.hi;
            error_bound_mm = error_bound_mm.max(total);
            pieces.push(cubic);
        }
        if error_bound_mm <= error_mm {
            return Ok(ArcCubics { pieces, error_bound_mm });
        }
        // Numerical uncertainty is part of the error, never an excuse to use a
        // looser tolerance. A later n may qualify; at the caller cap refuse all.
    }
    Err(Invalid("cubic arc candidate: cannot certify the requested mm error within the piece cap"))
}

/// The exact-real radial bound with sin(beta)<=beta and
/// cos(beta)^2>=1-beta^2, evaluated outwards. The domain is beta<=PI/8.
fn radial_bound(radius: f64, h: Enclosure) -> Result<f64> {
    let beta = Enclosure::point(h.abs_upper()).div(Enclosure::point(4.0))?;
    let squared = beta.mul(beta)?;
    let sixth = squared.mul(squared)?.mul(squared)?;
    let denominator = Enclosure::point(1.0).sub(squared)?;
    let coefficient = Enclosure::point(2.0).div(Enclosure::point(27.0))?;
    let bound = Enclosure::point(radius).mul(coefficient)?.mul(sixth)?.div(denominator)?;
    Ok(bound.hi)
}

fn make_cubic(center: Vec2, radius: f64, a0: Enclosure, a1: Enclosure, h: Enclosure) -> Result<(Seg2, f64)> {
    let (s0, c0) = sin_cos(a0)?;
    let (s1, c1) = sin_cos(a1)?;
    let (sh, ch) = sin_cos(h.div(Enclosure::point(4.0))?)?;
    let r = Enclosure::point(radius);
    let handle = r.mul(Enclosure::point(4.0).div(Enclosure::point(3.0))?)?.mul(sh.div(ch)?)?;
    let cx = Enclosure::point(center.x);
    let cy = Enclosure::point(center.y);
    let x0 = cx.add(r.mul(c0)?)?;
    let y0 = cy.add(r.mul(s0)?)?;
    let x3 = cx.add(r.mul(c1)?)?;
    let y3 = cy.add(r.mul(s1)?)?;
    let (p0, e0) = stored_point(x0, y0)?;
    let (p1, e1) = stored_point(x0.sub(handle.mul(s0)?)?, y0.add(handle.mul(c0)?)?)?;
    let (p2, e2) = stored_point(x3.add(handle.mul(s1)?)?, y3.sub(handle.mul(c1)?)?)?;
    let (p3, e3) = stored_point(x3, y3)?;
    if p0 == p3 || p0 == p1 || p2 == p3 {
        return Err(Invalid("cubic arc candidate: coordinate precision collapses an endpoint or tangent"));
    }
    Ok((Seg2::Cubic { p0, p1, p2, p3 }, e0.max(e1).max(e2).max(e3)))
}

/// Nonnegative Bernstein coefficients sum to1, so the greatest control
/// displacement bounds the complete curve displacement. L1 bounds Euclidean
/// distance without trusting a rounded sqrt.
fn stored_point(x: Enclosure, y: Enclosure) -> Result<(Vec2, f64)> {
    let px = x.midpoint();
    let py = y.midpoint();
    let ex = x.sub(Enclosure::point(px))?.abs_upper();
    let ey = y.sub(Enclosure::point(py))?.abs_upper();
    let error = Enclosure::point(ex).add(Enclosure::point(ey))?.hi;
    Ok((Vec2::new(px, py), error))
}

/// Twenty-four terms, then an explicit Taylor/Lagrange remainder. Input
/// angles stay inside +/-3PI/2; no period reduction or system trig is needed.
/// sin's polynomial has degree47 (degree48 coefficient0), cos's degree46
/// (degree47 coefficient0), giving remainder orders49 and48 respectively.
fn sin_cos(x: Enclosure) -> Result<(Enclosure, Enclosure)> {
    let negative_square = x.mul(x)?.neg();
    let mut sine_term = x;
    let mut cosine_term = Enclosure::point(1.0);
    let mut sine = sine_term;
    let mut cosine = cosine_term;
    for k in 1..24 {
        let twice = (2 * k) as f64;
        sine_term = sine_term.mul(negative_square)?.div(Enclosure::point(twice * (twice + 1.0)))?;
        cosine_term = cosine_term.mul(negative_square)?.div(Enclosure::point((twice - 1.0) * twice))?;
        sine = sine.add(sine_term)?;
        cosine = cosine.add(cosine_term)?;
    }
    let sine_tail = taylor_tail(x.abs_upper(), 49)?;
    let cosine_tail = taylor_tail(x.abs_upper(), 48)?;
    Ok((
        sine.add(Enclosure { lo: -sine_tail, hi: sine_tail })?,
        cosine.add(Enclosure { lo: -cosine_tail, hi: cosine_tail })?,
    ))
}

fn taylor_tail(magnitude: f64, order: usize) -> Result<f64> {
    let mut tail = Enclosure::point(1.0);
    for j in 1..=order {
        tail = tail.mul(Enclosure::point(magnitude))?.div(Enclosure::point(j as f64))?;
    }
    Ok(tail.hi)
}

/// Local private arithmetic for this helper, not a second public interval API.
/// Exact f64 inputs denote single real numbers. Every operation expands each
/// rounded endpoint by one representable neighbour; nonfinite endpoints and
/// division by an interval containing zero fail closed.
#[derive(Clone, Copy, Debug)]
struct Enclosure {
    lo: f64,
    hi: f64,
}

impl Enclosure {
    fn point(value: f64) -> Self {
        Self { lo: value, hi: value }
    }

    fn outward(lo: f64, hi: f64) -> Result<Self> {
        let (lo, hi) = (lo.next_down(), hi.next_up());
        if lo.is_finite() && hi.is_finite() && lo <= hi {
            Ok(Self { lo, hi })
        } else {
            Err(Invalid("cubic arc candidate: arithmetic enclosure unavailable"))
        }
    }

    fn add(self, rhs: Self) -> Result<Self> {
        Self::outward(self.lo + rhs.lo, self.hi + rhs.hi)
    }

    fn neg(self) -> Self {
        Self { lo: -self.hi, hi: -self.lo }
    }

    fn sub(self, rhs: Self) -> Result<Self> {
        self.add(rhs.neg())
    }

    fn mul(self, rhs: Self) -> Result<Self> {
        let (lo, hi) = [self.lo * rhs.lo, self.lo * rhs.hi, self.hi * rhs.lo, self.hi * rhs.hi]
            .into_iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        Self::outward(lo, hi)
    }

    fn div(self, rhs: Self) -> Result<Self> {
        if rhs.lo <= 0.0 && rhs.hi >= 0.0 {
            return Err(Invalid("cubic arc candidate: denominator contains zero"));
        }
        self.mul(Self::outward(1.0 / rhs.hi, 1.0 / rhs.lo)?)
    }

    fn abs_upper(self) -> f64 {
        self.lo.abs().max(self.hi.abs())
    }

    fn midpoint(self) -> f64 {
        (self.lo * 0.5 + self.hi * 0.5).max(self.lo).min(self.hi)
    }
}
