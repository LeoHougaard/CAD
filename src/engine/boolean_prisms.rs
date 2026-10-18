//! Uniform extrusions use the existing exact planar arrangement, avoiding GFA
//! coincident-cap failures and removing internal boundaries before extrusion.
use super::*;
use brepkit_math::vec::Point3;
use sketch::ResolvedEntity;

impl Geometry {
    pub(super) fn boolean_solid(
        &mut self,
        op: ops::boolean::BooleanOp,
        a: SolidId,
        b: SolidId,
    ) -> Result<SolidId> {
        let faces = solid_faces(&self.topo, a).map_err(err)?;
        let mut axes = vec![];
        for face in &faces {
            if let FaceSurface::Cylinder(c) = self.topo.face(*face).map_err(err)?.surface() {
                axes.push(c.axis());
            }
        }
        if axes.is_empty() {
            for face in faces {
                if let FaceSurface::Plane { normal, .. } =
                    self.topo.face(face).map_err(err)?.surface()
                {
                    axes.push(*normal);
                }
            }
        }
        for (index, &axis) in axes.iter().enumerate() {
            if axes[..index].iter().any(|&other| other == axis || other == -axis) {
                continue;
            }
            let Some((cap, lo, hi)) = self.prism_cap(a, axis)? else {
                continue;
            };
            let Some((other, blo, bhi)) = self.prism_cap(b, axis)? else {
                continue;
            };
            if (lo - blo).abs() > 1e-7 || (hi - bhi).abs() > 1e-7 {
                continue;
            }
            let n = [axis.x(), axis.y(), axis.z()];
            let seed = if n[0].abs() < 0.8 {
                [1., 0., 0.]
            } else {
                [0., 1., 0.]
            };
            let u = v3(sub(seed, n.map(|x| x * dot(seed, n))))
                .normalize()
                .map_err(err)?;
            let v = axis.cross(u);
            let plane = Plane {
                origin: n.map(|x| x * lo),
                u: [u.x(), u.y(), u.z()],
                v: [v.x(), v.y(), v.z()],
                normal: n,
            };
            let Some(a) = self.cap_sketch(cap, &plane)? else {
                continue;
            };
            let Some(b) = self.cap_sketch(other, &plane)? else {
                continue;
            };
            let Ok(caps) = sketch::boolean_faces(&mut self.topo, &a, &b, op) else {
                break;
            };
            // Multiple disconnected parts need explicit part identities. Leave
            // that case to the general solid operation rather than joining air.
            if caps.len() != 1 {
                break;
            }
            return ops::extrude::extrude(&mut self.topo, caps[0], axis, hi - lo).map_err(err);
        }
        ops::boolean::boolean(&mut self.topo, op, a, b).map_err(err)
    }
    fn prism_cap(&self, solid: SolidId, axis: Vec3) -> Result<Option<(FaceId, f64, f64)>> {
        let mut caps = vec![];
        for id in solid_faces(&self.topo, solid).map_err(err)? {
            match self.topo.face(id).map_err(err)?.surface() {
                FaceSurface::Plane { normal, d } if normal.dot(axis).abs() > 1. - 1e-9 => {
                    caps.push((id, *d / normal.dot(axis)))
                }
                FaceSurface::Plane { normal, .. } if normal.dot(axis).abs() < 1e-9 => {}
                FaceSurface::Cylinder(c) if c.axis().cross(axis).length() < 1e-9 => {}
                _ => return Ok(None),
            }
        }
        if caps.len() != 2 {
            return Ok(None);
        }
        caps.sort_by(|a, b| a.1.total_cmp(&b.1));
        let height = caps[1].1 - caps[0].1;
        if height <= 1e-7 {
            return Ok(None);
        }
        let low = face_edges(&self.topo, caps[0].0).map_err(err)?;
        let high = face_edges(&self.topo, caps[1].0).map_err(err)?;
        if low.len() != high.len() {
            return Ok(None);
        }
        for id in low {
            let edge = self.topo.edge(id).map_err(err)?;
            let a = self.topo.vertex(edge.start()).map_err(err)?.point() + axis * height;
            let b = self.topo.vertex(edge.end()).map_err(err)?.point() + axis * height;
            let mut matches = 0;
            for &other in &high {
                let other = self.topo.edge(other).map_err(err)?;
                let c = self.topo.vertex(other.start()).map_err(err)?.point();
                let d = self.topo.vertex(other.end()).map_err(err)?.point();
                let same = match (edge.curve(), other.curve()) {
                    (EdgeCurve::Line, EdgeCurve::Line) => {
                        ((a - c).length() < 1e-7 && (b - d).length() < 1e-7)
                            || ((a - d).length() < 1e-7 && (b - c).length() < 1e-7)
                    }
                    (EdgeCurve::Circle(x), EdgeCurve::Circle(y)) => {
                        (x.center() + axis * height - y.center()).length() < 1e-7
                            && (x.radius() - y.radius()).abs() < 1e-7
                            && x.normal().cross(y.normal()).length() < 1e-9
                            && if edge.start() == edge.end() || other.start() == other.end() {
                                edge.start() == edge.end() && other.start() == other.end()
                            } else if x.normal().dot(y.normal()) > 0. {
                                (a - c).length() < 1e-7 && (b - d).length() < 1e-7
                            } else {
                                (a - d).length() < 1e-7 && (b - c).length() < 1e-7
                            }
                    }
                    _ => false,
                };
                if same {
                    matches += 1;
                }
            }
            if matches != 1 {
                return Ok(None);
            }
        }
        Ok(Some((caps[0].0, caps[0].1, caps[1].1)))
    }
    fn cap_sketch(&self, face: FaceId, plane: &Plane) -> Result<Option<ResolvedSketch>> {
        let local = |point: Point3| {
            let d = point - p3(plane.origin);
            [d.dot(v3(plane.u)), d.dot(v3(plane.v))]
        };
        let mut entities = vec![];
        for edge in face_edges(&self.topo, face).map_err(err)? {
            let edge = self.topo.edge(edge).map_err(err)?;
            let a = local(self.topo.vertex(edge.start()).map_err(err)?.point());
            let b = local(self.topo.vertex(edge.end()).map_err(err)?.point());
            let (kind, values) = match edge.curve() {
                EdgeCurve::Line => ("line", vec![a[0], a[1], b[0], b[1]]),
                EdgeCurve::Circle(c) => {
                    let center = local(c.center());
                    if edge.start() == edge.end() {
                        ("circle", vec![center[0], center[1], c.radius()])
                    } else {
                        (
                            "arc",
                            vec![
                                center[0],
                                center[1],
                                a[0],
                                a[1],
                                b[0],
                                b[1],
                                if c.normal().dot(v3(plane.normal)) > 0. {
                                    1.
                                } else {
                                    -1.
                                },
                            ],
                        )
                    }
                }
                _ => return Ok(None),
            };
            entities.push(ResolvedEntity {
                id: entities.len() as u64 + 1,
                kind: kind.into(),
                curve_identity: None,
                values,
                construction: false,
                fixed: false,
            });
        }
        Ok(Some(ResolvedSketch {
            entities,
            plane: plane.clone(),
            dof: 0,
        }))
    }
}
