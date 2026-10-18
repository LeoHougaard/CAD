//! Linked, offline assemblies with deterministic mate positions and frozen contexts.
mod context;
mod topology;
mod transform;
use crate::{
    engine::{Geometry, MeshView, Runtime, Scene},
    model::{Document, Feature, Plane, Result},
    plugins::{Numbers, Registry},
};
pub use context::{BindingKind, Context, ContextBinding};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
pub use transform::{Transform, validate_plane};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub path: Option<PathBuf>,
    pub revision: String,
    pub document: Document,
}
impl Source {
    pub fn new(
        id: String,
        name: String,
        document: Document,
        path: Option<PathBuf>,
    ) -> Result<Self> {
        document.validate()?;
        let revision = document_revision(&document)?;
        Ok(Self {
            id,
            name,
            path,
            revision,
            document,
        })
    }
    pub fn refresh_revision(&mut self) -> Result<()> {
        self.document.validate()?;
        self.revision = document_revision(&self.document)?;
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub source: String,
    pub part: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub fixed: bool,
}
impl Instance {
    pub fn new(id: String, source: String, part: String) -> Self {
        Self {
            name: id.clone(),
            id,
            source,
            part,
            transform: Transform::default(),
            fixed: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Connector {
    pub instance: String,
    #[serde(default)]
    pub frame: Transform,
    #[serde(default)]
    pub face: Option<String>,
    #[serde(default)]
    pub edge: Option<String>,
}
impl Connector {
    pub fn origin(instance: impl Into<String>) -> Self {
        Self {
            instance: instance.into(),
            frame: Transform::default(),
            face: None,
            edge: None,
        }
    }
    pub fn on_face(instance: impl Into<String>, face: impl Into<String>) -> Self {
        Self {
            face: Some(face.into()),
            ..Self::origin(instance)
        }
    }
    pub fn on_edge(instance: impl Into<String>, edge: impl Into<String>) -> Self {
        Self {
            edge: Some(edge.into()),
            ..Self::origin(instance)
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MateKind {
    Fastened,
    Revolute,
    Slider,
    Cylindrical,
    Planar,
    Ball,
}
impl MateKind {
    pub fn degrees_of_freedom(self) -> usize {
        match self {
            Self::Fastened => 0,
            Self::Revolute | Self::Slider => 1,
            Self::Cylindrical => 2,
            Self::Planar | Self::Ball => 3,
        }
    }
    fn allowed(self) -> ([bool; 3], [bool; 3]) {
        match self {
            Self::Fastened => ([false; 3], [false; 3]),
            Self::Revolute => ([false; 3], [false, false, true]),
            Self::Slider => ([false, false, true], [false; 3]),
            Self::Cylindrical => ([false, false, true], [false, false, true]),
            Self::Planar => ([true, true, false], [false, false, true]),
            Self::Ball => ([false; 3], [true; 3]),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Limit {
    pub min: f64,
    pub max: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mate {
    pub id: String,
    pub kind: MateKind,
    pub a: Connector,
    pub b: Connector,
    #[serde(default)]
    pub plugin: Option<Feature>,
    #[serde(default)]
    pub offset: Transform,
    #[serde(default)]
    pub position: [f64; 3],
    #[serde(default)]
    pub rotation_deg: [f64; 3],
    #[serde(default)]
    pub translation_limits: [Option<Limit>; 3],
    #[serde(default)]
    pub rotation_limits: [Option<Limit>; 3],
    #[serde(default)]
    pub suppressed: bool,
}
impl Mate {
    pub fn new(kind: MateKind, a: Connector, b: Connector) -> Self {
        Self {
            id: String::new(),
            kind,
            a,
            b,
            plugin: None,
            offset: Transform::default(),
            position: [0.; 3],
            rotation_deg: [0.; 3],
            translation_limits: [None; 3],
            rotation_limits: [None; 3],
            suppressed: false,
        }
    }
    fn validate(&self) -> Result<()> {
        identity(&self.id)?;
        self.a.frame.validate()?;
        self.b.frame.validate()?;
        self.offset.validate()?;
        for connector in [&self.a, &self.b] {
            if connector.face.is_some() && connector.edge.is_some() {
                return Err("A mate connector cannot reference both a face and an edge".into());
            }
            if connector
                .face
                .iter()
                .chain(&connector.edge)
                .any(|key| key.is_empty() || key.len() > 2048 || key.starts_with("untracked:"))
            {
                return Err(
                    "Mate connectors require a stable face or circular-edge reference".into(),
                );
            }
        }
        if self.a.instance == self.b.instance {
            return Err(format!("Mate {} connects an instance to itself", self.id));
        }
        let (linear, angular) = self.kind.allowed();
        for (values, limits, allowed) in [
            (self.position, self.translation_limits, linear),
            (self.rotation_deg, self.rotation_limits, angular),
        ] {
            for i in 0..3 {
                if !values[i].is_finite()
                    || values[i].abs() > 1e8
                    || (!allowed[i] && values[i].abs() > 1e-9)
                {
                    return Err(format!(
                        "Mate {} has a value on a constrained axis",
                        self.id
                    ));
                }
                if let Some(limit) = limits[i]
                    && (!allowed[i]
                        || !limit.min.is_finite()
                        || !limit.max.is_finite()
                        || limit.min > limit.max
                        || values[i] < limit.min - 1e-9
                        || values[i] > limit.max + 1e-9)
                {
                    return Err(format!("Mate {} exceeds or has invalid limits", self.id));
                }
            }
        }
        Ok(())
    }
    fn motion(&self) -> Transform {
        let mut t = Transform::from_euler_degrees(self.rotation_deg);
        t.translation = self.position;
        t
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssemblyDocument {
    pub format: u32,
    pub name: String,
    pub sources: Vec<Source>,
    pub instances: Vec<Instance>,
    pub mates: Vec<Mate>,
    pub contexts: Vec<Context>,
    pub next_id: u64,
}
impl Default for AssemblyDocument {
    fn default() -> Self {
        Self {
            format: 1,
            name: "Assembly".into(),
            sources: vec![],
            instances: vec![],
            mates: vec![],
            contexts: vec![],
            next_id: 1,
        }
    }
}
fn identity(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        Err("Invalid assembly identity".into())
    } else {
        Ok(())
    }
}
impl AssemblyDocument {
    pub fn allocate_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}{}", self.next_id);
            self.next_id += 1;
            if !self.sources.iter().any(|s| s.id == id)
                && !self.instances.iter().any(|i| i.id == id)
                && !self.mates.iter().any(|m| m.id == id)
                && !self.contexts.iter().any(|c| c.id == id)
            {
                return id;
            }
        }
    }
    pub fn revision(&self) -> Result<String> {
        Ok(revision(
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        ))
    }
    pub fn validate(&self) -> Result<()> {
        if self.format != 1 {
            return Err(format!("Unsupported assembly format {}", self.format));
        }
        if self.name.len() > 4096
            || self.sources.len() > 128
            || self.instances.len() > 1024
            || self.mates.len() > 2048
            || self.contexts.len() > 32
            || self.next_id == 0
            || self.next_id > 1_000_000_000
        {
            return Err("Assembly exceeds its data limits".into());
        }
        let mut ids = BTreeSet::new();
        let mut features = 0;
        for source in &self.sources {
            identity(&source.id)?;
            if !ids.insert(&source.id) {
                return Err("Duplicate assembly identity".into());
            }
            source.document.validate()?;
            features += source.document.features.len();
            if source.revision != document_revision(&source.document)? {
                return Err(format!(
                    "Source {} revision does not match its snapshot",
                    source.id
                ));
            }
        }
        if features > 10000 {
            return Err("Assembly sources exceed 10000 total features".into());
        }
        for instance in &self.instances {
            identity(&instance.id)?;
            if !ids.insert(&instance.id) {
                return Err("Duplicate assembly identity".into());
            }
            instance.transform.validate()?;
            if !self.sources.iter().any(|s| s.id == instance.source) {
                return Err(format!(
                    "Instance {} references missing source {}",
                    instance.id, instance.source
                ));
            }
            if instance.part.is_empty() || instance.part.len() > 1024 {
                return Err("Invalid source part reference".into());
            }
        }
        for mate in &self.mates {
            mate.validate()?;
            if !ids.insert(&mate.id) {
                return Err("Duplicate assembly identity".into());
            }
            for c in [&mate.a, &mate.b] {
                if !self.instances.iter().any(|i| i.id == c.instance) {
                    return Err(format!(
                        "Mate {} references missing instance {}",
                        mate.id, c.instance
                    ));
                }
            }
        }
        for context in &self.contexts {
            identity(&context.id)?;
            if !ids.insert(&context.id) {
                return Err("Duplicate assembly identity".into());
            }
            context.validate()?;
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Assembly exceeds the 32 MiB input limit".into());
        }
        let document: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        document.validate()?;
        Ok(document)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Assembly exceeds the 32 MiB input limit".into());
        }
        crate::settings::write_atomic(path, |f| f.write_all(&bytes).map_err(|e| e.to_string()))
    }
    pub fn save_checked(&self, path: &Path, expected: Option<&str>) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let _lock = crate::file_lock::FileLock::try_acquire(path)?;
        if path.exists() {
            let expected =
                expected.ok_or("Overwriting an assembly requires its expected revision")?;
            if Self::load(path)?.revision()? != expected {
                return Err("Assembly changed in another session; reload before saving".into());
            }
        } else if expected.is_some() {
            return Err("Assembly was removed in another session".into());
        }
        self.save(path)
    }
}
pub fn revision(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{hash:016x}")
}
pub fn document_revision(document: &Document) -> Result<String> {
    Ok(revision(
        &serde_json::to_vec(document).map_err(|e| e.to_string())?,
    ))
}

pub struct EvaluatedSource {
    pub geometry: Geometry,
    pub scene: Arc<Scene>,
    pub face_frames: BTreeMap<(String, String), Transform>,
    pub edge_frames: BTreeMap<(String, String), Transform>,
}
pub struct AssemblyEvaluation {
    pub scene: Arc<Scene>,
    pub poses: BTreeMap<String, Transform>,
    pub sources: BTreeMap<String, Arc<EvaluatedSource>>,
    pub evaluated_sources: usize,
    pub degrees_of_freedom: usize,
}
pub struct AssemblyRuntime {
    pub registry: Arc<Registry>,
    cache: BTreeMap<(String, String), Arc<EvaluatedSource>>,
}
impl AssemblyRuntime {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            cache: BTreeMap::new(),
        }
    }
    pub fn mate_from_plugin(
        &self,
        id: &str,
        params: serde_json::Value,
        a: Connector,
        b: Connector,
    ) -> Result<Mate> {
        let mut feature = self.registry.create(id, &mut Document::default())?;
        let values = params
            .as_object()
            .ok_or("Mate plugin parameters must be an object")?;
        for (key, value) in values {
            feature.params[key] = value.clone();
        }
        let mut mate = Mate::new(MateKind::Fastened, a, b);
        mate.plugin = Some(feature);
        self.interpret_mate(&mut mate)?;
        Ok(mate)
    }
    pub fn resolve_mate(&self, mate: &mut Mate) -> Result<()> {
        let mut candidate = mate.clone();
        self.interpret_mate(&mut candidate)?;
        candidate.validate()?;
        *mate = candidate;
        Ok(())
    }
    fn interpret_mate(&self, mate: &mut Mate) -> Result<()> {
        let Some(feature) = &mate.plugin else {
            return Ok(());
        };
        let package = self
            .registry
            .plugins
            .get(&feature.plugin)
            .ok_or("Missing mate plugin")?;
        if package.manifest.category != "mate" {
            return Err("Assembly mates require a plugin in the mate category".into());
        }
        let plan = self
            .registry
            .evaluate(feature, &Numbers::new(&BTreeMap::new())?)?;
        if plan.get("op").and_then(serde_json::Value::as_str) != Some("mate") {
            return Err("Mate plugin must return op: mate".into());
        }
        mate.kind = serde_json::from_value(
            plan.get("kind")
                .cloned()
                .ok_or("Mate plugin must return a native mate kind")?,
        )
        .map_err(|e| e.to_string())?;
        for key in ["position", "rotation_deg"] {
            if let Some(value) = plan.get(key) {
                let vector = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if key == "position" {
                    mate.position = vector
                } else {
                    mate.rotation_deg = vector
                }
            }
        }
        if let Some(value) = plan.get("offset") {
            mate.offset = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn retain_sources(&mut self, sources: &[Source]) {
        self.cache.retain(|(id, revision), _| {
            sources
                .iter()
                .any(|s| &s.id == id && &s.revision == revision)
        });
    }
    fn source(&mut self, source: &Source) -> Result<Arc<EvaluatedSource>> {
        let key = (source.id.clone(), source.revision.clone());
        if let Some(e) = self.cache.get(&key) {
            return Ok(e.clone());
        }
        let result = Runtime::shared(self.registry.clone()).evaluate(&source.document)?;
        if let Some(error) = &result.scene.error {
            return Err(format!("Source {}: {error}", source.name));
        }
        let mut face_frames = BTreeMap::new();
        let mut edge_frames = BTreeMap::new();
        for part in &result.geometry.parts {
            let edges = result.geometry.edge_keys(part.solid)?;
            for (id, key) in &edges {
                if key.starts_with("untracked:")
                    || edges.iter().filter(|(_, k)| k == key).count() != 1
                {
                    continue;
                }
                if let brepkit_topology::edge::EdgeCurve::Circle(circle) = result
                    .geometry
                    .topo
                    .edge(*id)
                    .map_err(|e| e.to_string())?
                    .curve()
                {
                    let center = circle.center();
                    let normal = circle.normal();
                    let u = circle.u_axis();
                    let v = normal.cross(u);
                    let plane = Plane {
                        origin: [center.x(), center.y(), center.z()],
                        u: [u.x(), u.y(), u.z()],
                        v: [v.x(), v.y(), v.z()],
                        normal: [normal.x(), normal.y(), normal.z()],
                    };
                    edge_frames.insert(
                        (part.key.clone(), key.clone()),
                        Transform::from_plane(&plane)?,
                    );
                }
            }
        }
        for mesh in &result.scene.meshes {
            for face in &mesh.faces {
                if let Some(plane) = &face.plane {
                    let mut centered = plane.clone();
                    let mut sum = [0.; 3];
                    let mut weight = 0.;
                    for tri in mesh.indices[face.start..face.end].as_chunks::<3>().0 {
                        let [a, b, c] = [tri[0], tri[1], tri[2]]
                            .map(|i| mesh.positions[i as usize].map(f64::from));
                        let cross = transform::cross(
                            transform::add(b, transform::scale(a, -1.)),
                            transform::add(c, transform::scale(a, -1.)),
                        );
                        let area = cross.iter().map(|v| v * v).sum::<f64>().sqrt();
                        sum = transform::add(
                            sum,
                            transform::scale(transform::add(transform::add(a, b), c), area / 3.),
                        );
                        weight += area;
                    }
                    if weight > 1e-16 {
                        let center = transform::scale(sum, 1. / weight);
                        let displacement =
                            transform::add(center, transform::scale(plane.origin, -1.));
                        let offset = (0..3)
                            .map(|i| displacement[i] * plane.normal[i])
                            .sum::<f64>();
                        centered.origin =
                            transform::add(center, transform::scale(plane.normal, -offset));
                    }
                    face_frames.insert(
                        (mesh.part.clone(), face.key.clone()),
                        Transform::from_plane(&centered)?,
                    );
                }
            }
        }
        let evaluation = Arc::new(EvaluatedSource {
            geometry: result.geometry,
            scene: result.scene,
            face_frames,
            edge_frames,
        });
        self.cache.insert(key, evaluation.clone());
        Ok(evaluation)
    }
    pub fn evaluate(&mut self, document: &AssemblyDocument) -> Result<AssemblyEvaluation> {
        document.validate()?;
        self.retain_sources(&document.sources);
        let mut sources = BTreeMap::new();
        let mut evaluated_sources = 0;
        for source in &document.sources {
            if !self
                .cache
                .contains_key(&(source.id.clone(), source.revision.clone()))
            {
                evaluated_sources += 1;
            }
            sources.insert(source.id.clone(), self.source(source)?);
        }
        for instance in &document.instances {
            if !sources[&instance.source]
                .scene
                .meshes
                .iter()
                .any(|p| p.part == instance.part)
            {
                return Err(format!(
                    "Instance {} references missing part {}",
                    instance.id, instance.part
                ));
            }
        }
        let mut interpreted;
        let solve_document = if document
            .mates
            .iter()
            .any(|m| m.plugin.is_some() && !m.suppressed)
        {
            interpreted = document.clone();
            for mate in interpreted.mates.iter_mut().filter(|m| !m.suppressed) {
                self.interpret_mate(mate)?;
            }
            interpreted.validate()?;
            &interpreted
        } else {
            document
        };
        let (poses, degrees_of_freedom) = solve(solve_document, &sources)?;
        let scene = compose_scene(&document.instances, &poses, &sources, None)?;
        Ok(AssemblyEvaluation {
            scene: Arc::new(scene),
            poses,
            sources,
            evaluated_sources,
            degrees_of_freedom,
        })
    }
    pub fn update_source(
        &mut self,
        document: &mut AssemblyDocument,
        id: &str,
        source: Document,
    ) -> Result<()> {
        let mut candidate = document.clone();
        let entry = candidate
            .sources
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Missing assembly source")?;
        entry.document = source;
        entry.refresh_revision()?;
        self.evaluate(&candidate)?;
        *document = candidate;
        Ok(())
    }
    pub fn reload_source(&mut self, document: &mut AssemblyDocument, id: &str) -> Result<()> {
        let source = document
            .sources
            .iter()
            .find(|s| s.id == id)
            .ok_or("Missing assembly source")?;
        let path = source.path.as_ref().ok_or("Source has no linked file")?;
        self.update_source(document, id, Document::load(path)?)
    }
    pub fn reload_source_at(
        &mut self,
        document: &mut AssemblyDocument,
        id: &str,
        assembly_path: &Path,
    ) -> Result<()> {
        let source = document
            .sources
            .iter()
            .find(|s| s.id == id)
            .ok_or("Missing assembly source")?;
        let path = source.path.as_ref().ok_or("Source has no linked file")?;
        let path = if path.is_absolute() {
            path.clone()
        } else {
            assembly_path.parent().unwrap_or(Path::new(".")).join(path)
        };
        self.update_source(document, id, Document::load(&path)?)
    }
}
fn connector_frame(
    c: &Connector,
    document: &AssemblyDocument,
    sources: &BTreeMap<String, Arc<EvaluatedSource>>,
) -> Result<Transform> {
    let instance = document
        .instances
        .iter()
        .find(|i| i.id == c.instance)
        .ok_or("Missing mate instance")?;
    if let Some(key) = &c.edge {
        let key = source_key(&c.instance, key)?;
        let source = &sources[&instance.source];
        let part = source
            .geometry
            .parts
            .iter()
            .find(|p| p.key == instance.part)
            .ok_or("Missing source part")?;
        let edges = source.geometry.edge_keys(part.solid)?;
        let matches: Vec<_> = edges.iter().filter(|(_, k)| crate::model::reference_matches(k, key)).collect();
        if matches.len() != 1 {
            return Err(format!(
                "Mate edge {key} is missing or ambiguous on {}",
                instance.id
            ));
        }
        let frame=source.edge_frames.get(&(instance.part.clone(),matches[0].1.clone())).ok_or("Mate edge connectors require a stable circular edge; straight edges are not supported")?;
        Ok(frame.compose(c.frame))
    } else if let Some(key) = &c.face {
        let key = source_key(&c.instance, key)?;
        let mesh = sources[&instance.source]
            .scene
            .meshes
            .iter()
            .find(|p| p.part == instance.part)
            .ok_or("Missing source part")?;
        let matches: Vec<_> = mesh.faces.iter().filter(|f| crate::model::reference_matches(&f.key, key)).collect();
        if matches.len() != 1 {
            return Err(format!(
                "Mate face {key} is missing or ambiguous on {}",
                instance.id
            ));
        }
        let frame = sources[&instance.source]
            .face_frames
            .get(&(instance.part.clone(), matches[0].key.clone()))
            .ok_or("Mate connectors currently require a planar face or an explicit frame")?;
        Ok(frame.compose(c.frame))
    } else {
        Ok(c.frame)
    }
}
fn source_key<'a>(instance: &str, key: &'a str) -> Result<&'a str> {
    let prefix = format!("{instance}::");
    let key = if let Some(raw) = key.strip_prefix(&prefix) {
        raw
    } else if key.contains("::") {
        return Err("Connector reference belongs to a different assembly instance".into());
    } else {
        key
    };
    if key.starts_with("untracked:") {
        return Err("Mate connectors require a stable geometry reference".into());
    }
    Ok(key)
}
fn solve(
    document: &AssemblyDocument,
    sources: &BTreeMap<String, Arc<EvaluatedSource>>,
) -> Result<(BTreeMap<String, Transform>, usize)> {
    let mut edges = Vec::new();
    let mut pairs = BTreeSet::new();
    for mate in document.mates.iter().filter(|m| !m.suppressed) {
        let pair = if mate.a.instance < mate.b.instance {
            (&mate.a.instance, &mate.b.instance)
        } else {
            (&mate.b.instance, &mate.a.instance)
        };
        if !pairs.insert(pair) {
            return Err("Only one active mate between a pair of instances is supported".into());
        }
        let relation = connector_frame(&mate.a, document, sources)?
            .compose(mate.offset)
            .compose(mate.motion())
            .compose(connector_frame(&mate.b, document, sources)?.inverse());
        edges.push((mate, relation));
    }
    let mut poses: BTreeMap<_, _> = document
        .instances
        .iter()
        .filter(|i| i.fixed)
        .map(|i| (i.id.clone(), i.transform))
        .collect();
    let mut rigid = BTreeMap::new();
    let mut groups = Vec::new();
    for root in &document.instances {
        if rigid.contains_key(root.id.as_str()) {
            continue;
        }
        let mut component = BTreeSet::from([root.id.as_str()]);
        loop {
            let before = component.len();
            for (mate, _) in edges
                .iter()
                .filter(|(mate, _)| mate.kind == MateKind::Fastened)
            {
                if component.contains(mate.a.instance.as_str())
                    || component.contains(mate.b.instance.as_str())
                {
                    component.insert(&mate.a.instance);
                    component.insert(&mate.b.instance);
                }
            }
            if before == component.len() {
                break;
            }
        }
        for id in &component {
            rigid.insert(*id, groups.len());
        }
        groups.push(component);
    }
    let mut visited = BTreeSet::new();
    for root in 0..groups.len() {
        if visited.contains(&root) {
            continue;
        }
        let mut component = BTreeSet::from([root]);
        loop {
            let before = component.len();
            for (m, _) in edges.iter().filter(|(m, _)| m.kind != MateKind::Fastened) {
                let a = rigid[m.a.instance.as_str()];
                let b = rigid[m.b.instance.as_str()];
                if component.contains(&a) || component.contains(&b) {
                    component.insert(a);
                    component.insert(b);
                }
            }
            if before == component.len() {
                break;
            }
        }
        let linked = edges
            .iter()
            .filter(|(m, _)| {
                m.kind != MateKind::Fastened && component.contains(&rigid[m.a.instance.as_str()])
            })
            .count();
        let grounded: BTreeSet<_> = document
            .instances
            .iter()
            .filter(|i| i.fixed && component.contains(&rigid[i.id.as_str()]))
            .map(|i| rigid[i.id.as_str()])
            .collect();
        if linked >= component.len() || linked > 0 && grounded.len() > 1 {
            return Err(
                "Articulated closed-loop or multiple-grounded mate solving is not supported".into(),
            );
        }
        visited.extend(component);
    }
    let mut dof = 0;
    let mut tree_mates = BTreeSet::new();
    loop {
        loop {
            let mut changed = false;
            for (m, relative) in &edges {
                match (
                    poses.get(&m.a.instance).copied(),
                    poses.get(&m.b.instance).copied(),
                ) {
                    (Some(a), None) => {
                        poses.insert(m.b.instance.clone(), a.compose(*relative));
                        dof += m.kind.degrees_of_freedom();
                        tree_mates.insert(m.id.as_str());
                        changed = true;
                    }
                    (None, Some(b)) => {
                        poses.insert(m.a.instance.clone(), b.compose(relative.inverse()));
                        dof += m.kind.degrees_of_freedom();
                        tree_mates.insert(m.id.as_str());
                        changed = true;
                    }
                    _ => (),
                }
            }
            if !changed {
                break;
            }
        }
        let Some(root) = document
            .instances
            .iter()
            .find(|i| !poses.contains_key(&i.id))
        else {
            break;
        };
        poses.insert(root.id.clone(), root.transform);
        dof += 6;
    }
    for (mate, relative) in edges {
        if !tree_mates.contains(mate.id.as_str()) && mate.kind != MateKind::Fastened {
            return Err(format!(
                "Mate {} closes an articulated loop or joins grounded components; loop solving is not supported",
                mate.id
            ));
        }
        if !poses[&mate.a.instance]
            .compose(relative)
            .approximately(&poses[&mate.b.instance])
        {
            return Err(format!(
                "Mate {} conflicts with a fixed instance or a closed-loop pose; loop solving is not supported",
                mate.id
            ));
        }
    }
    for pose in poses.values() {
        pose.validate()?;
    }
    Ok((poses, dof))
}
pub(crate) fn compose_scene(
    instances: &[Instance],
    poses: &BTreeMap<String, Transform>,
    sources: &BTreeMap<String, Arc<EvaluatedSource>>,
    exclude: Option<&str>,
) -> Result<Scene> {
    let mut scene = Scene::default();
    let mut global = 0;
    for instance in instances.iter().filter(|i| exclude != Some(i.id.as_str())) {
        let source = sources
            .get(&instance.source)
            .ok_or("Missing frozen source")?;
        let original = source
            .scene
            .meshes
            .iter()
            .find(|m| m.part == instance.part)
            .ok_or("Missing frozen part")?;
        let pose = poses
            .get(&instance.id)
            .ok_or("Missing solved instance pose")?;
        let mut mesh = transform_mesh(original, pose);
        mesh.part = instance.id.clone();
        mesh.name = instance.name.clone();
        for face in &mut mesh.faces {
            face.part = instance.id.clone();
            face.key = format!("{}::{}", instance.id, face.key);
            face.global_id = global;
            global += 1;
        }
        for edge in &mut mesh.edges {
            edge.part = instance.id.clone();
            edge.key = format!("{}::{}", instance.id, edge.key);
        }
        scene.meshes.push(mesh);
    }
    Ok(scene)
}
pub fn transform_mesh(original: &MeshView, pose: &Transform) -> MeshView {
    let mut mesh = original.clone();
    for p in &mut mesh.positions {
        *p = pose.point(p.map(f64::from)).map(|v| v as f32);
    }
    for n in &mut mesh.normals {
        *n = pose.vector(n.map(f64::from)).map(|v| v as f32);
    }
    for face in &mut mesh.faces {
        face.plane = face.plane.as_ref().map(|p| pose.plane(p));
    }
    for edge in &mut mesh.edges {
        edge.axis = edge.axis.map(|(point, direction)| (pose.point(point), pose.vector(direction)));
        for point in &mut edge.points {
            *point = pose.point(point.map(f64::from)).map(|v| v as f32);
        }
    }
    mesh.centroid = pose.point(mesh.centroid);
    mesh.bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
    for p in &mesh.positions {
        for (i, v) in p.iter().enumerate() {
            mesh.bounds[0][i] = mesh.bounds[0][i].min(*v);
            mesh.bounds[1][i] = mesh.bounds[1][i].max(*v);
        }
    }
    mesh
}
impl AssemblyEvaluation {
    pub fn geometry(&self, document: &AssemblyDocument) -> Result<Geometry> {
        let mut geometry = Geometry::default();
        for instance in &document.instances {
            let source = self
                .sources
                .get(&instance.source)
                .ok_or("Missing evaluated assembly source")?;
            let part = source
                .geometry
                .parts
                .iter()
                .find(|p| p.key == instance.part)
                .ok_or("Missing exact source part")?;
            let solid =
                topology::copy_solid(&source.geometry.topo, &mut geometry.topo, part.solid)?;
            let pose = self
                .poses
                .get(&instance.id)
                .ok_or("Missing solved instance pose")?;
            brepkit_operations::transform::transform_solid(
                &mut geometry.topo,
                solid,
                &pose.matrix(),
            )
            .map_err(|e| e.to_string())?;
            geometry.parts.push(crate::engine::Part {
                key: instance.id.clone(),
                name: instance.name.clone(),
                solid,
                feature: format!("{}::{}", instance.id, part.feature),
                source_features: part.source_features.clone(),
                reference_sources: part.reference_sources.clone(),
            });
        }
        Ok(geometry)
    }
    pub fn export(&self, document: &AssemblyDocument, format: &str, path: &Path) -> Result<()> {
        self.geometry(document)?.export(format, path)
    }
    /// Exact source topology transformed on demand, independently of display tessellation.
    pub fn instance_geometry(&self, document: &AssemblyDocument, id: &str) -> Result<Geometry> {
        let instance = document
            .instances
            .iter()
            .find(|i| i.id == id)
            .ok_or("Missing assembly instance")?;
        let mut geometry = self
            .sources
            .get(&instance.source)
            .ok_or("Missing evaluated source")?
            .geometry
            .clone();
        geometry.parts.retain(|p| p.key == instance.part);
        let part = geometry
            .parts
            .first_mut()
            .ok_or("Missing exact source part")?;
        brepkit_operations::transform::transform_solid(
            &mut geometry.topo,
            part.solid,
            &self.poses[id].matrix(),
        )
        .map_err(|e| e.to_string())?;
        part.key = id.into();
        part.name = instance.name.clone();
        geometry.anchors.clear();
        geometry.sketches.clear();
        geometry.outputs.clear();
        Ok(geometry)
    }
}
