//! The flat pattern's DXF keeps ellipses and free-form curves as ELLIPSE and SPLINE entities, and
//! a laser-cutting shop can use it as it is: layers by role, bend notes, letters cut or etched,
//! one file per sheet body.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};
use solidcraft_geom::{Seg2, Vec2};
use solidcraft_io::{DxfLinetype, DxfShape};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

/// A 100 × 60 base flange (the default 2.5 mm rule) with `curves` drawn in a sketch and cut through it.
fn plate_with(curves: &[(&str, Value)]) -> Session {
    plate_in(Session::default(), curves)
}

/// A 100 × 60 base flange in `s` (of its rule), with `curves` drawn in a sketch and cut through it.
fn plate_in(mut s: Session, curves: &[(&str, Value)]) -> Session {
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Base"}));
    if curves.is_empty() {
        return s;
    }
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Cut"}));
    for (command, params) in curves {
        run(&mut s, command, params.clone());
    }
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Cut", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    s
}

/// A session whose active rule is 2 mm sheet with an inner bend radius of 1 mm.
fn sheet_of_2mm() -> Session {
    let mut s = Session::default();
    run(&mut s, "sheet.manage_rules", json!({"name": "Sheet2", "thickness": "2 mm", "k_factor": "0.5", "bend_radius": "1 mm", "active": true}));
    s
}

/// A 100 × 60 tray of 2 mm sheet with `curves` cut through its base: three 20 mm walls bend up and
/// one bends down.
fn tray_with(curves: &[(&str, Value)]) -> Session {
    let mut s = plate_in(sheet_of_2mm(), curves);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2], [100, 30, 2], [50, 60, 2]], "height": 20}));
    run(&mut s, "sheet.flange", json!({"edges": [[0, 30, 2]], "height": 20, "flip": true}));
    s
}

/// What a tray is marked with: a Ø5 hole, a 20 × 5 slot and the label "AB-12", 10 mm high at (20, 28).
fn marks() -> Vec<(&'static str, Value)> {
    vec![
        ("sketch.circle.center", json!({"center": [15, 15], "diameter": 5})),
        ("sketch.rectangle.two_point", json!({"p0": [70, 40], "p1": [90, 45]})),
        ("sketch.text", json!({"text": "AB-12", "at": [20, 28], "height": 10})),
    ]
}

fn export(s: &mut Session, params: Value) -> String {
    run(s, "sheet.export_dxf", params)["dxf"].as_str().unwrap_or_default().to_string()
}

/// Entity observations from the real layered reader, with file-owned handles resolved.
/// This is not a DXF parser: unsupported input fails at read_dxf_file without fallback.
#[derive(Debug, PartialEq)]
struct LayeredEntity {
    layer: String,
    shape: DxfShape,
}

fn layered_entities(dxf: &str) -> Vec<LayeredEntity> {
    let raw = entities(dxf);
    let file = solidcraft_io::read_dxf_file(dxf).unwrap();
    let observed: Vec<_> = file.entities()
        .iter()
        .map(|entity| LayeredEntity {
            layer: file.layer(entity.layer()).unwrap().name().to_string(),
            shape: entity.geometry().clone(),
        })
        .collect();
    // The production reader skips paper space. These exporter tests promise
    // every ENTITIES record, including its original kind, layer and order.
    assert_eq!(observed.len(), raw.len(), "layered observations must cover every raw entity");
    for (raw, observed) in raw.iter().zip(&observed) {
        assert_eq!(raw.0.as_str(), kind(observed), "raw entity kind/order must be preserved");
        assert_eq!(layer_of(raw), observed.layer, "raw entity layer/order must be preserved");
    }
    observed
}

fn kind(entity: &LayeredEntity) -> &'static str {
    match &entity.shape {
        DxfShape::Point(_) => "POINT",
        DxfShape::Line(..) => "LINE",
        DxfShape::Polyline { .. } | DxfShape::BulgedPolyline { .. } => "LWPOLYLINE",
        DxfShape::Circle { .. } => "CIRCLE",
        DxfShape::Arc { .. } => "ARC",
        DxfShape::Ellipse { .. } => "ELLIPSE",
        DxfShape::Spline { .. } => "SPLINE",
        DxfShape::Text { .. } | DxfShape::AlignedText { .. } => "TEXT",
    }
}

fn layered_count(entities: &[LayeredEntity], name: &str) -> usize {
    entities.iter().filter(|entity| kind(entity) == name).count()
}

/// The same first location checked by the glyph tests: LINE start, curve centre,
/// or first spline control. Other shape kinds are not glyph output in these cases.
fn glyph_point(entity: &LayeredEntity) -> Vec2 {
    match &entity.shape {
        DxfShape::Line(start, _) => *start,
        DxfShape::Circle { c, .. } | DxfShape::Arc { c, .. } | DxfShape::Ellipse { c, .. } => *c,
        DxfShape::Spline { ctrl, .. } => *ctrl.first().unwrap(),
        shape => panic!("unexpected glyph shape: {shape:?}"),
    }
}

/// Serialization-only observations retained where DxfShape has no raw wire field.
/// The entities of a DXF text: type and group codes.
type Entity = (String, Vec<(i32, String)>);

