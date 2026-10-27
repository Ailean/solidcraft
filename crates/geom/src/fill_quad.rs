//! Filling an N-sided hole with N four-sided B-spline patches about a centre point, each meeting
//! the faces beside the hole and its two neighbouring patches with G2: the corner where three or
//! more rounds meet, set back so that each round ends at its cross-section.
//!
//! The construction (the hole split into quadrilaterals about a centre: J. Hahn, "Filling
//! polygonal holes with rectangular patches", *Theory and Practice of Geometric Modeling*, 1989;
//! J. A. Gregory and J. Zhou, "Filling polygonal holes with bicubic patches", CAGD 11, 1994; the
//! patches' insides fixed by minimising a fairness energy under the continuity conditions, as in
//! H. P. Moreton and C. H. Séquin, "Functional optimization for fair surface design", SIGGRAPH
//! 1992):
//!
//! - **The split.** The hole's loop has `2N` edges: its `N` sides, each two edges meeting
//!   smoothly at the side's middle `M_k` (a set-back corner's hole is so: SMLib splits each side
//!   there). Patch `k` has the corners `V_k` (where side `k` starts), `M_k`, the centre `C` and
//!   `M_{k−1}`: on the unit square, edge `2k` (`V_k → M_k`) along `v = 0`, spoke `k`
//!   (`M_k → C`) up `u = 1`, spoke `k − 1` (`M_{k−1} → C`) along `v = 1` and edge `2k − 1` run
//!   back (`V_k → M_{k−1}`) up `u = 0`. Each edge is its patch's boundary row exactly — the
//!   hole's own curve, rational where it is (a round's section is a circular arc), in its own
//!   parameter up to a linear rational change (the standard form, end weights one: G. Farin,
//!   *Curves and Surfaces for CAGD*, §13.3), so the edge's pcurve on its patch is a line, or a
//!   rational line.
//! - **The spaces.** A patch's net is in the spline space, each way, that holds its two
//!   opposite sides: the boundary edge (raised to the common degree, keeping its continuity) and
//!   the spoke, whose space `Sp` (uniform, degree `q`) every patch shares. The weights are the
//!   boundary rows' own and one elsewhere, so `S = F / W` with `W` fixed and every derivative of
//!   `S` linear in the control points (NURBS Book §4.4, the quotient rule).
//! - **The unknowns.** `S_k = (F⁰_k + d_k) / W_k`: `F⁰_k` is fixed, the boundary rows and a
//!   straight spoke from each `M` to a first centre, Coons-blended inside (S. A. Coons, 1967);
//!   `d_k` is a spline in `Sp ⊗ Sp` that vanishes on the hole's edges, whose coefficients on a
//!   spoke are that spoke's own (shared by the two patches beside it, so the spoke is one curve
//!   in both) and whose corner at `C` is the centre's (shared by all). The hole's edges never
//!   move; everything else does.
//! - **The conditions**, by least squares at Gauss stations along every edge and spoke, with
//!   the frames frozen at the last solution and the fit repeated (Gauss–Newton):
//!   - G1 to a neighbour: `n·∂S/∂ν = 0` (`ν` across the edge, `n` its normal); a weak Hermite
//!     row `∂S/∂ν = s·w` (`w` the neighbour's leave direction, `s` the edge's speed) keeps the
//!     patch leaving into itself.
//!   - G2 to a neighbour: with G1, the surfaces share the curve, its tangent plane and so the
//!     normal curvature along it and the twist term, and are G2 exactly when their normal
//!     curvatures agree in one direction across it (L. Pegna and F.-E. Wolter, "Geometrical
//!     criteria to guarantee curvature continuity of blend surfaces", J. Mech. Des. 114, 1992,
//!     the linkage curve theorem): `n·∂²S/∂ν² = II(∂S/∂ν, ∂S/∂ν)`, `II` the neighbour's second
//!     fundamental form, `∂S/∂ν` from the last solution.
//!   - Across a spoke, between patches `k` (`∂/∂u` across) and `k + 1` (`∂/∂v` across): both
//!     cross derivatives and the spoke's own tangent in the tangent plane of the last solution
//!     (G1), and the linkage condition between the two patches: `II_k(X, X) = II_{k+1}(X, X)` for
//!     `X = ∂S_k/∂u`, written in patch `k + 1`'s derivatives as `X = α ∂S_{k+1}/∂v + β ∂S/∂t`;
//!     a weak row asks the two cross derivatives to be opposite across the spoke's tangent.
//!   - A small thin-plate energy `∫∫ |S_uu|² + 2|S_uv|² + |S_vv|²` per patch decides what the
//!     conditions leave free.
//! - **The hole's corners.** At `V_k` a round's section meets a face's curve where the round's
//!   own G1-only seam with that face ends. No surface is G2 to both there: the section's own
//!   curvature (1 / r of a round) is the patch's normal curvature along it, and so its normal
//!   curvature across the face's curve, where the face (a plane: 0) asks for its own; and a
//!   patch tangent to a plane along a curve that turns in it (curvature `k_φ`) cannot be G2 to a
//!   round along a section square to that curve — its normal curvature along the round's axis
//!   leaves the round's by about `k_φ·y / r` at `y` from the corner. The G2 rows of both edges at
//!   a corner fade in (a smoothstep) over [`FADE_SHARE`] of their side from it, where the patch
//!   passes from one curvature to the other; G1 is compatible there and held everywhere. The
//!   conflict goes away only when the hole's conditions agree at its corners: a round whose
//!   section eases to the face's curvature at its setback, and a face curve leaving the contact
//!   line without turning (zero curvature) — a property of the set-back rounds, not of the fill.
//! - **Refinement**: `Sp` starts with [`MIN_SPANS`] spans and doubles, up to
//!   [`MAX_QUAD_SPANS`], until every station within the fades' bounds is within the tolerances
//!   (normals and normal curvatures, at the hole's edges and across the spokes); otherwise
//!   "not supported yet", with how close the finest fill came.
//!
//! Input is hostile: sizes are capped and every evaluation checked; nothing here panics.

use crate::Vec3;
use crate::bspline::{HPoint, NurbsSurface, SplineError, SplineResult, invalid};
use crate::fill::Band;
use crate::spline_space::{Cond, Interpolant, Linear, Space, embed};

/// Most corners (patches) of a hole.
pub const MAX_QUAD_CORNERS: usize = 8;
/// Most spans of the spoke space.
pub const MAX_QUAD_SPANS: usize = 16;
/// Fewest spans of the spoke space (the first fill tried).
const MIN_SPANS: usize = 4;
/// The lowest degree of the patches: quartic, so that the curvature conditions are not met by
/// pieces of quadratics.
const MIN_DEGREE: usize = 4;
/// The share of a corner's fade, from the corner, where its G2 rows are not asked at all (the
/// smoothstep runs over the rest).
const FADE_ZERO: f64 = 0.5;
/// Weight of the weak rows (Hermite directions, cross speeds at spokes) against the G1 rows.
const WEAK: f64 = 0.02;
/// Weight of the thin-plate energy against the conditions (each made dimensionless).
const FAIRNESS: f64 = 1e-4;
/// Rounds of freezing the frames at the last solution and solving again.
const ROUNDS: usize = 6;
/// Stations along each edge and spoke where the fill is measured.
const CHECKS: usize = 64;
/// Most unknown points one solve takes (its normal equations are dense).
const MAX_UNKNOWN_POINTS: usize = 20_000;
/// Four-point Gauss–Legendre nodes and weights on `[−1, 1]`.
const GAUSS4: [(f64, f64); 4] = [
    (-0.861_136_311_594_052_6, 0.347_854_845_137_453_8),
    (-0.339_981_043_584_856_3, 0.652_145_154_862_546_2),
    (0.339_981_043_584_856_3, 0.652_145_154_862_546_2),
    (0.861_136_311_594_052_6, 0.347_854_845_137_453_8),
];

/// One edge of the hole, from the loop's vertex before it to the one after, on `[0, 1]`: a
/// B-spline, rational when `weights` is not empty (positive, the first and last one: the standard
/// form, which a caller reaches from any rational piece by a linear rational change of parameter).
#[derive(Clone, Debug, PartialEq)]
pub struct QuadEdge {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub points: Vec<Vec3>,
    pub weights: Vec<f64>,
}

/// What the face beside an edge is at a point of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The face's unit normal (either sign; the form is about it).
    pub normal: Vec3,
    /// The unit direction the face leaves the edge in, away from it (into the hole).
    pub leave: Vec3,
    /// The face's second fundamental form about `normal` on the frame `(t, leave)`, `t` the
    /// edge's unit tangent the loop's way: `[II(t, t), II(t, leave), II(leave, leave)]`.
    pub form: [f64; 3],
}

