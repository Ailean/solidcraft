use super::*;

/// A hexagonal hole in the plane z = 0, each side a line split at its middle, the plane beside
/// every edge: the fill is that plane (every patch flat), within the tolerances everywhere.
#[test]
fn a_flat_hexagon_fills_flat() {
    let corners: Vec<Vec3> = (0..6)
        .map(|k| {
            let a = k as f64 * std::f64::consts::PI / 3.0;
            Vec3::new(10.0 * a.cos(), 10.0 * a.sin(), 0.0)
        })
        .collect();
    let mut edges = Vec::new();
    for k in 0..6 {
        let (a, b) = (corners[k], corners[(k + 1) % 6]);
        let m = (a + b) * 0.5;
        edges.push(QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![a, m], weights: Vec::new() });
        edges.push(QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![m, b], weights: Vec::new() });
    }
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let e = &copy[j];
        let t = (e.points[1] - e.points[0]).normalized().unwrap();
        // The hole is counter-clockwise about +z: inside is on the left, z × t.
        let leave = Vec3::new(-t.y, t.x, 0.0);
        Ok(ts.iter().map(|_| Frame { normal: Vec3::new(0.0, 0.0, 1.0), leave, form: [0.0; 3] }).collect())
    };
    let fill = quad_fill(&edges, &mut frames, &QuadSpec { tol: 1e-9, angle_tol: 1e-3_f64.to_radians(), curvature_tol: 1e-6, fade: 1.0 }).unwrap();
    assert_eq!(fill.patches.len(), 6);
    for p in &fill.patches {
        for i in 0..=8 {
            for j in 0..=8 {
                let z = p.eval(i as f64 / 8.0, j as f64 / 8.0).unwrap().z;
                assert!(z.abs() < 1e-9, "{z}");
            }
        }
    }
    assert!(fill.centre.len() < 1.0, "{:?}", fill.centre);
}

/// Hostile input is refused: an odd number of edges, edges that do not meet, a rational edge
/// not in standard form.
#[test]
fn a_bad_hole_is_refused() {
    let line = |a: Vec3, b: Vec3| QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![a, b], weights: Vec::new() };
    let mut frames = |_: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        Ok(ts.iter().map(|_| Frame { normal: Vec3::new(0.0, 0.0, 1.0), leave: Vec3::new(1.0, 0.0, 0.0), form: [0.0; 3] }).collect())
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 1e-3, curvature_tol: 1e-3, fade: 1.0 };
    let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    assert!(quad_fill(&[line(p(0.0, 0.0), p(1.0, 0.0))], &mut frames, &spec).is_err());
    let apart: Vec<QuadEdge> = (0..6).map(|k| line(p(k as f64, 0.0), p(k as f64 + 0.5, 0.0))).collect();
    assert!(quad_fill(&apart, &mut frames, &spec).is_err());
    let mut bad = line(p(0.0, 0.0), p(1.0, 0.0));
    bad.weights = vec![2.0, 1.0];
    let mut six: Vec<QuadEdge> = (0..6).map(|k| line(p(k as f64, 0.0), p(k as f64 + 1.0, 0.0))).collect();
    six[0] = bad;
    assert!(quad_fill(&six, &mut frames, &spec).is_err());
}

