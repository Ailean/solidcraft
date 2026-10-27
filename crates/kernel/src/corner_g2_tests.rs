use solidcraft_geom::Vec3;

use crate::*;

/// The three r5 rounds at the corner (60, 40, 20) of a 60 x 40 x 20 box (ladder 07/009 and
/// 07/015).
fn box_corner() -> (Body, [Vec3; 3]) {
    let b = box_solid(Vec3::new(0.0, 0.0, 0.0), Vec3::new(60.0, 40.0, 20.0)).unwrap();
    (b, [Vec3::new(30.0, 40.0, 20.0), Vec3::new(60.0, 20.0, 20.0), Vec3::new(60.0, 40.0, 10.0)])
}

/// The corner's seams as the ladder grades them ([`corner_continuity`]): every tangent seam
/// within `within` of `near`, its curvature jump held `clear` mm from the vertices where seams
/// turn (the hole's corners, where a round's own G1 seam with a face ends; the sides' middles
/// and the centre, where the patches' spokes meet). Each seam is printed.
fn graded(name: &str, body: &Body, near: Vec3, within: f64, clear: f64) -> CornerContinuity {
    let c = corner_continuity(body, near, within, clear).unwrap();
    eprintln!(
        "{name}: {} seams, gap {:.1e} mm, angle {:.4}°, curvature jump {:.3e} per mm, {:.3e} clear of the turns ({} stations)",
        c.seams.len(),
        c.gap_mm,
        c.normal_angle_deg,
        c.curvature_jump_per_mm,
        c.curvature_jump_clear_per_mm,
        c.stations_clear
    );
    for s in &c.seams {
        eprintln!(
            "  seam {:>3} {:.2} mm: gap {:.1e}, angle {:.4}°, jump {:.3e}, clear {}",
            s.edge,
            s.length,
            s.gap_mm.value,
            s.normal_angle_deg.value,
            s.curvature_jump_per_mm.value,
            s.curvature_jump_clear_per_mm.map(|w| format!("{:.3e}", w.value)).unwrap_or_else(|| "-".into())
        );
    }
    c
}

/// The G2 corner of 07/015, set back the default 7.5 (1.5 r): one closed valid solid of the box's
/// six faces, the three rounds and the six patches about the centre (15 faces, as SMLib's own
/// setback corner, 07/013), within the box; graded as the ladder grades it, every seam of the
/// corner tangent and, 1.2 mm clear of the turns, curvature continuous to `CURVATURE_SHARE / r`
/// where the rolling ball's corner jumps 1 / r; the patches' edges are the hole's own, so the
/// gaps are rounding.
#[test]
fn the_box_corner_g2_patch_is_a_closed_solid_curvature_continuous_to_its_neighbours() {
    let (body, edges) = box_corner();
    let ball = fillet(&body, &edges, 5.0).unwrap();
    let g2 = fillet_g2_corners(&body, &edges, 5.0, &[]).unwrap();
    assert!(g2.validity().is_empty(), "{:?}", g2.validity());
    let v = crate::validity::validity(&g2);
    assert!(v.ok(), "{:?}", v);
    let m = measure(&g2).unwrap();
    eprintln!(
        "G2 corner: faces {} edges {} vertices {}, volume {:.6} (ball {:.6})",
        m.faces,
        m.edges,
        m.vertices,
        m.volume,
        measure(&ball).unwrap().volume
    );
    assert_eq!((m.faces, m.edges, m.vertices), (15, 36, 23));
    assert!((m.bbox.min - Vec3::ZERO).len() < 1e-6 && (m.bbox.max - Vec3::new(60.0, 40.0, 20.0)).len() < 1e-6, "{:?}", m.bbox);
    let corner = Vec3::new(60.0, 40.0, 20.0);
    let c = graded("G2 corner", &g2, corner, 15.0, 1.2);
    let b = graded("ball corner", &ball, corner, 15.0, 1.2);
    assert_eq!(c.seams.len(), 18);
    assert!(c.gap_mm < 1e-9 && c.normal_angle_deg <= crate::fill::PATCH_ANGLE_TOL_DEG, "{c:?}");
    assert!(c.stations_clear > 0 && c.curvature_jump_clear_per_mm <= CURVATURE_SHARE / 5.0, "{}", c.curvature_jump_clear_per_mm);
    assert!(b.curvature_jump_clear_per_mm > 0.19);
}