/// What the fill is asked for.
#[derive(Clone, Copy, Debug)]
pub struct QuadSpec {
    /// How far apart two edges' ends may be where they meet (the record's tolerance: each edge
    /// is its patch's row as it is, so the patches meet there within it).
    pub tol: f64,
    /// How far (radians) a patch's normal may turn from a neighbour's, or from the next patch's.
    pub angle_tol: f64,
    /// How far (1/length) its normal curvature across an edge or a spoke may be from the other
    /// side's, clear of the hole's corners' fades.
    pub curvature_tol: f64,
    /// How far from each corner of the hole (and from the centre) the G2 conditions fade in:
    /// the stretch where the patch passes from one neighbour's curvature to the other's; G2 is
    /// held beyond it.
    pub fade: f64,
}

/// A spoke: from its side's middle `M_k` (t = 0) to the centre (t = 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Spoke {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub points: Vec<Vec3>,
}

/// The worst of one measure along an edge or spoke, and where (a share of it from its start).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Worst {
    pub value: f64,
    pub at: f64,
}

/// What the fill achieves along one edge of the hole or one spoke.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeamFit {
    /// The angle (radians) between the two sides' normals.
    pub angle: Worst,
    /// The difference (1/length) of their normal curvatures across, everywhere, and clear of the
    /// hole's corners' fades.
    pub curvature: Worst,
    pub curvature_clear: Worst,
}

/// The fill: patch `k` on the unit square as the module documentation lays it out.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadFill {
    pub patches: Vec<NurbsSurface>,
    /// Spoke `k` from `M_k` to the centre.
    pub spokes: Vec<Spoke>,
    pub centre: Vec3,
    /// Per edge of the hole, then per spoke: what the fill achieves across it.
    pub edges: Vec<SeamFit>,
    pub spoke_fits: Vec<SeamFit>,
    /// Spans of the spoke space.
    pub spans: usize,
}

/// Fill the hole `edges` (2N of them, N from 3 to [`MAX_QUAD_CORNERS`], each starting where the
/// one before ends; edge `2k` starts at a corner of the hole and meets edge `2k + 1` smoothly)
/// with N patches meeting each edge's neighbour and each other with G2 (module docs).
/// `frames(j, ts)` gives the face beside edge `j` at its parameters `ts`.
pub fn quad_fill(edges: &[QuadEdge], frames: &mut dyn FnMut(usize, &[f64]) -> Result<Vec<Frame>, String>, spec: &QuadSpec) -> SplineResult<QuadFill> {
    let m = edges.len();
    let n = m / 2;
    if !m.is_multiple_of(2) || !(3..=MAX_QUAD_CORNERS).contains(&n) {
        return Err(SplineError::Unsupported(format!("a hole of {m} edges ({} to {} corners, two edges a side)", 3, MAX_QUAD_CORNERS)));
    }
    let positive = |x: f64| x.is_finite() && x > 0.0;
    if !(positive(spec.tol) && positive(spec.angle_tol) && positive(spec.curvature_tol) && positive(spec.fade)) {
        return invalid("the fill's tolerances must be positive");
    }
    let curves: Vec<Curve> = edges.iter().map(Curve::new).collect::<SplineResult<_>>()?;
    for k in 0..m {
        let (Some(a), Some(b)) = (curves.get(k), curves.get((k + 1) % m)) else { return invalid("edges") };
        let gap = a.point(1.0)?.dist(b.point(0.0)?);
        if !(gap <= spec.tol) {
            return invalid(format!("edge {} does not start where edge {k} ends ({gap:e} apart)", (k + 1) % m));
        }
    }
    let q = curves.iter().map(|c| c.space.degree()).max().unwrap_or(1).max(MIN_DEGREE);
    let degree_cap = crate::bspline::MAX_SPLINE_DEGREE;
    if q > degree_cap {
        return Err(SplineError::Unsupported(format!("a hole edge of degree {q}")));
    }
    // The frames along every edge, at the stations each level asks for (cached per parameter).
    let mut cache: Vec<Vec<(f64, Frame)>> = vec![Vec::new(); m];
    let mut frame_at = |j: usize, ts: &[f64]| -> SplineResult<Vec<Frame>> {
        let have = cache.get(j).ok_or_else(|| SplineError::Invalid("edge".into()))?;
        let missing: Vec<f64> = ts.iter().copied().filter(|t| !have.iter().any(|(s, _)| s == t)).collect();
        if !missing.is_empty() {
            let got = frames(j, &missing).map_err(SplineError::Invalid)?;
            if got.len() != missing.len() || !got.iter().all(finite_frame) {
                return invalid(format!("edge {j} has no finite frames"));
            }
            if let Some(slot) = cache.get_mut(j) {
                slot.extend(missing.into_iter().zip(got));
            }
        }
        let have = cache.get(j).ok_or_else(|| SplineError::Invalid("edge".into()))?;
        ts.iter().map(|t| have.iter().find(|(s, _)| s == t).map(|(_, f)| *f).ok_or_else(|| SplineError::Invalid("frame".into()))).collect()
    };
    let lengths: Vec<f64> = curves.iter().map(|c| c.length()).collect::<SplineResult<_>>()?;
    // A first centre: from each side's middle, along the direction its face leaves the hole
    // in, as far as the middles' mean is, averaged — a point near where the fill will pass,
    // not inside the body as the middles' mean of a convex corner is.
    let mut middles = Vec::with_capacity(n);
    for k in 0..n {
        let curve = curves.get(2 * k).ok_or_else(|| SplineError::Invalid("edge".into()))?;
        let f = frame_at(2 * k, &[1.0])?.first().copied().ok_or_else(|| SplineError::Invalid("frame".into()))?;
        middles.push((curve.point(1.0)?, f.leave));
    }
    let mean = middles.iter().fold(Vec3::ZERO, |s, (p, _)| s + *p) * (1.0 / n as f64);
    let centre0 = middles.iter().fold(Vec3::ZERO, |s, (p, l)| s + *p + *l * p.dist(mean)) * (1.0 / n as f64);
    let mut spans = MIN_SPANS;
    let mut best: Option<(f64, String)> = None;
    while spans <= MAX_QUAD_SPANS {
        let quilt = Quilt::new(&curves, q, spans, centre0, spec.fade)?;
        if quilt.unknowns > MAX_UNKNOWN_POINTS {
            break;
        }
        let stations = quilt.stations(&lengths, &mut frame_at)?;
        let x = quilt.solve(&stations)?;
        let fill = quilt.output(&x)?;
        let (edges_fit, spokes_fit) = quilt.measure(&x, &lengths, &mut frame_at)?;
        let worst = |f: &[SeamFit], g: fn(&SeamFit) -> f64| f.iter().map(g).fold(0.0f64, f64::max);
        let angle = worst(&edges_fit, |s| s.angle.value).max(worst(&spokes_fit, |s| s.angle.value));
        let curvature = worst(&edges_fit, |s| s.curvature_clear.value).max(worst(&spokes_fit, |s| s.curvature_clear.value));
        if angle <= spec.angle_tol && curvature <= spec.curvature_tol {
            return Ok(QuadFill { edges: edges_fit, spoke_fits: spokes_fit, spans, ..fill });
        }
        let score = angle / spec.angle_tol + curvature / spec.curvature_tol;
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((
                score,
                format!(
                    "with {spans} spans a side the patches turn {:.4}° from their neighbours (tolerance {:.4}°) and miss their curvature by {curvature:e} (tolerance {:e})",
                    angle.to_degrees(),
                    spec.angle_tol.to_degrees(),
                    spec.curvature_tol
                ),
            ));
        }
        spans *= 2;
    }
    Err(SplineError::Unsupported(format!(
        "filling this hole with patches curvature-continuous to its neighbours and each other: {}",
        best.map(|(_, m)| m).unwrap_or_else(|| "the fill needs more control points than one solve takes".into())
    )))
}

fn finite_frame(f: &Frame) -> bool {
    f.normal.is_finite() && f.leave.is_finite() && f.form.iter().all(|x| x.is_finite())
}

/// An edge as the fill holds it: its space and homogeneous coefficients (`w·P` and `w`).
#[derive(Clone, Debug)]
struct Curve {
    space: Space,
    h: Vec<Vec3>,
    w: Vec<f64>,
}

impl Curve {
    fn new(e: &QuadEdge) -> SplineResult<Curve> {
        let space = Space::new(e.degree, e.knots.clone())?;
        if space.domain() != [0.0, 1.0] {
            return invalid("a hole edge must be on [0, 1]");
        }
        if e.points.len() != space.count() || !e.points.iter().all(|p| p.is_finite()) {
            return invalid("a hole edge's control points do not match its knots");
        }
        let w: Vec<f64> = if e.weights.is_empty() { vec![1.0; e.points.len()] } else { e.weights.clone() };
        if w.len() != e.points.len() || !w.iter().all(|x| x.is_finite() && *x > 0.0) {
            return invalid("a hole edge's weights must be positive, one per control point");
        }
        let ends = [w.first().copied().unwrap_or(0.0), w.last().copied().unwrap_or(0.0)];
        if ends.iter().any(|x| (x - 1.0).abs() > 1e-9) {
            return invalid("a rational hole edge must be in standard form (end weights one)");
        }
        let h = e.points.iter().zip(&w).map(|(p, x)| *p * *x).collect();
        Ok(Curve { space, h, w })
    }

