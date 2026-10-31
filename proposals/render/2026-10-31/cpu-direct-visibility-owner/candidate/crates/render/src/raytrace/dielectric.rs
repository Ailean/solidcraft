//! Original bounded smooth dielectric optics; no renderer source adapted.
//! Air is 1.0. Distances and absorption coefficients use millimetres.
use super::{Bounds, Result, TraceError, Triangle, unit};
use crate::{ProductScene, job::DielectricOptions};
use solidcraft_geom::Vec3;

pub const MAX_OVERRIDES: usize = 8;
pub const MAX_BOUNDARIES: u8 = 8;
pub const MAX_PATHS: u16 = 511;

#[derive(Clone, Copy, Debug)]
pub struct Interface {
    pub reflected: Vec3,
    pub transmitted: Option<Vec3>,
    pub reflectance: f64,
}

/// Exact unpolarized dielectric Fresnel and Snell, with the normal towards
/// the incident medium. Equal indices have no interface, including grazing.
pub fn interface(direction: Vec3, normal: Vec3, incident: f64, transmitted: f64) -> Result<Interface> {
    polarized_interface(direction, normal, incident, transmitted).map(|(optics, _)| optics)
}

fn polarized_interface(direction: Vec3, normal: Vec3, incident: f64, transmitted: f64) -> Result<(Interface, [f64; 2])> {
    if ![incident, transmitted].into_iter().all(|n| n.is_finite() && (1.0..=3.0).contains(&n)) {
        return Err(TraceError::InvalidInput("dielectric IOR outside 1..3"));
    }
    let d = unit(direction).ok_or(TraceError::InvalidInput("dielectric direction"))?;
    let n = unit(normal).ok_or(TraceError::InvalidInput("dielectric normal"))?;
    let cosine = -d.dot(n);
    if cosine < 0.0 {
        return Err(TraceError::InvalidInput("dielectric incident hemisphere"));
    }
    let cosine = cosine.clamp(0.0, 1.0);
    let reflected = unit(d + n * (2.0 * cosine)).ok_or(TraceError::InvalidInput("reflected direction"))?;
    if incident == transmitted {
        return Ok((Interface { reflected, transmitted: Some(d), reflectance: 0.0 }, [0.0; 2]));
    }
    let ratio = incident / transmitted;
    let sine2 = ratio * ratio * (1.0 - cosine * cosine).max(0.0);
    if sine2 >= 1.0 {
        return Ok((Interface { reflected, transmitted: None, reflectance: 1.0 }, [1.0; 2]));
    }
    let cos_t = (1.0 - sine2).sqrt();
    let rs = (incident * cosine - transmitted * cos_t) / (incident * cosine + transmitted * cos_t);
    let rp = (transmitted * cosine - incident * cos_t) / (transmitted * cosine + incident * cos_t);
    let refracted = unit(d * ratio + n * (ratio * cosine - cos_t)).ok_or(TraceError::InvalidInput("refracted direction"))?;
    let polarized = [(rs * rs).clamp(0.0, 1.0), (rp * rp).clamp(0.0, 1.0)];
    Ok((Interface { reflected, transmitted: Some(refracted), reflectance: (polarized[0] + polarized[1]) * 0.5 }, polarized))
}

