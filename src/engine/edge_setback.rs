//! Exact planar setbacks preserve hole wires and unrelated analytic surfaces.
//! The kernel's polygon chamfer currently drops those wires; its general blend
//! trimmer can also fail there. This local BRep construction moves only the
//! contact vertices and adds the bevel plane. It never tessellates geometry.
use super::*;
use brepkit_topology::{
    edge::Edge,
    face::Face,
    shell::Shell,
    solid::Solid,
    vertex::{Vertex, VertexId},
    wire::{OrientedEdge, Wire},
};
impl Geometry {
    pub(super) fn has_analytic_chamfer_rims(
        &self,
        solid: SolidId,
        edges: &[EdgeId],
    ) -> Result<bool> {
        let adjacency = edge_to_face_map(&self.topo, solid).map_err(err)?;
        for &id in edges {
            let edge = self.topo.edge(id).map_err(err)?;
            if !matches!(edge.curve(), EdgeCurve::Circle(_)) || edge.start() != edge.end() {
                return Ok(false);
            }
            let Some(pair) = adjacency.get(&id.index()) else {
                return Ok(false);
            };
            if pair.len() != 2 {
                return Ok(false);
            }
            let mut cap = None;
            let mut wall = None;
            for &id in pair {
                match self.topo.face(id).map_err(err)?.surface() {
                    FaceSurface::Plane { .. } => cap = Some(id),
                    FaceSurface::Cylinder(_) | FaceSurface::Cone(_) => wall = Some(id),
                    _ => return Ok(false),
                }
            }
            let (Some(cap), Some(wall)) = (cap, wall) else {
                return Ok(false);
            };
            let face = self.topo.face(wall).map_err(err)?;
            if !face.inner_wires().is_empty() {
                return Ok(false);
            }
            let mut seams = BTreeSet::new();
            let mut circles = BTreeSet::new();
            for oe in self.topo.wire(face.outer_wire()).map_err(err)?.edges() {
                let edge = self.topo.edge(oe.edge()).map_err(err)?;
                match edge.curve() {
                    EdgeCurve::Line => {
                        seams.insert(oe.edge());
                    }
                    EdgeCurve::Circle(_) if edge.start() == edge.end() => {
                        circles.insert(oe.edge());
                    }
                    _ => return Ok(false),
                }
            }
            if seams.len() != 1 || circles.len() > 2 {
                return Ok(false);
            }
            for id in face_edges(&self.topo, cap).map_err(err)? {
                if !matches!(
                    self.topo.edge(id).map_err(err)?.curve(),
                    EdgeCurve::Line | EdgeCurve::Circle(_)
                ) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    pub(super) fn prismatic_chamfer(
        &mut self,
        solid: SolidId,
        edges: &[EdgeId],
        distance: f64,
        owner: &str,
    ) -> Result<Option<SolidId>> {
        if distance <= 1e-7 {
            return Err("Chamfer distance must be positive".into());
        }
        let available = self.edge_keys_scoped(solid, Some(owner))?;
        let keys = edges
            .iter()
            .map(|id| {
                available
                    .iter()
                    .find(|(e, _)| e == id)
                    .map(|(_, key)| key.clone())
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("Chamfer edge is missing")?;
        let mut result = solid;
        for key in keys {
            let edges: Vec<_> = self
                .edge_keys_scoped(result, Some(owner))?
                .into_iter()
                .filter(|(_, k)| reference_matches(k, &key))
                .collect();
            if edges.len() != 1 {
                return Ok(None);
            }
            let Some(next) = self.planar_setback(result, edges[0].0, distance)? else {
                return Ok(None);
            };
            result = next;
        }
        Ok(Some(result))
    }
    fn planar_setback(
        &mut self,
        solid: SolidId,
        id: EdgeId,
        distance: f64,
    ) -> Result<Option<SolidId>> {
        let original = self.topo.solid(solid).map_err(err)?.clone();
        if !original.inner_shells().is_empty() {
            return Ok(None);
        }
        let faces = solid_faces(&self.topo, solid).map_err(err)?;
        let adjacency = edge_to_face_map(&self.topo, solid).map_err(err)?;
        let edge = self.topo.edge(id).map_err(err)?.clone();
        if !matches!(edge.curve(), EdgeCurve::Line) {
            return Ok(None);
        }
        let a = self.topo.vertex(edge.start()).map_err(err)?.point();
        let b = self.topo.vertex(edge.end()).map_err(err)?.point();
        let axis = (b - a).normalize().map_err(err)?;
        let Some(adjacent) = adjacency.get(&id.index()) else {
            return Ok(None);
        };
        if adjacent.len() != 2 {
            return Ok(None);
        }
        let mut inward = vec![];
        let mut outward = vec![];
        for &face_id in adjacent {
            let face = self.topo.face(face_id).map_err(err)?;
            let FaceSurface::Plane { normal, .. } = face.surface() else {
                return Ok(None);
            };
            let wire = self.topo.wire(face.outer_wire()).map_err(err)?;
            let Some(oe) = wire.edges().iter().find(|e| e.edge() == id) else {
                return Ok(None);
            };
            let points = wire
                .edges()
                .iter()
                .map(|oe| {
                    let e = self.topo.edge(oe.edge()).map_err(err)?;
                    self.topo
                        .vertex(oe.oriented_start(e))
                        .map(|v| v.point())
                        .map_err(err)
                })
                .collect::<Result<Vec<_>>>()?;
            let signed = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(p, q)| (*p - a).cross(*q - a).dot(*normal))
                .sum::<f64>();
            if signed.abs() < 1e-9 {
                return Ok(None);
            }
            inward.push(normal.cross(if oe.is_forward() { axis } else { -axis }) * signed.signum());
            outward.push(*normal * if face.is_reversed() { -1. } else { 1. });
        }
        if inward[0].dot(outward[1]) >= -1e-7 || inward[1].dot(outward[0]) >= -1e-7 {
            return Ok(None);
        }
        let u = inward[0].normalize().map_err(err)? * distance;
        let v = inward[1].normalize().map_err(err)? * distance;
        let points = [a + u, b + u, a + v, b + v];
        let normal = (outward[0] + outward[1]).normalize().map_err(err)?;
        let plane_d = normal.dot(points[0] - p3([0.; 3]));
        let mut affected = adjacent.to_vec();
        for &vertex in &[edge.start(), edge.end()] {
            let mut ends = vec![];
            for &face_id in &faces {
                if adjacent.contains(&face_id) {
                    continue;
                }
                let touches = face_edges(&self.topo, face_id)
                    .map_err(err)?
                    .into_iter()
                    .any(|e| {
                        self.topo
                            .edge(e)
                            .is_ok_and(|e| e.start() == vertex || e.end() == vertex)
                    });
                if touches {
                    let face = self.topo.face(face_id).map_err(err)?;
                    if !matches!(face.surface(),FaceSurface::Plane{normal,..} if normal.dot(axis).abs()>1.-1e-7)
                    {
                        return Ok(None);
                    }
                    ends.push(face_id);
                }
            }
            if ends.len() != 1 {
                return Ok(None);
            }
            affected.extend(ends);
        }
        affected.sort();
        affected.dedup();
        // Leave curved boundaries intact only when wholly behind the bevel.
        // A bevel intersecting a hole needs the general analytic trimmer.
        for &face_id in &affected {
            let face = self.topo.face(face_id).map_err(err)?;
            for oe in self.topo.wire(face.outer_wire()).map_err(err)?.edges() {
                if !matches!(
                    self.topo.edge(oe.edge()).map_err(err)?.curve(),
                    EdgeCurve::Line
                ) {
                    return Ok(None);
                }
            }
            for &hole in face.inner_wires() {
                for oe in self.topo.wire(hole).map_err(err)?.edges() {
                    let e = self.topo.edge(oe.edge()).map_err(err)?;
                    let max = match e.curve() {
                        EdgeCurve::Line => [e.start(), e.end()]
                            .into_iter()
                            .map(|v| {
                                self.topo
                                    .vertex(v)
                                    .map(|v| normal.dot(v.point() - p3([0.; 3])))
                                    .map_err(err)
                            })
                            .collect::<Result<Vec<_>>>()?
                            .into_iter()
                            .fold(f64::NEG_INFINITY, f64::max),
                        EdgeCurve::Circle(c) => {
                            normal.dot(c.center() - p3([0.; 3]))
                                + c.radius() * (1. - normal.dot(c.normal()).powi(2)).max(0.).sqrt()
                        }
                        _ => return Ok(None),
                    };
                    if max >= plane_d - 1e-7 {
                        return Ok(None);
                    }
                }
            }
        }
        let new_vertices = points.map(|p| self.topo.add_vertex(Vertex::new(p, 1e-7)));
        let mut line_edges: HashMap<(usize, usize), EdgeId> = HashMap::new();
        for e in solid_edges(&self.topo, solid).map_err(err)? {
            let edge = self.topo.edge(e).map_err(err)?;
            if matches!(edge.curve(), EdgeCurve::Line) {
                line_edges.insert(pair(edge.start(), edge.end()), e);
            }
        }
        let mut replacements = HashMap::new();
        for &face_id in &affected {
            let mut face = self.topo.face(face_id).map_err(err)?.clone();
            let wire = self.topo.wire(face.outer_wire()).map_err(err)?;
            let mut vertices = vec![];
            let mut spans = HashMap::new();
            for (i, oe) in wire.edges().iter().enumerate() {
                let e = self.topo.edge(oe.edge()).map_err(err)?;
                let vertex = oe.oriented_start(e);
                if vertex != edge.start() && vertex != edge.end() {
                    vertices.push(vertex);
                    spans.insert(vertex, (vertex, vertex));
                    continue;
                }
                let begin = vertices.len();
                let end = usize::from(vertex == edge.end());
                if face_id == adjacent[0] {
                    vertices.push(new_vertices[end]);
                } else if face_id == adjacent[1] {
                    vertices.push(new_vertices[2 + end]);
                } else {
                    let prev = &wire.edges()[(i + wire.edges().len() - 1) % wire.edges().len()];
                    let prev_e = self.topo.edge(prev.edge()).map_err(err)?;
                    let previous = self
                        .topo
                        .vertex(prev.oriented_start(prev_e))
                        .map_err(err)?
                        .point();
                    let point = self.topo.vertex(vertex).map_err(err)?.point();
                    if (previous - point).dot(u) > (previous - point).dot(v) {
                        vertices.extend([new_vertices[end], new_vertices[2 + end]]);
                    } else {
                        vertices.extend([new_vertices[2 + end], new_vertices[end]]);
                    }
                }
                spans.insert(vertex, (vertices[begin], *vertices.last().unwrap()));
            }
            // Oversized setbacks must not invert or collapse an original edge.
            for oe in wire.edges() {
                let e = self.topo.edge(oe.edge()).map_err(err)?;
                let a = oe.oriented_start(e);
                let b = oe.oriented_end(e);
                let old = self.topo.vertex(b).map_err(err)?.point()
                    - self.topo.vertex(a).map_err(err)?.point();
                let new = self.topo.vertex(spans[&b].0).map_err(err)?.point()
                    - self.topo.vertex(spans[&a].1).map_err(err)?.point();
                if old.dot(new) <= 1e-7 * old.length() {
                    return Err("Chamfer distance collapses or reverses a boundary edge".into());
                }
            }
            face.set_outer_wire(self.setback_wire(&vertices, &mut line_edges)?);
            let replacement = self.topo.add_face(face);
            replacements.insert(face_id, replacement);
        }
        let mut bevel = vec![
            new_vertices[0],
            new_vertices[1],
            new_vertices[3],
            new_vertices[2],
        ];
        if (points[1] - points[0])
            .cross(points[3] - points[0])
            .dot(normal)
            < 0.
        {
            bevel.reverse();
        }
        let wire = self.setback_wire(&bevel, &mut line_edges)?;
        let bevel = self.topo.add_face(Face::new(
            wire,
            vec![],
            FaceSurface::Plane { normal, d: plane_d },
        ));
        let mut faces = faces
            .into_iter()
            .map(|f| replacements.get(&f).copied().unwrap_or(f))
            .collect::<Vec<_>>();
        faces.push(bevel);
        let shell = self.topo.add_shell(Shell::new(faces).map_err(err)?);
        Ok(Some(self.topo.add_solid(Solid::new(shell, vec![]))))
    }
    fn setback_wire(
        &mut self,
        vertices: &[VertexId],
        edges: &mut HashMap<(usize, usize), EdgeId>,
    ) -> Result<brepkit_topology::wire::WireId> {
        let mut wire = vec![];
        for (&a, &b) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
            let id = *edges
                .entry(pair(a, b))
                .or_insert_with(|| self.topo.add_edge(Edge::new(a, b, EdgeCurve::Line)));
            let forward = self.topo.edge(id).map_err(err)?.start() == a;
            wire.push(OrientedEdge::new(id, forward));
        }
        Ok(self.topo.add_wire(Wire::new(wire, true).map_err(err)?))
    }
}
fn pair(a: VertexId, b: VertexId) -> (usize, usize) {
    if a.index() < b.index() {
        (a.index(), b.index())
    } else {
        (b.index(), a.index())
    }
}