    /// Run backwards (`t ↦ 1 − t`).
    fn reversed(&self) -> SplineResult<Curve> {
        let knots: Vec<f64> = self.space.knots().iter().rev().map(|k| 1.0 - k).collect();
        let mut h = self.h.clone();
        h.reverse();
        let mut w = self.w.clone();
        w.reverse();
        Ok(Curve { space: Space::new(self.space.degree(), knots)?, h, w })
    }

    /// The point and the first two derivatives at `t`.
    fn derivs(&self, t: f64) -> SplineResult<[Vec3; 3]> {
        let mut f = [Vec3::ZERO; 3];
        let mut w = [0.0; 3];
        for d in 0..3 {
            if let (Some(fs), Some(ws)) = (f.get_mut(d), w.get_mut(d)) {
                *fs = self.space.eval(&self.h, t, d)?;
                *ws = self.space.eval(&self.w, t, d)?;
            }
        }
        let [f0, f1, f2] = f;
        let [w0, w1, w2] = w;
        if !(w0 > 0.0) {
            return invalid("a hole edge's weight vanishes");
        }
        let p = f0 * (1.0 / w0);
        let d1 = (f1 - p * w1) * (1.0 / w0);
        let d2 = (f2 - d1 * (2.0 * w1) - p * w2) * (1.0 / w0);
        Ok([p, d1, d2])
    }

    fn point(&self, t: f64) -> SplineResult<Vec3> {
        Ok(self.derivs(t)?[0])
    }

    /// Arc length, by Gauss quadrature on each span.
    fn length(&self) -> SplineResult<f64> {
        let mut s = 0.0;
        for w in breaks(&self.space).windows(2) {
            let (a, b) = (w[0], w[1]);
            for (x, g) in GAUSS4 {
                let t = 0.5 * (a + b) + 0.5 * (b - a) * x;
                s += 0.5 * (b - a) * g * self.derivs(t)?[1].len();
            }
        }
        Ok(s)
    }

    /// The arc length from `t = 0` to `t`.
    fn length_to(&self, t: f64) -> SplineResult<f64> {
        let mut s = 0.0;
        for w in breaks(&self.space).windows(2) {
            let (a, b) = (w[0], w[1].min(t));
            if b <= a {
                break;
            }
            for (x, g) in GAUSS4 {
                let u = 0.5 * (a + b) + 0.5 * (b - a) * x;
                s += 0.5 * (b - a) * g * self.derivs(u)?[1].len();
            }
        }
        Ok(s)
    }
}

/// A space's distinct knots, its ends included.
fn breaks(s: &Space) -> Vec<f64> {
    let mut out = s.knots().to_vec();
    out.dedup();
    out
}

/// Each distinct knot of a space with how often it repeats inside (0 at the ends).
fn multiplicities(s: &Space) -> Vec<(f64, usize)> {
    let [a, b] = s.domain();
    let mut out: Vec<(f64, usize)> = Vec::new();
    for &k in s.knots() {
        if k == a || k == b {
            continue;
        }
        match out.last_mut() {
            Some((x, c)) if *x == k => *c += 1,
            _ => out.push((k, 1)),
        }
    }
    out
}

/// The space of degree `q` holding a spline of `curve` (its continuity at each knot kept) and
/// every spline of `sp`.
fn union_space(q: usize, curve: &Space, sp: &Space) -> SplineResult<Space> {
    let raise = q - curve.degree().min(q);
    let mut all: Vec<(f64, usize)> = multiplicities(curve).into_iter().map(|(x, c)| (x, (c + raise).min(q))).collect();
    for (x, c) in multiplicities(sp) {
        match all.iter_mut().find(|(y, _)| *y == x) {
            Some((_, d)) => *d = (*d).max(c),
            None => all.push((x, c)),
        }
    }
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut knots = vec![0.0; q + 1];
    for (x, c) in all {
        knots.extend(std::iter::repeat_n(x, c));
    }
    knots.extend(std::iter::repeat_n(1.0, q + 1));
    Space::new(q, knots)
}

/// The Greville abscissae of a space.
fn greville(s: &Space) -> Vec<f64> {
    let (p, k) = (s.degree(), s.knots());
    (0..s.count()).map(|i| (1..=p).map(|r| k.get(i + r).copied().unwrap_or(0.0)).sum::<f64>() / p as f64).collect()
}

/// The coefficients in `to` of the spline `ctrl` of `from`, which `to` contains: [`embed`] where
/// it applies (the same degree, or one polynomial piece), else interpolation at `to`'s Greville
/// abscissae, which reproduces a spline of the space exactly (Schoenberg–Whitney), checked.
fn carry<T: Linear>(from: &Space, ctrl: &[T], to: &Space, norm: fn(T) -> f64) -> SplineResult<Vec<T>> {
    match embed(from, ctrl, to) {
        Ok(x) => return Ok(x),
        Err(SplineError::Unsupported(_)) => {}
        Err(e) => return Err(e),
    }
    let g = greville(to);
    let conds: Vec<Cond> = g.iter().map(|t| Cond { t: *t, der: 0 }).collect();
    let data: Vec<T> = g.iter().map(|t| from.eval(ctrl, *t, 0)).collect::<SplineResult<_>>()?;
    let out = Interpolant::new(to.clone(), conds)?.fit(&data)?;
    // Exact up to rounding: the two agree everywhere.
    let scale = ctrl.iter().map(|c| norm(*c)).fold(0.0f64, f64::max).max(1.0);
    for w in breaks(to).windows(2) {
        for f in [0.25, 0.5, 0.75] {
            let t = w[0] + (w[1] - w[0]) * f;
            let miss = norm(from.eval(ctrl, t, 0)?.plus(to.eval(&out, t, 0)?.times(-1.0)));
            if !(miss <= 1e-10 * scale) {
                return invalid(format!("raising a hole edge's degree missed it by {miss:e}"));
            }
        }
    }
    Ok(out)
}

