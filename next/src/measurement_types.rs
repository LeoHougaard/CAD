//! Values measured by the native kernel. Coordinates are in document world space.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SketchPick {
    pub index: i32,
    pub point: i32,
}

/// Native names/paths are scoped to a document revision. Never replay a detached
/// EdgeN/FaceN against a changed revision or turn a render mesh into geometry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub object: String,
    #[serde(default)]
    pub subelement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch_geometry: Option<SketchPick>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Metric {
    pub key: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub display: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Point {
    pub value: [f64; 3],
    pub display: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Property {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Selection {
    pub reference: Reference,
    pub shape_type: String,
    pub geometry_type: Option<String>,
    pub metrics: Vec<Metric>,
    pub center: Point,
    pub geometry_center: Option<Point>,
    pub axis: Option<[f64; 3]>,
    pub direction: Option<[f64; 3]>,
    pub properties: Vec<Property>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Distance {
    pub metric: Metric,
    pub witnesses: Vec<[Point; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Measurement {
    pub document: String,
    pub revision: u64,
    pub unit_system: String,
    pub selections: Vec<Selection>,
    pub distance: Option<Distance>,
    pub angle: Option<Metric>,
    pub angle_unavailable: Option<String>,
}