fn entities(dxf: &str) -> Vec<Entity> {
    let lines: Vec<&str> = dxf.lines().collect();
    let pairs: Vec<(i32, &str)> = lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim()))).collect();
    let start = pairs.iter().position(|p| *p == (2, "ENTITIES")).unwrap_or(0);
    let mut out: Vec<Entity> = Vec::new();
    for (code, value) in pairs.iter().skip(start + 1) {
        match (*code, *value) {
            (0, "ENDSEC") => break,
            (0, kind) => out.push((kind.to_string(), Vec::new())),
            _ => {
                if let Some(last) = out.last_mut() {
                    last.1.push((*code, value.to_string()));
                }
            }
        }
    }
    out
}

fn values(e: &Entity, code: i32) -> Vec<f64> {
    e.1.iter().filter(|(c, _)| *c == code).filter_map(|(_, v)| v.parse().ok()).collect()
}

fn point(e: &Entity, code: i32) -> Vec2 {
    Vec2::new(values(e, code).first().copied().unwrap_or(f64::NAN), values(e, code + 10).first().copied().unwrap_or(f64::NAN))
}

/// All loops of the flat pattern (outlines, cut-outs, islands).
fn loops(s: &Session) -> Vec<solidcraft_geom::Loop2> {
    let regions = s.model.state().sheets[0].flat_regions().unwrap_or_default();
    regions.iter().flat_map(|r| std::iter::once(r.outer.clone()).chain(r.holes.clone())).collect()
}

/// The profiles of the DXF read into a new sketch: their areas, largest first.
fn reimported_areas(dxf: &str) -> Vec<f64> {
    let mut s = Session::default();
    let id = run(&mut s, "sketch.insert_dxf", json!({"text": dxf, "name": "Back"}))["sketch"].as_u64().unwrap_or(0);
    run(&mut s, "sketch.finish", json!({}));
    let mut areas: Vec<f64> = s.model.state().sketch(id).map(|ss| ss.profiles.iter().map(|p| p.area).collect()).unwrap_or_default();
    areas.sort_by(|a, b| b.total_cmp(a));
    areas
}

#[test]
fn layered_observations_reject_an_extra_paper_space_entity() {
    let model = concat!(
        "0\nSECTION\n2\nENTITIES\n",
        "0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n",
        "0\nENDSEC\n0\nEOF\n",
    );
    assert_eq!(layered_entities(model).len(), 1);
    let paper = "0\nLINE\n67\n1\n8\n0\n10\n2\n20\n0\n11\n3\n21\n0\n";
    let with_paper = model.replacen("0\nENDSEC\n", &format!("{paper}0\nENDSEC\n"), 1);
    assert_eq!(entities(&with_paper).len(), 2);
    assert_eq!(solidcraft_io::read_dxf_file(&with_paper).unwrap().entities().len(), 1);
    let rejected = std::panic::catch_unwind(|| layered_entities(&with_paper))
        .expect_err("the extra paper-space entity must fail the export observation");
    let message = rejected.downcast_ref::<String>().map(String::as_str)
        .or_else(|| rejected.downcast_ref::<&str>().copied()).unwrap_or_default();
    assert!(message.contains("layered observations must cover every raw entity"), "{message}");
}

#[test]
fn an_elliptic_cut_out_is_one_ellipse() {
    let mut s = plate_with(&[("sketch.ellipse", json!({"center": [40, 30], "major": [55, 30], "minor_radius": 8}))]);
    let dxf = export(&mut s, json!({}));
    let es = layered_entities(&dxf);
    // The four lines of the outline and one ELLIPSE: no other entity.
    assert_eq!((layered_count(&es, "LINE"), layered_count(&es, "ELLIPSE"), es.len()), (4, 1, 5), "{:?}", es.iter().map(kind).collect::<Vec<_>>());
    // By hand: centre (40, 30), the major semi-axis 15 along x (to (55, 30) or (25, 30)), the
    // minor one 8, so the ratio is 8/15; the whole turn runs from parameter 0 to 2π.
    let e = es.iter().find(|e| kind(e) == "ELLIPSE").unwrap();
    let DxfShape::Ellipse { c, major, ratio, t0, t1 } = &e.shape else { panic!("expected ELLIPSE: {e:?}") };
    assert!(c.dist(Vec2::new(40.0, 30.0)) < 1e-9 && (major.x.abs() - 15.0).abs() < 1e-9 && major.y.abs() < 1e-9, "{c:?} {major:?}");
    assert!((*ratio - 8.0 / 15.0).abs() < 1e-9 && *t0 == 0.0 && (*t1 - TAU).abs() < 1e-9);
    // The model does not preserve whether the default normal was explicit.
    let wire = entities(&dxf);
    let wire_ellipse = wire.iter().find(|e| e.0 == "ELLIPSE").unwrap();
    assert_eq!(values(wire_ellipse, 230), [1.0]);
    // Our reader gets the same ellipse back, and the same plate: 6000 mm² less π·15·8.
    let read = solidcraft_io::read_dxf(&dxf).unwrap();
    assert!(read.iter().any(|g| matches!(g, solidcraft_io::Geom2::Ellipse { c, major, minor }
        if c.dist(Vec2::new(40.0, 30.0)) < 1e-9 && (major.dist(*c) - 15.0).abs() < 1e-9 && (minor - 8.0).abs() < 1e-9)));
    let areas = reimported_areas(&dxf);
    assert!(areas.len() == 2 && (areas[0] - (6000.0 - PI * 120.0)).abs() < 1e-6 && (areas[1] - PI * 120.0).abs() < 1e-6, "{areas:?}");
}