/// The smoothstep `3x² − 2x³` of `share / FADE_SHARE` (1 from [`FADE_SHARE`] on).
fn fade(reach: f64) -> f64 {
    let t = ((reach - FADE_ZERO) / (1.0 - FADE_ZERO)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far a station is into its corner's fade: its distance from the corner over the fade's
/// length.
fn reach(from_corner: f64, fade_length: f64) -> f64 {
    if fade_length > 0.0 { from_corner / fade_length } else { f64::INFINITY }
}

/// The value and derivatives `[f, f_u, f_v, f_uu, f_uv, f_vv]` of `F / W` from those of `F` and
/// `W` (the quotient rule, NURBS Book eq. 4.20).
fn quotient<T: Linear>(f: [T; 6], w: [f64; 6]) -> [T; 6] {
    let [f0, fu, fv, fuu, fuv, fvv] = f;
    let [w0, wu, wv, wuu, wuv, wvv] = w;
    let r = 1.0 / w0;
    let s0 = f0.times(r);
    let su = fu.plus(s0.times(-wu)).times(r);
    let sv = fv.plus(s0.times(-wv)).times(r);
    let suu = fuu.plus(su.times(-2.0 * wu)).plus(s0.times(-wuu)).times(r);
    let suv = fuv.plus(su.times(-wv)).plus(sv.times(-wu)).plus(s0.times(-wuv)).times(r);
    let svv = fvv.plus(sv.times(-2.0 * wv)).plus(s0.times(-wvv)).times(r);
    [s0, su, sv, suu, suv, svv]
}

/// The orders `(du, dv)` of the six derivatives a jet holds.
const ORDERS: [(usize, usize); 6] = [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)];

/// A patch's value and derivatives at a point (`[S, S_u, S_v, S_uu, S_uv, S_vv]`) as the fixed
/// part plus a linear combination of the unknown points.
struct Jet {
    fixed: [Vec3; 6],
    terms: Vec<(usize, [f64; 6])>,
}

impl Jet {
    /// The derivatives at the unknowns' values `x`.
    fn at(&self, x: &[Vec3]) -> [Vec3; 6] {
        let mut out = self.fixed;
        for (m, c) in &self.terms {
            let p = x.get(*m).copied().unwrap_or(Vec3::ZERO);
            for (o, k) in out.iter_mut().zip(c) {
                *o += p * *k;
            }
        }
        out
    }
}

/// The basis functions' values and first two derivatives at `t`: the first index and, per
/// function, `[N, N', N'']`.
fn basis3(s: &Space, t: f64) -> SplineResult<(usize, Vec<[f64; 3]>)> {
    let (first, b0) = s.basis(t, 0)?;
    let (_, b1) = s.basis(t, 1)?;
    let (_, b2) = s.basis(t, 2)?;
    Ok((first, b0.iter().zip(&b1).zip(&b2).map(|((a, b), c)| [*a, *b, *c]).collect()))
}

/// One patch's fixed part and how its unknowns enter.
struct Patch {
    uspace: Space,
    vspace: Space,
    /// The fixed net `w·P⁰` and its weights, u fastest.
    h0: Vec<Vec3>,
    w: Vec<f64>,
    /// Per coefficient of `Sp ⊗ Sp` (u fastest): its unknown, `None` where it is zero.
    dof: Vec<Option<usize>>,
    /// How an unknown of `Sp ⊗ Sp` enters `F = Σ N_i N_j w_ij P_ij`: per basis function `a` of
    /// `Sp`, the coefficients in the patch's spaces of `Σ_i N_i (ω_i − 1) E_ai` and
    /// `Σ_i N_i e_i E_ai` along u (`E` the embedding of `Sp`, `ω` the boundary row's weights, `e`
    /// the first basis function of `Sp` there), and the same along v.
    fu: Vec<Vec<f64>>,
    gu: Vec<Vec<f64>>,
    fv: Vec<Vec<f64>>,
    gv: Vec<Vec<f64>>,
}

/// The fill's patches and unknowns for one spoke space.
struct Quilt {
    n: usize,
    sp: Space,
    patches: Vec<Patch>,
    unknowns: usize,
    /// The edges (loop order), and the fill's first centre.
    curves: Vec<Curve>,
    centre0: Vec3,
    /// Each spoke's start `M_k`.
    middles: Vec<Vec3>,
    /// The corners' fade length ([`QuadSpec::fade`]).
    fade: f64,
}

/// A station: where, and what is asked there.
enum Station {
    /// Edge `edge` at its parameter `t`, which is patch `patch`'s side `v = 0` (`along_u`) or
    /// `u = 0`, at the patch parameter `s`; the face's frame there (normal turned to the patches'
    /// side), the edge's unit tangent the loop's way, the integration weight and the G2 fade.
    /// `corner`: the other edge's tangent at the hole's corner this edge's side starts at, and how
    /// much more its face curves along it there than this edge's face does.
    Edge { patch: usize, along_u: bool, s: f64, frame: Frame, tangent: Vec3, weight: f64, g2: f64, corner: (Vec3, f64) },
    /// Spoke `spoke` at `t`, with its integration weight.
    Spoke { spoke: usize, t: f64, weight: f64 },
}

impl Quilt {
    fn new(curves: &[Curve], q: usize, spans: usize, centre0: Vec3, fade: f64) -> SplineResult<Quilt> {
        let m = curves.len();
        let n = m / 2;
        // Breaks closer together towards both ends (Chebyshev–Lobatto points), where the
        // patches' corners are: the hole's corners, the sides' middles and the centre.
        let sp = Space::with_breaks(
            q,
            &(0..=spans)
                .map(|i| {
                    let x = i as f64 / spans as f64;
                    0.5 * (1.0 - (std::f64::consts::PI * x).cos())
                })
                .map(|x| x.clamp(0.0, 1.0))
                .collect::<Vec<_>>(),
        )?;
        let c = sp.count();
        let middles: Vec<Vec3> =
            (0..n).map(|k| curves.get(2 * k).ok_or_else(|| SplineError::Invalid("edge".into()))?.point(1.0)).collect::<SplineResult<_>>()?;
        // Unknowns: the centre, each spoke's inner coefficients, each patch's interior.
        let inner = c - 2;
        let spoke_id = |k: usize, j: usize| 1 + k * inner + (j - 1);
        let patch_id = |k: usize, i: usize, j: usize| 1 + n * inner + k * inner * inner + (j - 1) * inner + (i - 1);
        let unknowns = 1 + n * inner + n * inner * inner;
        let mut patches = Vec::with_capacity(n);
        for k in 0..n {
            let row = curves.get(2 * k).ok_or_else(|| SplineError::Invalid("edge".into()))?.clone();
            let col = curves.get((2 * k + m - 1) % m).ok_or_else(|| SplineError::Invalid("edge".into()))?.reversed()?;
            let uspace = union_space(q, &row.space, &sp)?;
            let vspace = union_space(q, &col.space, &sp)?;
            let norm3 = |p: Vec3| p.len();
            let norm1 = |x: f64| x.abs();
            let rh = carry(&row.space, &row.h, &uspace, norm3)?;
            let rw = carry(&row.space, &row.w, &uspace, norm1)?;
            let ch = carry(&col.space, &col.h, &vspace, norm3)?;
            let cw = carry(&col.space, &col.w, &vspace, norm1)?;
            let (cu, cv) = (uspace.count(), vspace.count());
            let rp: Vec<Vec3> = rh.iter().zip(&rw).map(|(h, w)| *h * (1.0 / *w)).collect();
            let cp: Vec<Vec3> = ch.iter().zip(&cw).map(|(h, w)| *h * (1.0 / *w)).collect();
            let (gu, gv) = (greville(&uspace), greville(&vspace));
            let c = sp.count();
            let unit = |a: usize| (0..c).map(|i| if i == a { 1.0 } else { 0.0 }).collect::<Vec<f64>>();
            let eu_all: Vec<Vec<f64>> = (0..c).map(|a| embed(&sp, &unit(a), &uspace)).collect::<SplineResult<_>>()?;
            let ev_all: Vec<Vec<f64>> = (0..c).map(|a| embed(&sp, &unit(a), &vspace)).collect::<SplineResult<_>>()?;
            let (eu, ev) = (eu_all.first().cloned().unwrap_or_default(), ev_all.first().cloned().unwrap_or_default());
            let fu: Vec<Vec<f64>> =
                eu_all.iter().map(|e| e.iter().enumerate().map(|(i, x)| (rw.get(i).copied().unwrap_or(1.0) - 1.0) * x).collect()).collect();
            let gu_: Vec<Vec<f64>> =
                eu_all.iter().map(|e| e.iter().enumerate().map(|(i, x)| eu.get(i).copied().unwrap_or(0.0) * x).collect()).collect();
            let fv: Vec<Vec<f64>> =
                ev_all.iter().map(|e| e.iter().enumerate().map(|(j, x)| ev.get(j).copied().unwrap_or(0.0) * x).collect()).collect();
            let gv_: Vec<Vec<f64>> =
                ev_all.iter().map(|e| e.iter().enumerate().map(|(j, x)| (cw.get(j).copied().unwrap_or(1.0) - 1.0) * x).collect()).collect();
            let mk = middles.get(k).copied().unwrap_or(Vec3::ZERO);
            let mprev = middles.get((k + n - 1) % n).copied().unwrap_or(Vec3::ZERO);
            let get = |a: &[Vec3], i: usize| a.get(i).copied().unwrap_or(Vec3::ZERO);
            let at = |a: &[f64], i: usize| a.get(i).copied().unwrap_or(0.0);
            // The four sides of the fixed net: the edges, and straight spokes to the first centre.
            let bottom = |i: usize| get(&rp, i);
            let left = |j: usize| get(&cp, j);
            let right = |j: usize| mk + (centre0 - mk) * at(&gv, j);
            let top = |i: usize| mprev + (centre0 - mprev) * at(&gu, i);
            let (p00, p10, p01, p11) = (bottom(0), bottom(cu - 1), left(cv - 1), centre0);
            let mut h0 = Vec::with_capacity(cu * cv);
            let mut w = Vec::with_capacity(cu * cv);
            for j in 0..cv {
                for i in 0..cu {
                    let (x, y) = (at(&gu, i), at(&gv, j));
                    let p = match (i, j) {
                        (_, 0) => bottom(i),
                        (0, _) => left(j),
                        (i, _) if i + 1 == cu => right(j),
                        (_, j) if j + 1 == cv => top(i),
                        _ => {
                            let ruled_v = bottom(i) * (1.0 - y) + top(i) * y;
                            let ruled_u = left(j) * (1.0 - x) + right(j) * x;
                            let bilinear = (p00 * (1.0 - x) + p10 * x) * (1.0 - y) + (p01 * (1.0 - x) + p11 * x) * y;
                            ruled_v + ruled_u - bilinear
                        }
                    };
                    // The edges' weights, eased to one over the first span of `Sp` (the first
                    // basis function of `Sp` in each direction): one on the spokes and inside.
                    let weight = 1.0 + (at(&rw, i) - 1.0) * at(&ev, j) + (at(&cw, j) - 1.0) * at(&eu, i);
                    if !(weight > 0.0) {
                        return invalid("a patch's weights do not stay positive");
                    }
                    h0.push(p * weight);
                    w.push(weight);
                }
            }
            let prev = (k + n - 1) % n;
            let mut dof = vec![None; c * c];
            for j in 1..c {
                for i in 1..c {
                    let id = match (i + 1 == c, j + 1 == c) {
                        (true, true) => 0,
                        (true, false) => spoke_id(k, j),
                        (false, true) => spoke_id(prev, i),
                        (false, false) => patch_id(k, i, j),
                    };
                    if let Some(slot) = dof.get_mut(i + c * j) {
                        *slot = Some(id);
                    }
                }
            }
            patches.push(Patch { uspace, vspace, h0, w, dof, fu, gu: gu_, fv, gv: gv_ });
        }
        Ok(Quilt { n, sp, patches, unknowns, curves: curves.to_vec(), centre0, middles, fade })
    }

    /// Patch `k`'s jet at `(u, v)`.
    fn jet(&self, k: usize, u: f64, v: f64) -> SplineResult<Jet> {
        let p = self.patches.get(k).ok_or_else(|| SplineError::Invalid("patch".into()))?;
        let (iu, bu) = basis3(&p.uspace, u)?;
        let (iv, bv) = basis3(&p.vspace, v)?;
        let cu = p.uspace.count();
        let mut f = [Vec3::ZERO; 6];
        let mut w = [0.0; 6];
        for (b, nv) in bv.iter().enumerate() {
            for (a, nu) in bu.iter().enumerate() {
                let idx = (iv + b) * cu + iu + a;
                let (Some(h), Some(x)) = (p.h0.get(idx), p.w.get(idx)) else { return invalid("net") };
                for (d, (du, dv)) in ORDERS.iter().enumerate() {
                    let c = nu.get(*du).copied().unwrap_or(0.0) * nv.get(*dv).copied().unwrap_or(0.0);
                    if let (Some(fs), Some(ws)) = (f.get_mut(d), w.get_mut(d)) {
                        *fs += *h * c;
                        *ws += x * c;
                    }
                }
            }
        }
        if !(w.first().copied().unwrap_or(0.0) > 0.0) {
            return invalid("a patch's weight vanishes");
        }
        let fixed = quotient(f, w);
        let c = self.sp.count();
        let (su, du) = basis3(&self.sp, u)?;
        let (sv, dv) = basis3(&self.sp, v)?;
        // A 1D spline of the patch's space along u (v) at the point, from its coefficients.
        let along = |coef: &[f64], first: usize, basis: &[[f64; 3]]| -> [f64; 3] {
            let mut out = [0.0; 3];
            for (k, nb) in basis.iter().enumerate() {
                let x = coef.get(first + k).copied().unwrap_or(0.0);
                for (o, b) in out.iter_mut().zip(nb) {
                    *o += x * b;
                }
            }
            out
        };
        let mut terms = Vec::with_capacity(du.len() * dv.len());
        for (b, nv) in dv.iter().enumerate() {
            let (fvb, gvb) = (
                along(p.fv.get(sv + b).map(|x| x.as_slice()).unwrap_or(&[]), iv, &bv),
                along(p.gv.get(sv + b).map(|x| x.as_slice()).unwrap_or(&[]), iv, &bv),
            );
            for (a, nu) in du.iter().enumerate() {
                let Some(Some(id)) = p.dof.get((sv + b) * c + su + a) else { continue };
                let (fua, gua) = (
                    along(p.fu.get(su + a).map(|x| x.as_slice()).unwrap_or(&[]), iu, &bu),
                    along(p.gu.get(su + a).map(|x| x.as_slice()).unwrap_or(&[]), iu, &bu),
                );
                let mut fc = [0.0; 6];
                for (d, (ou, ov)) in ORDERS.iter().enumerate() {
                    let g = |x: &[f64; 3], y: &[f64; 3]| x.get(*ou).copied().unwrap_or(0.0) * y.get(*ov).copied().unwrap_or(0.0);
                    if let Some(slot) = fc.get_mut(d) {
                        *slot = nu.get(*ou).copied().unwrap_or(0.0) * nv.get(*ov).copied().unwrap_or(0.0) + g(&fua, &fvb) + g(&gua, &gvb);
                    }
                }
                terms.push((*id, quotient(fc, w)));
            }
        }
        Ok(Jet { fixed, terms })
    }

    /// The stations: Gauss points of every span of each edge's patch space, and of each spoke.
    fn stations(&self, lengths: &[f64], frame_at: &mut dyn FnMut(usize, &[f64]) -> SplineResult<Vec<Frame>>) -> SplineResult<Vec<Station>> {
        let n = self.n;
        let m = 2 * n;
        let mut out = Vec::new();
        // The patches' natural normal against the frames' at a first station: which way the
        // frames' normals are turned.
        let sides = self.frames_side(frame_at)?;
        for k in 0..n {
            let p = self.patches.get(k).ok_or_else(|| SplineError::Invalid("patch".into()))?;
            for along_u in [true, false] {
                let (edge, space) = if along_u { (2 * k, &p.uspace) } else { ((2 * k + m - 1) % m, &p.vspace) };
                let curve = self.curves.get(edge).ok_or_else(|| SplineError::Invalid("edge".into()))?;
                let mut ss = Vec::new();
                let mut ws = Vec::new();
                for w in breaks(space).windows(2) {
                    let (a, b) = (w[0], w[1]);
                    for (x, g) in GAUSS4 {
                        ss.push(0.5 * (a + b) + 0.5 * (b - a) * x);
                        ws.push(0.5 * (b - a) * g);
                    }
                }
                // The hole's corner V_k: the curvature the other edge's face has along that edge
                // there, against this edge's face's (`delta`), and that edge's tangent.
                let other = if along_u { (2 * k + m - 1) % m } else { 2 * k };
                let (here, there) = if along_u { (0.0, 1.0) } else { (1.0, 0.0) };
                let corner = {
                    let oc = self.curves.get(other).ok_or_else(|| SplineError::Invalid("edge".into()))?;
                    let to = oc.derivs(there)?[1].normalized().ok_or_else(|| SplineError::Invalid("a hole edge stops".into()))?;
                    let fo = frame_at(other, &[there])?.first().copied().ok_or_else(|| SplineError::Invalid("frame".into()))?;
                    let fh = frame_at(edge, &[here])?.first().copied().ok_or_else(|| SplineError::Invalid("frame".into()))?;
                    let th = curve.derivs(here)?[1].normalized().ok_or_else(|| SplineError::Invalid("a hole edge stops".into()))?;
                    let (so, sh) = (sides.get(other).copied().unwrap_or(1.0), sides.get(edge).copied().unwrap_or(1.0));
                    let (a, b) = (to.dot(th), to.dot(fh.leave));
                    let mine = sh * (a * a * fh.form[0] + 2.0 * a * b * fh.form[1] + b * b * fh.form[2]);
                    (to, so * fo.form[0] - mine)
                };
                // The edge's own parameter: the patch's along `v = 0`, run back up `u = 0`.
                let ts: Vec<f64> = ss.iter().map(|s| if along_u { *s } else { 1.0 - s }).collect();
                let frames = frame_at(edge, &ts)?;
                for ((s, t), (f, wt)) in ss.iter().zip(&ts).zip(frames.iter().zip(&ws)) {
                    let [_, d1, _] = curve.derivs(*t)?;
                    let tangent = d1.normalized().ok_or_else(|| SplineError::Invalid("a hole edge stops".into()))?;
                    // Distance from the corner `V_k` along the side, as a share of it.
                    let from_corner = if along_u { curve.length_to(*t)? } else { lengths.get(edge).copied().unwrap_or(0.0) - curve.length_to(*t)? };
                    let share = reach(from_corner, self.fade);
                    let side = sides.get(edge).copied().unwrap_or(1.0);
                    // The face's normal turned square to the edge the patch holds: a record's
                    // edge lies in its face's tangent plane only to the record's tolerance (an
                    // approximated section turns out of it by a hundredth of a degree), and no
                    // surface through the edge has a normal that is not square to it.
                    let normal = (f.normal - tangent * f.normal.dot(tangent)).normalized().unwrap_or(f.normal) * side;
                    let leave = (f.leave - tangent * f.leave.dot(tangent) - normal * f.leave.dot(normal)).normalized().unwrap_or(f.leave);
                    let frame = Frame { normal, leave, form: f.form.map(|x| x * side) };
                    out.push(Station::Edge { patch: k, along_u, s: *s, frame, tangent, weight: *wt, g2: fade(share), corner });
                }
            }
            // Spoke k: its stations on the breaks of both patches' spaces along it.
            let next = self.patches.get((k + 1) % n).ok_or_else(|| SplineError::Invalid("patch".into()))?;
            let mut all = breaks(&p.vspace);
            all.extend(breaks(&next.uspace));
            all.sort_by(f64::total_cmp);
            all.dedup();
            for w in all.windows(2) {
                let (a, b) = (w[0], w[1]);
                for (x, g) in GAUSS4 {
                    out.push(Station::Spoke { spoke: k, t: 0.5 * (a + b) + 0.5 * (b - a) * x, weight: 0.5 * (b - a) * g });
                }
            }
        }
        Ok(out)
    }

    /// Per edge, +1 when the face's normal as the frames give it lies on the side of the
    /// patches' natural normal, −1 when not. Along an edge the patch's natural normal is the
    /// edge's tangent (the loop's way) × the direction into the hole, whichever side of the
    /// square the edge is: so the sign is the frame's `normal · (t × leave)` at the edge's middle
    /// (the leave direction, away from the face, does not depend on the normal's sign).
    fn frames_side(&self, frame_at: &mut dyn FnMut(usize, &[f64]) -> SplineResult<Vec<Frame>>) -> SplineResult<Vec<f64>> {
        (0..2 * self.n)
            .map(|j| {
                let curve = self.curves.get(j).ok_or_else(|| SplineError::Invalid("edge".into()))?;
                let [_, d1, _] = curve.derivs(0.5)?;
                let f = frame_at(j, &[0.5])?;
                Ok(if f.first().map(|f| f.normal.dot(d1.cross(f.leave))).unwrap_or(0.0) >= 0.0 { 1.0 } else { -1.0 })
            })
            .collect()
    }

    /// Solve the rounds: freeze the frames at the last solution (the fixed nets at first), fit.
    fn solve(&self, stations: &[Station]) -> SplineResult<Vec<Vec3>> {
        let mut x = vec![Vec3::ZERO; self.unknowns];
        let size = self.size();
        // Which unknowns share a row, once: their order for a narrow band.
        let jets: Vec<(Jet, Option<Jet>)> = stations
            .iter()
            .map(|st| match st {
                Station::Edge { patch, along_u, s, .. } => {
                    let (u, v) = if *along_u { (*s, 0.0) } else { (0.0, *s) };
                    Ok((self.jet(*patch, u, v)?, None))
                }
                Station::Spoke { spoke, t, .. } => Ok((self.jet(*spoke, 1.0, *t)?, Some(self.jet((*spoke + 1) % self.n, *t, 1.0)?))),
            })
            .collect::<SplineResult<_>>()?;
        let mut pattern = Normal::pattern(self.unknowns)?;
        self.assemble(&mut pattern, stations, &jets, &x, false)?;
        for k in 0..self.n {
            self.energy(k, &mut pattern, size)?;
        }
        let order = pattern.ordering()?;
        // The energy does not change from round to round: assembled once.
        let mut base = Normal::banded(&order, &self.patches)?;
        for k in 0..self.n {
            self.energy(k, &mut base, size)?;
        }
        for round in 0..ROUNDS {
            // The first round has no frames of its own to freeze: it fits the edges' G1 and
            // the spokes' parametric tangency (opposite cross derivatives) under the energy,
            // a fair start; the rounds after it ask for G2, and for G1 across the spokes
            // geometrically, about that start's frames.
            let first = round == 0;
            let mut ne = base.clone();
            self.assemble(&mut ne, stations, &jets, &x, first)?;
            x = ne.solve()?;
        }
        Ok(x)
    }

    /// The rows of one round about the last solution `x` (`first`: the first round's).
    fn assemble(&self, ne: &mut Normal, stations: &[Station], jets: &[(Jet, Option<Jet>)], x: &[Vec3], first: bool) -> SplineResult<()> {
        let size = self.size();
        for (st, (ja, jb)) in stations.iter().zip(jets) {
            match st {
                Station::Edge { along_u, frame, tangent, weight, g2, corner, .. } => {
                    let jet = ja;
                    let now = jet.at(x);
                    let (cross, second, along) = if *along_u { (2, 5, 1) } else { (1, 3, 2) };
                    let xnu = now[cross];
                    let speed = xnu.len().max(1e-12 * size);
                    let wt = weight.sqrt();
                    // G1.
                    ne.dot(jet, &[(cross, 1.0)], frame.normal, 0.0, wt / speed);
                    // Leaving into the patch, about as fast as the edge runs.
                    // (First round only: after it the energy decides what the conditions leave
                    // free.)
                    if first {
                        let run = now[along].len();
                        ne.vec(jet, &[(cross, 1.0)], frame.leave * run, WEAK * wt / speed);
                    }
                    // G2: the face's normal curvature in the patch's cross direction.
                    if *g2 > 0.0 && !first {
                        let (a, b) = (xnu.dot(*tangent), xnu.dot(frame.leave));
                        // The face's own, and near the corner the other face's curvature along
                        // the other edge, which the patch has there (it holds both edges):
                        // compatible targets, the difference fading out with the weights.
                        let (to, delta) = *corner;
                        let ii =
                            a * a * frame.form[0] + 2.0 * a * b * frame.form[1] + b * b * frame.form[2] + (1.0 - g2) * delta * xnu.dot(to).powi(2);
                        // `n·S_νν = II(S_ν, S_ν)` linearised about the last `S_ν = X`:
                        // `n·S_νν − 2 II(X, S_ν) = −II(X, X)`, `II(X, ·)` the tangent vector `g`.
                        let g = *tangent * (a * frame.form[0] + b * frame.form[1])
                            + frame.leave * (a * frame.form[1] + b * frame.form[2])
                            + to * ((1.0 - g2) * delta * xnu.dot(to));
                        ne.rows(&[(jet, second, frame.normal), (jet, cross, g * -2.0)], -ii, g2 * wt * size / (speed * speed));
                    }
                }
                Station::Spoke { spoke, t, weight } => {
                    let k = *spoke;
                    let (a, Some(b)) = (ja, jb) else { return invalid("a spoke station's jets") };
                    let (na, nb) = (a.at(x), b.at(x));
                    // Fixed α, β at first; then the last solution's own (its B-cross derivative
                    // in the plane of its A-cross derivative and the spoke's tangent), which
                    // leaves the iteration free to settle them.
                    let (alpha, beta) = match (first, frozen_ab(&na, &nb)) {
                        (false, Some(ab)) => ab,
                        _ => self.reparameterisation(k, *t)?,
                    };
                    let speed = na[1].len().max(nb[2].len()).max(1e-12 * size);
                    let wt = weight.sqrt();
                    // G1: `∂S_{k+1}/∂v = −α ∂S_k/∂u + β ∂S/∂t`, the two patches' cross
                    // derivatives and the spoke in one plane, with fixed α, β.
                    for axis in [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)] {
                        ne.pair(a, &[(1, alpha), (2, -beta)], b, &[(2, 1.0)], axis, 0.0, wt / speed);
                    }
                    if first {
                        continue;
                    }
                    // G2: with `X = ∂S_k/∂u = (β ∂S/∂t − ∂S_{k+1}/∂v) / α`, II_k(X, X) =
                    // II_{k+1}(X, X): `α² n·S_k,uu = n·S_{k+1},vv − 2β n·S_{k+1},uv + β² n·S_uu`
                    // about the last solution's normal.
                    let normal = (na[1].cross(na[2]).normalized().unwrap_or(Vec3::ZERO) + nb[1].cross(nb[2]).normalized().unwrap_or(Vec3::ZERO))
                        .normalized()
                        .unwrap_or(Vec3::ZERO);
                    ne.pair(a, &[(3, alpha * alpha)], b, &[(5, -1.0), (4, 2.0 * beta), (3, -beta * beta)], normal, 0.0, wt * size / (speed * speed));
                }
            }
        }
        Ok(())
    }

    /// The G1 relation's scalars across spoke `k` at `t`: `∂S_{k+1}/∂v = −α ∂S_k/∂u + β ∂S/∂t`.
    /// At the side's middle (t = 0) both cross derivatives run along the side, so α is the
    /// ratio of the two halves' speeds there and β is 0; at the centre α is 1 and β is
    /// `2 cos(2π/N)`, the relation of N patches about a regular centre (the spokes' tangents
    /// there an affine image of a regular star); linear between.
    fn reparameterisation(&self, k: usize, t: f64) -> SplineResult<(f64, f64)> {
        let a = self.jet(k, 1.0, 0.0)?.fixed;
        let b = self.jet((k + 1) % self.n, 0.0, 1.0)?.fixed;
        let (sa, sb) = (a[1].len(), b[2].len());
        if !(sa > 0.0 && sb > 0.0) {
            return invalid("a hole edge stops at its side's middle");
        }
        let alpha0 = sb / sa;
        let beta1 = 2.0 * (std::f64::consts::TAU / self.n as f64).cos();
        Ok((alpha0 + (1.0 - alpha0) * t, beta1 * t))
    }

    /// The fill's size: the largest distance between two corners of the hole.
    fn size(&self) -> f64 {
        let pts: Vec<Vec3> = self.curves.iter().filter_map(|c| c.point(0.0).ok()).collect();
        let mut s = 0.0f64;
        for a in &pts {
            for b in &pts {
                s = s.max(a.dist(*b));
            }
        }
        s.max(1e-9)
    }

    /// Patch `k`'s thin-plate energy, by Gauss quadrature on every cell of its spaces.
    fn energy(&self, k: usize, ne: &mut Normal, size: f64) -> SplineResult<()> {
        let p = self.patches.get(k).ok_or_else(|| SplineError::Invalid("patch".into()))?;
        let (bu, bv) = (breaks(&p.uspace), breaks(&p.vspace));
        // Over the size squared, so that its rows weigh as the (dimensionless) conditions' do
        // at any scale.
        let lambda = FAIRNESS / (size * size);
        for wu in bu.windows(2) {
            for wv in bv.windows(2) {
                let (hu, hv) = (wu[1] - wu[0], wv[1] - wv[0]);
                for (xu, gu) in GAUSS4 {
                    for (xv, gv) in GAUSS4 {
                        let u = 0.5 * (wu[0] + wu[1]) + 0.5 * hu * xu;
                        let v = 0.5 * (wv[0] + wv[1]) + 0.5 * hv * xv;
                        let jet = self.jet(k, u, v)?;
                        let wt = (lambda * 0.25 * hu * hv * gu * gv).sqrt();
                        ne.vec(&jet, &[(3, 1.0)], Vec3::ZERO, wt);
                        ne.vec(&jet, &[(4, 1.0)], Vec3::ZERO, wt * std::f64::consts::SQRT_2);
                        ne.vec(&jet, &[(5, 1.0)], Vec3::ZERO, wt);
                    }
                }
            }
        }
        Ok(())
    }

    /// The patches and spokes for the unknowns `x`.
    fn output(&self, x: &[Vec3]) -> SplineResult<QuadFill> {
        let c = self.sp.count();
        let value = |id: Option<usize>| id.and_then(|i| x.get(i)).copied().unwrap_or(Vec3::ZERO);
        let mut patches = Vec::with_capacity(self.n);
        for p in &self.patches {
            let (cu, cv) = (p.uspace.count(), p.vspace.count());
            // The unknowns' net in Sp ⊗ Sp, carried into the patch's spaces: rows, then columns.
            let d: Vec<Vec3> = p.dof.iter().map(|id| value(*id)).collect();
            let mut rows: Vec<Vec<Vec3>> = Vec::with_capacity(c);
            for j in 0..c {
                let row: Vec<Vec3> = (0..c).map(|i| d.get(i + c * j).copied().unwrap_or(Vec3::ZERO)).collect();
                rows.push(embed(&self.sp, &row, &p.uspace)?);
            }
            let mut net = vec![HPoint::weighted(Vec3::ZERO, 1.0); cu * cv];
            for i in 0..cu {
                let col: Vec<Vec3> = rows.iter().map(|r| r.get(i).copied().unwrap_or(Vec3::ZERO)).collect();
                let col = embed(&self.sp, &col, &p.vspace)?;
                for (j, dp) in col.iter().enumerate() {
                    let idx = i + cu * j;
                    let (Some(h), Some(w)) = (p.h0.get(idx), p.w.get(idx)) else { return invalid("net") };
                    // `P = P⁰ + E·D`, weighed as the fixed net is.
                    let point = *h * (1.0 / *w) + *dp;
                    if let Some(slot) = net.get_mut(idx) {
                        *slot = HPoint::weighted(point, *w);
                    }
                }
            }
            patches.push(NurbsSurface::from_homogeneous(
                [p.uspace.degree(), p.vspace.degree()],
                [p.uspace.knots().to_vec(), p.vspace.knots().to_vec()],
                net,
            )?);
        }
        let g = greville(&self.sp);
        let centre = self.centre0 + value(Some(0));
        let mut spokes = Vec::with_capacity(self.n);
        for k in 0..self.n {
            let p = self.patches.get(k).ok_or_else(|| SplineError::Invalid("patch".into()))?;
            let mk = self.middles.get(k).copied().unwrap_or(Vec3::ZERO);
            let points = (0..c)
                .map(|j| mk + (self.centre0 - mk) * g.get(j).copied().unwrap_or(0.0) + value(p.dof.get((c - 1) + c * j).copied().flatten()))
                .collect();
            spokes.push(Spoke { degree: self.sp.degree(), knots: self.sp.knots().to_vec(), points });
        }
        Ok(QuadFill { patches, spokes, centre, edges: Vec::new(), spoke_fits: Vec::new(), spans: 0 })
    }

    /// What the fill `x` achieves along every edge and spoke, at [`CHECKS`] + 1 stations each.
    fn measure(
        &self,
        x: &[Vec3],
        lengths: &[f64],
        frame_at: &mut dyn FnMut(usize, &[f64]) -> SplineResult<Vec<Frame>>,
    ) -> SplineResult<(Vec<SeamFit>, Vec<SeamFit>)> {
        let n = self.n;
        let m = 2 * n;
        let sides = self.frames_side(frame_at)?;
        let ts: Vec<f64> = (0..=CHECKS).map(|i| i as f64 / CHECKS as f64).collect();
        let mut edges = vec![SeamFit::default(); m];
        for (j, fit) in edges.iter_mut().enumerate() {
            let curve = self.curves.get(j).ok_or_else(|| SplineError::Invalid("edge".into()))?;
            // Edge 2k is patch k's `v = 0`; edge 2k + 1 is patch k + 1's `u = 0`, run back.
            let (patch, along_u) = if j % 2 == 0 { (j / 2, true) } else { ((j / 2 + 1) % n, false) };
            // Its side's corner is at its start (even) or its end (odd).
            let frames = frame_at(j, &ts)?;
            let side = sides.get(j).copied().unwrap_or(1.0);
            for (t, f) in ts.iter().zip(&frames) {
                let (u, v) = if along_u { (*t, 0.0) } else { (0.0, 1.0 - t) };
                let s = self.jet(patch, u, v)?.at(x);
                let [_, d1, _] = curve.derivs(*t)?;
                let Some(tangent) = d1.normalized() else { continue };
                let Some(normal) = s[1].cross(s[2]).normalized() else { continue };
                let fnormal = f.normal * side;
                let angle = normal.dot(fnormal).clamp(-1.0, 1.0).acos();
                // The normal curvatures square to the edge: the face's along `leave`, the patch's
                // along its own tangent square to the edge.
                let Some(across) = normal.cross(tangent).normalized() else { continue };
                let mine = second_form(&s, normal, across);
                let theirs = f.form[2] * side;
                let jump = (mine - theirs).abs();
                let from_corner = if j % 2 == 0 { curve.length_to(*t)? } else { lengths.get(j).copied().unwrap_or(0.0) - curve.length_to(*t)? };
                let share = reach(from_corner, self.fade);
                worse(&mut fit.angle, angle, *t);
                worse(&mut fit.curvature, jump, *t);
                if share >= 1.0 {
                    worse(&mut fit.curvature_clear, jump, *t);
                }
            }
        }
        let centre = self.jet(0, 1.0, 1.0)?.at(x)[0];
        let mut spokes = vec![SeamFit::default(); n];
        for (k, fit) in spokes.iter_mut().enumerate() {
            for t in &ts {
                let a = self.jet(k, 1.0, *t)?.at(x);
                let b = self.jet((k + 1) % n, *t, 1.0)?.at(x);
                let (Some(na), Some(nb)) = (a[1].cross(a[2]).normalized(), b[1].cross(b[2]).normalized()) else { continue };
                let angle = na.dot(nb).clamp(-1.0, 1.0).acos();
                let Some(tangent) = a[2].normalized() else { continue };
                let Some(across) = na.cross(tangent).normalized() else { continue };
                let jump = (second_form(&a, na, across) - second_form(&b, na, across)).abs();
                worse(&mut fit.angle, angle, *t);
                worse(&mut fit.curvature, jump, *t);
                // Clear of the centre, where N patches meet at one point (as the hole's corners
                // are, a vertex where seams turn): the vertex enclosure problem of G2 — tensor
                // patches about a point of N ≠ 4 meet with G2 there only under conditions on
                // their reparameterisations (J. Peters, "Smooth interpolation of a mesh of
                // curves", Constr. Approx. 7, 1991; X. Ye and H. Nowacki, "Ensuring compatibility
                // of G2-continuous surface patches around a nodepoint", CAGD 13, 1996), which the
                // fit meets only approximately. Measured everywhere, held clear of it.
                if a[0].dist(centre) >= self.fade {
                    worse(&mut fit.curvature_clear, jump, *t);
                }
            }
        }
        Ok((edges, spokes))
    }
}