/// Set back 8, as SMLib's own setback test: outside the corner cube [52, 60] x [32, 40] x
/// [12, 20] the body is the sphere corner's, and inside it the convex body holds the hull of its
/// points on the cube's faces (299.0201, by a hull of them sampled) and lies within the three
/// rounds' prisms (`fillet_setback_tests`).
#[test]
fn set_back_8_the_g2_corner_lies_between_the_hull_and_the_prisms() {
    let pi = std::f64::consts::PI;
    let (body, edges) = box_corner();
    let sphere = measure(&fillet(&body, &edges, 5.0).unwrap()).unwrap().volume;
    let g2 = fillet_g2_corners(&body, &edges, 5.0, &[(Vec3::new(60.0, 40.0, 20.0), 8.0)]).unwrap();
    let v = measure(&g2).unwrap().volume;
    let outside = sphere - (512.0 - 225.0 * (1.0 - pi / 4.0) - 125.0 * (1.0 - pi / 6.0));
    let prisms = 512.0 - 225.0 * (1.0 - pi / 4.0) - 125.0 * (2f64.sqrt() - 1.0);
    eprintln!("G2 corner set back 8: volume {v:.6} in [{:.6}, {:.6}]", outside + 299.0201, outside + prisms);
    assert!(v > outside + 299.0201 && v < outside + prisms);
    let c = graded("G2 corner set back 8", &g2, Vec3::new(60.0, 40.0, 20.0), 15.0, 1.2);
    assert!(c.normal_angle_deg <= crate::fill::PATCH_ANGLE_TOL_DEG && c.curvature_jump_clear_per_mm <= CURVATURE_SHARE / 5.0, "{c:?}");
}

/// Long setbacks, where SMLib's own patch bulges past the faces (beyond about 2 r, by 0.25 at
/// 3 r): the G2 patches meet the faces' own planes along their curves and stay inside the box,
/// tangent across every seam and curvature continuous clear of the turns by the corner's fade
/// (`FADE_SHARE` of the hole's size: 0.15 times the setback covers it).
#[test]
fn a_long_setback_keeps_the_g2_patch_inside_the_box() {
    let (body, edges) = box_corner();
    let corner = Vec3::new(60.0, 40.0, 20.0);
    for s in [11.0, 13.0, 15.0] {
        let g2 = fillet_g2_corners(&body, &edges, 5.0, &[(corner, s)]).unwrap();
        let m = g2.mesh(&MeshTolerance::chord(1e-3)).unwrap();
        let over = m.positions.iter().map(|p| (p.x - 60.0).max(p.y - 40.0).max(p.z - 20.0)).fold(f64::NEG_INFINITY, f64::max);
        let c = graded(&format!("set back {s}"), &g2, corner, 1.1 * s + 1.0, 0.15 * s);
        eprintln!("set back {s}: past the box by {over:.2e}");
        assert!(
            over < 1e-3 && c.normal_angle_deg <= crate::fill::PATCH_ANGLE_TOL_DEG && c.curvature_jump_clear_per_mm <= CURVATURE_SHARE / 5.0,
            "{s}: {over} {c:?}"
        );
    }
}

/// Every edge of a box rounded: eight corners, each set back and filled, one closed solid.
#[test]
#[ignore = "SMLib: at the corners of some octants (the box's (0, 0, 0)) the setback's section comes back as an exact arc for one half and a polynomial spline 1.7e-5 mm off the round for the other, its tangent out of the round's tangent plane, which the G2 corner cannot meet within its tolerances"]
fn a_box_with_every_edge_rounded_has_eight_g2_corners() {
    let body = box_solid(Vec3::ZERO, Vec3::new(30.0, 20.0, 10.0)).unwrap();
    let mut edges = Vec::new();
    for (x, y) in [(0.0, 0.0), (30.0, 0.0), (0.0, 20.0), (30.0, 20.0)] {
        edges.push(Vec3::new(x, y, 5.0));
    }
    for (y, z) in [(0.0, 0.0), (20.0, 0.0), (0.0, 10.0), (20.0, 10.0)] {
        edges.push(Vec3::new(15.0, y, z));
    }
    for (x, z) in [(0.0, 0.0), (30.0, 0.0), (0.0, 10.0), (30.0, 10.0)] {
        edges.push(Vec3::new(x, 10.0, z));
    }
    let g2 = fillet_g2_corners(&body, &edges, 2.0, &[]).unwrap();
    assert!(g2.validity().is_empty(), "{:?}", g2.validity());
    let v = crate::validity::validity(&g2);
    let m = measure(&g2).unwrap();
    eprintln!("eight G2 corners: {} faces, volume {:.4}, valid {}", m.faces, m.volume, v.ok());
    assert!(v.ok(), "{v:?}");
    assert_eq!(m.faces, 6 + 12 + 8 * 6);
}

