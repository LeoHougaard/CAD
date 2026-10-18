//! Exact topology transfer between independent source arenas for assembly export.
use crate::model::Result;
use brepkit_topology::{
    Topology,
    edge::{Edge, EdgeId},
    face::{Face, FaceId},
    shell::{Shell, ShellId},
    solid::{Solid, SolidId},
    vertex::{Vertex, VertexId},
    wire::{OrientedEdge, Wire, WireId},
};
use std::collections::BTreeMap;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(super) fn copy_solid(source: &Topology, target: &mut Topology, id: SolidId) -> Result<SolidId> {
    Import {
        source,
        target,
        vertices: BTreeMap::new(),
        edges: BTreeMap::new(),
        wires: BTreeMap::new(),
        faces: BTreeMap::new(),
    }
    .solid(id)
}
struct Import<'a> {
    source: &'a Topology,
    target: &'a mut Topology,
    vertices: BTreeMap<usize, VertexId>,
    edges: BTreeMap<usize, EdgeId>,
    wires: BTreeMap<usize, WireId>,
    faces: BTreeMap<usize, FaceId>,
}
impl Import<'_> {
    fn vertex(&mut self, id: VertexId) -> Result<VertexId> {
        if let Some(id) = self.vertices.get(&id.index()) {
            return Ok(*id);
        }
        let original = self.source.vertex(id).map_err(error)?;
        let copy = self
            .target
            .add_vertex(Vertex::new(original.point(), original.tolerance()));
        self.vertices.insert(id.index(), copy);
        Ok(copy)
    }
    fn edge(&mut self, id: EdgeId) -> Result<EdgeId> {
        if let Some(id) = self.edges.get(&id.index()) {
            return Ok(*id);
        }
        let original = self.source.edge(id).map_err(error)?.clone();
        let a = self.vertex(original.start())?;
        let b = self.vertex(original.end())?;
        let copy = self.target.add_edge(Edge::with_tolerance(
            a,
            b,
            original.curve().clone(),
            original.tolerance(),
        ));
        self.edges.insert(id.index(), copy);
        Ok(copy)
    }
    fn wire(&mut self, id: WireId) -> Result<WireId> {
        if let Some(id) = self.wires.get(&id.index()) {
            return Ok(*id);
        }
        let original = self.source.wire(id).map_err(error)?.clone();
        let mut edges = vec![];
        for edge in original.edges() {
            edges.push(OrientedEdge::new(
                self.edge(edge.edge())?,
                edge.is_forward(),
            ));
        }
        let copy = self
            .target
            .add_wire(Wire::new(edges, original.is_closed()).map_err(error)?);
        self.wires.insert(id.index(), copy);
        Ok(copy)
    }
    fn face(&mut self, id: FaceId) -> Result<FaceId> {
        if let Some(id) = self.faces.get(&id.index()) {
            return Ok(*id);
        }
        let original = self.source.face(id).map_err(error)?.clone();
        let outer = self.wire(original.outer_wire())?;
        let mut holes = vec![];
        for &hole in original.inner_wires() {
            holes.push(self.wire(hole)?);
        }
        let face = if original.is_reversed() {
            Face::new_reversed(outer, holes, original.surface().clone())
        } else {
            Face::new(outer, holes, original.surface().clone())
        };
        let copy = self.target.add_face(face);
        self.faces.insert(id.index(), copy);
        Ok(copy)
    }
    fn shell(&mut self, id: ShellId) -> Result<ShellId> {
        let original = self.source.shell(id).map_err(error)?;
        let ids = original.faces().to_vec();
        let mut faces = vec![];
        for face in ids {
            faces.push(self.face(face)?);
        }
        Ok(self.target.add_shell(Shell::new(faces).map_err(error)?))
    }
    fn solid(&mut self, id: SolidId) -> Result<SolidId> {
        let original = self.source.solid(id).map_err(error)?;
        let outer = original.outer_shell();
        let ids = original.inner_shells().to_vec();
        let outer = self.shell(outer)?;
        let mut holes = vec![];
        for id in ids {
            holes.push(self.shell(id)?);
        }
        Ok(self.target.add_solid(Solid::new(outer, holes)))
    }
}
