//! Generated roles follow source curves. Geometry is an ownership witness,
//! never part of a saved identity. Ambiguous split/merged patches stay unresolved.
use super::*;
use brepkit_math::{nurbs::surface::NurbsSurface, surfaces::{ConicalSurface, CylindricalSurface}};

#[derive(Clone)]
pub(super) struct GeneratedFace {
    pub key: String,
    pub source: String,
    pub surface: FaceSurface,
    pub boundary: Vec<EdgeAnchor>,
    pub frame: Option<Plane>,
}

pub(super) fn transformed_surface(surface: &FaceSurface, matrix: &Mat4) -> Result<FaceSurface> {
    let vector = |v: Vec3| Vec3::new(
        dot([matrix.0[0][0], matrix.0[0][1], matrix.0[0][2]], [v.x(), v.y(), v.z()]),
        dot([matrix.0[1][0], matrix.0[1][1], matrix.0[1][2]], [v.x(), v.y(), v.z()]),
        dot([matrix.0[2][0], matrix.0[2][1], matrix.0[2][2]], [v.x(), v.y(), v.z()]),
    );
    Ok(match surface {
        FaceSurface::Plane { normal, d } => {
            let point = matrix.mul_point(p3([0.; 3]) + *normal * (*d / normal.dot(*normal)));
            let normal = vector(*normal);
            FaceSurface::Plane { normal, d: normal.dot(point - p3([0.; 3])) }
        }
        FaceSurface::Cylinder(c) => FaceSurface::Cylinder(CylindricalSurface::with_ref_dir(
            matrix.mul_point(c.origin()), vector(c.axis()), c.radius(), vector(c.x_axis())).map_err(err)?),
        FaceSurface::Cone(c) => FaceSurface::Cone(ConicalSurface::with_ref_dir(
            matrix.mul_point(c.apex()), vector(c.axis()), c.half_angle(), vector(c.x_axis())).map_err(err)?),
        FaceSurface::Nurbs(n) => FaceSurface::Nurbs(NurbsSurface::new(n.degree_u(), n.degree_v(),
            n.knots_u().to_vec(), n.knots_v().to_vec(),
            n.control_points().iter().map(|row| row.iter().map(|p| matrix.mul_point(*p)).collect()).collect(),
            n.weights().to_vec()).map_err(err)?),
        _ => return Err("Generated reference surface cannot be placed precisely".into()),
    })
}

fn same_surface(a: &FaceSurface, b: &FaceSurface) -> bool {
    const EPS: f64 = 1e-7;
    match (a, b) {
        (FaceSurface::Plane { normal:a, d:ad }, FaceSurface::Plane { normal:b, d:bd }) =>
            a.cross(*b).length() < 1e-9 && (ad - bd * a.dot(*b).signum()).abs() < EPS,
        (FaceSurface::Cylinder(a), FaceSurface::Cylinder(b)) =>
            a.axis().cross(b.axis()).length() < 1e-9 && (a.radius() - b.radius()).abs() < EPS
                && (a.origin() - b.origin()).cross(a.axis()).length() < EPS,
        (FaceSurface::Cone(a), FaceSurface::Cone(b)) =>
            (a.apex() - b.apex()).length() < EPS && (a.axis() - b.axis()).length() < 1e-9
                && (a.half_angle() - b.half_angle()).abs() < 1e-9,
        (FaceSurface::Nurbs(a), FaceSurface::Nurbs(b)) =>
            a.degree_u() == b.degree_u() && a.degree_v() == b.degree_v()
                && a.knots_u() == b.knots_u() && a.knots_v() == b.knots_v()
                && a.weights() == b.weights()
                && a.control_points().len() == b.control_points().len()
                && a.control_points().iter().zip(b.control_points()).all(|(a,b)|
                    a.len() == b.len() && a.iter().zip(b).all(|(a,b)| (*a - *b).length() < EPS)),
        _ => false,
    }
}