/// What the G2 corner cannot do is refused: a setback naming no corner, one that does not reach
/// past where the rounds meet; two rounded edges at a corner are no G2 corner, the plain fillet.
#[test]
fn what_the_g2_corner_cannot_build_is_refused() {
    let (body, edges) = box_corner();
    let off = fillet_g2_corners(&body, &edges, 5.0, &[(Vec3::new(30.0, 20.0, 10.0), 8.0)]).unwrap_err().to_string();
    assert!(off.contains("names no G2 corner"), "{off}");
    let short = fillet_g2_corners(&body, &edges, 5.0, &[(Vec3::new(60.0, 40.0, 20.0), 4.0)]);
    eprintln!("set back 4 at r5: {short:?}");
    assert!(short.is_err());
    let two = fillet_g2_corners(&body, &edges[..2], 5.0, &[]).unwrap();
    let plain = fillet(&body, &edges[..2], 5.0).unwrap();
    assert_eq!(measure(&two).unwrap().faces, measure(&plain).unwrap().faces);
}

/// The 60 x 40 x 20 box with a 20 x 20 x 10 notch cut from its top corner (60, 40, 20): its
/// inner corner (40, 20, 10) where three concave edges meet, and the notch's outer corner
/// (60, 20, 10) where the concave edge along x meets two convex ones.
fn notched() -> Body {
    let a = box_solid(Vec3::ZERO, Vec3::new(60.0, 40.0, 20.0)).unwrap();
    let b = box_solid(Vec3::new(40.0, 20.0, 10.0), Vec3::new(70.0, 50.0, 30.0)).unwrap();
    boolean(&a, &b, BoolOp::Cut).unwrap().unwrap()
}

/// The notch's inner corner, three concave r5 rounds (ladder 07/019): set back 7.5 and filled,
/// one closed valid solid with the ball corner's faces but its sphere (13), and the six
/// patches; tangent across every seam and curvature continuous clear of the turns, where the
/// ball's jump 1 / r.
#[test]
fn a_concave_corner_takes_the_g2_patch_too() {
    let body = notched();
    let edges = [Vec3::new(50.0, 20.0, 10.0), Vec3::new(40.0, 30.0, 10.0), Vec3::new(40.0, 20.0, 15.0)];
    let ball = fillet(&body, &edges, 5.0).unwrap();
    let g2 = fillet_g2_corners(&body, &edges, 5.0, &[]).unwrap();
    assert!(g2.validity().is_empty(), "{:?}", g2.validity());
    let v = crate::validity::validity(&g2);
    assert!(v.ok(), "{v:?}");
    let (m, mb) = (measure(&g2).unwrap(), measure(&ball).unwrap());
    let near = Vec3::new(40.0, 20.0, 10.0);
    let c = graded("concave G2 corner", &g2, near, 11.0, 1.2);
    let b = graded("concave ball corner", &ball, near, 11.0, 1.2);
    eprintln!("concave G2 corner: {} faces (ball {}), volume {:.4} (ball {:.4})", m.faces, mb.faces, m.volume, mb.volume);
    assert_eq!(m.faces, mb.faces - 1 + 6);
    assert!(c.normal_angle_deg <= crate::fill::PATCH_ANGLE_TOL_DEG && c.curvature_jump_clear_per_mm <= CURVATURE_SHARE / 5.0, "{c:?}");
    assert!(b.curvature_jump_clear_per_mm > 0.19);
}

/// The notch's outer corner, two convex rounds and a concave one: SMLib sets back only a corner
/// of one convexity, so the G2 corner is refused as not supported (the rolling ball builds).
#[test]
fn a_mixed_convexity_corner_is_refused() {
    let body = notched();
    let edges = [Vec3::new(50.0, 20.0, 10.0), Vec3::new(60.0, 30.0, 10.0), Vec3::new(60.0, 20.0, 15.0)];
    assert!(fillet(&body, &edges, 5.0).is_ok());
    let err = fillet_g2_corners(&body, &edges, 5.0, &[]).unwrap_err().to_string();
    eprintln!("mixed corner: {err}");
    // The refusal names the corner and why: its rounds are not all of one convexity.
    assert!(err.contains("not supported yet") && err.contains("(60.000, 20.000, 10.000)") && err.contains("all convex or all concave"), "{err}");
}