#[test]
fn a_spline_cut_out_is_the_splines_it_was() {
    let points = json!([[20, 15], [40, 45], [65, 40], [85, 20]]);
    let mut s = plate_with(&[("sketch.spline.fit_point", json!({"points": points})), ("sketch.line", json!({"points": [[85, 20], [20, 15]]}))]);
    let dxf = export(&mut s, json!({}));
    let es = layered_entities(&dxf);
    // The cut-out's segments of the flat pattern, one entity each: its cubic pieces are SPLINEs
    // of degree 3 over the same four control points, its line a LINE.
    let hole: Vec<Seg2> = loops(&s).get(1).map(|l| l.segs.clone()).unwrap_or_default();
    let cubics: Vec<[Vec2; 4]> =
        hole.iter().filter_map(|g| if let Seg2::Cubic { p0, p1, p2, p3 } = g { Some([*p0, *p1, *p2, *p3]) } else { None }).collect();
    assert!(!cubics.is_empty() && cubics.len() + 1 == hole.len(), "{hole:?}");
    assert_eq!((layered_count(&es, "LINE"), layered_count(&es, "SPLINE"), es.len()), (5, cubics.len(), 5 + cubics.len()));
    for (spline, want) in es.iter().filter(|e| kind(e) == "SPLINE").zip(&cubics) {
        let DxfShape::Spline { degree, knots, ctrl, weights } = &spline.shape else { panic!("expected SPLINE: {spline:?}") };
        assert_eq!((*degree, knots.len(), ctrl.len()), (3, 8, 4));
        assert!(weights.is_none());
        assert_eq!(knots.as_slice(), [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        assert!(want.iter().enumerate().all(|(i, p)| (ctrl[i].x - p.x).abs() < 1e-9 && (ctrl[i].y - p.y).abs() < 1e-9), "{ctrl:?} {want:?}");
    }
    // Planar flag 8 and the literal wire counts are not retained by DxfShape.
    let wire = entities(&dxf);
    for spline in wire.iter().filter(|e| e.0 == "SPLINE") {
        assert_eq!((values(spline, 70), values(spline, 71), values(spline, 72), values(spline, 73)), (vec![8.0], vec![3.0], vec![8.0], vec![4.0]));
    }
    // The same plate comes back.
    let areas = reimported_areas(&dxf);
    let plate = s.model.state().sheets[0].flat_regions().unwrap_or_default()[0].area();
    assert!((areas[0] - plate).abs() < 1e-6, "{areas:?} {plate}");
}

#[test]
fn a_g4_blend_cut_out_is_one_spline_of_degree_nine_and_closes() {
    // Three sides of a 20 × 20 square, closed on the right by a G4 blend from (40, 20) to (40, 40):
    // one Bézier of degree 9 over ten control points (solidcraft#5269).
    let mut s = plate_with(&[]);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Cut"}));
    let mut line =
        |a: [i32; 2], b: [i32; 2]| run(&mut s, "sketch.line", json!({"points": [a, b]}))["curves"][0].as_str().unwrap_or_default().to_string();
    let (bottom, top) = (line([20, 20], [40, 20]), line([40, 40], [20, 40]));
    line([20, 40], [20, 20]);
    run(&mut s, "sketch.blend_curve", json!({"a": format!("{bottom}.end"), "b": format!("{top}.start"), "continuity": "g4"}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Cut", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    let hole: Vec<Seg2> = loops(&s).get(1).map(|l| l.segs.clone()).unwrap_or_default();
    let blend: Vec<Vec2> = hole.iter().find_map(|g| if let Seg2::Bezier { n: 9, p } = g { Some(p[..10].to_vec()) } else { None }).unwrap_or_default();
    assert_eq!((blend.len(), hole.len()), (10, 4), "{hole:?}");

    // One SPLINE of degree 9 with all ten control points: its ends are the blend's own.
    let dxf = export(&mut s, json!({}));
    let es = layered_entities(&dxf);
    assert_eq!((layered_count(&es, "LINE"), layered_count(&es, "SPLINE"), es.len()), (7, 1, 8));
    let spline = es.iter().find(|e| kind(e) == "SPLINE").unwrap();
    let DxfShape::Spline { degree, knots, ctrl, weights } = &spline.shape else { panic!("expected SPLINE: {spline:?}") };
    assert_eq!((*degree, knots.len(), ctrl.len()), (9, 20, 10));
    assert!(weights.is_none());
    // Preserve the original exact wire flags/counts in addition to typed geometry.
    let wire = entities(&dxf);
    let wire_spline = wire.iter().find(|e| e.0 == "SPLINE").unwrap();
    assert_eq!((values(wire_spline, 70), values(wire_spline, 71), values(wire_spline, 72), values(wire_spline, 73)), (vec![8.0], vec![9.0], vec![20.0], vec![10.0]));
    assert!(ctrl.len() == 10 && ctrl.iter().zip(&blend).all(|(a, b)| a.dist(*b) < 1e-9), "{ctrl:?} {blend:?}");
    let ends = [ctrl[0], ctrl[9]];
    assert!(ends.contains(&Vec2::new(40.0, 20.0)) && ends.contains(&Vec2::new(40.0, 40.0)), "{ends:?}");

    // The cut-out closes, so the same plate and the same cut-out come back.
    let areas = reimported_areas(&dxf);
    let plate = s.model.state().sheets[0].flat_regions().unwrap_or_default()[0].area();
    assert!(areas.len() == 2 && (areas[0] - plate).abs() < 1e-6 && (areas[1] - (6000.0 - plate)).abs() < 1e-6, "{areas:?} {plate}");
}

#[test]
fn text_is_splines_and_lines_not_chords() {
    let mut s = plate_with(&[("sketch.text", json!({"text": "AB", "at": [30, 20], "height": 12}))]);
    let dxf = export(&mut s, json!({}));
    let es = layered_entities(&dxf);
    // One entity for each segment of the flat pattern: a straight one a LINE, a glyph's
    // quadratic and cubic curves SPLINEs of degree 2 and 3.
    let segs: Vec<Seg2> = loops(&s).iter().flat_map(|l| l.segs.clone()).collect();
    let n = |f: &dyn Fn(&Seg2) -> bool| segs.iter().filter(|g| f(g)).count();
    let (lines, quadratics, cubics) = (
        n(&|g| matches!(g, Seg2::Line { .. })),
        n(&|g| matches!(g, Seg2::Conic { w, .. } if (*w - 1.0).abs() < 1e-12)),
        n(&|g| matches!(g, Seg2::Cubic { .. })),
    );
    assert_eq!(lines + quadratics + cubics, segs.len(), "only lines and Béziers");
    assert_eq!((layered_count(&es, "LINE"), layered_count(&es, "SPLINE"), es.len()), (lines, quadratics + cubics, segs.len()));
    let degree = |d: u8| es.iter().filter(|e| matches!(&e.shape, DxfShape::Spline { degree, .. } if *degree == d)).count();
    assert_eq!((degree(2), degree(3)), (quadratics, cubics));
    // The plate with the letters cut out comes back from the file with its area (the largest region; the islands inside the
    // letters come in no fixed order).
    let areas = reimported_areas(&dxf);
    let plate = s.model.state().sheets[0].flat_regions().unwrap_or_default().iter().map(|r| r.area()).fold(0.0, f64::max);
    assert!((areas[0] - plate).abs() < 1e-3, "{areas:?} {plate}");
}

#[test]
fn arcs_option_writes_only_lines_and_tangent_arcs_within_a_thousandth() {
    let mut s = plate_with(&[("sketch.ellipse", json!({"center": [40, 30], "major": [55, 30], "minor_radius": 8}))]);
    let dxf = export(&mut s, json!({"curves": "arcs"}));
    let es = layered_entities(&dxf);
    assert!(es.iter().all(|e| kind(e) == "LINE" || kind(e) == "ARC"), "{:?}", es.iter().map(kind).collect::<Vec<_>>());
    let arcs: Vec<(Vec2, f64, [Vec2; 2], Vec<Vec2>)> = es
        .iter()
        .filter(|e| kind(e) == "ARC")
        .map(|e| {
            let DxfShape::Arc { c, r, a0, a1 } = &e.shape else { panic!("expected ARC: {e:?}") };
            let (c, r, (a0, a1)) = (*c, *r, (a0.to_radians(), a1.to_radians()));
            let sweep = (a1 - a0).rem_euclid(TAU);
            let samples = (0..=20).map(|k| c + Vec2::from_angle(a0 + sweep * f64::from(k) / 20.0) * r).collect::<Vec<Vec2>>();
            (c, r, [c + Vec2::from_angle(a0) * r, c + Vec2::from_angle(a1) * r], samples)
        })
        .collect();
    // Far fewer than the 336 chords of 0.001 mm.
    assert!(arcs.len() >= 8 && arcs.len() < 100, "{} arcs", arcs.len());
    // Distance to the ellipse ((x − 40)/15)² + ((y − 30)/8)² = 1, to first order |f| / |∇f|.
    let off = |p: Vec2| {
        let (x, y) = (p.x - 40.0, p.y - 30.0);
        let f = (x / 15.0).powi(2) + (y / 8.0).powi(2) - 1.0;
        f.abs() / Vec2::new(2.0 * x / 225.0, 2.0 * y / 64.0).len()
    };
    // (The joint of a biarc's two arcs lies within the tolerance of the curve, not on it.)
    for (_, _, ends, samples) in &arcs {
        let offs: Vec<f64> = samples.iter().map(|p| off(*p)).collect();
        assert!(offs.iter().all(|d| *d <= 1e-3 + 1e-6), "{ends:?}: {offs:?}");
    }
    // Tangent: at each joint the radii of the two arcs lie on one line.
    for (i, (c, r, ends, _)) in arcs.iter().enumerate() {
        for end in ends {
            let others: Vec<_> = arcs.iter().enumerate().filter(|(j, (_, _, e, _))| *j != i && e.iter().any(|q| q.dist(*end) < 1e-6)).collect();
            assert_eq!(others.len(), 1, "joint {end:?}");
            let (_, (c2, r2, _, _)) = others[0];
            assert!((*end - *c).cross(*end - *c2).abs() < 1e-6 * r * r2, "arcs are not tangent at {end:?}");
        }
    }
    // The plate, read back, is the exact one within the perimeter times 0.001 mm.
    let areas = reimported_areas(&dxf);
    assert!((areas[0] - (6000.0 - PI * 120.0)).abs() < 0.1, "{areas:?}");
}

#[test]
fn arcs_option_has_no_splines_for_text_and_the_curves_value_is_checked() {
    let mut s = plate_with(&[("sketch.text", json!({"text": "AB", "at": [30, 20], "height": 12}))]);
    let es = layered_entities(&export(&mut s, json!({"curves": "ARCS"})));
    assert!(es.iter().all(|e| kind(e) == "LINE" || kind(e) == "ARC") && layered_count(&es, "ARC") > 0);
    let exact = layered_entities(&export(&mut s, json!({"curves": "exact"})));
    assert!(layered_count(&exact, "SPLINE") > 0 && layered_count(&exact, "ARC") == 0);
    assert!(s.execute("sheet.export_dxf", &json!({"curves": "chords"})).is_err());
}

#[test]
fn bend_lines_and_layers_are_still_there() {
    let mut s = plate_with(&[("sketch.ellipse", json!({"center": [40, 30], "major": [55, 30], "minor_radius": 8}))]);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 20}));
    let out = run(&mut s, "sheet.export_dxf", json!({}));
    let es = layered_entities(out["dxf"].as_str().unwrap_or_default());
    let on = |layer: &str| es.iter().filter(|e| e.layer == layer).count();
    assert_eq!(on("BEND_UP") + on("BEND_DOWN"), out["bends"].as_u64().unwrap_or(0) as usize);
    // The ellipse is a cut-out, on CUT; the outline is the plate's.
    assert!(on("BEND_UP") + on("BEND_DOWN") > 0 && layered_count(&es, "ELLIPSE") == 1 && on("CUT") == 1);
    assert_eq!(on("OUTLINE") + on("CUT"), es.len() - on("BEND_UP") - on("BEND_DOWN"));
}

/// The layer an entity is on.
fn layer_of(e: &Entity) -> String {
    e.1.iter().find(|(c, _)| *c == 8).map(|(_, v)| v.clone()).unwrap_or_default()
}

/// The text of an entity's group code.
fn string(e: &Entity, code: i32) -> String {
    e.1.iter().find(|(c, _)| *c == code).map(|(_, v)| v.clone()).unwrap_or_default()
}

/// What the entities of a layer are, without their handles and the layer: to compare layers of two files.
fn on_layer(es: &[Entity], layer: &str) -> Vec<(String, Vec<(i32, String)>)> {
    es.iter().filter(|e| layer_of(e) == layer).map(|e| (e.0.clone(), e.1.iter().filter(|(c, _)| ![5, 8].contains(c)).cloned().collect())).collect()
}

/// The LAYER table: name → (colour, linetype).
fn layer_table(dxf: &str) -> std::collections::BTreeMap<String, (i64, String)> {
    let file = solidcraft_io::read_dxf_file(dxf).unwrap();
    let observed = file.layers()
        .iter()
        .map(|layer| {
            let linetype = match layer.linetype() {
                DxfLinetype::Continuous => "Continuous",
                DxfLinetype::Dashed => "DASHED",
                DxfLinetype::Hidden => "HIDDEN",
            };
            (layer.name().to_string(), (i64::from(layer.color()), linetype.to_string()))
        })
        .collect();
    // The model initializes layer 0 even if its declaration was omitted.
    // Retain the original wire expectation until that presence is observable.
    assert_eq!(observed, serialized_layer_table(dxf));
    observed
}

/// The LAYER table: name → (colour, linetype).
fn serialized_layer_table(dxf: &str) -> std::collections::BTreeMap<String, (i64, String)> {
    let lines: Vec<&str> = dxf.lines().collect();
    let pairs: Vec<(i32, &str)> = lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim()))).collect();
    let start = pairs.iter().position(|p| *p == (2, "LAYER")).unwrap_or(0);
    let mut records: Vec<Vec<(i32, &str)>> = Vec::new();
    for p in pairs.iter().skip(start + 1) {
        match *p {
            (0, "ENDTAB") => break,
            (0, _) => records.push(Vec::new()),
            _ => {
                if let Some(record) = records.last_mut() {
                    record.push(*p);
                }
            }
        }
    }
    records
        .iter()
        .filter_map(|r| {
            let get = |code: i32| r.iter().find(|(c, _)| *c == code).map(|(_, v)| *v);
            Some((get(2)?.to_string(), (get(62)?.parse().ok()?, get(6)?.to_string())))
        })
        .collect()
}

