//! Native geometry execution. This module knows operations, never feature types.
mod construction_planes;
mod edge_setback;
mod feature_controls;
mod sweep_loft;
use crate::{
    model::{reference_matches, Document, Feature, Plane, Result},
    plugins::{Numbers, Registry},
    sketch::{self, ResolvedSketch, p3, v3},
};
use brepkit_math::{mat::Mat4, vec::Vec3};
use brepkit_operations as ops;
use brepkit_topology::{
    Topology,
    edge::{EdgeCurve, EdgeId},
    explorer::{edge_to_face_map, face_edges, solid_edges, solid_faces},
    face::{FaceId, FaceSurface},
    solid::SolidId,
};
use feature_controls::analytic_edge_axis;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::Instant,
};

// Brepkit exposes a process-wide mesh-fallback counter. Keep native operation
// transactions together so another Runtime cannot taint the exactness check.
// This does not serialize independent application processes or viewport work.
static PRECISE_OPERATION: Mutex<()> = Mutex::new(());

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn mesh_extent(
    positions: &[brepkit_math::vec::Point3],
    indices: &[u32],
) -> ([[f32; 3]; 2], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions {
        for (i, x) in [p.x(), p.y(), p.z()].into_iter().enumerate() {
            lo[i] = lo[i].min(x);
            hi[i] = hi[i].max(x);
        }
    }
    let mut centroid = std::array::from_fn(|i| (lo[i] + hi[i]) * 0.5);
    if let Some(&origin) = positions.first() {
        let mut moment = Vec3::new(0., 0., 0.);
        let mut volume6 = 0.;
        for tri in indices.as_chunks::<3>().0 {
            let a = positions[tri[0] as usize] - origin;
            let b = positions[tri[1] as usize] - origin;
            let c = positions[tri[2] as usize] - origin;
            let v = a.dot(b.cross(c));
            volume6 += v;
            moment += (a + b + c) * v;
        }
        if volume6.abs() > f64::EPSILON {
            let p = origin + moment * (0.25 / volume6);
            centroid = [p.x(), p.y(), p.z()];
        }
    }
    ([lo.map(|x| x as f32), hi.map(|x| x as f32)], centroid)
}
fn num(p: &Value, key: &str, default: f64) -> f64 {
    p.get(key).and_then(Value::as_f64).unwrap_or(default)
}
fn text<'a>(p: &'a Value, key: &str, default: &'a str) -> &'a str {
    p.get(key).and_then(Value::as_str).unwrap_or(default)
}
fn flag(p: &Value, key: &str) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Clone, Debug)]
pub struct Part {
    pub key: String,
    pub name: String,
    pub solid: SolidId,
    pub feature: String,
    /// Only actual contributors may supply persistent geometric references.
    pub source_features: BTreeSet<String>,
    /// Exact profile namespaces; these are separate from feature dependencies.
    pub reference_sources: BTreeSet<String>,
}
#[derive(Clone)]
pub struct Geometry {
    pub topo: Topology,
    pub parts: Vec<Part>,
    pub sketches: BTreeMap<String, ResolvedSketch>,
    pub planes: BTreeMap<String, Plane>,
    /// Persistent key, analytical frame and owning reference namespace.
    pub anchors: Vec<(String, Plane, String)>,
    pub outputs: BTreeMap<String, Vec<String>>,
    edge_anchors: Vec<EdgeAnchor>,
}
impl Default for Geometry {
    fn default() -> Self {
        Self {
            topo: Topology::new(),
            parts: vec![],
            sketches: BTreeMap::new(),
            planes: BTreeMap::new(),
            anchors: vec![],
            outputs: BTreeMap::new(),
            edge_anchors: vec![],
        }
    }
}

#[derive(Clone)]
struct EdgeAnchor {
    source: String,
    key: String,
    /// Source profile side for curved-face naming; vertical edges have none.
    side: Option<String>,
    start: brepkit_math::vec::Point3,
    end: brepkit_math::vec::Point3,
    midpoint: brepkit_math::vec::Point3,
    circle: Option<(brepkit_math::vec::Point3, Vec3, f64, bool)>,
}
impl EdgeAnchor {
    fn matches(&self, topo: &Topology, id: EdgeId) -> bool {
        const TOL: f64 = 1e-7;
        let Ok(edge) = topo.edge(id) else {
            return false;
        };
        let (Ok(a), Ok(b)) = (topo.vertex(edge.start()), topo.vertex(edge.end())) else {
            return false;
        };
        let (a, b) = (a.point(), b.point());
        if !(((a - self.start).length() < TOL && (b - self.end).length() < TOL)
            || ((b - self.start).length() < TOL && (a - self.end).length() < TOL))
        {
            return false;
        }
        match (self.circle, edge.curve()) {
            (None, EdgeCurve::Line) => {}
            (Some((center, normal, radius, full)), EdgeCurve::Circle(c)) => {
                if (center - c.center()).length() > TOL
                    || (radius - c.radius()).abs() > TOL
                    || (normal.dot(c.normal()).abs() - 1.).abs() > 1e-8
                    || full != ((a - b).length() < 1e-9)
                {
                    return false;
                }
            }
            _ => return false,
        }
        let (lo, hi) = edge.curve().domain_with_endpoints(a, b);
        let middle = edge.curve().evaluate_with_endpoints((lo + hi) / 2., a, b);
        (middle - self.midpoint).length() < TOL
    }
}