/// A circle of radius 6 in z = 0 split into twelve 30° arcs (rational quadratics in standard
/// form), the hole a cap of the sphere of radius 10 through it: its frames that sphere's. The
/// fill is G2 to the sphere within the tolerances, its centre on the axis under the pole.
#[test]
fn a_spherical_cap_fills_g2() {
    let (rho, big) = (6.0f64, 10.0f64);
    let h = (big * big - rho * rho).sqrt();
    let centre = Vec3::new(0.0, 0.0, -h);
    let step = std::f64::consts::PI / 6.0;
    let at = |a: f64| Vec3::new(rho * a.cos(), rho * a.sin(), 0.0);
    let half = 0.5 * step;
    let w = half.cos();
    let edges: Vec<QuadEdge> = (0..12)
        .map(|k| {
            let (a0, a1) = (k as f64 * step, (k + 1) as f64 * step);
            let mid = Vec3::new(rho * (a0 + half).cos(), rho * (a0 + half).sin(), 0.0) * (1.0 / w);
            QuadEdge { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points: vec![at(a0), mid, at(a1)], weights: vec![1.0, w, 1.0] }
        })
        .collect();
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let c = Curve::new(&copy[j]).map_err(|e| e.to_string())?;
        ts.iter()
            .map(|t| {
                let [p, d1, _] = c.derivs(*t).map_err(|e| e.to_string())?;
                let n = (p - centre) * (1.0 / big);
                let tan = d1.normalized().unwrap();
                // Into the cap: towards the pole, square to the edge in the sphere's tangent plane.
                let leave = n.cross(tan).normalized().unwrap();
                Ok(Frame { normal: n, leave, form: [-1.0 / big, 0.0, -1.0 / big] })
            })
            .collect()
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 0.05f64.to_radians(), curvature_tol: 1e-3, fade: 1.0 };
    let fill = quad_fill(&edges, &mut frames, &spec).unwrap();
    assert_eq!(fill.patches.len(), 6);
    let top = big - h;
    eprintln!("cap: centre {:?} (the pole at z = {top})", fill.centre);
    // The energy decides the middle, which a cap's G2 rim does not fix: it lies on the axis,
    // between the rim's plane and the pole.
    assert!(fill.centre.z > 0.5 * top && fill.centre.z < top + 1e-6 && fill.centre.x.hypot(fill.centre.y) < 1e-6, "{:?}", fill.centre);
}

/// A hexagon on the paraboloid z = (x² + y²) / 20, each side the parabola over a straight line
/// split at its middle, the paraboloid beside every edge: the paraboloid itself is a fill of
/// the patches' kind (a quadratic of bilinear maps); the fill meets it within the tolerances.
#[test]
fn a_paraboloid_hexagon_fills_g2() {
    let r = 10.0f64;
    let lift = |x: f64, y: f64| Vec3::new(x, y, (x * x + y * y) / (2.0 * r));
    let corners: Vec<(f64, f64)> = (0..6)
        .map(|k| {
            let a = k as f64 * std::f64::consts::PI / 3.0;
            (6.0 * a.cos(), 6.0 * a.sin())
        })
        .collect();
    // The parabola over the segment from a to b: a quadratic Bézier.
    let piece = |a: (f64, f64), b: (f64, f64)| {
        let m = (0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1));
        let (pa, pb, pm) = (lift(a.0, a.1), lift(b.0, b.1), lift(m.0, m.1));
        let mid = pm * 2.0 - (pa + pb) * 0.5;
        QuadEdge { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points: vec![pa, mid, pb], weights: Vec::new() }
    };
    let mut edges = Vec::new();
    for k in 0..6 {
        let (a, b) = (corners[k], corners[(k + 1) % 6]);
        let m = (0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1));
        edges.push(piece(a, m));
        edges.push(piece(m, b));
    }
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let c = Curve::new(&copy[j]).map_err(|e| e.to_string())?;
        ts.iter()
            .map(|t| {
                let [p, d1, _] = c.derivs(*t).map_err(|e| e.to_string())?;
                let (zx, zy) = (p.x / r, p.y / r);
                let up = (1.0 + zx * zx + zy * zy).sqrt();
                let n = Vec3::new(-zx, -zy, 1.0) * (1.0 / up);
                let tan = d1.normalized().unwrap();
                let leave = n.cross(tan).normalized().unwrap();
                // II(X, X) = (X_x² + X_y²) / (r·up) for the graph z = (x² + y²) / 2r.
                let ii = |x: Vec3, y: Vec3| (x.x * y.x + x.y * y.y) / (r * up);
                Ok(Frame { normal: n, leave, form: [ii(tan, tan), ii(tan, leave), ii(leave, leave)] })
            })
            .collect()
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 0.05f64.to_radians(), curvature_tol: 1e-3, fade: 1.0 };
    let fill = quad_fill(&edges, &mut frames, &spec).unwrap();
    eprintln!("paraboloid: centre {:?}", fill.centre);
    // On the axis, at most as high as the paraboloid's rim: the energy settles the middle.
    assert!(fill.centre.x.hypot(fill.centre.y) < 1e-6 && fill.centre.z > -1e-6 && fill.centre.z < 1.8, "{:?}", fill.centre);
}