fn vec2(v: &Value) -> Vec2 {
    Vec2::new(v[0].as_f64().unwrap_or(f64::NAN), v[1].as_f64().unwrap_or(f64::NAN))
}

#[test]
fn the_flat_pattern_is_on_layers_by_role_and_inner_contours_come_first() {
    let mut s = tray_with(&marks());
    let out = run(&mut s, "sheet.export_dxf", json!({}));
    let dxf = out["dxf"].as_str().unwrap_or_default();
    let es = layered_entities(dxf);
    let on = |layer: &str| es.iter().filter(|e| e.layer == layer).collect::<Vec<_>>();
    // OUTLINE: the tray's outer contour alone, one entity for each of its segments (lines and arcs).
    let regions = s.model.state().sheets[0].flat_regions().unwrap_or_default();
    let plate =
        regions.iter().max_by(|a, b| a.area().total_cmp(&b.area())).map(|r| r.outer.clone()).unwrap_or(solidcraft_geom::Loop2 { segs: Vec::new() });
    assert_eq!(on("OUTLINE").len(), solidcraft_io::loop_shapes(&plate).len());
    assert!(on("OUTLINE").iter().all(|e| kind(e) == "LINE" || kind(e) == "ARC"), "{:?}", on("OUTLINE").iter().map(|e| kind(e)).collect::<Vec<_>>());
    // CUT: what is cut out inside it that is no text: the Ø5 hole as one CIRCLE, the slot's four lines.
    let cut = on("CUT");
    assert_eq!((layered_count_of(&cut, "CIRCLE"), layered_count_of(&cut, "LINE"), cut.len()), (1, 4, 5), "{:?}", cut.iter().map(|e| kind(e)).collect::<Vec<_>>());
    // TEXT: the letters of "AB-12" and the counters of A and B, all within the label's box (5 letters
    // of at most 10 mm from (20, 28)).
    let letters = on("TEXT");
    assert!(letters.len() > 20, "{} letter entities", letters.len());
    for e in &letters {
        let p = glyph_point(e);
        assert!(p.x > 19.0 && p.x < 80.0 && p.y > 27.0 && p.y < 39.0, "{} at {p:?}", kind(e));
    }
    // The bend lines by direction: three walls up, one down.
    assert_eq!((on("BEND_UP").len(), on("BEND_DOWN").len(), out["bends"].as_u64()), (3, 1, Some(4)));
    // Inner contours are written before the outline, so a program that follows the file cuts inside-out.
    let last_inner = es.iter().rposition(|e| matches!(e.layer.as_str(), "CUT" | "TEXT"));
    let first_outline = es.iter().position(|e| e.layer == "OUTLINE");
    let (Some(last_inner), Some(first_outline)) = (last_inner, first_outline) else { panic!("{last_inner:?} {first_outline:?}") };
    assert!(last_inner < first_outline, "an inner contour at {last_inner}, the outline from {first_outline}");
    // The layers, with the colours and linetypes the guide gives; nothing else is declared.
    let want = [
        ("0", 7, "Continuous"),
        ("OUTLINE", 7, "Continuous"),
        ("CUT", 4, "Continuous"),
        ("TEXT", 2, "Continuous"),
        ("BEND_UP", 1, "DASHED"),
        ("BEND_DOWN", 5, "DASHED"),
    ];
    assert_eq!(layer_table(dxf), want.map(|(n, c, l)| (n.to_string(), (c, l.to_string()))).into_iter().collect());
}