/// Incoherent parallel-sheet returns for unpolarized incident light. Sum each
/// polarization's internal returns before averaging; virtual thickness affects
/// absorption only. This closure does not transport Stokes state between hits.
pub(super) fn thin_sheet(direction: Vec3, normal: Vec3, material: &DielectricOptions) -> Result<(Interface, Vec3, Vec3)> {
    let direction = unit(direction).ok_or(TraceError::InvalidInput("dielectric direction"))?;
    let normal = unit(normal).ok_or(TraceError::InvalidInput("dielectric normal"))?;
    let (optics, polarized) = polarized_interface(direction, normal, 1.0, material.ior)?;
    let inside = optics.transmitted.ok_or(TraceError::InvalidInput("air to sheet transmission"))?;
    let cos_t = -inside.dot(normal);
    // Zero virtual thickness has zero optical distance, including the index-matched
    // grazing limit. A positive thickness with no crossing direction is undefined.
    let distance = if material.thickness_mm == 0.0 {
        0.0
    } else if cos_t > 0.0 {
        material.thickness_mm / cos_t
    } else {
        return Err(TraceError::InvalidInput("grazing sheet thickness is unsupported"));
    };
    let a = attenuation(material.absorption_per_mm, distance)?;
    let weights = |a: f64| {
        let series = |f: f64| {
            let denominator = 1.0 - f * f * a * a;
            if denominator == 0.0 { (1.0, 0.0) } else { (f + (1.0 - f).powi(2) * f * a * a / denominator, (1.0 - f).powi(2) * a / denominator) }
        };
        let s = series(polarized[0]);
        let p = series(polarized[1]);
        ((s.0 + p.0) * 0.5, (s.1 + p.1) * 0.5)
    };
    let (rr, tr) = weights(a.x);
    let (rg, tg) = weights(a.y);
    let (rb, tb) = weights(a.z);
    Ok((Interface { transmitted: Some(direction), ..optics }, Vec3::new(rr, rg, rb), Vec3::new(tr, tg, tb)))
}

pub fn attenuation(sigma: [f64; 3], distance_mm: f64) -> Result<Vec3> {
    if !distance_mm.is_finite()
        || !(0.0..=super::MAX_DISTANCE_MM).contains(&distance_mm)
        || !sigma.into_iter().all(|v| v.is_finite() && (0.0..=100.0).contains(&v))
    {
        return Err(TraceError::InvalidInput("dielectric absorption or distance"));
    }
    let [r, g, b] = sigma.map(|s| (-s * distance_mm).exp());
    Ok(Vec3::new(r, g, b))
}

pub fn multiply(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}

#[derive(Clone, Copy, Debug)]
pub enum Certificate {
    ThinSheet,
    ClosedBox(Bounds),
}