#[derive(Clone, Debug)]
pub struct FaceView {
    pub key: String,
    pub part: String,
    pub start: usize,
    pub end: usize,
    pub plane: Option<Plane>,
    pub global_id: usize,
}
#[derive(Clone, Debug)]
pub struct EdgeView {
    pub key: String,
    pub part: String,
    pub points: Vec<[f32; 3]>,
    pub seam: bool,
    /// Exact analytical axis for straight and circular edges.
    pub axis: Option<([f64; 3], [f64; 3])>,
}
#[derive(Clone, Debug)]
pub struct MeshView {
    pub part: String,
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Per triangle-corner normals, preserving sharp boundaries between faces.
    pub normals: Vec<[f32; 3]>,
    pub faces: Vec<FaceView>,
    pub edges: Vec<EdgeView>,
    pub volume: f64,
    pub bounds: [[f32; 3]; 2],
    /// Volume centroid of the display mesh, computed once in the worker.
    pub centroid: [f64; 3],
}
#[derive(Clone, Debug)]
pub struct FeatureStatus {
    pub id: String,
    pub error: Option<String>,
    pub millis: f64,
    pub suppressed: bool,
    pub rolled_back: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub meshes: Vec<MeshView>,
    pub sketches: BTreeMap<String, ResolvedSketch>,
    pub planes: BTreeMap<String, Plane>,
    /// Computed by the geometry worker, never by pointer-hover rendering.
    pub regions: BTreeMap<String, Vec<sketch::ResolvedRegion>>,
    pub statuses: Vec<FeatureStatus>,
    pub millis: f64,
    pub reused_features: usize,
    pub error: Option<String>,
}

pub struct Runtime {
    pub registry: Arc<Registry>,
    checkpoint: Option<Checkpoint>,
}
struct Checkpoint {
    prefix: Vec<Feature>,
    variables: BTreeMap<String, String>,
    format: u32,
    geometry: Geometry,
    statuses: Vec<FeatureStatus>,
}
pub struct Evaluation {
    pub geometry: Geometry,
    pub scene: Arc<Scene>,
}

impl Runtime {
    pub fn new(registry: Registry) -> Self {
        Self::shared(Arc::new(registry))
    }
    pub fn shared(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            checkpoint: None,
        }
    }
    pub fn evaluate(&mut self, document: &Document) -> Result<Evaluation> {
        let clock = Instant::now();
        document.validate()?;
        let numbers = Numbers::for_document(document)?;
        let end = document
            .rollback
            .unwrap_or(document.features.len())
            .min(document.features.len());
        let mut geometry = Geometry::default();
        let mut statuses = vec![];
        let mut start = 0;
        if let Some(c) = &self.checkpoint
            && c.variables == document.variables
            && c.format == document.format
            && c.prefix.len() <= end
            && document.features[..c.prefix.len()] == c.prefix
        {
            geometry = c.geometry.clone();
            statuses = c.statuses.clone();
            start = c.prefix.len();
        }
        let reused = start;
        let mut failed = false;
        for (index, feature) in document.features.iter().enumerate().skip(start) {
            let t = Instant::now();
            let rolled_back = index >= end;
            if !failed && !rolled_back && index + 1 == end {
                // One bounded prefix cache. Geometry is never cloned by the render loop.
                self.checkpoint = Some(Checkpoint {
                    prefix: document.features[..index].to_vec(),
                    variables: document.variables.clone(),
                    format: document.format,
                    geometry: geometry.clone(),
                    statuses: statuses.clone(),
                });
            }
            let result = if rolled_back || feature.suppressed {
                Ok(())
            } else if failed {
                Err("Blocked by an earlier feature failure".into())
            } else {
                let _operation = PRECISE_OPERATION
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let before = ops::boolean::mesh_fallback_count();
                let parts_before = geometry.parts.clone();
                let anchors_before = geometry.anchors.len();
                let edge_anchors_before = geometry.edge_anchors.len();
                let result = (|| {
                    let plan = self.registry.evaluate(feature, &numbers)?;
                    let operations = if let Some(a) = plan.as_array() {
                        a.clone()
                    } else {
                        vec![plan]
                    };
                    if operations.len() > 200 {
                        return Err("A feature may run at most 200 operations".into());
                    }
                    for (op_index, operation) in operations.iter().enumerate() {
                        geometry.execute(feature, op_index, operation, &numbers)?;
                    }
                    if ops::boolean::mesh_fallback_count() != before {
                        return Err("This operation required a mesh approximation. The precise result was rejected; try a simpler profile or a different operation.".into());
                    }
                    for part in &geometry.parts {
                        if parts_before.iter().any(|old| old.solid == part.solid) {
                            continue;
                        }
                        let report = ops::validate::validate_solid(&geometry.topo, part.solid)
                            .map_err(err)?;
                        if !report.is_valid() {
                            return Err(format!(
                                "Invalid solid rejected: {}",
                                report
                                    .issues
                                    .iter()
                                    .filter(|i| i.severity == ops::validate::Severity::Error)
                                    .map(|i| i.description.as_str())
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            ));
                        }
                    }
                    Ok(())
                })();
                if result.is_err() {
                    geometry.parts = parts_before;
                    geometry.anchors.truncate(anchors_before);
                    geometry.edge_anchors.truncate(edge_anchors_before);
                    geometry.sketches.remove(&feature.id);
                    geometry.planes.remove(&feature.id);
                    geometry.outputs.remove(&feature.id);
                }
                result
            };
            if result.is_err() {
                failed = true;
            }
            statuses.push(FeatureStatus {
                id: feature.id.clone(),
                error: result.err(),
                millis: t.elapsed().as_secs_f64() * 1000.,
                suppressed: feature.suppressed,
                rolled_back,
            });
        }
        let mut scene = geometry.scene()?;
        scene.statuses = statuses;
        scene.reused_features = reused;
        scene.millis = clock.elapsed().as_secs_f64() * 1000.;
        scene.error = scene.statuses.iter().find_map(|s| s.error.clone());
        Ok(Evaluation {
            geometry,
            scene: Arc::new(scene),
        })
    }
}

