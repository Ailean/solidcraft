//! G2 fillet corners (`corner: G2`): the corner where three or more rounded edges meet, filled
//! with patches that meet every face around it to the second order, instead of the rolling
//! ball's sphere (or SMLib's Coons patch), which is only tangent to the rounds and keeps its own
//! 1 / r across each seam.
//!
//! The corner is set back: SMLib runs each round whole to its cross-section a setback from the
//! vertex (an iso-curve of the round) and leaves the corner open
//! ([`crate::blend_smlib::fillet_setback_open`]), so the hole's boundary is the rounds' sections
//! and, on each face between two rounds, SMLib's curve from one round's contact line to the
//! next, each side split at its middle. The hole is filled as N-sided holes are: one four-sided
//! patch per corner of the hole, meeting at a centre point ([`crate::fill::quad_patch`],
//! `solidcraft_geom::fill_quad`), each holding its two half-sides exactly as its boundary rows,
//! G2 to the faces beside them and to its two neighbouring patches across their spokes. The
//! faces are sewn into one solid by SMLib ([`crate::stitch`]).
//!
//! At the hole's corners, where a section ends on a face, the round's own G1-only seam with the
//! face ends: no surface is G2 to both there (the section's curvature 1 / r is the patch's along
//! it, and so across the face's curve, where the face asks for its own; and a face curve that
//! turns in its plane leaves the patch's curvature along the round's axis off the round's). The
//! patches pass from one curvature to the other within [`FADE_SHARE`] of the hole's size from
//! each corner, and are G2 beyond it. Making the corner conditions agree is a property of the
//! set-back rounds: a section eased to the face's curvature at its setback, and a face curve
//! leaving the contact line without turning.

use solidcraft_geom::fill_quad::QuadFill;

use crate::body::Body;
use crate::{KernelError, Result};

/// A corner's default setback: this times its round's radius along each edge. It must reach
/// past where the rounds meet without one (the radius, at a square corner), and SMLib's own
/// patch bulges past the faces beyond about twice the radius; one and a half leaves the patch a
/// side as long as the round's section is wide, with room either way.
pub const DEFAULT_SETBACK_FACTOR: f64 = 1.5;

/// How far the patch's normal curvature across a seam may be from the face's, clear of the hole's
/// corners: this share of the corner's round's curvature 1 / r, the jump a rolling ball makes.
pub const CURVATURE_SHARE: f64 = 5e-3;

/// How far from each corner of the hole, and from the patches' centre, the patch passes from
/// one neighbour's curvature to the other's: this share of the hole's size (the largest distance
/// between two of its vertices; 1.06 mm at a box corner's r5 rounds set back 7.5). G2 is held
/// beyond it.
pub const FADE_SHARE: f64 = 0.1;

/// The patch filling one open corner of `sheet` (a sheet from
/// [`crate::blend_smlib::fillet_setback_open`]): `hole` its edges in order, each with whether the
/// face beside it runs it forwards; `radius` the corner's round's radius, which sets the
/// curvature tolerance. The patch is a one-face sheet on the sheet's own edges.
pub(crate) fn corner_patch(sheet: &Body, hole: &[(usize, bool)], radius: f64) -> Result<(Body, QuadFill)> {
    if !(radius.is_finite() && radius > 0.0) {
        return Err(KernelError::Invalid("a G2 corner's radius must be finite and positive".into()));
    }
    let record = crate::native::brep_record(sheet)?;
    let points: Vec<[f64; 3]> = hole
        .iter()
        .filter_map(|(e, _)| record.edges.get(*e))
        .flat_map(|e| e.vertices)
        .filter_map(|v| record.vertices.get(v as usize).map(|x| x.point))
        .collect();
    let d = |a: &[f64; 3], b: &[f64; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let size = points.iter().flat_map(|a| points.iter().map(move |b| d(a, b))).fold(0.0f64, f64::max);
    if !(size > 0.0 && size.is_finite()) {
        return Err(KernelError::Failed("a G2 corner's hole has no size".into()));
    }
    crate::fill::quad_patch(sheet, hole, CURVATURE_SHARE / radius, FADE_SHARE * size)
}

#[cfg(test)]
#[path = "corner_g2_tests.rs"]
mod tests;