/// How many entities of `name` there are among `list`.
fn layered_count_of(list: &[&LayeredEntity], name: &str) -> usize {
    list.iter().filter(|entity| kind(entity) == name).count()
}

#[test]
fn letters_can_be_etched_instead_of_cut() {
    let mut s = tray_with(&marks());
    let cut = entities(&export(&mut s, json!({})));
    let etched_dxf = export(&mut s, json!({"text": "etch"}));
    let etched = entities(&etched_dxf);
    // The same contours, on ETCH instead of TEXT: nothing of them is cut.
    assert!(!on_layer(&cut, "TEXT").is_empty());
    assert_eq!(on_layer(&etched, "ETCH"), on_layer(&cut, "TEXT"));
    assert!(on_layer(&etched, "TEXT").is_empty());
    // The hole and the slot are no text: they stay where they were, as do the outline and the bends.
    for layer in ["OUTLINE", "CUT", "BEND_UP", "BEND_DOWN"] {
        assert_eq!(on_layer(&etched, layer), on_layer(&cut, layer), "{layer}");
    }
    let layers = layer_table(&etched_dxf);
    assert_eq!(layers.get("ETCH"), Some(&(6, "Continuous".to_string())));
    assert!(!layers.contains_key("TEXT"));
    // `cut` is the default, and a value that is neither is refused.
    assert_eq!(export(&mut s, json!({"text": "cut"})), export(&mut s, json!({})));
    assert!(s.execute("sheet.export_dxf", &json!({"text": "fill"})).is_err());
}