/// A four-sided hole on a cylinder of radius 5 about x: two cross-section arcs (rational) and
/// two generators, each split at its middle, the cylinder beside every edge: the fill is G2 to
/// it within the tolerances (the cylinder itself is one such fill).
#[test]
fn a_cylinder_window_fills_g2() {
    let r = 5.0f64;
    let at = |x: f64, a: f64| Vec3::new(x, r * a.cos(), r * a.sin());
    let (a0, a1) = (30f64.to_radians(), 90f64.to_radians());
    let line = |p: Vec3, q: Vec3| QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![p, q], weights: Vec::new() };
    let arc = |x: f64, from: f64, to: f64| {
        let half = 0.5 * (to - from);
        let w = half.cos();
        let mid = at(x, from + half) * (1.0 / w);
        let mid = Vec3::new(x, mid.y, mid.z);
        QuadEdge { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points: vec![at(x, from), mid, at(x, to)], weights: vec![1.0, w, 1.0] }
    };
    let am = 0.5 * (a0 + a1);
    let edges = vec![
        line(at(0.0, a0), at(3.0, a0)),
        line(at(3.0, a0), at(6.0, a0)),
        arc(6.0, a0, am),
        arc(6.0, am, a1),
        line(at(6.0, a1), at(3.0, a1)),
        line(at(3.0, a1), at(0.0, a1)),
        arc(0.0, a1, am),
        arc(0.0, am, a0),
    ];
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let c = Curve::new(&copy[j]).map_err(|e| e.to_string())?;
        ts.iter()
            .map(|t| {
                let [p, d1, _] = c.derivs(*t).map_err(|e| e.to_string())?;
                let n = Vec3::new(0.0, p.y, p.z) * (1.0 / r);
                let tan = d1.normalized().unwrap();
                // Into the hole: towards its middle, square to the edge in the tangent plane.
                let mid = at(3.0, am);
                let mut leave = n.cross(tan).normalized().unwrap();
                if leave.dot(mid - p) < 0.0 {
                    leave = leave * -1.0;
                }
                let ii = |x: Vec3, y: Vec3| {
                    let e = Vec3::new(0.0, -p.z, p.y) * (1.0 / r);
                    -x.dot(e) * y.dot(e) / r
                };
                Ok(Frame { normal: n, leave, form: [ii(tan, tan), ii(tan, leave), ii(leave, leave)] })
            })
            .collect()
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 0.05f64.to_radians(), curvature_tol: 1e-3, fade: 1.0 };
    let fill = quad_fill(&edges, &mut frames, &spec).unwrap();
    eprintln!("cylinder window: centre {:?}, radius {}", fill.centre, fill.centre.y.hypot(fill.centre.z));
}