/// The scalars of `∂S_b/∂v = −α ∂S_a/∂u + β ∂S/∂t` that fit two jets across a spoke best (least
/// squares in the plane of `∂S_a/∂u` and the spoke's tangent `∂S_a/∂v`); `None` when that plane
/// degenerates or α is not positive.
fn frozen_ab(a: &[Vec3; 6], b: &[Vec3; 6]) -> Option<(f64, f64)> {
    let (x, t, y) = (a[1], a[2], b[2]);
    let (e, f, g) = (x.dot(x), x.dot(t), t.dot(t));
    let det = e * g - f * f;
    if !(det > 1e-24 * e * g) {
        return None;
    }
    let (p, r) = (y.dot(x), y.dot(t));
    let alpha = -(g * p - f * r) / det;
    let beta = (e * r - f * p) / det;
    (alpha > 0.0 && alpha.is_finite() && beta.is_finite()).then_some((alpha, beta))
}

fn worse(w: &mut Worst, value: f64, at: f64) {
    if value > w.value || !value.is_finite() {
        *w = Worst { value, at };
    }
}

/// The normal curvature of a patch with derivatives `s` along the unit tangent `dir`, about
/// `normal`: `II(X, X)` for `dir = a S_u + b S_v`.
fn second_form(s: &[Vec3; 6], normal: Vec3, dir: Vec3) -> f64 {
    let [_, su, sv, suu, suv, svv] = *s;
    let (e, f, g) = (su.dot(su), su.dot(sv), sv.dot(sv));
    let det = e * g - f * f;
    if !(det > 0.0) {
        return f64::NAN;
    }
    let (du, dv) = (dir.dot(su), dir.dot(sv));
    let (a, b) = ((g * du - f * dv) / det, (e * dv - f * du) / det);
    normal.dot(suu) * a * a + 2.0 * normal.dot(suv) * a * b + normal.dot(svv) * b * b
}