impl Geometry {
    pub(super) fn generated_face_key(&self, face: FaceId, allowed: impl Fn(&str)->bool) -> Result<Option<String>> {
        let surface = self.topo.face(face).map_err(err)?.surface();
        let candidates: Vec<_> = self.generated_faces.iter()
            .filter(|record| allowed(&record.source) && same_surface(&record.surface, surface)).collect();
        let identities: BTreeSet<_> = candidates.iter().map(|r| &r.key).collect();
        if identities.is_empty() { return Ok(None); }
        let edges = face_edges(&self.topo, face).map_err(err)?;
        // A single shared junction edge cannot distinguish coplanar neighbours.
        let required = if identities.len() == 1 { 1 } else { 2 };
        let matches: BTreeSet<_> = candidates.iter().filter(|record|
            record.boundary.iter().filter(|anchor| edges.iter().any(|id| boundary_survives(anchor, &self.topo, *id))).count() >= required)
            .map(|record| &record.key).collect();
        Ok((matches.len() == 1).then(|| (*matches.first().unwrap()).clone()))
    }

    /// Match construction-owned edges to the generated face before Booleans can
    /// trim it. This captures provenance without relying on kernel arena order.
    pub(super) fn register_generated_face(&mut self, source: &str, key: String, solid: SolidId,
        boundary: Vec<EdgeAnchor>) -> Result<()> {
        let candidates: Vec<_> = solid_faces(&self.topo, solid).map_err(err)?.into_iter().filter(|face|
            face_edges(&self.topo, *face).is_ok_and(|edges|
                boundary.iter().all(|anchor| edges.iter().any(|id| anchor.matches(&self.topo, *id)))))
            .collect();
        if candidates.len() != 1 { return Err("Generated face lost its exact source boundary".into()); }
        let face = self.topo.face(candidates[0]).map_err(err)?;
        let surface = face.surface().clone();
        let frame = if let FaceSurface::Plane { normal, .. } = &surface {
            let normal = *normal * if face.is_reversed() { -1. } else { 1. };
            let edge = boundary.iter().find(|e| e.circle.is_none()).ok_or("Generated planar side has no straight boundary")?;
            let u = (edge.end - edge.start).normalize().map_err(err)?;
            let array = |v:Vec3| [v.x(), v.y(), v.z()];
            Some(Plane { origin:[edge.start.x(), edge.start.y(), edge.start.z()],
                u:array(u), v:array(normal.cross(u)), normal:array(normal) })
        } else { None };
        self.generated_faces.push(GeneratedFace { key, source: source.into(), surface, boundary:boundary.clone(), frame });
        self.edge_anchors.extend(boundary);
        Ok(())
    }
}

fn boundary_survives(anchor:&EdgeAnchor, topo:&Topology, id:EdgeId)->bool {
    if anchor.matches(topo,id) { return true; }
    let Ok(edge)=topo.edge(id) else { return false; };
    match (anchor.circle,edge.curve()) {
        (Some((center,normal,radius,true)),EdgeCurve::Circle(c)) =>
            (center-c.center()).length()<1e-7 && normal.cross(c.normal()).length()<1e-9
                && (radius-c.radius()).abs()<1e-7,
        (None,EdgeCurve::Line) => {
            let direction=anchor.end-anchor.start;let length=direction.length();
            if length<1e-7 { return false; }
            [edge.start(),edge.end()].into_iter().all(|id|topo.vertex(id).is_ok_and(|vertex|{
                let delta=vertex.point()-anchor.start;
                delta.cross(direction).length()/length<1e-7
                    && delta.dot(direction)>=-1e-7*length
                    && delta.dot(direction)<=length*length+1e-7*length
            }))
        }
        _=>false,
    }
}

pub(super) fn line(source: &str, key: String, a: brepkit_math::vec::Point3,
    b: brepkit_math::vec::Point3) -> EdgeAnchor {
    EdgeAnchor { source:source.into(), key, side:None, start:a, end:b, midpoint:a+(b-a)*0.5, circle:None }
}

pub(super) fn circle(source: &str, key: String, center: brepkit_math::vec::Point3,
    normal: Vec3, radius: f64, start: brepkit_math::vec::Point3) -> EdgeAnchor {
    EdgeAnchor { source:source.into(), key, side:None, start, end:start,
        midpoint:center-(start-center), circle:Some((center,normal,radius,true)) }
}
