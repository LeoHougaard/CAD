//! Feature discovery and contextual native defaults. Property types, enums and
//! computed defaults come from the worker's pinned native schema. This catalog
//! does not implement geometry or replace native feature validation.

use crate::body_commands::body_of;
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FeatureDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub type_id: &'static str,
    pub group: &'static str,
    pub preferred_properties: &'static [&'static str],
}

macro_rules! feature {
    ($id:literal,$label:literal,$type_id:literal,$group:literal,[$($property:literal),*]) => {
        FeatureDefinition { id:$id,label:$label,type_id:$type_id,group:$group,preferred_properties:&[$($property),*] }
    };
}

static DEFINITIONS: &[FeatureDefinition] = &[
    feature!(
        "feature.pad",
        "Pad / Extrude",
        "PartDesign::Pad",
        "Additive",
        [
            "Profile",
            "SideType",
            "Type",
            "Length",
            "Type2",
            "Length2",
            "Offset",
            "Offset2",
            "UpToFace",
            "UpToFace2",
            "UpToShape",
            "UpToShape2",
            "TaperAngle",
            "TaperAngle2",
            "UseCustomVector",
            "ReferenceAxis",
            "Direction",
            "AlongSketchNormal",
            "Reversed"
        ]
    ),
    feature!(
        "feature.revolution",
        "Revolve",
        "PartDesign::Revolution",
        "Additive",
        [
            "Profile",
            "ReferenceAxis",
            "Type",
            "Angle",
            "Angle2",
            "Midplane",
            "Reversed"
        ]
    ),
    feature!(
        "feature.additive_loft",
        "Additive loft",
        "PartDesign::AdditiveLoft",
        "Additive",
        ["Profile", "Sections", "Ruled", "Closed"]
    ),
    feature!(
        "feature.additive_pipe",
        "Additive sweep",
        "PartDesign::AdditivePipe",
        "Additive",
        [
            "Profile",
            "Spine",
            "Transition",
            "Transformation",
            "AuxiliarySpine"
        ]
    ),
    feature!(
        "feature.pocket",
        "Pocket",
        "PartDesign::Pocket",
        "Subtractive",
        [
            "Profile",
            "SideType",
            "Type",
            "Length",
            "Type2",
            "Length2",
            "Offset",
            "Offset2",
            "UpToFace",
            "UpToFace2",
            "UpToShape",
            "UpToShape2",
            "TaperAngle",
            "TaperAngle2",
            "UseCustomVector",
            "ReferenceAxis",
            "Direction",
            "AlongSketchNormal",
            "Reversed"
        ]
    ),
    feature!(
        "feature.groove",
        "Groove / Revolved cut",
        "PartDesign::Groove",
        "Subtractive",
        [
            "Profile",
            "ReferenceAxis",
            "Type",
            "Angle",
            "Angle2",
            "Midplane",
            "Reversed"
        ]
    ),
    feature!(
        "feature.subtractive_loft",
        "Subtractive loft",
        "PartDesign::SubtractiveLoft",
        "Subtractive",
        ["Profile", "Sections", "Ruled", "Closed"]
    ),
    feature!(
        "feature.subtractive_pipe",
        "Subtractive sweep",
        "PartDesign::SubtractivePipe",
        "Subtractive",
        [
            "Profile",
            "Spine",
            "Transition",
            "Transformation",
            "AuxiliarySpine"
        ]
    ),
    feature!(
        "feature.hole",
        "Hole",
        "PartDesign::Hole",
        "Subtractive",
        [
            "Profile",
            "Diameter",
            "DepthType",
            "Depth",
            "Threaded",
            "HoleCutType"
        ]
    ),
    feature!(
        "feature.fillet",
        "Fillet",
        "PartDesign::Fillet",
        "Dress-up",
        ["Base", "Radius", "UseAllEdges"]
    ),
    feature!(
        "feature.chamfer",
        "Chamfer",
        "PartDesign::Chamfer",
        "Dress-up",
        [
            "Base",
            "ChamferType",
            "Size",
            "Size2",
            "Angle",
            "FlipDirection",
            "UseAllEdges"
        ]
    ),
    feature!(
        "feature.draft",
        "Draft",
        "PartDesign::Draft",
        "Dress-up",
        ["Base", "Angle", "NeutralPlane", "PullDirection", "Reversed"]
    ),
    feature!(
        "feature.thickness",
        "Shell / Thickness",
        "PartDesign::Thickness",
        "Dress-up",
        ["Base", "Value", "Mode", "Join", "Intersection", "Reversed"]
    ),
    feature!(
        "feature.linear_pattern",
        "Linear pattern",
        "PartDesign::LinearPattern",
        "Transform",
        [
            "Originals",
            "Direction",
            "Length",
            "Occurrences",
            "Reversed"
        ]
    ),
    feature!(
        "feature.polar_pattern",
        "Circular pattern",
        "PartDesign::PolarPattern",
        "Transform",
        ["Originals", "Axis", "Angle", "Occurrences", "Reversed"]
    ),
    feature!(
        "feature.mirrored",
        "Mirror",
        "PartDesign::Mirrored",
        "Transform",
        ["Originals", "MirrorPlane"]
    ),
    feature!(
        "feature.multi_transform",
        "Multiple transforms",
        "PartDesign::MultiTransform",
        "Transform",
        ["Originals", "Transformations"]
    ),
    feature!(
        "feature.scaled",
        "Scaled pattern",
        "PartDesign::Scaled",
        "Transform",
        ["Originals", "Factor", "Occurrences"]
    ),
    feature!(
        "datum.plane",
        "Datum plane",
        "PartDesign::Plane",
        "Reference",
        ["AttachmentSupport", "MapMode", "AttachmentOffset"]
    ),
    feature!(
        "datum.line",
        "Datum line",
        "PartDesign::Line",
        "Reference",
        ["AttachmentSupport", "MapMode", "AttachmentOffset"]
    ),
    feature!(
        "datum.point",
        "Datum point",
        "PartDesign::Point",
        "Reference",
        ["AttachmentSupport", "MapMode", "AttachmentOffset"]
    ),
    feature!(
        "document.part",
        "Part container",
        "App::Part",
        "Document",
        ["Group", "Placement"]
    ),
    feature!(
        "part.box",
        "Box",
        "Part::Box",
        "Part primitives",
        [
            "Height",
            "Length",
            "Width",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "part.chamfer",
        "Chamfer",
        "Part::Chamfer",
        "Part operations",
        ["Base", "Edges"]
    ),
    feature!(
        "part.common",
        "Intersect",
        "Part::Common",
        "Part operations",
        ["Base", "Tool", "Refine"]
    ),
    feature!(
        "part.cone",
        "Cone",
        "Part::Cone",
        "Part primitives",
        [
            "Height",
            "Angle",
            "Radius1",
            "Radius2",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "part.cut",
        "Subtract",
        "Part::Cut",
        "Part operations",
        ["Base", "Tool", "Refine"]
    ),
    feature!(
        "part.cylinder",
        "Cylinder",
        "Part::Cylinder",
        "Part primitives",
        [
            "Height",
            "Angle",
            "Radius",
            "FirstAngle",
            "SecondAngle",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "part.extrusion",
        "Extrusion",
        "Part::Extrusion",
        "Part operations",
        [
            "Base",
            "DirMode",
            "Dir",
            "DirLink",
            "LengthFwd",
            "LengthRev",
            "Solid",
            "Symmetric",
            "TaperAngle",
            "TaperAngleRev"
        ]
    ),
    feature!(
        "part.fillet",
        "Fillet",
        "Part::Fillet",
        "Part operations",
        ["Base", "Edges"]
    ),
    feature!(
        "part.fuse",
        "Union of two objects",
        "Part::Fuse",
        "Part operations",
        ["Base", "Tool", "Refine"]
    ),
    feature!(
        "part.multi_fuse",
        "Union of multiple objects",
        "Part::MultiFuse",
        "Part operations",
        ["Shapes", "Refine"]
    ),
    feature!(
        "part.revolution",
        "Revolution",
        "Part::Revolution",
        "Part operations",
        [
            "Source",
            "Axis",
            "AxisLink",
            "Base",
            "Angle",
            "Solid",
            "Symmetric"
        ]
    ),
    feature!(
        "part.sphere",
        "Sphere",
        "Part::Sphere",
        "Part primitives",
        [
            "Radius",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "part.torus",
        "Torus",
        "Part::Torus",
        "Part primitives",
        [
            "Radius1",
            "Radius2",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_box",
        "Additive Box",
        "PartDesign::AdditiveBox",
        "Additive primitives",
        [
            "Height",
            "Length",
            "Width",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_cone",
        "Additive Cone",
        "PartDesign::AdditiveCone",
        "Additive primitives",
        [
            "Height",
            "Angle",
            "Radius1",
            "Radius2",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_cylinder",
        "Additive Cylinder",
        "PartDesign::AdditiveCylinder",
        "Additive primitives",
        [
            "Height",
            "Angle",
            "Radius",
            "FirstAngle",
            "SecondAngle",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_ellipsoid",
        "Additive Ellipsoid",
        "PartDesign::AdditiveEllipsoid",
        "Additive primitives",
        [
            "Radius1",
            "Radius2",
            "Radius3",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_helix",
        "Additive Helix",
        "PartDesign::AdditiveHelix",
        "Additive",
        [
            "Profile",
            "ReferenceAxis",
            "Mode",
            "Pitch",
            "Height",
            "Angle",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_prism",
        "Additive Prism",
        "PartDesign::AdditivePrism",
        "Additive primitives",
        [
            "Height",
            "Polygon",
            "Circumradius",
            "FirstAngle",
            "SecondAngle",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_sphere",
        "Additive Sphere",
        "PartDesign::AdditiveSphere",
        "Additive primitives",
        [
            "Radius",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_torus",
        "Additive Torus",
        "PartDesign::AdditiveTorus",
        "Additive primitives",
        [
            "Radius1",
            "Radius2",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.additive_wedge",
        "Additive Wedge",
        "PartDesign::AdditiveWedge",
        "Additive primitives",
        [
            "Xmin",
            "Xmax",
            "X2min",
            "X2max",
            "Ymin",
            "Ymax",
            "Zmin",
            "Zmax",
            "Z2min",
            "Z2max",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "body.boolean",
        "Body Boolean",
        "PartDesign::Boolean",
        "Body operations",
        ["Group", "Type", "Refine"]
    ),
    feature!(
        "datum.coordinate_system",
        "Datum coordinate system",
        "PartDesign::CoordinateSystem",
        "Reference",
        [
            "AttachmentSupport",
            "MapMode",
            "AttachmentOffset",
            "Placement"
        ]
    ),
    feature!(
        "reference.shape_binder",
        "Shape binder",
        "PartDesign::ShapeBinder",
        "Reference",
        ["Support", "TraceSupport"]
    ),
    feature!(
        "reference.sub_shape_binder",
        "Subshape binder",
        "PartDesign::SubShapeBinder",
        "Reference",
        [
            "Support",
            "BindMode",
            "Relative",
            "BindCopyOnChange",
            "Offset",
            "MakeFace",
            "Fuse",
            "Refine"
        ]
    ),
    feature!(
        "feature.subtractive_box",
        "Subtractive Box",
        "PartDesign::SubtractiveBox",
        "Subtractive primitives",
        [
            "Height",
            "Length",
            "Width",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_cone",
        "Subtractive Cone",
        "PartDesign::SubtractiveCone",
        "Subtractive primitives",
        [
            "Height",
            "Angle",
            "Radius1",
            "Radius2",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_cylinder",
        "Subtractive Cylinder",
        "PartDesign::SubtractiveCylinder",
        "Subtractive primitives",
        [
            "Height",
            "Angle",
            "Radius",
            "FirstAngle",
            "SecondAngle",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_ellipsoid",
        "Subtractive Ellipsoid",
        "PartDesign::SubtractiveEllipsoid",
        "Subtractive primitives",
        [
            "Radius1",
            "Radius2",
            "Radius3",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_helix",
        "Subtractive Helix",
        "PartDesign::SubtractiveHelix",
        "Subtractive",
        [
            "Profile",
            "ReferenceAxis",
            "Mode",
            "Pitch",
            "Height",
            "Angle",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_prism",
        "Subtractive Prism",
        "PartDesign::SubtractivePrism",
        "Subtractive primitives",
        [
            "Height",
            "Polygon",
            "Circumradius",
            "FirstAngle",
            "SecondAngle",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_sphere",
        "Subtractive Sphere",
        "PartDesign::SubtractiveSphere",
        "Subtractive primitives",
        [
            "Radius",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_torus",
        "Subtractive Torus",
        "PartDesign::SubtractiveTorus",
        "Subtractive primitives",
        [
            "Radius1",
            "Radius2",
            "Angle1",
            "Angle2",
            "Angle3",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
    feature!(
        "feature.subtractive_wedge",
        "Subtractive Wedge",
        "PartDesign::SubtractiveWedge",
        "Subtractive primitives",
        [
            "Xmin",
            "Xmax",
            "X2min",
            "X2max",
            "Ymin",
            "Ymax",
            "Zmin",
            "Zmax",
            "Z2min",
            "Z2max",
            "AttachmentSupport",
            "MapMode",
            "Placement"
        ]
    ),
];

pub fn definitions() -> &'static [FeatureDefinition] {
    DEFINITIONS
}

pub fn get(id_or_type: &str) -> Option<&'static FeatureDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.id == id_or_type || definition.type_id == id_or_type)
}

pub struct FeatureContext<'a> {
    pub body: Option<&'a str>,
    pub name: &'a str,
    pub selected_object: Option<&'a str>,
    pub selections: &'a [(String, String)],
}

fn objects(state: &Value) -> &[Value] {
    state["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn object<'a>(state: &'a Value, name: &str) -> Option<&'a Value> {
    objects(state).iter().find(|object| object["name"] == name)
}

fn identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn selected_objects<'a>(
    state: &'a Value,
    context: &FeatureContext<'_>,
) -> Result<Vec<&'a Value>, String> {
    let mut selected = Vec::new();
    for name in context
        .selected_object
        .into_iter()
        .chain(context.selections.iter().map(|(name, _)| name.as_str()))
    {
        let object = object(state, name)
            .ok_or_else(|| format!("Selected native object no longer exists: {name}"))?;
        if !selected
            .iter()
            .any(|existing: &&Value| existing["name"] == object["name"])
        {
            selected.push(object);
        }
    }
    Ok(selected)
}

fn link(name: &str) -> Value {
    json!({"link":name})
}

fn sublink(name: &str, elements: &[String]) -> Value {
    json!({"link":name,"subelements":elements})
}

fn origin_reference(state: &Value, body: Option<&str>, role: &str) -> Option<Value> {
    let body = object(state, body?)?;
    let origin = object(state, body["properties"]["Origin"]["link"].as_str()?)?;
    origin["properties"]["OriginFeatures"]
        .as_array()?
        .iter()
        .find_map(|reference| {
            let name = reference["link"].as_str()?;
            let candidate = object(state, name)?;
            (candidate["properties"]["Role"] == role).then(|| sublink(name, &[String::new()]))
        })
}

fn one_selected_source(
    context: &FeatureContext<'_>,
    prefix: &str,
) -> Result<Option<Value>, String> {
    let mut source = None::<String>;
    let mut elements = Vec::<String>::new();
    for (object, element) in context.selections {
        if !element.starts_with(prefix) {
            continue;
        }
        if !element.strip_prefix(prefix).is_some_and(|index| {
            !index.is_empty()
                && index.bytes().all(|character| character.is_ascii_digit())
                && index.parse::<u64>().is_ok_and(|index| index > 0)
        }) {
            return Err(format!("Choose a native {prefix} subelement"));
        }
        if source.as_ref().is_some_and(|source| source != object) {
            return Err("Select subelements from one native feature".into());
        }
        source = Some(object.clone());
        if !elements.contains(element) {
            elements.push(element.clone());
        }
    }
    Ok(source.map(|source| sublink(&source, &elements)))
}

/// Suggest only explicitly selected links and useful initial scalar values.
/// Missing required references stay unset for the native property form. Native
/// schema/semantic checks remain authoritative when the command is submitted.
pub fn defaults(
    definition: &FeatureDefinition,
    state: &Value,
    context: &FeatureContext<'_>,
) -> Result<Map<String, Value>, String> {
    if !identifier(context.name) {
        return Err("Choose an ASCII native feature name".into());
    }
    if let Some(body) = context.body
        && !object(state, body).is_some_and(|object| object["type"] == "PartDesign::Body")
    {
        return Err("Choose an existing native Body".into());
    }
    let selected = selected_objects(state, context)?;
    let same_body = |object: &Value| {
        context.body.is_none_or(|body| {
            body_of(state, object["name"].as_str().unwrap_or_default()).as_deref() == Some(body)
        })
    };
    let profiles: Vec<_> = selected
        .iter()
        .copied()
        .filter(|object| object["type"] == "Sketcher::SketchObject" && same_body(object))
        .collect();
    let profile = profiles.first().and_then(|object| object["name"].as_str());
    let mut properties = Map::new();
    match definition.type_id {
        "PartDesign::Pad"
        | "PartDesign::Pocket"
        | "PartDesign::Revolution"
        | "PartDesign::Groove"
        | "PartDesign::AdditiveLoft"
        | "PartDesign::SubtractiveLoft"
        | "PartDesign::AdditivePipe"
        | "PartDesign::SubtractivePipe"
        | "PartDesign::Hole"
        | "PartDesign::AdditiveHelix"
        | "PartDesign::SubtractiveHelix" => {
            if let Some(profile) = profile {
                properties.insert("Profile".into(), link(profile));
            }
        }
        _ => (),
    }
    match definition.type_id {
        "PartDesign::Revolution"
        | "PartDesign::Groove"
        | "PartDesign::AdditiveHelix"
        | "PartDesign::SubtractiveHelix" => {
            if matches!(
                definition.type_id,
                "PartDesign::Revolution" | "PartDesign::Groove"
            ) {
                properties.insert("Angle".into(), json!(360.));
            }
            if let Some(profile) = profile {
                properties.insert("ReferenceAxis".into(), sublink(profile, &["V_Axis".into()]));
            }
        }
        "PartDesign::AdditiveLoft" | "PartDesign::SubtractiveLoft" => {
            let sections: Vec<_> = profiles
                .iter()
                .skip(1)
                .filter_map(|object| object["name"].as_str())
                .map(|name| sublink(name, &[String::new()]))
                .collect();
            if !sections.is_empty() {
                properties.insert("Sections".into(), json!(sections));
            }
        }
        "PartDesign::AdditivePipe" | "PartDesign::SubtractivePipe" => {
            if let Some(spine) = one_selected_source(context, "Edge")? {
                properties.insert("Spine".into(), spine);
            }
        }
        "PartDesign::Fillet" | "PartDesign::Chamfer" => {
            if let Some(base) = one_selected_source(context, "Edge")? {
                properties.insert("Base".into(), base);
            }
            properties.insert(
                if definition.type_id.ends_with("Fillet") {
                    "Radius"
                } else {
                    "Size"
                }
                .into(),
                json!(2.),
            );
        }
        "PartDesign::Draft" | "PartDesign::Thickness" => {
            if let Some(base) = one_selected_source(context, "Face")? {
                properties.insert("Base".into(), base);
            }
        }
        "PartDesign::LinearPattern"
        | "PartDesign::PolarPattern"
        | "PartDesign::Mirrored"
        | "PartDesign::MultiTransform"
        | "PartDesign::Scaled" => {
            let originals: Vec<_> = selected
                .iter()
                .copied()
                .filter(|object| {
                    same_body(object)
                        && object["type"].as_str().is_some_and(|kind| {
                            kind.starts_with("PartDesign::") && kind != "PartDesign::Body"
                        })
                        && object["shape"]["solids"].as_u64().unwrap_or(0) > 0
                })
                .filter_map(|object| object["name"].as_str())
                .map(link)
                .collect();
            if !originals.is_empty() {
                properties.insert("Originals".into(), json!(originals));
            }
            let (property, role) = match definition.type_id {
                "PartDesign::LinearPattern" => ("Direction", "X_Axis"),
                "PartDesign::PolarPattern" => ("Axis", "Z_Axis"),
                "PartDesign::Mirrored" => ("MirrorPlane", "YZ_Plane"),
                _ => ("", ""),
            };
            if !property.is_empty()
                && let Some(reference) = origin_reference(state, context.body, role)
            {
                properties.insert(property.into(), reference);
            }
        }
        "Part::Cut" | "Part::Fuse" | "Part::Common" | "Part::MultiFuse" => {
            let mut explicit = Vec::new();
            for (name, _) in context.selections {
                let Some(candidate) = object(state, name)
                    .filter(|object| object["shape"]["solids"].as_u64().unwrap_or(0) > 0)
                else {
                    continue;
                };
                if !explicit
                    .iter()
                    .any(|existing: &&Value| existing["name"] == candidate["name"])
                {
                    explicit.push(candidate);
                }
            }
            let sources: Vec<_> = if explicit.len() >= 2 {
                &explicit
            } else {
                &selected
            }
            .iter()
            .filter(|object| object["shape"]["solids"].as_u64().unwrap_or(0) > 0)
            .filter_map(|object| object["name"].as_str())
            .collect();
            if definition.type_id == "Part::MultiFuse" {
                if !sources.is_empty() {
                    properties.insert(
                        "Shapes".into(),
                        json!(sources.into_iter().map(link).collect::<Vec<_>>()),
                    );
                }
            } else {
                if let Some(source) = sources.first() {
                    properties.insert("Base".into(), link(source));
                }
                if let Some(source) = sources.get(1) {
                    properties.insert("Tool".into(), link(source));
                }
            }
        }
        "Part::Extrusion" | "Part::Revolution" => {
            if let Some(source) = selected
                .iter()
                .find(|object| {
                    object.get("shape").is_some() || object["type"] == "Sketcher::SketchObject"
                })
                .and_then(|object| object["name"].as_str())
            {
                properties.insert(
                    if definition.type_id == "Part::Extrusion" {
                        "Base"
                    } else {
                        "Source"
                    }
                    .into(),
                    link(source),
                );
            }
            properties.insert("Solid".into(), json!(true));
            if definition.type_id == "Part::Extrusion" {
                properties.insert("LengthFwd".into(), json!(10.));
            }
        }
        "Part::Fillet" | "Part::Chamfer" => {
            if let Some(source) = one_selected_source(context, "Edge")? {
                let edges: Vec<_> = source["subelements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|value| value.as_str()?.strip_prefix("Edge")?.parse::<u64>().ok())
                    .map(|index| json!([index, 2., 2.]))
                    .collect();
                properties.insert("Base".into(), link(source["link"].as_str().unwrap()));
                properties.insert("Edges".into(), json!(edges));
            }
        }
        "PartDesign::ShapeBinder"
        | "PartDesign::SubShapeBinder"
        | "PartDesign::Plane"
        | "PartDesign::Line"
        | "PartDesign::Point"
        | "PartDesign::CoordinateSystem" => {
            let mut references = context.selections.to_vec();
            if references.is_empty()
                && let Some(source) = context.selected_object
            {
                references.push((source.into(), String::new()));
            }
            if !references.is_empty() {
                let property = if definition.type_id.ends_with("Binder") {
                    "Support"
                } else {
                    "AttachmentSupport"
                };
                properties.insert(
                    property.into(),
                    crate::property_commands::selected_links(&references, true),
                );
                if definition.type_id == "PartDesign::Plane"
                    && references.len() == 1
                    && references[0].1.starts_with("Face")
                {
                    properties.insert("MapMode".into(), json!("FlatFace"));
                }
            }
        }
        "PartDesign::Boolean" => {
            let mut tools = Vec::<Value>::new();
            for object in selected {
                if let Some(body) = body_of(state, object["name"].as_str().unwrap_or_default())
                    && context.body != Some(body.as_str())
                    && !tools.iter().any(|tool| tool["link"] == body)
                {
                    tools.push(link(&body));
                }
            }
            if !tools.is_empty() {
                properties.insert("Group".into(), json!(tools));
            }
        }
        "App::Part" if !selected.is_empty() => {
            properties.insert(
                "Group".into(),
                json!(
                    selected
                        .iter()
                        .filter_map(|object| object["name"].as_str())
                        .map(link)
                        .collect::<Vec<_>>()
                ),
            );
        }
        _ => (),
    }
    Ok(properties)
}

pub fn seed(
    state: &Value,
    type_id: &str,
    body: Option<&str>,
    name: &str,
    selected_object: Option<&str>,
    selections: &[(String, String)],
) -> Result<Map<String, Value>, String> {
    let definition = get(type_id).ok_or_else(|| format!("Unknown catalog feature: {type_id}"))?;
    defaults(
        definition,
        state,
        &FeatureContext {
            body,
            name,
            selected_object,
            selections,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body"},
            {"name":"Sketch","type":"Sketcher::SketchObject","body":"Body"},
            {"name":"Section","type":"Sketcher::SketchObject","body":"Body"},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body","shape":{"solids":1}},
            {"name":"OtherBody","type":"PartDesign::Body","body":"OtherBody"},
            {"name":"OtherPad","type":"PartDesign::Pad","body":"OtherBody","shape":{"solids":1}}
        ]})
    }

    #[test]
    fn catalog_keys_are_unique_and_native_type_lookup_agrees() {
        let mut ids = std::collections::BTreeSet::new();
        let mut types = std::collections::BTreeSet::new();
        for definition in definitions() {
            assert!(ids.insert(definition.id));
            assert!(types.insert(definition.type_id));
            assert_eq!(get(definition.id).unwrap().type_id, definition.type_id);
            assert_eq!(get(definition.type_id).unwrap().id, definition.id);
        }
    }

    #[test]
    fn selected_profiles_seed_links_and_revolution_axis_without_copying_history() {
        let state = state();
        let pad = seed(
            &state,
            "PartDesign::Pad",
            Some("Body"),
            "NewPad",
            Some("Sketch"),
            &[],
        )
        .unwrap();
        assert_eq!(pad["Profile"], link("Sketch"));
        assert!(!pad.contains_key("Length")); // Native Pad/Pocket defaults are authoritative.
        let revolve = seed(
            &state,
            "PartDesign::Revolution",
            Some("Body"),
            "Revolve",
            Some("Sketch"),
            &[],
        )
        .unwrap();
        assert_eq!(
            revolve["ReferenceAxis"],
            json!({"link":"Sketch","subelements":["V_Axis"]})
        );
        let loft = seed(
            &state,
            "PartDesign::AdditiveLoft",
            Some("Body"),
            "Loft",
            Some("Sketch"),
            &[("Section".into(), String::new())],
        )
        .unwrap();
        assert_eq!(
            loft["Sections"],
            json!([{"link":"Section","subelements":[""]}])
        );
    }

    #[test]
    fn subelement_seeds_are_grouped_and_cross_object_bases_rejected() {
        let state = state();
        let fillet = seed(
            &state,
            "PartDesign::Fillet",
            Some("Body"),
            "Fillet",
            None,
            &[
                ("Pad".into(), "Edge1".into()),
                ("Pad".into(), "Edge2".into()),
            ],
        )
        .unwrap();
        assert_eq!(
            fillet["Base"],
            json!({"link":"Pad","subelements":["Edge1","Edge2"]})
        );
        assert!(
            seed(
                &state,
                "PartDesign::Fillet",
                Some("Body"),
                "Fillet",
                None,
                &[
                    ("Pad".into(), "Edge1".into()),
                    ("OtherPad".into(), "Edge1".into())
                ]
            )
            .is_err()
        );
        assert!(
            seed(
                &state,
                "PartDesign::Pad",
                Some("Missing"),
                "Pad2",
                None,
                &[]
            )
            .is_err()
        );
        assert!(
            seed(
                &state,
                "PartDesign::Pad",
                Some("Body"),
                "Pad2",
                Some("Missing"),
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn patterns_seed_only_native_features_in_the_target_body() {
        let properties = seed(
            &state(),
            "PartDesign::LinearPattern",
            Some("Body"),
            "Pattern",
            Some("Pad"),
            &[("OtherPad".into(), "Face1".into())],
        )
        .unwrap();
        assert_eq!(properties["Originals"], json!([{"link":"Pad"}]));
        let empty = seed(&state(), "PartDesign::Pad", Some("Body"), "Pad2", None, &[]).unwrap();
        assert!(!empty.contains_key("Profile"));
    }

    #[test]
    fn native_origin_roles_resolve_axes_without_assuming_global_object_names() {
        let mut state = state();
        state["objects"][0]["properties"] = json!({"Origin":{"link":"Origin002"}});
        let objects = state["objects"].as_array_mut().unwrap();
        objects.push(
            json!({"name":"Origin002","properties":{"OriginFeatures":[{"link":"X_Axis009"}]}}),
        );
        objects.push(json!({"name":"X_Axis009","properties":{"Role":"X_Axis"}}));
        let properties = seed(
            &state,
            "PartDesign::LinearPattern",
            Some("Body"),
            "Pattern",
            Some("Pad"),
            &[],
        )
        .unwrap();
        assert_eq!(
            properties["Direction"],
            json!({"link":"X_Axis009","subelements":[""]})
        );
    }

    #[test]
    fn explicit_part_boolean_pair_overrides_stale_tree_focus() {
        let mut state = state();
        state["objects"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"Third","type":"Part::Box","shape":{"solids":1}}));
        let properties = seed(
            &state,
            "Part::Cut",
            None,
            "Cut",
            Some("Pad"),
            &[
                ("OtherPad".into(), "Face1".into()),
                ("Third".into(), "Face2".into()),
            ],
        )
        .unwrap();
        assert_eq!(properties["Base"], link("OtherPad"));
        assert_eq!(properties["Tool"], link("Third"));
    }
}