/// The normal equations over the unknown points' coordinates: first only which points share a
/// row (`Pattern`), then, in the order that pattern gives (reverse Cuthill–McKee, so the
/// matrix is a narrow band: E. Cuthill and J. McKee, 1969; A. George, 1971), the banded matrix
/// itself, solved by banded Cholesky.
#[derive(Clone)]
enum Store {
    Pattern(Vec<Vec<usize>>),
    Band { band: Band, pos: Vec<usize> },
}

#[derive(Clone)]
struct Normal {
    points: usize,
    store: Store,
    b: Vec<f64>,
}

/// An ordering of the unknown points and the band it gives (in points).
struct Ordering {
    pos: Vec<usize>,
    width: usize,
}

impl Normal {
    fn pattern(points: usize) -> SplineResult<Normal> {
        if points > MAX_UNKNOWN_POINTS {
            return Err(SplineError::TooLarge(format!("{points} unknown points (at most {MAX_UNKNOWN_POINTS})")));
        }
        Ok(Normal { points, store: Store::Pattern(vec![Vec::new(); points]), b: Vec::new() })
    }

    fn banded(order: &Ordering, _patches: &[Patch]) -> SplineResult<Normal> {
        let points = order.pos.len();
        let n = 3 * points;
        let bw = 3 * order.width + 2;
        if n.saturating_mul(bw + 1) > 400_000_000 {
            return Err(SplineError::TooLarge(format!("a band of {bw} over {n} unknowns")));
        }
        Ok(Normal { points, store: Store::Band { band: Band::new(n, bw), pos: order.pos.clone() }, b: vec![0.0; n] })
    }