/// A strict geometric subset, never a trusted mesh.closed flag. Only twelve
/// outward corner triangles, two complementary halves per box face, qualify.
pub fn certify(scene: &ProductScene, input: &DielectricOptions) -> Result<Certificate> {
    let body = scene.meshes.get(input.body_index).ok_or(TraceError::InvalidInput("dielectric body ordinal"))?;
    if body.name != input.body_name {
        return Err(TraceError::InvalidInput("dielectric captured body identity mismatch"));
    }
    let mesh = &body.mesh;
    let closed = matches!(input.boundary, crate::job::DielectricBoundary::ClosedBox);
    if (!closed && !(1..=2).contains(&mesh.triangles.len())) || (closed && mesh.triangles.len() != 12) || mesh.positions.len() > 36 {
        return Err(TraceError::InvalidInput("unsupported dielectric boundary tessellation"));
    }
    let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = -lo;
    let mut tris = [None; 12];
    for (slot, indices) in tris.iter_mut().zip(&mesh.triangles) {
        let mut vertices = [Vec3::ZERO; 3];
        for (v, index) in vertices.iter_mut().zip(indices) {
            *v = *mesh.positions.get(*index as usize).ok_or(TraceError::InvalidInput("dielectric vertex index"))?;
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        let triangle = Triangle::new(vertices)?;
        if triangle.geometric_normal().is_none() {
            return Err(TraceError::InvalidInput("degenerate dielectric boundary"));
        }
        *slot = Some(triangle);
    }
    if !closed {
        let first = tris.first().copied().flatten().ok_or(TraceError::InvalidInput("empty thin sheet"))?;
        let normal = first.geometric_normal().ok_or(TraceError::InvalidInput("thin sheet normal"))?;
        let [anchor, _, _] = first.vertices();
        for triangle in tris.into_iter().flatten() {
            if triangle.geometric_normal() != Some(normal) || triangle.vertices().into_iter().any(|p| (p - anchor).dot(normal) != 0.0) {
                return Err(TraceError::InvalidInput("thin sheet must be one planar consistently wound surface"));
            }
        }
        if let Some(second) = tris.get(1).copied().flatten() {
            let first = first.vertices();
            let second = second.vertices();
            let mut shared = [Vec3::ZERO; 2];
            let mut shared_count = 0usize;
            let mut other_first = None;
            let mut other_second = None;
            for p in first {
                if second.contains(&p) {
                    *shared.get_mut(shared_count).ok_or(TraceError::InvalidInput("duplicate thin sheet triangle"))? = p;
                    shared_count += 1;
                } else {
                    other_first = Some(p);
                }
            }
            for p in second {
                if !first.contains(&p) {
                    other_second = Some(p);
                }
            }
            let [a, b] = shared;
            let c = other_first.ok_or(TraceError::InvalidInput("thin sheet shared edge"))?;
            let d = other_second.ok_or(TraceError::InvalidInput("thin sheet shared edge"))?;
            if shared_count != 2 || (b - a).cross(c - a).dot(normal) * (b - a).cross(d - a).dot(normal) >= 0.0 {
                return Err(TraceError::InvalidInput("thin sheet triangles overlap or have no unique shared edge"));
            }
        }
        return Ok(Certificate::ThinSheet);
    }
    if lo.x >= hi.x || lo.y >= hi.y || lo.z >= hi.z {
        return Err(TraceError::InvalidInput("closed box needs positive extents"));
    }
    let bounds = Bounds::new(lo, hi)?;
    let mut masks = [[0u8; 2]; 6];
    let mut counts = [0usize; 6];
    for triangle in tris.into_iter().flatten() {
        let vertices = triangle.vertices();
        let n = triangle.geometric_normal().ok_or(TraceError::InvalidInput("box normal"))?;
        let axis = if n.x.abs() == 1.0 {
            0
        } else if n.y.abs() == 1.0 {
            1
        } else if n.z.abs() == 1.0 {
            2
        } else {
            return Err(TraceError::InvalidInput("box face is not axis aligned"));
        };
        let positive = super::component(n, axis) > 0.0;
        let plane = super::component(if positive { hi } else { lo }, axis);
        let face = axis * 2 + usize::from(positive);
        let mut mask = 0u8;
        for p in vertices {
            if super::component(p, axis) != plane {
                return Err(TraceError::InvalidInput("box winding or face plane"));
            }
            let mut corner = 0;
            for (bit, a) in [(0, (axis + 1) % 3), (1, (axis + 2) % 3)] {
                let value = super::component(p, a);
                if value == super::component(hi, a) {
                    corner |= 1 << bit;
                } else if value != super::component(lo, a) {
                    return Err(TraceError::InvalidInput("box vertex is not a corner"));
                }
            }
            mask |= 1 << corner;
        }
        let count = counts.get_mut(face).ok_or(TraceError::Internal("box face count"))?;
        let target = masks.get_mut(face).and_then(|pair| pair.get_mut(*count)).ok_or(TraceError::InvalidInput("too many box face triangles"))?;
        *target = mask;
        *count += 1;
    }
    for (count, [a, b]) in counts.into_iter().zip(masks) {
        // Three distinct corners each; missing corners must be opposite. Thus
        // both triangles share a diagonal and tile, rather than overlap.
        let missing_a = (!a) & 15;
        let missing_b = (!b) & 15;
        if count != 2
            || a.count_ones() != 3
            || b.count_ones() != 3
            || (a | b) != 15
            || !matches!((missing_a, missing_b), (1, 8) | (8, 1) | (2, 4) | (4, 2))
        {
            return Err(TraceError::InvalidInput("incomplete or ambiguous closed box face"));
        }
    }
    Ok(Certificate::ClosedBox(bounds))
}

pub fn contains(bounds: Bounds, point: Vec3) -> bool {
    let lo = bounds.min();
    let hi = bounds.max();
    point.x >= lo.x && point.x <= hi.x && point.y >= lo.y && point.y <= hi.y && point.z >= lo.z && point.z <= hi.z
}

pub fn overlaps(a: Bounds, b: Bounds) -> bool {
    let lo = a.min().max(b.min());
    let hi = a.max().min(b.max());
    lo.x <= hi.x && lo.y <= hi.y && lo.z <= hi.z
}

/// Immediate parents of already-certified axis-aligned boxes. Strict containment
/// is sufficient for this exact box subset; touching and partial overlap refuse.
/// Eight overrides bound this to at most 64 comparisons, without allocation.
pub(super) fn parents(certificates: &[Option<Certificate>; MAX_OVERRIDES], nested: bool) -> Result<[Option<usize>; MAX_OVERRIDES]> {
    let mut result = [None; MAX_OVERRIDES];
    for (child, certificate) in certificates.iter().enumerate() {
        let Some(Certificate::ClosedBox(bounds)) = certificate else { continue };
        for (candidate, certificate) in certificates.iter().enumerate() {
            let Some(Certificate::ClosedBox(container)) = certificate else { continue };
            if candidate == child || !overlaps(*bounds, *container) {
                continue;
            }
            if !nested {
                return Err(TraceError::InvalidInput("nested, touching or overlapping dielectric volumes are unsupported"));
            }
            if strictly_contains(*container, *bounds) {
                let parent = result.get_mut(child).ok_or(TraceError::Internal("dielectric child index"))?;
                let replace = match *parent {
                    None => true,
                    Some(previous) => match certificates.get(previous).copied().flatten() {
                        Some(Certificate::ClosedBox(old)) => strictly_contains(old, *container),
                        _ => return Err(TraceError::Internal("dielectric parent certificate")),
                    },
                };
                if replace {
                    *parent = Some(candidate);
                }
            } else if !strictly_contains(*bounds, *container) {
                return Err(TraceError::InvalidInput("dielectric boxes must be disjoint or strictly nested; touching and overlap are unsupported"));
            }
        }
    }
    Ok(result)
}

fn strictly_contains(outer: Bounds, inner: Bounds) -> bool {
    let lo = outer.min();
    let hi = outer.max();
    let a = inner.min();
    let b = inner.max();
    lo.x < a.x && lo.y < a.y && lo.z < a.z && hi.x > b.x && hi.y > b.y && hi.z > b.z
}

/// A certified axis-aligned face point, shared by propagation and Beer intervals.
/// This is used only by the explicit nested-box path, never by general spawn.
#[derive(Clone, Copy)]
pub(super) struct ExactBoxFace {
    pub point: Vec3,
    axis: usize,
    outward: f64,
    entering: bool,
}

impl ExactBoxFace {
    pub fn spawn(self, direction: Vec3, reflected: bool) -> Result<super::Ray> {
        let ray = super::Ray::new(self.point, direction, 0.0_f64.next_up(), super::MAX_DISTANCE_MM)?;
        let side = super::component(ray.direction(), self.axis) * self.outward;
        let exterior = if reflected { self.entering } else { !self.entering };
        if (exterior && side <= 0.0) || (!exterior && side >= 0.0) {
            return Err(TraceError::InvalidInput("nested box continuation is tangent or on the wrong side"));
        }
        Ok(ray)
    }
}

/// Recover the exact constant-axis coordinate from the actual certified face.
/// Tangential coordinates are validated, never clamped or displaced. Equal
/// ray/other-face depths conservatively refuse independently of ordinal ties.
pub(super) fn exact_box_face(bounds: Bounds, vertices: [Vec3; 3], normal: Vec3, position: Vec3, incoming: super::Ray) -> Result<ExactBoxFace> {
    let mut face = None;
    for axis in 0..3 {
        let [a, b, c] = vertices.map(|v| super::component(v, axis));
        if a == b && b == c {
            if face.is_some() {
                return Err(TraceError::InvalidInput("nested box face has ambiguous constant axes"));
            }
            face = Some((axis, a));
        }
    }
    let (axis, plane) = face.ok_or(TraceError::InvalidInput("nested box face is not axis aligned"))?;
    let lo = bounds.min();
    let hi = bounds.max();
    let outward = if plane == super::component(lo, axis) {
        -1.0
    } else if plane == super::component(hi, axis) {
        1.0
    } else {
        return Err(TraceError::InvalidInput("nested box face disagrees with its certificate"));
    };
    let axis_vector = match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        2 => Vec3::Z,
        _ => return Err(TraceError::Internal("nested box face axis")),
    };
    if normal != axis_vector * outward || Triangle::new(vertices)?.geometric_normal() != Some(normal) {
        return Err(TraceError::InvalidInput("nested box outward normal disagrees with its certified face"));
    }
    for vertex in vertices {
        for a in 0..3 {
            let coordinate = super::component(vertex, a);
            if coordinate != super::component(lo, a) && coordinate != super::component(hi, a) {
                return Err(TraceError::InvalidInput("nested box face vertex is not a certified corner"));
            }
        }
    }
    let point = match axis {
        0 => Vec3::new(plane, position.y, position.z),
        1 => Vec3::new(position.x, plane, position.z),
        2 => Vec3::new(position.x, position.y, plane),
        _ => return Err(TraceError::Internal("nested box point axis")),
    };
    if !point.is_finite() {
        return Err(TraceError::InvalidInput("nested box reconstructed point is not finite"));
    }
    let direction = super::component(incoming.direction(), axis);
    if direction == 0.0 {
        return Err(TraceError::InvalidInput("nested box incident ray is tangent"));
    }
    let depth = (plane - super::component(incoming.origin(), axis)) / direction;
    let [min, max] = incoming.interval();
    if !depth.is_finite() || depth < min || depth > max {
        return Err(TraceError::InvalidInput("nested box face is outside the incident ray interval"));
    }
    for a in 0..3 {
        if a == axis {
            continue;
        }
        let coordinate = super::component(point, a);
        if coordinate <= super::component(lo, a) || coordinate >= super::component(hi, a) {
            return Err(TraceError::InvalidInput("nested box edge, corner or outside-face point is ambiguous"));
        }
        let d = super::component(incoming.direction(), a);
        if d != 0.0 {
            for plane in [super::component(lo, a), super::component(hi, a)] {
                let other = (plane - super::component(incoming.origin(), a)) / d;
                if other == depth {
                    return Err(TraceError::InvalidInput("nested box simultaneous face depths are ambiguous"));
                }
            }
        }
    }
    Ok(ExactBoxFace { point, axis, outward, entering: direction * outward < 0.0 })
}

