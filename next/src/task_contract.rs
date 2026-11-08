//! Read-only Onshape task presentation. No document edits, JSON or GUI dependency.
//! Native schema/defaults/expressions and successful previews remain authoritative.
//! A counterpart is NOT a claim of complete native support. Missing bindings are
//! disabled specification rows, never editable fake properties. Do not fall back
//! to native property names or whole-document object lists in normal tasks.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { PartialCounterpart, MissingNativeMapping, DeveloperOnly }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Sketch, SketchRegion, PlanarFace, Face, Edge, LinearEdge, SketchLine,
    Curve, LinearCurve, Arc, Circle, CylindricalEdge, CylindricalFace, ConicalFace, Plane, Vertex,
    SketchPoint, CircleCenter, MateConnector, Part, Surface, Feature,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceKind {
    pub allowed: &'static [EntityKind],
    /// Counts geometric selections, not containing native object links.
    pub min: usize,
    pub max: Option<usize>,
    pub ordered: bool,
}
impl ReferenceKind {
    pub fn accepts(self, kind: EntityKind) -> bool { self.allowed.contains(&kind) }
    pub fn valid_count(self, count: usize) -> bool {
        count >= self.min && self.max.is_none_or(|max| count <= max)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// Presentation only: still intersect with writable native task schema.
    Native(&'static str),
    /// Existing special task payload, not a native property assignment.
    Task(&'static str),
    /// Requires an explicit adapter or missing native capability. Disabled.
    Missing(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When { Always, NotFull, Blind, Second, SecondBlind, CustomDirection, FaceEnd, PartEnd }
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub binding: Binding,
    pub reference: Option<ReferenceKind>,
    pub when: When,
}
#[derive(Clone, Copy, Debug)]
pub struct Feature {
    pub native_type: &'static str,
    pub command: &'static str,
    pub label: Option<&'static str>,
    pub status: Status,
    pub source: Option<&'static str>,
    pub limitation: &'static str,
}

pub static FEATURES: &[Feature] = &[
    Feature { native_type: "Part::Mirroring", command: "feature.part_mirror", label: Some("Mirror"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/mirror.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Loft", command: "feature.new_loft", label: Some("Loft"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/loft.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Helix", command: "feature.helix", label: Some("Helix"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/helix.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Pad", command: "feature.pad", label: Some("Extrude"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/extrude.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Revolution", command: "feature.revolution", label: Some("Revolve"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/revolve.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::AdditiveLoft", command: "feature.additive_loft", label: Some("Loft"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/loft.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::AdditivePipe", command: "feature.additive_pipe", label: Some("Sweep"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/sweep.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Pocket", command: "feature.pocket", label: Some("Extrude"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/extrude.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Groove", command: "feature.groove", label: Some("Revolve"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/revolve.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::SubtractiveLoft", command: "feature.subtractive_loft", label: Some("Loft"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/loft.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::SubtractivePipe", command: "feature.subtractive_pipe", label: Some("Sweep"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/sweep.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Hole", command: "feature.hole", label: Some("Hole"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/hole.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Fillet", command: "feature.fillet", label: Some("Fillet"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/fillet.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Chamfer", command: "feature.chamfer", label: Some("Chamfer"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/chamfer.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Draft", command: "feature.draft", label: Some("Draft"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/draft.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Thickness", command: "feature.thickness", label: Some("Shell"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/shell.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::LinearPattern", command: "feature.linear_pattern", label: Some("Linear pattern"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/linear_pattern.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::PolarPattern", command: "feature.polar_pattern", label: Some("Circular pattern"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/circular_pattern.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Mirrored", command: "feature.mirrored", label: Some("Mirror"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/mirror.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::MultiTransform", command: "feature.multi_transform", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::Scaled", command: "feature.scaled", label: None, status: Status::MissingNativeMapping, source: None, limitation: "Transform scale exists in Onshape; native scaled pattern is not that task." },
    Feature { native_type: "PartDesign::Plane", command: "datum.plane", label: Some("Plane"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/plane.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::Line", command: "datum.line", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::Point", command: "datum.point", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "App::Part", command: "document.part", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "Part::Box", command: "part.box", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "Part::Chamfer", command: "part.chamfer", label: Some("Chamfer"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/chamfer.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Common", command: "part.common", label: Some("Boolean"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/boolean.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Cone", command: "part.cone", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "Part::Cut", command: "part.cut", label: Some("Boolean"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/boolean.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Cylinder", command: "part.cylinder", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "Part::Extrusion", command: "part.extrusion", label: Some("Extrude"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/extrude.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Fillet", command: "part.fillet", label: Some("Fillet"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/fillet.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Fuse", command: "part.fuse", label: Some("Boolean"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/boolean.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::MultiFuse", command: "part.multi_fuse", label: Some("Boolean"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/boolean.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Revolution", command: "part.revolution", label: Some("Revolve"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/revolve.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "Part::Sphere", command: "part.sphere", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "Part::Torus", command: "part.torus", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveBox", command: "feature.additive_box", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveCone", command: "feature.additive_cone", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveCylinder", command: "feature.additive_cylinder", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveEllipsoid", command: "feature.additive_ellipsoid", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveHelix", command: "feature.additive_helix", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditivePrism", command: "feature.additive_prism", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveSphere", command: "feature.additive_sphere", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveTorus", command: "feature.additive_torus", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::AdditiveWedge", command: "feature.additive_wedge", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::Boolean", command: "body.boolean", label: Some("Boolean"), status: Status::PartialCounterpart, source: Some("https://cad.onshape.com/help/Content/PartStudio/boolean.htm"), limitation: "Subset only; see field bindings and integration handoff." },
    Feature { native_type: "PartDesign::CoordinateSystem", command: "datum.coordinate_system", label: None, status: Status::MissingNativeMapping, source: None, limitation: "Coordinate system is not a Mate connector; ownership and alignment adapter missing." },
    Feature { native_type: "PartDesign::ShapeBinder", command: "reference.shape_binder", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubShapeBinder", command: "reference.sub_shape_binder", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveBox", command: "feature.subtractive_box", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveCone", command: "feature.subtractive_cone", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveCylinder", command: "feature.subtractive_cylinder", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveEllipsoid", command: "feature.subtractive_ellipsoid", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveHelix", command: "feature.subtractive_helix", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractivePrism", command: "feature.subtractive_prism", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveSphere", command: "feature.subtractive_sphere", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveTorus", command: "feature.subtractive_torus", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
    Feature { native_type: "PartDesign::SubtractiveWedge", command: "feature.subtractive_wedge", label: None, status: Status::DeveloperOnly, source: None, limitation: "No direct built-in Onshape task counterpart. Keep in developer tools only." },
];

pub fn feature(native_type: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.native_type == native_type || f.command == native_type)
}
pub fn feature_label(native_type: &str) -> Option<&'static str> { feature(native_type)?.label }

use EntityKind::*;
const PROFILE: ReferenceKind = ReferenceKind { allowed: &[Sketch, SketchRegion, PlanarFace], min: 1, max: None, ordered: false };
const AXIS: ReferenceKind = ReferenceKind { allowed: &[LinearEdge, SketchLine, LinearCurve, Arc, Circle, CylindricalEdge, CylindricalFace, MateConnector], min: 1, max: Some(1), ordered: false };
const DIRECTION: ReferenceKind = ReferenceKind { allowed: &[LinearEdge, SketchLine, PlanarFace, MateConnector], min: 1, max: Some(1), ordered: false };
const EXTRUDE_DIRECTION: ReferenceKind = ReferenceKind { allowed: &[LinearEdge, SketchLine, LinearCurve, MateConnector], min: 1, max: Some(1), ordered: false };
const PATTERN_AXIS: ReferenceKind = ReferenceKind { allowed: &[LinearEdge, SketchLine, LinearCurve, Arc, Circle, CylindricalFace, ConicalFace, MateConnector], min: 1, max: Some(1), ordered: false };
const PLANE: ReferenceKind = ReferenceKind { allowed: &[Plane, PlanarFace, MateConnector], min: 1, max: Some(1), ordered: false };
const PARTS: ReferenceKind = ReferenceKind { allowed: &[Part, Surface], min: 1, max: None, ordered: false };
const PART: ReferenceKind = ReferenceKind { allowed: &[Part, Surface], min: 1, max: Some(1), ordered: false };
const FACES: ReferenceKind = ReferenceKind { allowed: &[Face, PlanarFace, CylindricalFace, ConicalFace], min: 1, max: None, ordered: false };
const EDGES_FACES: ReferenceKind = ReferenceKind { allowed: &[Edge, LinearEdge, Arc, Circle, Face, PlanarFace, CylindricalFace, ConicalFace], min: 1, max: None, ordered: false };
const PROFILES: ReferenceKind = ReferenceKind { allowed: &[Sketch, SketchRegion, Face, PlanarFace, Surface, Edge, Curve, Vertex, SketchPoint], min: 2, max: None, ordered: true };
const PATH: ReferenceKind = ReferenceKind { allowed: &[Sketch, SketchLine, Edge, LinearEdge, Arc, Circle, Curve], min: 1, max: None, ordered: true };
const FEATURES_REF: ReferenceKind = ReferenceKind { allowed: &[Feature], min: 1, max: None, ordered: false };
const POINTS: ReferenceKind = ReferenceKind { allowed: &[Sketch, SketchPoint, Vertex, CircleCenter, MateConnector], min: 1, max: None, ordered: false };
macro_rules! n {
    ($key:literal,$label:literal,$reference:expr,$when:ident) => {
        Field { key:$key,label:$label,binding:Binding::Native($key),reference:$reference,when:When::$when }
    };
}
macro_rules! t {
    ($key:literal,$label:literal,$path:literal,$reference:expr) => {
        Field { key:$key,label:$label,binding:Binding::Task($path),reference:$reference,when:When::Always }
    };
}
macro_rules! missing {
    ($key:literal,$label:literal,$reason:literal,$reference:expr) => {
        Field { key:$key,label:$label,binding:Binding::Missing($reason),reference:$reference,when:When::Always }
    };
}
const EXTRUDE: &[Field] = &[
    n!("Profile","Faces and sketch regions to extrude",Some(PROFILE),Always),
    n!("Type","End type",None,Always), n!("Length","Depth",None,Blind),
    n!("UpToFace","Face to extrude up to",Some(PLANE),FaceEnd),
    n!("UpToShape","Part to extrude up to",Some(PART),PartEnd),
    n!("Reversed","Opposite direction",None,Always),
    n!("UseCustomVector","Direction",None,Always),
    n!("ReferenceAxis","Direction",Some(EXTRUDE_DIRECTION),CustomDirection),
    missing!("$starting_offset","Starting offset","Native termination Offset is not Starting offset",None),
    missing!("$symmetric","Symmetric","Adapt SideType; do not expose its native dropdown",None),
    n!("TaperAngle","Draft",None,Always),
    missing!("$second","Second end position","Adapt SideType without losing inactive values",None),
    n!("Type2","End type",None,Second),n!("Length2","Depth",None,SecondBlind),
    n!("UpToFace2","Face to extrude up to",Some(PLANE),Second),
    n!("UpToShape2","Part to extrude up to",Some(PART),Second),
    n!("TaperAngle2","Draft",None,Second),
];
const REVOLVE: &[Field] = &[
    n!("Profile","Faces and sketch regions to revolve",Some(PROFILE),Always),
    n!("ReferenceAxis","Revolve axis",Some(AXIS),Always),
    missing!("$full","Full revolve","Default checked; adapter must preserve native Angle/Type",None),
    n!("Type","End type",None,NotFull),n!("Angle","Revolve angle",None,Blind),
    n!("Reversed","Opposite direction",None,NotFull),
    n!("Midplane","Symmetric",None,Blind),
    missing!("$second","Second End Position","Adapter required for native Angle2/Type",None),
    n!("Angle2","Revolve angle",None,SecondBlind),
    missing!("$offset","Offset","Native angle termination offset adapter unavailable",None),
    missing!("$face_end","Face to revolve up to","Current native catalog has no verified face-end binding",Some(PLANE)),
    missing!("$part_end","Part to revolve up to","Current native catalog has no verified part-end binding",Some(PART)),
    missing!("$merge","Merge scope","No verified arbitrary merge-scope adapter",Some(PARTS)),
];
const LOFT: &[Field] = &[
    missing!("$profiles","Profiles","Combine Profile and Sections as one ordered list",Some(PROFILES)),
    missing!("$start","Start profile condition","Native constraints require explicit enum adapter",None),
    missing!("$end","End profile condition","Native constraints require explicit enum adapter",None),
    missing!("$guides","Guides and continuity","No catalog-native guide binding",Some(PATH)),
];
const NEW_LOFT: &[Field] = &[
    t!("Sections","Profiles","loft_sections",Some(PROFILES)),
];
const SWEEP: &[Field] = &[
    n!("Profile","Faces and sketch regions to sweep",Some(PROFILE),Always),
    n!("Spine","Sweep path",Some(PATH),Always),
    missing!("$control","Profile control","Transformation is not a label-equivalent enum",None),
    missing!("$twist","Twist","No catalog-native twist adapter",None),
    missing!("$scale","Scale","No catalog-native scale adapter",None),
];
const HOLE: &[Field] = &[
    n!("HoleCutType","Hole style",None,Always),
    n!("Profile","Sketch points to place holes",Some(POINTS),Always),
    missing!("$merge","Merge scope","Body tip only; no arbitrary merge scope",Some(PARTS)),
    n!("Diameter","Diameter",None,Always),
    missing!("$start","Start plane","No equivalent native start-plane selector",Some(PLANE)),
    n!("DepthType","Termination",None,Always),n!("Depth","Depth",None,Blind),
    // Hole style-specific dimensions are selected by the existing Hole task
    // schema. HoleCutDiameter has different labels for each style; do not use a
    // single unconditional label here.
    n!("Reversed","Opposite direction",None,Always),
];
const FILLET: &[Field] = &[n!("Base","Entities to fillet",Some(EDGES_FACES),Always), n!("Radius","Radius",None,Always)];
const CHAMFER: &[Field] = &[
    n!("Base","Entities to chamfer",Some(EDGES_FACES),Always),
    missing!("$measurement","Measurement","Offset/Tangent need native semantic verification",None),
    n!("ChamferType","Chamfer type",None,Always), n!("Size","Distance",None,Always),
    n!("Size2","Distance",None,Always),n!("Angle","Angle",None,Always),
    n!("FlipDirection","Opposite direction",None,Always),
];
const PART_FILLET: &[Field] = &[missing!("Edges","Entities to fillet","Native Edges tuples combine indices and radii; dedicated serializer required",Some(EDGES_FACES))];
const PART_CHAMFER: &[Field] = &[missing!("Edges","Entities to chamfer","Native Edges tuples combine indices and distances; dedicated serializer required",Some(EDGES_FACES))];
const DRAFT: &[Field] = &[
    n!("NeutralPlane","Neutral plane",Some(PLANE),Always),n!("Base","Entities to draft",Some(FACES),Always),
    n!("Angle","Draft angle",None,Always),n!("Reversed","Opposite direction",None,Always),
];
const SHELL: &[Field] = &[n!("Base","Faces to shell",Some(FACES),Always),n!("Value","Shell thickness",None,Always),n!("Reversed","Opposite direction",None,Always),missing!("$hollow","Hollow","Zero-face shell must be verified in native adapter",Some(PARTS))];
const LINEAR: &[Field] = &[
    n!("Originals","Features to pattern",Some(FEATURES_REF),Always),n!("Direction","Direction",Some(DIRECTION),Always),
    missing!("$distance","Distance","Native Length is total extent; Onshape Distance is instance spacing",None),
    n!("Occurrences","Instance count",None,Always),n!("Reversed","Opposite direction",None,Always),
];
const CIRCULAR: &[Field] = &[
    n!("Originals","Features to pattern",Some(FEATURES_REF),Always),n!("Axis","Axis of pattern",Some(PATTERN_AXIS),Always),
    n!("Angle","Angle",None,Always),n!("Occurrences","Instance count",None,Always),
    missing!("$equal","Equal spacing","Native angle distribution requires explicit adapter",None),
    n!("Reversed","Opposite direction",None,Always),
];
const MIRROR: &[Field] = &[n!("Originals","Features to mirror",Some(FEATURES_REF),Always),n!("MirrorPlane","Mirror plane",Some(PLANE),Always)];
const PART_MIRROR: &[Field] = &[t!("Source","Entities to mirror","part_mirror.source",Some(PARTS)),t!("MirrorPlane","Mirror plane","part_mirror.plane",Some(PLANE))];
const BOOLEAN: &[Field] = &[n!("Base","Targets",Some(PARTS),Always),n!("Tool","Tools",Some(PARTS),Always)];
const UNION: &[Field] = &[missing!("$tools","Tools","Combine Base and Tool without changing native links",Some(PARTS))];
const MULTI_UNION: &[Field] = &[n!("Shapes","Tools",Some(PARTS),Always)];
const BODY_BOOLEAN: &[Field] = &[n!("Type","Operation type",None,Always),n!("Group","Tools",Some(PARTS),Always)];
const PLANE_FIELDS: &[Field] = &[
    t!("AttachmentSupport","Entities","offset_plane.support",Some(PLANE)),
    t!("AttachmentOffset","Offset distance","offset_plane.magnitude",None),
    t!("MapReversed","Opposite direction","offset_plane.flipped",None),
    missing!("$type","Plane type","Only offset-plane adapter is currently described here",None),
];
const HELIX: &[Field] = &[
    t!("$reference","Conical or cylindrical face","helix_reference",Some(ReferenceKind {allowed:&[CylindricalFace,ConicalFace],min:1,max:Some(1),ordered:false})),
    n!("StartAngle","Start angle",None,Always),n!("Turns","Target revolutions",None,Always),
    missing!("LocalCoord","Clockwise","Local handedness is not verified as Onshape viewing direction",None),
];
const PART_EXTRUDE: &[Field] = &[
    n!("Base","Faces and sketch regions to extrude",Some(PROFILE),Always),
    n!("LengthFwd","Depth",None,Always),
    n!("Symmetric","Symmetric",None,Always),
    missing!("DirLink","Direction","DirMode and DirLink require coordinated adapter",Some(DIRECTION)),
];
const PART_REVOLVE: &[Field] = &[
    n!("Source","Faces and sketch regions to revolve",Some(PROFILE),Always),
    n!("AxisLink","Revolve axis",Some(AXIS),Always),
    missing!("$full","Full revolve","Default checked; adapt native Angle without overwriting it on render",None),
    n!("Angle","Revolve angle",None,Blind),n!("Symmetric","Symmetric",None,Blind),
];
/// Ordered task rows; Missing rows describe gaps and MUST NOT be editable.
pub fn fields(type_id: &str) -> &'static [Field] {
    match feature(type_id).map(|f| f.native_type).unwrap_or(type_id) {
        "PartDesign::Pad" | "PartDesign::Pocket" => EXTRUDE,
        "PartDesign::Revolution" | "PartDesign::Groove" => REVOLVE,
        "PartDesign::AdditiveLoft" | "PartDesign::SubtractiveLoft" => LOFT,
        "Part::Loft" => NEW_LOFT,
        "PartDesign::AdditivePipe" | "PartDesign::SubtractivePipe" => SWEEP,
        "PartDesign::Hole" => HOLE,
        "PartDesign::Fillet" => FILLET, "Part::Fillet" => PART_FILLET,
        "PartDesign::Chamfer" => CHAMFER, "Part::Chamfer" => PART_CHAMFER,
        "PartDesign::Draft" => DRAFT, "PartDesign::Thickness" => SHELL,
        "PartDesign::LinearPattern" => LINEAR, "PartDesign::PolarPattern" => CIRCULAR,
        "PartDesign::Mirrored" => MIRROR, "Part::Mirroring" => PART_MIRROR,
        "Part::Cut" => BOOLEAN, "Part::Fuse" | "Part::Common" => UNION,
        "Part::MultiFuse" => MULTI_UNION, "PartDesign::Boolean" => BODY_BOOLEAN,
        "PartDesign::Plane" => PLANE_FIELDS, "Part::Helix" => HELIX,
        "Part::Extrusion" => PART_EXTRUDE, "Part::Revolution" => PART_REVOLVE,
        _ => &[],
    }
}
pub fn field(type_id: &str, property: &str) -> Option<&'static Field> {
    fields(type_id).iter().find(|f| f.key == property)
}
pub fn field_label(type_id: &str, property: &str) -> Option<&'static str> { Some(field(type_id,property)?.label) }
pub fn reference_kind(type_id: &str, property: &str) -> Option<ReferenceKind> { field(type_id,property)?.reference }
/// No fallback to all native properties. Includes task payload fields, excludes gaps.
pub fn preferred_fields(type_id: &str) -> impl Iterator<Item=&'static Field> {
    fields(type_id).iter().filter(|f| !matches!(f.binding,Binding::Missing(_)))
}
#[derive(Clone, Copy, Debug)]
pub struct Visibility<'a> {
    pub full_revolve: bool,
    pub end_type: &'a str,
    pub second: bool,
    pub second_end_type: &'a str,
    pub custom_direction: bool,
}
impl Default for Visibility<'_> {
    fn default() -> Self { Self {full_revolve:true,end_type:"Blind",second:false,second_end_type:"Blind",custom_direction:false} }
}
pub fn visible_fields<'a>(type_id: &str, v: &'a Visibility<'a>) -> impl Iterator<Item=&'static Field> + 'a {
    let revolve = feature_label(type_id)==Some("Revolve");
    // Collect the static slice first: returned iterator does not borrow type_id.
    let rows = fields(type_id);
    rows.iter().filter(move |f| !matches!(f.binding,Binding::Missing(_)) && match f.when {
        When::Always => true, When::NotFull => !v.full_revolve,
        When::Blind => (!revolve || !v.full_revolve) && v.end_type=="Blind",
        When::Second => v.second && (!revolve || !v.full_revolve),
        When::SecondBlind => v.second && v.second_end_type=="Blind" && (!revolve || !v.full_revolve),
        When::CustomDirection => v.custom_direction,
        When::FaceEnd => v.end_type=="Up to face", When::PartEnd => v.end_type=="Up to part",
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumOption { pub display: &'static str, pub native: Option<&'static str> }
const fn option(display: &'static str, native: Option<&'static str>) -> EnumOption { EnumOption {display,native} }
const EXTRUDE_ENDS: &[EnumOption] = &[
    option("Blind",Some("Length")),option("Up to next",Some("UpToFirst")),
    option("Up to face",Some("UpToFace")),option("Up to part",Some("UpToShape")),
    option("Up to vertex",None),option("Through all",Some("ThroughAll")),
];
const REVOLVE_ENDS: &[EnumOption] = &[
    option("Blind",Some("Angle")),option("Up to next",None),option("Up to face",None),
    option("Up to part",None),option("Up to vertex",None),
];
/// native=None means documented Onshape option with no verified mapping, disabled.
/// Also intersect native=Some with the live native schema enumeration.
const HOLE_END_OPTIONS: &[EnumOption] = &[option("Blind",Some("Dimension")),option("Up to next",None),option("Up to entity",None),option("Through all",Some("ThroughAll"))];
const HOLE_STYLE_OPTIONS: &[EnumOption] = &[option("Simple",Some("None")),option("Counterbore",Some("Counterbore")),option("Countersink",Some("Countersink"))];
const CHAMFER_TYPE_OPTIONS: &[EnumOption] = &[option("Equal distance",None),option("Two distances",Some("Two distances")),option("Distance and angle",Some("Distance and Angle"))];
pub fn enum_options(type_id: &str, property: &str) -> &'static [EnumOption] {
    match (feature_label(type_id),property) {
        (Some("Extrude"),"Type" | "Type2") if field(type_id,property).is_some() => EXTRUDE_ENDS,
        (Some("Revolve"),"Type") if field(type_id,property).is_some() => REVOLVE_ENDS,
        (Some("Hole"),"DepthType") => HOLE_END_OPTIONS,
        (Some("Hole"),"HoleCutType") => HOLE_STYLE_OPTIONS,
        (Some("Chamfer"),"ChamferType") => CHAMFER_TYPE_OPTIONS,
        // Native ChamferType and sweep Transformation require verified schema adapters.
        _ => &[],
    }
}
pub fn enum_display(type_id: &str, property: &str, native: &str) -> Option<&'static str> {
    enum_options(type_id,property).iter().find(|o| o.native==Some(native)).map(|o| o.display)
}
pub fn enum_value(type_id: &str, property: &str, display: &str) -> Option<&'static str> {
    enum_options(type_id,property).iter().find(|o| o.display==display)?.native
}
/// Picks must already carry resolved native identities. This is deliberately
/// separate from EntityKind: an internal Origin container is never selectable,
/// but a public plane rendered from it can be, with a stable resolved identity.
#[derive(Clone, Copy, Debug)]
pub struct Pick {
    pub kind: EntityKind,
    pub public_geometry: bool,
    pub identity_current: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickError { NoReferenceField, Cardinality, InternalEntity, StaleIdentity, GeometryKind, NativeUnsupported }
/// Validate the entire collection before replacing a draft field. No truncation,
/// mutation or native serialization. Root's callback enforces actual native
/// support (including body ownership, single-object links and exact geometry).
pub fn validate_picks(
    type_id: &str, property: &str, picks: &[Pick],
    native_accepts: impl Fn(&[Pick]) -> bool,
) -> Result<(), PickError> {
    let kind=reference_kind(type_id,property).ok_or(PickError::NoReferenceField)?;
    if !kind.valid_count(picks.len()) { return Err(PickError::Cardinality); }
    for pick in picks {
        if !pick.public_geometry { return Err(PickError::InternalEntity); }
        if !pick.identity_current { return Err(PickError::StaleIdentity); }
        if !kind.accepts(pick.kind) { return Err(PickError::GeometryKind); }
    }
    if !native_accepts(picks) { return Err(PickError::NativeUnsupported); }
    Ok(())
}
/// Fixed native operation, for initial UI selection only. It is not a model edit.
pub fn fixed_operation(type_id: &str) -> Option<&'static str> {
    match feature(type_id)?.native_type {
        "PartDesign::Pocket" | "PartDesign::Groove" | "PartDesign::SubtractiveLoft" | "PartDesign::SubtractivePipe" => Some("Remove"),
        "PartDesign::AdditiveLoft" | "PartDesign::AdditivePipe" => Some("Add"),
        "Part::Fuse" | "Part::MultiFuse" => Some("Union"),
        "Part::Cut" => Some("Subtract"), "Part::Common" => Some("Intersect"),
        // Pad/Revolution can create the first solid or add to the existing tip.
        _ => None,
    }
}
/// Source-of-truth operation names, not bindings or enabled choices.
pub const SOLID_OPERATIONS: &[&str] = &["New","Add","Remove","Intersect"];
pub const CREATION_TYPES: &[&str] = &["Solid","Surface","Thin"];
pub const BOOLEAN_OPERATIONS: &[&str] = &["Union","Subtract","Intersect"];
pub const SWEEP_CONTROLS: &[&str] = &["None","Keep profile orientation","Lock profile faces","Lock profile direction"];
pub const CHAMFER_TYPES: &[&str] = &["Equal distance","Two distances","Distance and angle"];
pub const LOFT_CONDITIONS: &[&str] = &["Normal to profile","Tangent to profile","Match tangent","Match curvature","Normal direction","Tangent direction"];
pub const PATTERN_TYPES: &[&str] = &["Part pattern","Face pattern","Feature pattern"];
pub const HELIX_TYPES: &[&str] = &["Cylinder/Cone","Axis","Circle"];
pub const HELIX_INPUT_TYPES: &[&str] = &["Turns","Pitch","Turns and pitch"];
pub const HELIX_TURN_DIRECTIONS: &[&str] = &["Clockwise","Counterclockwise"];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_classifies_every_pinned_native_type_without_aliasing_developer_tools() {
        assert_eq!(FEATURES.len(),60);
        let mut types=std::collections::BTreeSet::new();
        for f in FEATURES {
            assert!(types.insert(f.native_type));
            assert_eq!(feature(f.command).unwrap().native_type,f.native_type);
            assert_eq!(f.source.is_some(),f.status==Status::PartialCounterpart);
            if f.status!=Status::PartialCounterpart { assert!(fields(f.native_type).is_empty()); }
        }
        for t in ["Part::Box","PartDesign::AdditiveHelix","PartDesign::Line","App::Part","PartDesign::ShapeBinder"] {
            assert_eq!(feature(t).unwrap().status,Status::DeveloperOnly);
            assert_eq!(feature_label(t),None);
        }
        assert_eq!(feature("PartDesign::CoordinateSystem").unwrap().status,Status::MissingNativeMapping);
        assert_eq!(feature_label("unknown"),None);
    }
    #[test]
    fn reference_collection_rejects_bad_picks_atomically_and_requires_native_admission() {
        let valid=Pick{kind:Circle,public_geometry:true,identity_current:true};
        assert_eq!(validate_picks("PartDesign::Revolution","ReferenceAxis",&[valid],|_|true),Ok(()));
        assert_eq!(validate_picks("PartDesign::Revolution","ReferenceAxis",&[valid,valid],|_|true),Err(PickError::Cardinality));
        assert_eq!(validate_picks("PartDesign::Revolution","ReferenceAxis",&[Pick{public_geometry:false,..valid}],|_|true),Err(PickError::InternalEntity));
        assert_eq!(validate_picks("PartDesign::Revolution","ReferenceAxis",&[Pick{identity_current:false,..valid}],|_|true),Err(PickError::StaleIdentity));
        assert_eq!(validate_picks("PartDesign::Revolution","Profile",&[valid],|_|true),Err(PickError::GeometryKind));
        assert_eq!(validate_picks("PartDesign::Revolution","ReferenceAxis",&[valid],|_|false),Err(PickError::NativeUnsupported));
        assert_eq!(validate_picks("PartDesign::Revolution","Body",&[valid],|_|true),Err(PickError::NoReferenceField));
        let axis=reference_kind("PartDesign::Groove","ReferenceAxis").unwrap();
        for kind in [LinearEdge,SketchLine,LinearCurve,Arc,Circle,CylindricalEdge,CylindricalFace,MateConnector] { assert!(axis.accepts(kind)); }
        assert!(!axis.accepts(Curve));
    }
    #[test]
    fn full_revolve_hides_angular_controls_without_changing_native_state() {
        let v=Visibility::default();
        let keys=visible_fields("PartDesign::Revolution",&v).map(|f|f.key).collect::<Vec<_>>();
        assert_eq!(keys,["Profile","ReferenceAxis"]);
        let v=Visibility{full_revolve:false,second:true,..v};
        let keys=visible_fields("PartDesign::Revolution",&v).map(|f|f.key).collect::<Vec<_>>();
        assert!(keys.contains(&"Angle") && keys.contains(&"Angle2"));
        assert_eq!(field_label("PartDesign::Revolution","ReferenceAxis"),Some("Revolve axis"));
    }
    #[test]
    fn ordered_profiles_and_singular_axis_have_geometric_cardinality() {
        let profiles=reference_kind("Part::Loft","Sections").unwrap();
        assert!(profiles.ordered && profiles.valid_count(2) && profiles.valid_count(12));
        assert!(!profiles.valid_count(1));
        let mirror=reference_kind("Part::Mirroring","MirrorPlane").unwrap();
        assert!(mirror.accepts(PlanarFace) && mirror.accepts(MateConnector));
        assert!(!mirror.accepts(Part));
    }
    #[test]
    fn unknown_enums_and_missing_options_never_become_native_writes() {
        assert_eq!(enum_display("PartDesign::Hole","HoleCutType","None"),Some("Simple"));
        assert_eq!(enum_value("PartDesign::Pad","Type","Blind"),Some("Length"));
        assert_eq!(enum_value("PartDesign::Pad","Type","Up to vertex"),None);
        assert_eq!(enum_display("PartDesign::Pad","Type","?TwoLengths"),None);
        assert_eq!(enum_value("PartDesign::AdditivePipe","Transformation","Keep profile orientation"),None);
        assert_eq!(enum_display("PartDesign::Hole","HoleCutType","DIN 7984"),None);
    }
    #[test]
    fn normal_fields_do_not_leak_implementation_properties_or_claim_spacing_equivalence() {
        for f in FEATURES {
            for row in preferred_fields(f.native_type) {
                assert!(!["Body","Origin","Placement","MapMode","Solid","Ruled","Closed","UseAllEdges","Refine","rawAxis"].contains(&row.key));
                assert!(!row.label.contains("ReferenceAxis") && !row.label.contains("AttachmentSupport"));
            }
        }
        assert!(!preferred_fields("PartDesign::LinearPattern").any(|f|f.key=="Length"));
        assert!(matches!(field("PartDesign::LinearPattern","$distance").unwrap().binding,Binding::Missing(_)));
        assert_eq!(fields("Part::Revolution")[0].key,"Source");
        assert_eq!(fields("PartDesign::Revolution")[0].key,"Profile");
    }
}