    /// The reverse Cuthill–McKee order of the pattern's points (each component from a point of
    /// least degree, neighbours by rising degree), and its band.
    fn ordering(&self) -> SplineResult<Ordering> {
        let Store::Pattern(adj) = &self.store else { return invalid("no pattern") };
        let adj: Vec<Vec<usize>> = adj
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        let degree = |i: usize| adj.get(i).map(|v| v.len()).unwrap_or(0);
        let mut seen = vec![false; self.points];
        let mut order: Vec<usize> = Vec::with_capacity(self.points);
        while let Some(start) = (0..self.points).filter(|i| !seen.get(*i).copied().unwrap_or(true)).min_by_key(|i| degree(*i)) {
            if let Some(slot) = seen.get_mut(start) {
                *slot = true;
            }
            let mut head = order.len();
            order.push(start);
            while let Some(&p) = order.get(head) {
                head += 1;
                let mut next: Vec<usize> = adj.get(p).into_iter().flatten().copied().filter(|q| !seen.get(*q).copied().unwrap_or(true)).collect();
                next.sort_by_key(|q| degree(*q));
                for q in next {
                    if let Some(slot) = seen.get_mut(q)
                        && !*slot
                    {
                        *slot = true;
                        order.push(q);
                    }
                }
            }
        }
        order.reverse();
        let mut pos = vec![0usize; self.points];
        for (i, p) in order.iter().enumerate() {
            if let Some(slot) = pos.get_mut(*p) {
                *slot = i;
            }
        }
        let mut width = 0usize;
        for (i, v) in adj.iter().enumerate() {
            for j in v {
                width = width.max(pos.get(i).copied().unwrap_or(0).abs_diff(pos.get(*j).copied().unwrap_or(0)));
            }
        }
        Ok(Ordering { pos, width })
    }