/// Hard crossing ceiling for eight disjoint certified boxes (two faces each).
pub const MAX_DIRECT_CROSSINGS: u8 = (MAX_OVERRIDES * 2) as u8;

#[derive(Clone, Copy)]
struct DirectHit {
    body: usize,
    vertices: [Vec3; 3],
    geometry: super::TriangleHit,
}

/// Incremental RGB visibility along ONE straight exterior-to-exterior connection.
/// This is a direct-light approximation, not refracted transport or caustics.
/// Selected optics are revalidated and certified against the borrowed capture.
/// Boxes must be disjoint; sheets touching/inside boxes and endpoints on/inside
/// boxes refuse. Unselected geometry blocks. The interval is [min,max): geometry
/// exactly at a finite light endpoint does not block that light.
///
/// No BVH tie API exists for this consumer yet. A pass scans the captured source,
/// one triangle (or body transition) per step. Equal-depth different bodies refuse
/// rather than silently skipping an opaque blocker behind a selected boundary.
/// Cost is O((crossings+1)*(triangles+bodies)), not accelerated traversal. Every
/// step debits the caller's cumulative work and checks cancellation. Memory is
/// fixed, with borrowed scene/options. Valid setup/steps create no containers;
/// invalid options may construct the existing validator's bounded error String.
/// The future prepared-scene consumer passes its existing `PreparedScene.source`.
pub struct DirectVisibility<'a> {
    source: &'a ProductScene,
    options: &'a crate::job::RaytraceOptions,
    state: DirectVisibilityState,
}