#[test]
fn engraved_text_is_etched_and_other_pockets_keep_their_depth_layer() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Base"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Marks"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [70, 40], "p1": [90, 45]}));
    run(&mut s, "sketch.text", json!({"text": "AB-12", "at": [20, 28], "height": 10}));
    run(&mut s, "sketch.finish", json!({}));
    // Part way through the sheet: pockets, 1 mm deep from the bottom.
    run(&mut s, "solid.extrude", json!({"sketch": "Marks", "distance": 1, "operation": "cut"}));
    let dxf = export(&mut s, json!({}));
    let es = layered_entities(&dxf);
    // The rectangle is a pocket of the depth its layer names; the letters are a marking, on ETCH.
    let pocket = "POCKET_BOTTOM_Z0.000000_TO_1.000000_MM";
    let on = |layer: &str| es.iter().filter(|e| e.layer == layer).collect::<Vec<_>>();
    assert_eq!((layered_count_of(&on(pocket), "LINE"), on(pocket).len()), (4, 4));
    let letters = on("ETCH");
    assert!(letters.len() > 20, "{} letter entities", letters.len());
    for e in &letters {
        let p = glyph_point(e);
        assert!(p.x > 19.0 && p.x < 80.0 && p.y > 27.0 && p.y < 39.0, "{} at {p:?}", kind(e));
    }
    let layers = layer_table(&dxf);
    assert!(layers.contains_key(pocket) && layers.contains_key("ETCH") && !layers.contains_key("TEXT"), "{layers:?}");
    // `text` is about letters cut through: it changes nothing here.
    assert_eq!(on_layer(&entities(&export(&mut s, json!({"text": "etch"}))), "ETCH"), on_layer(&entities(&dxf), "ETCH"));
}