impl Geometry {
    fn target(&self, key: &str) -> Result<usize> {
        if key.is_empty() || key == "@last" {
            return self
                .parts
                .len()
                .checked_sub(1)
                .ok_or("There is no target part".into());
        }
        if let Some(i) = self.parts.iter().position(|p| p.key == key) {
            return Ok(i);
        }
        if let Some(keys) = self.outputs.get(key) {
            if keys.len() != 1 {
                return Err("Choose one specific part from this feature's output".into());
            }
            return self
                .parts
                .iter()
                .position(|p| p.key == keys[0])
                .ok_or("Referenced part is no longer present".into());
        }
        Err(format!(
            "Part '{key}' is missing or its feature is suppressed"
        ))
    }
    fn record_output(&mut self, f: &Feature, keys: Vec<String>) {
        self.outputs.insert(f.id.clone(), keys);
    }
    fn add_result(
        &mut self,
        f: &Feature,
        solid: SolidId,
        mode: &str,
        target: &str,
        ordinal: usize,
    ) -> Result<()> {
        if mode == "New" {
            let key = format!("{}:{ordinal}", f.id);
            self.parts.push(Part {
                key: key.clone(),
                name: f.name.clone(),
                solid,
                feature: f.id.clone(),
                source_features: BTreeSet::from([f.id.clone()]),
                reference_sources: BTreeSet::from([f.id.clone()]),
            });
            self.outputs.entry(f.id.clone()).or_default().push(key);
        } else {
            let i = self.target(target)?;
            // Native operations receive copies. A failure cannot mutate a committed part.
            let base = ops::copy::copy_solid(&mut self.topo, self.parts[i].solid).map_err(err)?;
            let op = match mode {
                "Add" => ops::boolean::BooleanOp::Fuse,
                "Remove" => ops::boolean::BooleanOp::Cut,
                "Intersect" => ops::boolean::BooleanOp::Intersect,
                _ => return Err(format!("Unknown operation {mode}")),
            };
            let result = ops::boolean::boolean(&mut self.topo, op, base, solid).map_err(err)?;
            self.parts[i].solid = result;
            self.parts[i].feature = f.id.clone();
            self.parts[i].source_features.insert(f.id.clone());
            self.parts[i].reference_sources.insert(f.id.clone());
            self.record_output(f, vec![self.parts[i].key.clone()]);
        }
        Ok(())
    }
    pub fn resolve_plane(&self, p: &Value, numbers: &Numbers) -> Result<Plane> {
        let offset = numbers.value(p.get("offset").unwrap_or(&Value::from(0)))?;
        if let Some(value) = p.get("context_plane").filter(|v| !v.is_null()) {
            let plane: Plane = serde_json::from_value(value.clone())
                .map_err(|e| format!("Invalid context plane: {e}"))?;
            plane.validate()?;
            return Ok(plane.shifted(offset));
        }
        let support = text(p, "support", "");
        if let Some(plane) = self.planes.get(support) {
            return Ok(plane.shifted(offset));
        }
        if support.is_empty() {
            return Ok(Plane::named(text(p, "plane", "Top"), offset));
        }
        let anchor = self
            .anchors
            .iter()
            .find(|(key, _, _)| reference_matches(key, support))
            .ok_or_else(|| {
                format!("Sketch support '{support}' is missing. Select a new plane or face.")
            })?;
        let mut matches = 0;
        let mut conflicting_orientation = false;
        for part in &self.parts {
            for (id, key) in self.face_keys(part.solid)? {
                if !reference_matches(&key, support) {
                    continue;
                }
                let face = self.topo.face(id).map_err(err)?;
                if let FaceSurface::Plane { normal, d } = face.surface()
                    && plane_matches(&anchor.1, *normal, *d)
                {
                    matches += 1;
                    let sign = if face.is_reversed() { -1. } else { 1. };
                    let outward = [normal.x() * sign, normal.y() * sign, normal.z() * sign];
                    conflicting_orientation |= dot(anchor.1.normal, outward) < 1. - 1e-6;
                }
            }
        }
        if matches == 0 || conflicting_orientation {
            let reason = if conflicting_orientation {
                "with conflicting orientation"
            } else {
                "on the original plane"
            };
            return Err(format!(
                "Sketch support resolves to {matches} faces {reason}. Repair the reference instead of guessing."
            ));
        }
        // A boolean can split a support into coplanar faces. The sketch needs
        // their common oriented plane, while its original frame stays stable.
        Ok(anchor.1.shifted(offset))
    }
    fn register_extrude_anchors(&mut self, source: &str, sk: &ResolvedSketch, start: f64, end: f64, legacy: bool) {
        let sign = if end > start { 1. } else { -1. };
        let mut a = sk.plane.shifted(start);
        a.normal = a.normal.map(|v| -sign * v);
        a.v = cross(a.normal, a.u);
        let mut b = sk.plane.shifted(end);
        b.normal = b.normal.map(|v| sign * v);
        b.v = cross(b.normal, b.u);
        self.anchors.push((format!("{}:start", source), a, source.into()));
        self.anchors.push((format!("{}:end", source), b, source.into()));
        if legacy && let Ok(loops) = sketch::loops(sk) {
            for l in loops {
                if let sketch::RegionLoop::Polygon { entity, points } = l {
                    for i in 0..points.len() {
                        let origin = sk.plane.at(points[i]);
                        let next = sk.plane.at(points[(i + 1) % points.len()]);
                        let mut u = sub(next, origin);
                        let length = dot(u, u).sqrt();
                        if length < 1e-8 {
                            continue;
                        }
                        u = u.map(|x| x / length);
                        let n = cross(u, sk.plane.normal);
                        self.anchors.push((
                            format!("{}:side:{entity}:{i}", source),
                            Plane {
                                origin,
                                u,
                                v: sk.plane.normal,
                                normal: n,
                            },
                            source.into(),
                        ));
                    }
                }
            }
        }
    }
    fn register_profile_edges(
        &mut self,
        source: &str,
        sk: &ResolvedSketch,
        edges: &[sketch::RegionEdge],
        start: f64,
        end: f64,
    ) {
        let a = sk.plane.shifted(start);
        let b = sk.plane.shifted(end);
        let mut vertices = BTreeSet::new();
        for edge in edges {
            let side = format!("{}:region-side:{}", source, edge.key);
            for (cap, plane) in [("start", &a), ("end", &b)] {
                self.edge_anchors.push(EdgeAnchor {
                    source: source.into(),
                    key: format!("{}:profile:{}:{cap}", source, edge.key),
                    side: edge.circle.map(|_| side.clone()),
                    start: p3(plane.at(edge.start)),
                    end: p3(plane.at(edge.end)),
                    midpoint: p3(plane.at(edge.midpoint)),
                    circle: edge.circle.map(|(center, radius)| {
                        (
                            p3(plane.at(center)),
                            v3(plane.normal),
                            radius,
                            (edge.start[0] - edge.end[0]).hypot(edge.start[1] - edge.end[1]) < 1e-9,
                        )
                    }),
                });
            }
            for (key, point) in [(&edge.start_key, edge.start), (&edge.end_key, edge.end)] {
                if !vertices.insert(key.clone()) {
                    continue;
                }
                let pa = p3(a.at(point));
                let pb = p3(b.at(point));
                self.edge_anchors.push(EdgeAnchor {
                    source: source.into(),
                    key: format!("{}:region-vertex:{key}", source),
                    side: None,
                    start: pa,
                    end: pb,
                    midpoint: pa + (pb - pa) * 0.5,
                    circle: None,
                });
            }
        }
    }
    fn execute(&mut self, f: &Feature, ordinal: usize, p: &Value, numbers: &Numbers) -> Result<()> {
        let op = text(p, "op", "");
        match op {
            "plane" => self.execute_plane(f, p)?,
            "boolean" => self.execute_boolean(f, p)?,
            "sweep" => self.execute_sweep(f, ordinal, p)?,
            "loft" => self.execute_loft(f, ordinal, p)?,
            "sketch" => {
                let plane = self.resolve_plane(&f.params, numbers)?;
                let s = f.sketch().ok_or("Sketch geometry is invalid")?;
                self.sketches
                    .insert(f.id.clone(), sketch::resolve(&s, plane, numbers)?);
            }
            "revolve" => self.execute_revolve(f, ordinal, p)?,
            "extrude" => self.execute_extrude(f, ordinal, p)?,
            "box" | "cylinder" => {
                let h = num(p, "height", 10.);
                let solid = if op == "box" {
                    ops::primitives::make_box(
                        &mut self.topo,
                        num(p, "width", 10.),
                        num(p, "depth", 10.),
                        h,
                    )
                    .map_err(err)?
                } else {
                    ops::primitives::make_cylinder(&mut self.topo, num(p, "radius", 5.), h)
                        .map_err(err)?
                };
                ops::transform::transform_solid(
                    &mut self.topo,
                    solid,
                    &Mat4::translation(num(p, "x", 0.), num(p, "y", 0.), num(p, "z", 0.)),
                )
                .map_err(err)?;
                self.add_result(
                    f,
                    solid,
                    text(p, "mode", "New"),
                    text(p, "target", ""),
                    ordinal,
                )?;
            }
            "fillet" | "chamfer" => {
                let i = self.target(text(p, "target", ""))?;
                let solid =
                    ops::copy::copy_solid(&mut self.topo, self.parts[i].solid).map_err(err)?;
                let available = self.edge_keys_scoped(solid, Some(&self.parts[i].key))?;
                let keys: Vec<String> = p
                    .get("edges")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                if keys.is_empty() {
                    return Err("Select one or more edges, or choose All edges".into());
                }
                if keys.iter().any(|key| key.contains("untracked:")) {
                    return Err("Fillet or chamfer on this edge is unsupported until it has a persistent reference".into());
                }
                let edges = if keys == ["@all"] {
                    solid_edges(&self.topo, solid).map_err(err)?
                } else {
                    keys.iter()
                        .map(|k| self.resolve_edge_scoped(solid, &self.parts[i].key, k, &available))
                        .collect::<Result<Vec<_>>>()?
                };
                // The polygon chamfer handles planar corner junctions directly. The
                // blend engine currently fails trimming a box when all edges meet.
                let planar = solid_faces(&self.topo, solid)
                    .map_err(err)?
                    .iter()
                    .all(|id| {
                        matches!(
                            self.topo.face(*id).map(|f| f.surface()),
                            Ok(FaceSurface::Plane { .. })
                        )
                    });
                let result = if op == "fillet" {
                    ops::blend_ops::fillet_v2(&mut self.topo, solid, &edges, num(p, "radius", 2.))
                        .map(|r| r.solid)
                } else if self.has_analytic_chamfer_rims(solid, &edges)?
                    || (planar
                        && !solid_faces(&self.topo, solid)
                            .map_err(err)?
                            .iter()
                            .any(|id| {
                                self.topo
                                    .face(*id)
                                    .is_ok_and(|face| !face.inner_wires().is_empty())
                            }))
                {
                    ops::chamfer::chamfer(&mut self.topo, solid, &edges, num(p, "distance", 2.))
                } else {
                    if let Some(result) = self.prismatic_chamfer(
                        solid,
                        &edges,
                        num(p, "distance", 2.),
                        &self.parts[i].key.clone(),
                    )? {
                        Ok(result)
                    } else {
                        ops::blend_ops::chamfer_v2(
                            &mut self.topo,
                            solid,
                            &edges,
                            num(p, "distance", 2.),
                            num(p, "distance", 2.),
                        )
                        .map(|r| r.solid)
                    }
                }
                .map_err(err)?;
                self.parts[i].solid = result;
                self.parts[i].feature = f.id.clone();
                self.parts[i].source_features.insert(f.id.clone());
                self.record_output(f, vec![self.parts[i].key.clone()]);
            }
            "shell" => {
                let i = self.target(text(p, "target", ""))?;
                let solid =
                    ops::copy::copy_solid(&mut self.topo, self.parts[i].solid).map_err(err)?;
                let keys: Vec<_> = p
                    .get("faces")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if keys.is_empty() {
                    return Err("Select the face or faces to remove for Shell".into());
                }
                if keys.iter().any(|key| key.contains("untracked:")) {
                    return Err(
                        "Shell on this face is unsupported until it has a persistent reference"
                            .into(),
                    );
                }
                let available = self.face_keys_scoped(solid, Some(&self.parts[i].key))?;
                let faces = keys
                    .iter()
                    .map(|k| {
                        let hits: Vec<_> = available.iter().filter(|(_, key)| reference_matches(key, k)).collect();
                        if hits.len() != 1 {
                            Err("Shell face reference is missing or ambiguous".into())
                        } else {
                            Ok(hits[0].0)
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                let result =
                    ops::shell_op::shell(&mut self.topo, solid, num(p, "thickness", 2.), &faces)
                        .map_err(err)?;
                self.parts[i].solid = result;
                self.parts[i].feature = f.id.clone();
                self.parts[i].source_features.insert(f.id.clone());
                self.record_output(f, vec![self.parts[i].key.clone()]);
            }
            "mirror" | "pattern" | "transform" => {
                let i = self.target(text(p, "target", ""))?;
                let original = self.parts[i].clone();
                let reference_sources = original.reference_sources.clone();
                let mut source_features = original.source_features.clone();
                source_features.insert(f.id.clone());
                let mut produced = vec![];
                if op == "mirror" {
                    let plane = Plane::named(text(p, "plane", "Right"), num(p, "offset", 0.));
                    let result = ops::mirror::mirror(
                        &mut self.topo,
                        original.solid,
                        p3(plane.origin),
                        v3(plane.normal),
                    )
                    .map_err(err)?;
                    if flag(p, "keep") {
                        let key = format!("{}:0", f.id);
                        self.parts.push(Part {
                            key: key.clone(),
                            name: f.name.clone(),
                            solid: result,
                            feature: f.id.clone(),
                            source_features: source_features.clone(),
                            reference_sources: reference_sources.clone(),
                        });
                        produced.push(key);
                    } else {
                        self.parts[i].solid = result;
                        self.parts[i].source_features = source_features.clone();
                        produced.push(original.key);
                    }
                } else if op == "pattern" {
                    let count = num(p, "count", 3.);
                    if count < 2. || count > 100. || count.fract() != 0. {
                        return Err("Instances must be a whole number between 2 and 100".into());
                    }
                    let spacing = num(p, "spacing", 30.);
                    if spacing.abs() < 1e-7 {
                        return Err("Pattern spacing must be nonzero".into());
                    }
                    let axis = match text(p, "axis", "X") {
                        "Y" => [0., 1., 0.],
                        "Z" => [0., 0., 1.],
                        _ => [1., 0., 0.],
                    };
                    produced.push(original.key);
                    for n in 1..count as usize {
                        let result =
                            ops::copy::copy_solid(&mut self.topo, original.solid).map_err(err)?;
                        ops::transform::transform_solid(
                            &mut self.topo,
                            result,
                            &Mat4::translation(
                                axis[0] * spacing * n as f64,
                                axis[1] * spacing * n as f64,
                                axis[2] * spacing * n as f64,
                            ),
                        )
                        .map_err(err)?;
                        let key = format!("{}:{n}", f.id);
                        self.parts.push(Part {
                            key: key.clone(),
                            name: format!("{} {}", original.name, n + 1),
                            solid: result,
                            feature: f.id.clone(),
                            source_features: source_features.clone(),
                            reference_sources: reference_sources.clone(),
                        });
                        produced.push(key);
                    }
                } else {
                    let result =
                        ops::copy::copy_solid(&mut self.topo, original.solid).map_err(err)?;
                    ops::transform::transform_solid(
                        &mut self.topo,
                        result,
                        &Mat4::translation(num(p, "x", 0.), num(p, "y", 0.), num(p, "z", 0.)),
                    )
                    .map_err(err)?;
                    if flag(p, "copy") {
                        let key = format!("{}:0", f.id);
                        self.parts.push(Part {
                            key: key.clone(),
                            name: f.name.clone(),
                            solid: result,
                            feature: f.id.clone(),
                            source_features: source_features.clone(),
                            reference_sources: reference_sources.clone(),
                        });
                        produced.push(key);
                    } else {
                        self.parts[i].solid = result;
                        self.parts[i].source_features = source_features;
                        produced.push(original.key);
                    }
                }
                self.record_output(f, produced);
            }
            _ => {
                return Err(format!(
                    "The plugin requested an unavailable operation '{op}'"
                ));
            }
        }
        Ok(())
    }
    pub fn face_keys(&self, solid: SolidId) -> Result<Vec<(FaceId, String)>> {
        let owner = self
            .parts
            .iter()
            .find(|p| p.solid == solid)
            .map(|p| p.key.as_str());
        self.face_keys_scoped(solid, owner)
    }
    fn face_keys_scoped(
        &self,
        solid: SolidId,
        owner: Option<&str>,
    ) -> Result<Vec<(FaceId, String)>> {
        let prefix = owner.map(|key| format!("{}:", key.split(':').next().unwrap_or(key)));
        let sources = owner
            .and_then(|key| self.parts.iter().find(|part| part.key == key))
            .map(|part| &part.reference_sources);
        let allowed = |source: &str| sources.is_some_and(|s| s.contains(source));
        solid_faces(&self.topo, solid)
            .map_err(err)?
            .into_iter()
            .map(|id| {
                let face = self.topo.face(id).map_err(err)?;
                let key = match face.surface() {
                    FaceSurface::Plane { normal, d } => self
                        .anchors
                        .iter()
                        .filter(|(_, p, source)| allowed(source) && plane_matches(p, *normal, *d))
                        .min_by_key(|(key, _, _)| {
                            usize::from(!prefix.as_ref().is_some_and(|p| key.starts_with(p)))
                        })
                        .map(|(key, _, _)| key.clone())
                        .unwrap_or_else(|| {
                            format!(
                                "untracked:plane:{:.7}:{:.7}:{:.7}:{:.7}",
                                normal.x(),
                                normal.y(),
                                normal.z(),
                                d
                            )
                        }),
                    FaceSurface::Cylinder(_) => {
                        let mut sources = BTreeSet::new();
                        for edge in face_edges(&self.topo, id).map_err(err)? {
                            for anchor in &self.edge_anchors {
                                if let Some(side) = &anchor.side
                                    && allowed(&anchor.source)
                                    && anchor.matches(&self.topo, edge)
                                {
                                    sources.insert(side.clone());
                                }
                            }
                        }
                        let owned = sources
                            .iter()
                            .filter(|key| prefix.as_ref().is_some_and(|p| key.starts_with(p)))
                            .cloned()
                            .collect::<BTreeSet<_>>();
                        if !owned.is_empty() {
                            sources = owned;
                        }
                        if sources.len() == 1 {
                            sources.into_iter().next().unwrap()
                        } else {
                            format!("untracked:curved:{}", id.index())
                        }
                    }
                    _ => format!("untracked:curved:{}", id.index()),
                };
                Ok((id, key))
            })
            .collect()
    }
    pub fn edge_keys(&self, solid: SolidId) -> Result<Vec<(EdgeId, String)>> {
        let owner = self
            .parts
            .iter()
            .find(|p| p.solid == solid)
            .map(|p| p.key.as_str());
        self.edge_keys_scoped(solid, owner)
    }
    fn resolve_edge_scoped(
        &self,
        solid: SolidId,
        owner: &str,
        key: &str,
        available: &[(EdgeId, String)],
    ) -> Result<EdgeId> {
        let mut hits: BTreeSet<_> = available
            .iter()
            .filter(|(_, k)| reference_matches(k, key))
            .map(|(id, _)| *id)
            .collect();
        // A formerly untracked adjacent face may acquire its correct source name.
        // Retain old explicit source-edge aliases, bounded by the same contributors.
        if hits.is_empty() && !key.contains("untracked:") {
            let sources = &self.parts[self.target(owner)?].reference_sources;
            for anchor in self.edge_anchors.iter().filter(|a| {
                reference_matches(&a.key, key) && sources.contains(&a.source)
            }) {
                for id in solid_edges(&self.topo, solid).map_err(err)? {
                    if anchor.matches(&self.topo, id) {
                        hits.insert(id);
                    }
                }
            }
        }
        if hits.len() != 1 {
            return Err(format!(
                "Edge reference '{key}' resolves to {} edges. Reselect it.",
                hits.len()
            ));
        }
        Ok(*hits.first().unwrap())
    }
    fn edge_keys_scoped(
        &self,
        solid: SolidId,
        owner: Option<&str>,
    ) -> Result<Vec<(EdgeId, String)>> {
        let faces = self.face_keys_scoped(solid, owner)?;
        let sources = owner
            .and_then(|key| self.parts.iter().find(|part| part.key == key))
            .map(|part| &part.reference_sources);
        let mut adjacent: HashMap<EdgeId, Vec<String>> = HashMap::new();
        for (id, key) in faces {
            for edge in face_edges(&self.topo, id).map_err(err)? {
                adjacent.entry(edge).or_default().push(key.clone());
            }
        }
        let named = solid_edges(&self.topo, solid)
            .map_err(err)?
            .into_iter()
            .map(|edge| {
                let mut names = adjacent.remove(&edge).unwrap_or_default();
                names.sort();
                names.dedup();
                Ok((edge, names.join(" | ")))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut counts = BTreeMap::new();
        for (_, key) in &named {
            *counts.entry(key.clone()).or_insert(0usize) += 1;
        }
        Ok(named
            .into_iter()
            .map(|(edge, key)| {
                if counts[&key] > 1 || key.contains("untracked:") {
                    let anchors: BTreeSet<_> = self
                        .edge_anchors
                        .iter()
                        .filter(|a| {
                            sources.is_some_and(|s| s.contains(&a.source))
                        })
                        .filter(|a| a.matches(&self.topo, edge))
                        .map(|a| a.key.clone())
                        .collect();
                    if anchors.len() == 1 {
                        return (edge, anchors.into_iter().next().unwrap());
                    }
                    if counts[&key] > 1 {
                        return (edge, format!("untracked:ambiguous-edge:{key}"));
                    }
                }
                (edge, key)
            })
            .collect())
    }
    pub fn scene(&self) -> Result<Scene> {
        let mut scene = Scene {
            sketches: self.sketches.clone(),
            planes: self.planes.clone(),
            regions: self
                .sketches
                .iter()
                .filter_map(|(id, sk)| {
                    sketch::regions(sk)
                        .ok()
                        .map(|regions| (id.clone(), regions))
                })
                .collect(),
            ..Default::default()
        };
        let mut global_id = 1;
        for part in &self.parts {
            let (mesh, offsets) = ops::tessellate::tessellate_solid_grouped_with_tolerance(
                &self.topo, part.solid, 0.04, 0.08,
            )
            .map_err(err)?;
            if mesh.indices.len() > 3_000_000 {
                return Err("Display mesh exceeds one million triangles".into());
            }
            let (bounds, centroid) = mesh_extent(&mesh.positions, &mesh.indices);
            let positions = mesh
                .positions
                .iter()
                .map(|p| [p.x() as f32, p.y() as f32, p.z() as f32])
                .collect();
            let mut faces = vec![];
            let mut normals = Vec::with_capacity(mesh.indices.len());
            for (i, (id, key)) in self.face_keys(part.solid)?.into_iter().enumerate() {
                let face = self.topo.face(id).map_err(err)?;
                let mut cache = HashMap::new();
                for &vertex in &mesh.indices[offsets[i] as usize..offsets[i + 1] as usize] {
                    let normal = *cache.entry(vertex).or_insert_with(|| {
                        let point = mesh.positions[vertex as usize];
                        let (u, v) = face.surface().project_point(point).unwrap_or((0., 0.));
                        let n = face.surface().normal(u, v);
                        let sign = if face.is_reversed() { -1. } else { 1. };
                        [
                            (n.x() * sign) as f32,
                            (n.y() * sign) as f32,
                            (n.z() * sign) as f32,
                        ]
                    });
                    normals.push(normal);
                }
                let plane = if key.starts_with("untracked:") {
                    None
                } else {
                    self.anchors
                        .iter()
                        .find(|(k, _, _)| reference_matches(k, &key))
                        .map(|(_, p, _)| p.clone())
                };
                faces.push(FaceView {
                    key,
                    part: part.key.clone(),
                    start: offsets[i] as usize,
                    end: offsets[i + 1] as usize,
                    plane,
                    global_id,
                });
                global_id += 1;
            }
            let mut edges = vec![];
            let uses = edge_to_face_map(&self.topo, part.solid).map_err(err)?;
            for (id, key) in self.edge_keys(part.solid)? {
                let seam = uses
                    .get(&id.index())
                    .is_some_and(|faces| faces.len() == 2 && faces[0] == faces[1]);
                let edge = self.topo.edge(id).map_err(err)?;
                let a = self.topo.vertex(edge.start()).map_err(err)?.point();
                let b = self.topo.vertex(edge.end()).map_err(err)?.point();
                let (t0, t1) = edge.curve().domain_with_endpoints(a, b);
                let n = if matches!(edge.curve(), EdgeCurve::Line) {
                    1
                } else {
                    128
                };
                let points = (0..=n)
                    .map(|i| {
                        let p = edge.curve().evaluate_with_endpoints(
                            t0 + (t1 - t0) * i as f64 / n as f64,
                            a,
                            b,
                        );
                        [p.x() as f32, p.y() as f32, p.z() as f32]
                    })
                    .collect();
                edges.push(EdgeView {
                    key,
                    part: part.key.clone(),
                    points,
                    seam,
                    axis: analytic_edge_axis(&self.topo, id)?.map(|(point, direction)| {
                        (
                            [point.x(), point.y(), point.z()],
                            [direction.x(), direction.y(), direction.z()],
                        )
                    }),
                });
            }
            let volume = solid_volume(&self.topo, part.solid)?;
            scene.meshes.push(MeshView {
                part: part.key.clone(),
                name: part.name.clone(),
                positions,
                indices: mesh.indices,
                normals,
                faces,
                edges,
                volume,
                bounds,
                centroid,
            });
        }
        Ok(scene)
    }
    pub fn export(&self, format: &str, path: &std::path::Path) -> Result<()> {
        if self.parts.is_empty() {
            return Err("There are no parts to export".into());
        }
        use std::io::{BufWriter, Seek, SeekFrom, Write};
        crate::model::atomic_write(path, |file| {
            let mut writer = BufWriter::new(file);
            match format {
                #[cfg(feature = "step")]
                "step" => {
                    let text = brepkit_io::step::write_step(
                        &self.topo,
                        &self.parts.iter().map(|p| p.solid).collect::<Vec<_>>(),
                    )
                    .map_err(err)?;
                    writer.write_all(text.as_bytes()).map_err(err)?;
                }
                "stl" => {
                    let mut header = [0u8; 84];
                    header[..20].copy_from_slice(b"Rust CAD millimeters");
                    writer.write_all(&header).map_err(err)?;
                    let mut count = 0u32;
                    for part in &self.parts {
                        let mesh = ops::tessellate::tessellate_solid_with_tolerance(
                            &self.topo, part.solid, 0.03, 0.1,
                        )
                        .map_err(err)?;
                        for tri in mesh.indices.as_chunks::<3>().0 {
                            let p = [
                                mesh.positions[tri[0] as usize],
                                mesh.positions[tri[1] as usize],
                                mesh.positions[tri[2] as usize],
                            ];
                            let n = (p[1] - p[0])
                                .cross(p[2] - p[0])
                                .normalize()
                                .unwrap_or(Vec3::new(0., 0., 1.));
                            let mut record = [0u8; 50];
                            for (i, v) in [
                                n.x(),
                                n.y(),
                                n.z(),
                                p[0].x(),
                                p[0].y(),
                                p[0].z(),
                                p[1].x(),
                                p[1].y(),
                                p[1].z(),
                                p[2].x(),
                                p[2].y(),
                                p[2].z(),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                record[i * 4..i * 4 + 4].copy_from_slice(&(v as f32).to_le_bytes());
                            }
                            writer.write_all(&record).map_err(err)?;
                            count = count
                                .checked_add(1)
                                .ok_or("STL exceeds the triangle count limit")?;
                        }
                    }
                    writer.seek(SeekFrom::Start(80)).map_err(err)?;
                    writer.write_all(&count.to_le_bytes()).map_err(err)?;
                }
                _ => return Err(format!("Exporter '{format}' is not installed")),
            }
            writer.flush().map_err(err)
        })
    }
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Measure a solid, integrating complete bilinear ruled patches exactly before
/// using the kernel's analytic and tessellation-based measurements.
pub fn solid_volume(topo: &Topology, solid: SolidId) -> Result<f64> {
    match sweep_loft::exact_ruled_volume(topo, solid) {
        Some(volume) => Ok(volume),
        None => ops::measure::solid_volume(topo, solid, 0.04).map_err(err),
    }
}
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn plane_matches(plane: &Plane, n: Vec3, d: f64) -> bool {
    let normal = [n.x(), n.y(), n.z()];
    let alignment = dot(plane.normal, normal);
    (alignment.abs() - 1.).abs() < 1e-6 && (dot(normal, plane.origin) - d).abs() < 1e-5
}