// Entirely private, reference-free and deliberately neither Clone nor Copy.
struct DirectVisibilityState {
    certificates: [Option<Certificate>; MAX_OVERRIDES],
    ray: super::Ray,
    body: usize,
    triangle: usize,
    scanned: usize,
    nearest: Option<DirectHit>,
    ambiguous: bool,
    optical: bool,
    medium: Option<(usize, Vec3)>,
    sheets: u8,
    transmittance: Vec3,
    crossings: u8,
    crossing_limit: u8,
    terminal: bool,
}

impl<'a> DirectVisibility<'a> {
    /// Bounded setup: at most eight small certificates and 64 overlap checks.
    /// Setup also spends the SAME caller-owned budget; no fresh work credit.
    pub fn new(
        source: &'a ProductScene,
        options: &'a crate::job::RaytraceOptions,
        ray: super::Ray,
        crossing_limit: u8,
        work: &mut super::WorkBudget,
        cancel: &crate::job::CancelToken,
    ) -> Result<Self> {
        work.tick(cancel)?;
        if !(1..=MAX_DIRECT_CROSSINGS).contains(&crossing_limit) || source.meshes.len() > crate::job::MAX_CPU_BODIES {
            return Err(TraceError::InvalidInput("direct visibility crossing or body bound"));
        }
        options.validate().map_err(|_| TraceError::InvalidInput("invalid direct visibility options"))?;
        let mut certificates = [None; MAX_OVERRIDES];
        let start = ray.at(ray.interval()[0])?;
        let end = ray.at(ray.interval()[1])?;
        for (slot, input) in certificates.iter_mut().zip(&options.dielectrics) {
            work.tick(cancel)?;
            let certificate = certify(source, input)?;
            if let Certificate::ClosedBox(bounds) = certificate {
                if contains(bounds, ray.origin()) || contains(bounds, start) || contains(bounds, end) {
                    return Err(TraceError::InvalidInput("direct visibility endpoints must be outside certified boxes"));
                }
            }
            *slot = Some(certificate);
        }
        for (i, a) in certificates.iter().take(options.dielectrics.len()).enumerate() {
            for (j, b) in certificates.iter().take(options.dielectrics.len()).enumerate().skip(i + 1) {
                work.tick(cancel)?;
                let (Some(a), Some(b)) = (a, b) else { continue };
                let bounds = |slot: usize, certificate: Certificate| -> Result<Bounds> {
                    match certificate {
                        Certificate::ClosedBox(bounds) => Ok(bounds),
                        Certificate::ThinSheet => {
                            let input = options.dielectrics.get(slot).ok_or(TraceError::Internal("direct certificate slot"))?;
                            let body = source.meshes.get(input.body_index).ok_or(TraceError::Internal("direct certificate body"))?;
                            let bounds = body.mesh.bounds();
                            Bounds::new(bounds.min, bounds.max)
                        }
                    }
                };
                if (matches!(a, Certificate::ClosedBox(_)) || matches!(b, Certificate::ClosedBox(_)))
                    && overlaps(bounds(i, *a)?, bounds(j, *b)?)
                {
                    return Err(TraceError::InvalidInput("direct visibility boxes must be disjoint from other selected boundaries"));
                }
            }
        }
        super::check_cancel(cancel)?;
        Ok(Self {
            source, options, state: DirectVisibilityState {
                certificates, ray, body: 0, triangle: 0, scanned: 0,
                nearest: None, ambiguous: false, optical: false, medium: None, sheets: 0,
                transmittance: Vec3::new(1.0, 1.0, 1.0), crossings: 0, crossing_limit, terminal: false,
            },
        })
    }

