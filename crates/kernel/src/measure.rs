use serde::Serialize;
use solidcraft_geom::{Aabb3, Vec3};

use crate::Result;
use crate::body::Body;
use crate::topo::{TopoCounts, merged_topology};

/// Measured properties of a body.
#[derive(Clone, Debug, Serialize)]
pub struct BodyMeasure {
    pub volume: f64,
    pub area: f64,
    pub centroid: Vec3,
    pub bbox: Aabb3,
    /// Raw kernel topology counts (closed curves and surfaces may be split in pieces).
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub shells: usize,
    /// Counts after merging split pieces of the same analytic surface/curve (comparable with
    /// systems that use periodic faces and edges).
    pub merged: TopoCounts,
    /// What volume, area and centre come from: `smlib` (exact, integrated on the surfaces) or
    /// `mesh` (a mesh body's own triangles).
    pub measured_by: &'static str,
    /// Anything the measurement says about itself.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Volume, area, centre and box of a body, as [`measure`] works them out, without the topology
/// counts.
#[derive(Clone, Debug, Serialize)]
pub struct MassMeasure {
    pub volume: f64,
    pub area: f64,
    pub centroid: Vec3,
    pub bbox: Aabb3,
    /// As [`BodyMeasure::measured_by`].
    pub measured_by: &'static str,
    /// As [`BodyMeasure::notes`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// The relative accuracy SMLib integrates at: the finest of SmApiBrepComputeMassProperties'
/// documented range ("clamped to [1e-4, 1e-1]", SmApiQueries.h). Its cost is "governed by
/// dRelativeAccuracy".
const ACCURACY: f64 = 1e-4;

/// Measure a body. A solid body's volume, area and centre are SMLib's, integrated on its exact
/// surfaces, and its box is SMLib's exact box; a body SMLib cannot measure is an error. A mesh
/// body is measured from its triangles, exactly.
pub fn measure(b: &Body) -> Result<BodyMeasure> {
    if let Some((pos, tris)) = b.triangle_mesh() {
        // Exact for a mesh body; topology counts are facets, sides and vertices.
        let m = b.mesh(&crate::MeshTolerance::chord(1.0))?;
        let mm = m.measure();
        let edges = b.mesh.as_ref().map(|m| m.edge_count()).unwrap_or(0);
        let merged =
            TopoCounts { faces: tris.len(), edges, vertices: pos.len(), face_types: [("mesh".to_string(), tris.len())].into_iter().collect() };
        return Ok(BodyMeasure {
            volume: mm.volume,
            area: mm.area,
            centroid: mm.centroid,
            bbox: m.bounds(),
            faces: tris.len(),
            edges,
            vertices: pos.len(),
            shells: 1,
            merged,
            measured_by: "mesh",
            notes: Vec::new(),
        });
    }
    let m = measure_mass(b)?;
    let topo = b.mesh(&crate::MeshTolerance::chord((b.size() * 1e-3).max(1e-3)))?;
    Ok(BodyMeasure {
        volume: m.volume,
        area: m.area,
        centroid: m.centroid,
        bbox: m.bbox,
        faces: b.face_count(),
        edges: b.edge_count(),
        vertices: b.vertex_count(),
        shells: b.shell_count(),
        merged: merged_topology(b, &topo)?,
        measured_by: m.measured_by,
        notes: m.notes,
    })
}

/// [`measure`]'s volume, area, centre and box alone (for callers that need no topology counts,
/// such as an interference check).
pub fn measure_mass(b: &Body) -> Result<MassMeasure> {
    if b.triangle_mesh().is_some() {
        let m = measure(b)?;
        return Ok(MassMeasure { volume: m.volume, area: m.area, centroid: m.centroid, bbox: m.bbox, measured_by: m.measured_by, notes: m.notes });
    }
    let m = crate::guard("SMLib mass properties", || {
        b.smlib()?.mass_properties(ACCURACY).map_err(|e| crate::KernelError::Failed(format!("SMLib mass properties: {e}")))
    })?;
    let [x, y, z] = m.centroid;
    Ok(MassMeasure { volume: m.volume, area: m.area, centroid: Vec3::new(x, y, z), bbox: b.exact_bounds()?, measured_by: "smlib", notes: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoolOp, boolean, cylinder, torus};

    /// The mass properties alone are [`measure`]'s, to the bit; and on a curved intersection
    /// they are the textbook ones.
    #[test]
    fn mass_alone_is_the_full_measure() {
        let ring = torus(Vec3::ZERO, 10.0, 2.0).unwrap();
        let post = cylinder(Vec3::new(10.0, 0.0, -5.0), Vec3::Z, 1.5, 10.0).unwrap();
        let shared = boolean(&ring, &post, BoolOp::Intersect).unwrap().unwrap();
        for b in [&ring, &shared] {
            let (full, mass) = (measure(b).unwrap(), measure_mass(b).unwrap());
            assert_eq!(full.volume.to_bits(), mass.volume.to_bits());
            assert_eq!(full.area.to_bits(), mass.area.to_bits());
            assert_eq!(full.centroid, mass.centroid);
            assert_eq!((full.bbox.min, full.bbox.max), (mass.bbox.min, mass.bbox.max));
        }
        let want = 2.0 * std::f64::consts::PI.powi(2) * 10.0 * 4.0;
        let got = measure_mass(&ring).unwrap().volume;
        assert!((got - want).abs() < 1e-3 * want, "{got} against {want}");
        assert!(measure(&ring).unwrap().merged.faces > 0);
    }
}