#[test]
fn a_bend_note_sits_beside_each_bend_line_when_asked_for() {
    let mut s = tray_with(&[]);
    let flat = run(&mut s, "sheet.flat_pattern", json!({}));
    let bends = flat["bends"].as_array().cloned().unwrap_or_default();
    assert_eq!(bends.len(), 4);
    // Not unless asked for: a program that follows the file would etch them.
    let plain = export(&mut s, json!({}));
    assert!(entities(&plain).iter().all(|e| e.0 != "TEXT") && !layer_table(&plain).contains_key("BEND_NOTES"));
    let dxf = export(&mut s, json!({"bend_notes": true}));
    let notes: Vec<Entity> = entities(&dxf).into_iter().filter(|e| e.0 == "TEXT").collect();
    assert_eq!(notes.len(), 4);
    assert!(notes.iter().all(|e| layer_of(e) == "BEND_NOTES"));
    assert_eq!(layer_table(&dxf).get("BEND_NOTES"), Some(&(8, "Continuous".to_string())));
    for b in &bends {
        // The walls bend by 90° round an inner radius of 1 mm: "B1 UP 90° R1" (`%%d` is the degree sign).
        let up = b["direction"] == "up";
        let text = format!("B{} {} 90%%d R1", b["bend"].as_u64().unwrap_or(0), if up { "UP" } else { "DOWN" });
        let Some(note) = notes.iter().find(|e| string(e, 1) == text) else {
            panic!("no note `{text}` among {:?}", notes.iter().map(|e| string(e, 1)).collect::<Vec<_>>())
        };
        // Centred (72 = 1), 3 mm high, parallel to the line: along x it reads left to right, along y upwards.
        let (start, end) = (vec2(&b["start"]), vec2(&b["end"]));
        let angle = if (end - start).x.abs() > (end - start).y.abs() { 0.0 } else { 90.0 };
        assert!(values(note, 72) == [1.0] && values(note, 40) == [3.0] && (values(note, 50)[0] - angle).abs() < 1e-9, "{note:?}");
        // On the left of the way it reads, half its height from the middle of the line.
        let off = point(note, 11) - (start + end) * 0.5;
        assert!(off.dist(Vec2::from_angle(angle.to_radians()).perp() * 1.5) < 1e-9, "{text}: {off:?}");
    }
    // The height is the caller's.
    let tall = entities(&export(&mut s, json!({"bend_notes": true, "note_height": 5})));
    let note = tall.iter().find(|e| e.0 == "TEXT");
    assert!(note.is_some_and(|e| values(e, 40) == [5.0]));
}

#[test]
fn a_bend_note_gives_the_included_angle_when_the_rule_reads_it_so() {
    // The included angle is 180° less the bend angle: 122° for a 58° bend, 90° for a 90° one.
    for (deg, included) in [(58.0, 122.0), (90.0, 90.0)] {
        let mut plain = Vec::new();
        for display in ["bend", "included"] {
            let mut s = sheet_of_2mm();
            run(&mut s, "sheet.manage_rules", json!({"name": "Sheet2", "angle_display": display}));
            let mut s = plate_in(s, &[]);
            run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2]], "height": 20, "angle": format!("{deg} deg")}));
            let dxf = export(&mut s, json!({"bend_notes": true}));
            let notes: Vec<String> = entities(&dxf).iter().filter(|e| e.0 == "TEXT").map(|e| string(e, 1)).collect();
            let shown = if display == "included" { included } else { deg };
            assert_eq!(notes, [format!("B1 UP {shown}%%d R1")], "{deg}° under {display}");
            plain.push(export(&mut s, json!({})));
        }
        // Without notes the setting changes nothing in the file.
        assert_eq!(plain.first(), plain.get(1), "{deg}°");
    }
}

/// A body whose base flange names its rule keeps that rule's notes whichever rule is active when
/// the DXF is written, and a bend down reads its included angle the same way.
#[test]
fn a_bend_note_follows_the_bodys_own_rule_and_bends_down_too() {
    let mut s = sheet_of_2mm();
    run(&mut s, "sheet.manage_rules", json!({"name": "Sheet2", "angle_display": "included"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Base", "rule": "Sheet2"}));
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2]], "height": 20, "angle": "58 deg", "flip": true}));
    let notes = |s: &mut Session| -> Vec<String> {
        entities(&export(s, json!({"bend_notes": true}))).iter().filter(|e| e.0 == "TEXT").map(|e| string(e, 1)).collect()
    };
    assert_eq!(notes(&mut s), ["B1 DOWN 122%%d R1"]);
    run(&mut s, "sheet.manage_rules", json!({"name": "Other", "thickness": "2 mm", "angle_display": "bend", "active": true}));
    assert_eq!(notes(&mut s), ["B1 DOWN 122%%d R1"], "another active rule does not change this body's notes");
}

/// Two sheet bodies: a lid and, 200 mm away, a side wall with one bend.
fn lid_and_wall() -> Session {
    let mut s = Session::default();
    for (sketch, p0, p1, body) in [("Lid", [0, 0], [100, 60], "Lid"), ("Wall", [200, 0], [250, 40], "Side Wall")] {
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": sketch}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": p0, "p1": p1}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sheet.flange", json!({"sketch": sketch, "body_name": body}));
    }
    run(&mut s, "sheet.flange", json!({"edges": [[225, 0, 2.5]], "height": 15, "body": "Side Wall"}));
    s
}