    /// None means real work remains. Only Some(rgb) is a completed connection.
    /// Work/crossing exhaustion, cancellation and ambiguity are errors, never black
    /// or white fallback values. Success OR error makes this cursor terminal.
    pub fn step(&mut self, work: &mut super::WorkBudget, cancel: &crate::job::CancelToken) -> Result<Option<Vec3>> {
        if self.state.terminal {
            return Err(TraceError::InvalidInput("direct visibility cursor is terminal"));
        }
        let result = self.advance(work, cancel);
        if !matches!(result, Ok(None)) {
            self.state.terminal = true;
        }
        result
    }

    fn advance(&mut self, work: &mut super::WorkBudget, cancel: &crate::job::CancelToken) -> Result<Option<Vec3>> {
        work.tick(cancel)?;
        if !self.state.optical {
            if let Some(body) = self.source.meshes.get(self.state.body) {
                // A certified sheet is ONE plane. Its two triangles can round
                // their depths differently at a shared edge; it must never be
                // attenuated twice. Skip that entire immutable certified body
                // after its first crossing, not a numerical distance interval.
                if self.options.dielectrics.iter().enumerate().any(|(slot, input)| input.body_index == self.state.body && self.state.sheets & (1u8 << slot) != 0) {
                    self.state.body += 1;
                    self.state.triangle = 0;
                    return Ok(None);
                }
                if let Some(indices) = body.mesh.triangles.get(self.state.triangle) {
                    self.state.scanned = self.state.scanned.checked_add(1).filter(|n| *n <= crate::job::MAX_CPU_TRIANGLES)
                        .ok_or(TraceError::WorkLimit)?;
                    let mut vertices = [Vec3::ZERO; 3];
                    for (point, index) in vertices.iter_mut().zip(indices) {
                        *point = *body.mesh.positions.get(*index as usize).ok_or(TraceError::InvalidInput("direct visibility vertex index"))?;
                    }
                    self.state.triangle += 1;
                    if let Some(geometry) = Triangle::new(vertices)?.intersect(&self.state.ray) {
                        if geometry.distance < self.state.ray.interval()[1] {
                            let hit = DirectHit { body: self.state.body, vertices, geometry };
                            match self.state.nearest {
                                None => self.state.nearest = Some(hit),
                                Some(old) if geometry.distance < old.geometry.distance => {
                                    self.state.nearest = Some(hit);
                                    self.state.ambiguous = false;
                                }
                                Some(old) if geometry.distance == old.geometry.distance && old.body != self.state.body => self.state.ambiguous = true,
                                _ => {}
                            }
                        }
                    }
                } else {
                    self.state.body += 1;
                    self.state.triangle = 0;
                }
                return Ok(None);
            }
            self.state.optical = true;
            return Ok(None);
        }
        if self.state.ambiguous {
            return Err(TraceError::InvalidInput("coincident direct visibility bodies are ambiguous"));
        }
        let Some(hit) = self.state.nearest else {
            return if self.state.medium.is_some() {
                Err(TraceError::InvalidInput("direct visibility missed its certified box exit"))
            } else {
                Ok(Some(self.state.transmittance))
            };
        };
        let Some((slot, input)) = self.options.dielectrics.iter().enumerate().find(|(_, input)| input.body_index == hit.body) else {
            return Ok(Some(Vec3::ZERO));
        };
        if self.state.crossings == self.state.crossing_limit {
            return Err(TraceError::WorkLimit);
        }
        let outward = hit.geometry.geometric_normal;
        let entering = outward.dot(self.state.ray.direction()) < 0.0;
        let normal = if entering { outward } else { -outward };
        match self.state.certificates.get(slot).copied().flatten().ok_or(TraceError::Internal("direct visibility certificate missing"))? {
            Certificate::ThinSheet => {
                if self.state.medium.is_some() {
                    return Err(TraceError::InvalidInput("direct visibility sheet inside a box"));
                }
                let (_, _, transmission) = thin_sheet(self.state.ray.direction(), normal, input)?;
                self.state.transmittance = multiply(self.state.transmittance, transmission);
                self.state.sheets |= 1u8 << slot;
            }
            Certificate::ClosedBox(bounds) => {
                let [a, b, c] = hit.vertices;
                let [wa, wb, wc] = hit.geometry.barycentric;
                let face = exact_box_face(bounds, hit.vertices, outward, a * wa + b * wb + c * wc, self.state.ray)?;
                // External-angle Fresnel on BOTH straight connections. The ray is
                // not bent, so an internal critical angle here would invent TIR.
                let transmit = 1.0 - interface(self.state.ray.direction(), normal, 1.0, input.ior)?.reflectance;
                self.state.transmittance = self.state.transmittance * transmit;
                if entering {
                    if self.state.medium.is_some() {
                        return Err(TraceError::InvalidInput("direct visibility entered overlapping boxes"));
                    }
                    self.state.medium = Some((slot, face.point));
                } else {
                    let (active, start) = self.state.medium.ok_or(TraceError::InvalidInput("direct visibility exited without entry"))?;
                    if active != slot {
                        return Err(TraceError::InvalidInput("direct visibility box exit identity mismatch"));
                    }
                    let delta = face.point - start;
                    let scale = delta.x.abs().max(delta.y.abs()).max(delta.z.abs());
                    if scale == 0.0 || !scale.is_finite() {
                        return Err(TraceError::InvalidInput("direct visibility box segment is degenerate"));
                    }
                    let distance = scale * (delta / scale).len2().sqrt();
                    self.state.transmittance = multiply(self.state.transmittance, attenuation(input.absorption_per_mm, distance)?);
                    self.state.medium = None;
                }
            }
        }
        if !self.state.transmittance.is_finite() || [self.state.transmittance.x, self.state.transmittance.y, self.state.transmittance.z].into_iter().any(|v| !(0.0..=1.0).contains(&v)) {
            return Err(TraceError::InvalidInput("invalid direct visibility transmittance"));
        }
        self.state.crossings += 1;
        if self.state.transmittance == Vec3::ZERO {
            return Ok(Some(Vec3::ZERO));
        }
        // Preserve the original origin AND normalized direction. Calling Ray::new
        // again could renormalize it and change the tie/interval calculations.
        self.state.ray.min = hit.geometry.distance.next_up();
        if self.state.ray.min >= self.state.ray.max {
            return if self.state.medium.is_some() {
                Err(TraceError::InvalidInput("direct visibility interval cannot resolve the exit"))
            } else {
                Ok(Some(self.state.transmittance))
            };
        }
        self.state.body = 0;
        self.state.triangle = 0;
        self.state.scanned = 0;
        self.state.nearest = None;
        self.state.ambiguous = false;
        self.state.optical = false;
        Ok(None)
    }
}