/// The cylinder window with the plane tangent to the cylinder along its first generator beside
/// that side: the round's G1-only seam with a plane ends at two corners of the hole, as at a
/// set-back corner; G2 clear of those corners.
#[test]
fn a_cylinder_window_beside_a_tangent_plane_fills_g2() {
    let r = 5.0f64;
    let at = |x: f64, a: f64| Vec3::new(x, r * a.cos(), r * a.sin());
    let (a0, a1) = (30f64.to_radians(), 90f64.to_radians());
    let line = |p: Vec3, q: Vec3| QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![p, q], weights: Vec::new() };
    let arc = |x: f64, from: f64, to: f64| {
        let half = 0.5 * (to - from);
        let w = half.cos();
        let mid = at(x, from + half) * (1.0 / w);
        let mid = Vec3::new(x, mid.y, mid.z);
        QuadEdge { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points: vec![at(x, from), mid, at(x, to)], weights: vec![1.0, w, 1.0] }
    };
    let am = 0.5 * (a0 + a1);
    let edges = vec![
        line(at(0.0, a0), at(3.0, a0)),
        line(at(3.0, a0), at(6.0, a0)),
        arc(6.0, a0, am),
        arc(6.0, am, a1),
        line(at(6.0, a1), at(3.0, a1)),
        line(at(3.0, a1), at(0.0, a1)),
        arc(0.0, a1, am),
        arc(0.0, am, a0),
    ];
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let c = Curve::new(&copy[j]).map_err(|e| e.to_string())?;
        ts.iter()
            .map(|t| {
                let [p, d1, _] = c.derivs(*t).map_err(|e| e.to_string())?;
                let n = Vec3::new(0.0, p.y, p.z) * (1.0 / r);
                let tan = d1.normalized().unwrap();
                // Into the hole: towards its middle, square to the edge in the tangent plane.
                let mid = at(3.0, am);
                let mut leave = n.cross(tan).normalized().unwrap();
                if leave.dot(mid - p) < 0.0 {
                    leave = leave * -1.0;
                }
                let ii = |x: Vec3, y: Vec3| {
                    let e = Vec3::new(0.0, -p.z, p.y) * (1.0 / r);
                    -x.dot(e) * y.dot(e) / r
                };
                if j < 2 {
                    return Ok(Frame { normal: n, leave, form: [0.0; 3] });
                }
                Ok(Frame { normal: n, leave, form: [ii(tan, tan), ii(tan, leave), ii(leave, leave)] })
            })
            .collect()
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 0.05f64.to_radians(), curvature_tol: 1e-3, fade: 1.0 };
    let fill = quad_fill(&edges, &mut frames, &spec).unwrap();
    eprintln!("cylinder window: centre {:?}, radius {}", fill.centre, fill.centre.y.hypot(fill.centre.z));
}

/// The cylinder window mirrored (z → −z): its loop runs the other way round its normal.
#[test]
fn a_mirrored_cylinder_window_fills_g2() {
    let r = 5.0f64;
    let at = |x: f64, a: f64| Vec3::new(x, r * a.cos(), -r * a.sin());
    let (a0, a1) = (30f64.to_radians(), 90f64.to_radians());
    let line = |p: Vec3, q: Vec3| QuadEdge { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![p, q], weights: Vec::new() };
    let arc = |x: f64, from: f64, to: f64| {
        let half = 0.5 * (to - from);
        let w = half.cos();
        let mid = at(x, from + half) * (1.0 / w);
        let mid = Vec3::new(x, mid.y, mid.z);
        QuadEdge { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points: vec![at(x, from), mid, at(x, to)], weights: vec![1.0, w, 1.0] }
    };
    let am = 0.5 * (a0 + a1);
    let edges = vec![
        line(at(0.0, a0), at(3.0, a0)),
        line(at(3.0, a0), at(6.0, a0)),
        arc(6.0, a0, am),
        arc(6.0, am, a1),
        line(at(6.0, a1), at(3.0, a1)),
        line(at(3.0, a1), at(0.0, a1)),
        arc(0.0, a1, am),
        arc(0.0, am, a0),
    ];
    let copy = edges.clone();
    let mut frames = |j: usize, ts: &[f64]| -> Result<Vec<Frame>, String> {
        let c = Curve::new(&copy[j]).map_err(|e| e.to_string())?;
        ts.iter()
            .map(|t| {
                let [p, d1, _] = c.derivs(*t).map_err(|e| e.to_string())?;
                let n = Vec3::new(0.0, p.y, p.z) * (1.0 / r);
                let tan = d1.normalized().unwrap();
                // Into the hole: towards its middle, square to the edge in the tangent plane.
                let mid = at(3.0, am);
                let mut leave = n.cross(tan).normalized().unwrap();
                if leave.dot(mid - p) < 0.0 {
                    leave = leave * -1.0;
                }
                let ii = |x: Vec3, y: Vec3| {
                    let e = Vec3::new(0.0, -p.z, p.y) * (1.0 / r);
                    -x.dot(e) * y.dot(e) / r
                };
                Ok(Frame { normal: n, leave, form: [ii(tan, tan), ii(tan, leave), ii(leave, leave)] })
            })
            .collect()
    };
    let spec = QuadSpec { tol: 1e-9, angle_tol: 0.05f64.to_radians(), curvature_tol: 1e-3, fade: 1.0 };
    let fill = quad_fill(&edges, &mut frames, &spec).unwrap();
    eprintln!("cylinder window: centre {:?}, radius {}", fill.centre, fill.centre.y.hypot(fill.centre.z));
}