#[test]
fn each_sheet_body_can_have_a_file_of_its_own() {
    let mut s = lid_and_wall();
    let r = run(&mut s, "sheet.export_dxf", json!({"per_body": true}));
    let files = r["files"].as_array().cloned().unwrap_or_default();
    assert_eq!(files.iter().map(|f| f["body"].as_str().unwrap_or_default()).collect::<Vec<_>>(), ["Lid", "Side Wall"]);
    assert_eq!(files.iter().map(|f| f["bends"].as_u64()).collect::<Vec<_>>(), [Some(0), Some(1)]);
    // Each is the file `body` asks for: the lid's has no bend line, the wall's has its one.
    for f in &files {
        let single = export(&mut s, json!({"body": f["body"]}));
        assert_eq!(f["dxf"].as_str(), Some(single.as_str()), "{}", f["body"]);
    }
    let bend_lines = |f: &Value| layered_entities(f["dxf"].as_str().unwrap_or_default()).iter().filter(|e| e.layer == "BEND_UP").count();
    assert_eq!((bend_lines(&files[0]), bend_lines(&files[1])), (0, 1));
    // With a path, each body is written to a file named after it, beside the path, and no text comes back.
    let dir = solidcraft_io::vfs::temp_dir().join(format!("solidcraft-dxf-per-body-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_default();
    let path = dir.join("panel.dxf").to_string_lossy().to_string();
    let r = run(&mut s, "sheet.export_dxf", json!({"per_body": true, "path": path}));
    let written = r["files"].as_array().cloned().unwrap_or_default();
    let names: Vec<_> = written
        .iter()
        .map(|f| std::path::Path::new(f["path"].as_str().unwrap_or_default()).file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    assert_eq!(names, [Some("panel-Lid.dxf".to_string()), Some("panel-Side Wall.dxf".to_string())]);
    for (f, written) in files.iter().zip(&written) {
        assert!(written.get("dxf").is_none());
        let on_disk = std::fs::read_to_string(written["path"].as_str().unwrap_or_default()).unwrap_or_default();
        assert_eq!(f["dxf"].as_str(), Some(on_disk.as_str()));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_export_options_are_refused_not_ignored() {
    let mut s = tray_with(&[]);
    for p in [
        json!({"text": "fill"}),
        json!({"text": 1}),
        json!({"bend_notes": "yes"}),
        json!({"bend_notes": 1}),
        json!({"note_height": 0}),
        json!({"note_height": -3}),
        json!({"note_height": 1e9}),
        json!({"note_height": "tall"}),
        json!({"per_body": "yes"}),
        json!({"per_body": true, "body": "Body1"}),
    ] {
        assert!(s.execute("sheet.export_dxf", &p).is_err(), "{p}");
    }
    // Nothing was left half done.
    assert!(s.execute("sheet.export_dxf", &json!({})).is_ok());
}

#[test]
fn the_file_of_a_body_is_named_after_it_and_no_two_share_a_name() {
    // (Windows writes the separator it adds as `\`.)
    let names = |path: &str, bodies: &[&str]| -> Vec<String> {
        super::body_paths(path, &bodies.iter().map(|b| b.to_string()).collect::<Vec<_>>()).into_iter().map(|p| p.replace('\\', "/")).collect()
    };
    assert_eq!(names("out/panel.dxf", &["Lid", "Side Wall"]), ["out/panel-Lid.dxf", "out/panel-Side Wall.dxf"]);
    // The extension is the path's, and .dxf when it has none.
    assert_eq!(names("out/panel", &["Lid"]), ["out/panel-Lid.dxf"]);
    assert_eq!(names("panel.DXF", &["Lid"]), ["panel-Lid.DXF"]);
    // What a file name cannot hold becomes `_`; a name that is nothing but that stays a name.
    assert_eq!(names("p.dxf", &["a/b:c", "  ", "wall."]), ["p-a_b_c.dxf", "p-body.dxf", "p-wall.dxf"]);
    // Two bodies that would share a file (in any case) get a number.
    assert_eq!(names("p.dxf", &["Lid", "lid", "LID", "Lid_2"]), ["p-Lid.dxf", "p-lid_2.dxf", "p-LID_3.dxf", "p-Lid_2_2.dxf"]);
}

/// #4314: a cutter that cuts every line gets a file without the bend lines, or with everything
/// on layer 0. The default keeps the role layers and the dashed bend lines.
#[test]
fn bend_lines_can_be_left_out_and_everything_put_on_layer_0() {
    let mut s = tray_with(&marks());
    let default = layered_entities(&export(&mut s, json!({})));
    let bends = |es: &[LayeredEntity]| es.iter().filter(|e| matches!(e.layer.as_str(), "BEND_UP" | "BEND_DOWN")).count();
    assert_eq!(bends(&default), 4, "the tray's four bends");
    let without = layered_entities(&export(&mut s, json!({"bend_lines": false})));
    assert_eq!(bends(&without), 0);
    assert_eq!(without.len(), default.len() - 4, "only the bend lines go");
    let single = layered_entities(&export(&mut s, json!({"layers": "single"})));
    assert_eq!(single.len(), default.len());
    assert!(single.iter().all(|e| e.layer == "0"), "{:?}", single.iter().map(|e| &e.layer).collect::<Vec<_>>());
    for bad in [json!({"bend_lines": "no"}), json!({"layers": "one"})] {
        assert!(s.execute("sheet.export_dxf", &bad).is_err(), "{bad}");
    }
}
