use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, String>;
mod profiles;
pub use profiles::ProfileReference;
mod references;
pub use references::{canonical_reference, reference_matches};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaneReference {
    Origin { plane: String },
    Datum { feature: String },
    Face { part: String, face: String },
}
impl PlaneReference {
    pub fn validate(&self) -> Result<()> {
        let reference = |s: &str, max: usize| !s.is_empty() && s != "@last" && s.len() <= max;
        match self {
            Self::Origin { plane } if ["Top", "Front", "Right"].contains(&plane.as_str()) => Ok(()),
            Self::Datum { feature } if reference(feature, 128) => Ok(()),
            Self::Face { part, face }
                if reference(part, 1024)
                    && reference(face, 8192)
                    && !face.contains("untracked:") =>
            {
                Ok(())
            }
            _ => Err(concat!(
                "Choose an origin plane, an earlier construction plane, ",
                "or a persistent planar face"
            )
            .into()),
        }
    }
}

pub fn atomic_write(
    path: &std::path::Path,
    write: impl FnOnce(&mut std::fs::File) -> Result<()>,
) -> Result<()> {
    let name = path
        .file_name()
        .ok_or("Choose a file name")?
        .to_string_lossy();
    let temporary = path.with_file_name(format!("{name}.tmp"));
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    write(&mut file)?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(temporary, path).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub format: u32,
    pub name: String,
    pub variables: BTreeMap<String, String>,
    pub features: Vec<Feature>,
    pub rollback: Option<usize>,
    pub next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format: 2,
            name: "Untitled".into(),
            variables: BTreeMap::new(),
            features: vec![],
            rollback: None,
            next_id: 1,
        }
    }
}