/// Owns one immutable capture and at most one direct-light connection inline.
/// Moving this owner moves its scene/options and reference-free cursor together.
/// No state or mutable capture access escapes: certificates cannot be rebound to
/// a different scene/options pair by a sibling consumer. Construction only moves
/// the capture; `begin` retains the original validation and caller-owned ledger.
/// The existing borrowed `DirectVisibility` API remains independently available.
pub(super) struct DirectVisibilityOwner {
    source: ProductScene,
    options: crate::job::RaytraceOptions,
    state: Option<DirectVisibilityState>,
}

impl DirectVisibilityOwner {
    pub(super) fn new(source: ProductScene, options: crate::job::RaytraceOptions) -> Self {
        Self { source, options, state: None }
    }

    pub(super) fn source(&self) -> &ProductScene {
        &self.source
    }

    pub(super) fn options(&self) -> &crate::job::RaytraceOptions {
        &self.options
    }

    /// Start explicitly on this owner's capture. Never replace an active scan.
    /// A failed constructor preserves the previous idle or terminal state.
    pub(super) fn begin(
        &mut self,
        ray: super::Ray,
        crossing_limit: u8,
        work: &mut super::WorkBudget,
        cancel: &crate::job::CancelToken,
    ) -> Result<()> {
        if self.state.as_ref().is_some_and(|state| !state.terminal) {
            return Err(TraceError::InvalidInput("direct visibility connection is active"));
        }
        let state = DirectVisibility::new(&self.source, &self.options, ray, crossing_limit, work, cancel)?.state;
        self.state = Some(state);
        Ok(())
    }

    /// Advance once, rebinding only to the immutable capture owned here.
    /// Retain terminal state on success AND error; only explicit `begin` starts
    /// another connection. Rebinding itself has no work, validation or allocation.
    pub(super) fn step(&mut self, work: &mut super::WorkBudget, cancel: &crate::job::CancelToken) -> Result<Option<Vec3>> {
        let state = self.state.take().ok_or(TraceError::InvalidInput("direct visibility connection has not begun"))?;
        let mut cursor = DirectVisibility { source: &self.source, options: &self.options, state };
        let result = cursor.step(work, cancel);
        self.state = Some(cursor.state);
        result
    }
}

#[cfg(test)]
#[path = "dielectric_tests.rs"]
pub(super) mod tests;