    /// Add `w2 · (Σ c_m·X_m − r)²` for the row with vector coefficients `terms`.
    fn accumulate(&mut self, terms: &[(usize, Vec3)], r: f64, w2: f64) {
        match &mut self.store {
            Store::Pattern(adj) => {
                for &(m, _) in terms {
                    if let Some(list) = adj.get_mut(m) {
                        list.extend(terms.iter().map(|(k, _)| *k));
                    }
                }
            }
            Store::Band { band, pos } => {
                for &(m, cm) in terms {
                    let Some(&pm) = pos.get(m) else { continue };
                    for (a, da) in [cm.x, cm.y, cm.z].iter().enumerate() {
                        let row = 3 * pm + a;
                        if let Some(slot) = self.b.get_mut(row) {
                            *slot += w2 * da * r;
                        }
                        for &(k, ck) in terms {
                            let Some(&pk) = pos.get(k) else { continue };
                            for (b, db) in [ck.x, ck.y, ck.z].iter().enumerate() {
                                let col = 3 * pk + b;
                                if row <= col {
                                    band.add(row, col, w2 * da * db);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The row `Σ dir_i · ∂S_i = rhs` over derivatives of jets, each with its own direction,
    /// weighed.
    fn rows(&mut self, parts: &[(&Jet, usize, Vec3)], rhs: f64, weight: f64) {
        if !(weight > 0.0) || !weight.is_finite() {
            return;
        }
        let mut terms: Vec<(usize, Vec3)> = Vec::new();
        let mut fixed = 0.0;
        for (jet, d, dir) in parts {
            fixed += dir.dot(jet.fixed.get(*d).copied().unwrap_or(Vec3::ZERO));
            for (m, c) in &jet.terms {
                let v = *dir * c.get(*d).copied().unwrap_or(0.0);
                match terms.iter_mut().find(|(i, _)| i == m) {
                    Some(slot) => slot.1 += v,
                    None => terms.push((*m, v)),
                }
            }
        }
        self.accumulate(&terms, rhs - fixed, weight * weight);
    }

    /// The row `dir · Σ mult·∂S = rhs`, weighed.
    fn dot(&mut self, jet: &Jet, which: &[(usize, f64)], dir: Vec3, rhs: f64, weight: f64) {
        let parts: Vec<(&Jet, usize, Vec3)> = which.iter().map(|(d, m)| (jet, *d, dir * *m)).collect();
        self.rows(&parts, rhs, weight);
    }

    /// The row `dir · (Σ ∂S_a + Σ ∂S_b) = rhs` over two patches' jets, weighed.
    fn pair(&mut self, a: &Jet, wa: &[(usize, f64)], b: &Jet, wb: &[(usize, f64)], dir: Vec3, rhs: f64, weight: f64) {
        let mut parts: Vec<(&Jet, usize, Vec3)> = wa.iter().map(|(d, m)| (a, *d, dir * *m)).collect();
        parts.extend(wb.iter().map(|(d, m)| (b, *d, dir * *m)));
        self.rows(&parts, rhs, weight);
    }

    /// The three rows `Σ mult·∂S = rhs`, weighed.
    fn vec(&mut self, jet: &Jet, which: &[(usize, f64)], rhs: Vec3, weight: f64) {
        for (axis, r) in [(Vec3::new(1.0, 0.0, 0.0), rhs.x), (Vec3::new(0.0, 1.0, 0.0), rhs.y), (Vec3::new(0.0, 0.0, 1.0), rhs.z)] {
            self.dot(jet, which, axis, r, weight);
        }
    }

    fn solve(self) -> SplineResult<Vec<Vec3>> {
        let Store::Band { band, pos } = self.store else { return invalid("no matrix") };
        let x = band.solve_fast(self.b)?;
        let at = |i: usize| x.get(i).copied().unwrap_or(0.0);
        Ok(pos.iter().map(|p| Vec3::new(at(3 * p), at(3 * p + 1), at(3 * p + 2))).collect())
    }
}

#[cfg(test)]
#[path = "fill_quad_tests.rs"]
mod tests;