impl Document {
    pub fn allocate_id(&mut self) -> String {
        let id = format!("f{}", self.next_id);
        self.next_id += 1;
        id
    }
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        self.validate()?;
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        use std::io::Write;
        atomic_write(path, |file| {
            file.write_all(&text).map_err(|e| e.to_string())
        })
    }
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if metadata.len() > 32 * 1024 * 1024 {
            return Err("Document exceeds the 32 MiB input limit".into());
        }
        let doc: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        doc.validate()?;
        Ok(doc)
    }
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.format, 1 | 2) {
            return Err(format!("Unsupported document format {}", self.format));
        }
        let mut ids = std::collections::BTreeSet::new();
        if self.features.len() > 2000 || self.variables.len() > 1000 {
            return Err("Document has more than 2000 features".into());
        }
        if self.next_id == 0 || self.next_id > 1_000_000_000 {
            return Err("Invalid feature identity counter".into());
        }
        for f in &self.features {
            if f.id.is_empty() || f.id.len() > 128 || !f.params.is_object() {
                return Err("Invalid feature identity or parameters".into());
            }
            if f.id
                .strip_prefix('f')
                .and_then(|n| n.parse::<u64>().ok())
                .is_some_and(|n| n >= self.next_id)
            {
                return Err("Feature identity counter would reuse an existing feature".into());
            }
            if !ids.insert(&f.id) {
                return Err(format!("Duplicate feature identity {}", f.id));
            }
            if f.params.get("geometry").is_some() {
                let sk = f.sketch().ok_or("Malformed sketch geometry")?;
                if sk.entities.len() > 1000 || sk.constraints.len() > 4000 {
                    return Err("Sketch exceeds its entity or constraint limit".into());
                }
                for c in &sk.constraints {
                    if c.points.iter().any(|i| *i > 3)
                        || c.value.len() > 2048
                        || c.position
                            .is_some_and(|p| p.iter().any(|x| !x.is_finite() || x.abs() > 1e8))
                    {
                        return Err("Invalid sketch constraint data".into());
                    }
                }
                sk.validate_curve_identities()?;
                let mut entity_ids = std::collections::BTreeSet::new();
                for e in &sk.entities {
                    if e.id == 0 || e.id > sk.next_id || !entity_ids.insert(e.id) {
                        return Err(
                            "Invalid sketch entity identity counter or duplicate entity".into()
                        );
                    }
                }
            }
        }
        if self.rollback.is_some_and(|r| r > self.features.len()) {
            return Err("Invalid rollback position".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Feature {
    pub id: String,
    pub plugin: String,
    pub plugin_version: String,
    pub plugin_hash: String,
    pub name: String,
    pub suppressed: bool,
    pub params: Value,
}

impl Feature {
    pub fn sketch(&self) -> Option<Sketch> {
        serde_json::from_value(self.params.get("geometry")?.clone()).ok()
    }
    pub fn set_sketch(&mut self, sketch: &Sketch) {
        self.params["geometry"] = serde_json::to_value(sketch).expect("sketch is JSON data");
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Sketch {
    pub entities: Vec<Entity>,
    pub constraints: Vec<SketchConstraint>,
    pub next_id: u64,
}

impl Sketch {
    pub fn allocate_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
    pub fn add_rect(&mut self, a: [f64; 2], b: [f64; 2]) {
        let id = self.allocate_id();
        self.entities.push(Entity {
            id,
            kind: "rectangle".into(),
            curve_identity: None,
            values: vec![
                fmt(a[0].min(b[0])),
                fmt(a[1].min(b[1])),
                fmt((a[0] - b[0]).abs()),
                fmt((a[1] - b[1]).abs()),
            ],
            construction: false,
        });
    }
    pub fn add_circle(&mut self, center: [f64; 2], radius: f64) {
        let id = self.allocate_id();
        self.entities.push(Entity {
            id,
            kind: "circle".into(),
            curve_identity: None,
            values: vec![fmt(center[0]), fmt(center[1]), fmt(radius)],
            construction: false,
        });
    }
    /// Center and two endpoints, with an explicit orientation for major arcs.
    pub fn add_arc(&mut self, center: [f64; 2], start: [f64; 2], end: [f64; 2], clockwise: bool) {
        let id = self.allocate_id();
        self.entities.push(Entity {
            id,
            kind: "arc".into(),
            curve_identity: None,
            values: [
                center[0],
                center[1],
                start[0],
                start[1],
                end[0],
                end[1],
                if clockwise { -1. } else { 1. },
            ]
            .into_iter()
            .map(fmt)
            .collect(),
            construction: false,
        });
    }
    pub fn add_line(&mut self, a: [f64; 2], b: [f64; 2]) {
        let id = self.allocate_id();
        for entity in &self.entities {
            if entity.kind == "line" {
                for (endpoint, p) in [a, b].into_iter().enumerate() {
                    for other in 0..2 {
                        let q = [
                            entity.values[other * 2].parse::<f64>().ok(),
                            entity.values[other * 2 + 1].parse::<f64>().ok(),
                        ];
                        if let [Some(x), Some(y)] = q
                            && (p[0] - x).hypot(p[1] - y) < 1e-6
                        {
                            self.constraints.push(SketchConstraint {
                                kind: "Coincident".into(),
                                a: id,
                                b: entity.id,
                                value: format!("{endpoint}:{other}"),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        self.entities.push(Entity {
            id,
            kind: "line".into(),
            curve_identity: None,
            values: vec![fmt(a[0]), fmt(a[1]), fmt(b[0]), fmt(b[1])],
            construction: false,
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entity {
    pub id: u64,
    pub kind: String,
    /// Logical source curve retained across a representation-only replacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_identity: Option<String>,
    pub values: Vec<String>,
    pub construction: bool,
}

/// Shared names for region boundaries, their endpoints and generated solid edges.
pub fn curve_key(kind: &str, id: u64, edge: usize) -> String {
    match kind {
        "rectangle" => format!("r{id}.{edge}"),
        "circle" => format!("c{id}"),
        "arc" => format!("a{id}"),
        _ => format!("l{id}"),
    }
}
impl Entity {
    pub fn curve_key(&self, edge: usize) -> String {
        self.curve_identity.clone().unwrap_or_else(|| curve_key(&self.kind, self.id, edge))
    }
}
impl Sketch {
    pub fn validate_curve_identities(&self) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for entity in &self.entities {
            if let Some(key) = &entity.curve_identity {
                if key.len() > 32 || key.len() < 2 { return Err("Invalid sketch curve identity".into()); }
                let (kind, id, edge) = match key.as_bytes()[0] {
                    b'r' if entity.kind == "line" => {
                        let (id, edge) = key[1..].split_once('.').ok_or("Invalid rectangle curve identity")?;
                        ("rectangle", id.parse::<u64>().map_err(|_| "Invalid curve identity")?, edge.parse::<usize>().map_err(|_| "Invalid curve identity")?)
                    }
                    b'l' if entity.kind == "line" => ("line",key[1..].parse::<u64>().map_err(|_| "Invalid curve identity")?,0),
                    b'c' if entity.kind == "circle" => ("circle",key[1..].parse::<u64>().map_err(|_| "Invalid curve identity")?,0),
                    b'a' if entity.kind == "arc" => ("arc",key[1..].parse::<u64>().map_err(|_| "Invalid curve identity")?,0),
                    _ => return Err("Curve identity does not match its geometry".into()),
                };
                if id == 0 || id > self.next_id || edge > 3 || curve_key(kind,id,edge) != *key {
                    return Err("Invalid or noncanonical sketch curve identity".into());
                }
            }
            for edge in 0..if entity.kind == "rectangle" {4} else {1} {
                if !seen.insert(entity.curve_key(edge)) { return Err("Duplicate sketch curve identity".into()); }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SketchConstraint {
    pub kind: String,
    pub a: u64,
    #[serde(default)]
    pub b: u64,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub points: [usize; 2],
    #[serde(default)]
    pub position: Option<[f64; 2]>,
    #[serde(default)]
    pub reference: bool,
    #[serde(default)]
    pub negative: bool,
    #[serde(default)]
    pub relative_position: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Plane {
    pub origin: [f64; 3],
    pub u: [f64; 3],
    pub v: [f64; 3],
    pub normal: [f64; 3],
}

impl Plane {
    pub fn validate(&self) -> Result<()> {
        let dot = |a: [f64; 3], b: [f64; 3]| a.into_iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
        if self
            .origin
            .iter()
            .chain(&self.u)
            .chain(&self.v)
            .chain(&self.normal)
            .any(|v| !v.is_finite() || v.abs() > 1e8)
        {
            return Err("Reference plane contains invalid coordinates".into());
        }
        let cross = [
            self.u[1] * self.v[2] - self.u[2] * self.v[1],
            self.u[2] * self.v[0] - self.u[0] * self.v[2],
            self.u[0] * self.v[1] - self.u[1] * self.v[0],
        ];
        if [self.u, self.v, self.normal]
            .iter()
            .any(|&v| (dot(v, v) - 1.).abs() > 1e-7)
            || dot(self.u, self.v).abs() > 1e-7
            || dot(cross, self.normal) < 1. - 1e-7
        {
            return Err("Reference plane must have an orthonormal, right-handed frame".into());
        }
        Ok(())
    }
    pub fn named(name: &str, offset: f64) -> Self {
        let (u, v, n) = match name {
            "Front" => ([1., 0., 0.], [0., 0., 1.], [0., -1., 0.]),
            "Right" => ([0., 1., 0.], [0., 0., 1.], [1., 0., 0.]),
            _ => ([1., 0., 0.], [0., 1., 0.], [0., 0., 1.]),
        };
        Self {
            origin: n.map(|x| x * offset),
            u,
            v,
            normal: n,
        }
    }
    pub fn at(&self, p: [f64; 2]) -> [f64; 3] {
        std::array::from_fn(|i| self.origin[i] + self.u[i] * p[0] + self.v[i] * p[1])
    }
    pub fn shifted(&self, d: f64) -> Self {
        let mut p = self.clone();
        for i in 0..3 {
            p.origin[i] += p.normal[i] * d;
        }
        p
    }
}

pub fn fmt(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

pub fn demo() -> Document {
    use crate::plugins::Registry;
    let registry = Registry::builtin().expect("built-in plugins compile");
    let mut d = Document {
        name: "Mounting plate".into(),
        ..Default::default()
    };
    d.variables.insert("width".into(), "80".into());
    d.variables.insert("height".into(), "50".into());
    d.variables.insert("thickness".into(), "8".into());
    let mut sketch = Sketch::default();
    sketch.add_rect([-40., -25.], [40., 25.]);
    sketch.entities[0].values = vec![
        "-width/2".into(),
        "-height/2".into(),
        "width".into(),
        "height".into(),
    ];
    let mut s = registry.create("sketch", &mut d).unwrap();
    s.name = "Plate outline".into();
    s.set_sketch(&sketch);
    d.features.push(s);
    let mut e = registry.create("extrude", &mut d).unwrap();
    e.params["sketch"] = json!("f1");
    e.params["depth"] = json!("thickness");
    d.features.push(e);
    let mut holes = Sketch::default();
    for (i, (x, y)) in [(-28., -13.), (28., -13.), (28., 13.), (-28., 13.)]
        .into_iter()
        .enumerate()
    {
        holes.add_circle([x, y], 3.);
        holes.entities[i].values[0] = if x < 0. { "-width/2+12" } else { "width/2-12" }.into();
        holes.entities[i].values[1] = if y < 0. {
            "-height/2+12"
        } else {
            "height/2-12"
        }
        .into();
    }
    let mut s = registry.create("sketch", &mut d).unwrap();
    s.name = "Mounting holes".into();
    s.params["support"] = json!("f2:end");
    s.set_sketch(&holes);
    d.features.push(s);
    let mut cut = registry.create("extrude", &mut d).unwrap();
    cut.name = "Through holes".into();
    cut.params["sketch"] = json!("f3");
    cut.params["mode"] = json!("Remove");
    cut.params["depth"] = json!("thickness");
    cut.params["reverse"] = json!(true);
    cut.params["target"] = json!("f2");
    cut.params["targets"] = json!(["f2:0"]);
    d.features.push(cut);
    d
}
