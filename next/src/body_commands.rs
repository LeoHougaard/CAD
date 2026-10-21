//! Native Body queries and Boolean edits. These functions do not own history,
//! infer a second feature graph or mutate a document snapshot.

use serde_json::{Value, json};

fn objects(state: &Value) -> &[Value] {
    state["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The worker resolves ownership from the native document, including Body self
/// ownership. Detached display faces must never substitute for these links.
pub fn body_of(state: &Value, object: &str) -> Option<String> {
    objects(state)
        .iter()
        .find(|entry| entry["name"] == object)?["body"]
        .as_str()
        .map(str::to_owned)
}

fn displayable_part_shape(shape: &Value) -> bool {
    if shape["valid"] == false || shape["null"] == true {
        return false;
    }
    // Legacy solid snapshots can omit bounds. Native snapshots that carry
    // bounds must fit the current viewport's finite coordinate range.
    let finite_bounds = |bounds: &Value| {
        let Some(minimum) = bounds["min"].as_array().filter(|values| values.len() == 3) else {
            return false;
        };
        let Some(maximum) = bounds["max"].as_array().filter(|values| values.len() == 3) else {
            return false;
        };
        minimum.iter().zip(maximum).all(|(minimum, maximum)| {
            minimum
                .as_f64()
                .zip(maximum.as_f64())
                .is_some_and(|(minimum, maximum)| {
                    // The current viewport uses f32 vector lengths and squared
                    // distances. Leave headroom for those display calculations.
                    let display_limit = (f64::from(f32::MAX)).sqrt() / 16.;
                    minimum.is_finite()
                        && maximum.is_finite()
                        && minimum <= maximum
                        && minimum.abs() <= display_limit
                        && maximum.abs() <= display_limit
                })
        })
    };
    if shape["solids"].as_u64().unwrap_or(0) > 0 {
        return shape.get("bounds").is_none_or(finite_bounds);
    }
    shape["valid"] == true
        && shape["null"] == false
        && ["faces", "edges", "vertices"]
            .iter()
            .any(|kind| shape[kind].as_u64().unwrap_or(0) > 0)
        && finite_bounds(&shape["bounds"])
}

/// Display the native tip of each unconsumed visible Body. An empty result when
/// Bodies exist is intentional: historical features must not reappear as parts.
pub fn display_objects(state: &Value) -> Vec<String> {
    let all = objects(state);
    let bodies: Vec<_> = all
        .iter()
        .filter(|entry| entry["type"] == "PartDesign::Body")
        .collect();
    let mut result = Vec::new();
    for body in bodies {
        if let Some(object) = body["body_state"]["display_object"].as_str()
            && !result.iter().any(|existing| existing == object)
        {
            result.push(object.to_owned());
        }
    }
    // Standalone Part objects can coexist with Bodies. Their native visibility
    // decides display; Body history is never used as a standalone fallback.
    for entry in all.iter().filter(|entry| {
        entry["type"]
            .as_str()
            .is_some_and(|kind| kind.starts_with("Part::"))
            && displayable_part_shape(&entry["shape"])
            && entry["properties"]["Visibility"] != false
            && entry["effective_visibility"] != false
            && entry["body"].as_str().is_none()
    }) {
        if let Some(name) = entry["name"].as_str() {
            result.push(name.to_owned());
        }
    }
    for entry in all.iter().filter(|entry| {
        matches!(
            entry["type"].as_str(),
            Some("PartDesign::ShapeBinder" | "PartDesign::SubShapeBinder")
        ) && displayable_part_shape(&entry["shape"])
            && entry["properties"]["Visibility"] != false
            && entry["effective_visibility"] != false
            && entry["body"].as_str().is_none_or(|body| {
                all.iter()
                    .find(|entry| entry["name"] == body)
                    .is_none_or(|body| {
                        body["properties"]["Visibility"] != false
                            && body["effective_visibility"] != false
                    })
            })
    }) {
        if let Some(name) = entry["name"].as_str()
            && !result.iter().any(|existing| existing == name)
        {
            result.push(name.to_owned());
        }
    }
    for entry in all.iter().filter(|entry| {
        entry["type"] == "Sketcher::SketchObject"
            && entry["shape"]["null"] == false
            && entry["properties"]["Visibility"] != false
            && entry["effective_visibility"] != false
            && entry["body"].as_str().is_none_or(|body| {
                all.iter()
                    .find(|o| o["name"] == body)
                    .is_none_or(|o| o["properties"]["Visibility"] != false)
            })
    }) {
        if let Some(name) = entry["name"].as_str() {
            result.push(name.to_owned());
        }
    }
    result
}

fn identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Construct one atomic native feature. Tool Bodies remain native linked
/// documents objects. FreeCAD owns their consumption and subsequent recompute.
pub fn boolean_edits(
    name: &str,
    target: &str,
    tools: &[String],
    operation: &str,
) -> Result<Vec<Value>, String> {
    if !identifier(name) || !identifier(target) || tools.iter().any(|tool| !identifier(tool)) {
        return Err("Boolean names and Body references must be native ASCII identifiers".into());
    }
    if !["Fuse", "Cut", "Common"].contains(&operation) {
        return Err("Choose Union, Subtract or Intersect".into());
    }
    if tools.is_empty() {
        return Err("Select at least one tool Body".into());
    }
    if name == target || tools.iter().any(|tool| tool == name) {
        return Err("The Boolean feature needs its own name".into());
    }
    for (index, tool) in tools.iter().enumerate() {
        if tool == target {
            return Err("The target Body cannot also be a tool Body".into());
        }
        if tools[..index].contains(tool) {
            return Err("Select each tool Body once".into());
        }
    }
    Ok(vec![
        json!({"op":"add_object","type":"PartDesign::Boolean","name":name,"body":target}),
        json!({"op":"set","object":name,"property":"Group","value":tools.iter().map(|tool|json!({"link":tool})).collect::<Vec<_>>()}),
        json!({"op":"set","object":name,"property":"Type","value":operation}),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_uses_native_tip_consumption_and_ownership() {
        let state = json!({"objects":[
            {"name":"Target","type":"PartDesign::Body","body":"Target","body_state":{"tip":"Union","consumed_by":[],"display_object":"Union"}},
            {"name":"OldPad","type":"PartDesign::Pad","body":"Target","shape":{"solids":1}},
            {"name":"Union","type":"PartDesign::Boolean","body":"Target","shape":{"solids":1}},
            {"name":"Tool","type":"PartDesign::Body","body":"Tool","body_state":{"tip":"ToolPad","consumed_by":["Union"],"display_object":null}},
            {"name":"ToolPad","type":"PartDesign::Pad","body":"Tool","shape":{"solids":1}},
            {"name":"Separate","type":"PartDesign::Body","body":"Separate","body_state":{"tip":"OtherPad","display_object":"OtherPad"}}
        ]});
        assert_eq!(display_objects(&state), ["Union", "OtherPad"]);
        assert_eq!(body_of(&state, "OldPad").as_deref(), Some("Target"));
        assert_eq!(body_of(&state, "Tool").as_deref(), Some("Tool"));
        assert_eq!(body_of(&state, "ToolPad").as_deref(), Some("Tool"));
        assert_eq!(body_of(&state, "Missing"), None);
    }

    #[test]
    fn empty_bodies_do_not_resurrect_historical_solids() {
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body_state":{"display_object":null}},
            {"name":"OldPad","type":"PartDesign::Pad","shape":{"solids":1}}
        ]});
        assert!(display_objects(&state).is_empty());
        assert_eq!(
            display_objects(
                &json!({"objects":[{"name":"Box","type":"Part::Box","shape":{"solids":1}}]})
            ),
            ["Box"]
        );
    }

    #[test]
    fn standalone_parts_coexist_with_body_tips_and_respect_native_visibility() {
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body_state":{"display_object":"Pad"}},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body","shape":{"solids":1}},
            {"name":"Box","type":"Part::Box","shape":{"solids":1},"properties":{"Visibility":true}},
            {"name":"Cylinder","type":"Part::Cylinder","shape":{"solids":1}},
            {"name":"Hidden","type":"Part::Sphere","shape":{"solids":1},"properties":{"Visibility":false}},
            {"name":"Historical","type":"PartDesign::Pad","shape":{"solids":1}}
        ]});
        assert_eq!(display_objects(&state), ["Pad", "Box", "Cylinder"]);
    }

    #[test]
    fn imported_surfaces_wires_and_vertices_use_finite_native_shapes_and_ancestor_visibility() {
        let surface = json!({"null":false,"valid":true,"solids":0,"faces":1,"edges":4,"vertices":4,
            "bounds":{"min":[0,0,0],"max":[10,8,0]}});
        let wire = json!({"null":false,"valid":true,"solids":0,"faces":0,"edges":2,"vertices":3,
            "bounds":{"min":[-2,1,0],"max":[4,3,0]}});
        let vertex = json!({"null":false,"valid":true,"solids":0,"faces":0,"edges":0,"vertices":1,
            "bounds":{"min":[2,3,4],"max":[2,3,4]}});
        let mut state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","body_state":{"display_object":null}},
            {"name":"HiddenContainer","type":"App::Part","properties":{"Visibility":false}},
            {"name":"Surface","type":"Part::Feature","body":null,"shape":surface,"effective_visibility":true},
            {"name":"Wire","type":"Part::Feature","body":null,"shape":wire,"effective_visibility":true},
            {"name":"Vertex","type":"Part::Feature","body":null,"shape":vertex,"effective_visibility":true},
            {"name":"HiddenChild","type":"Part::Feature","body":null,"shape":surface,
                "properties":{"Visibility":true},"effective_visibility":false},
            {"name":"OwnedFeature","type":"Part::Feature","body":"Body","shape":surface},
            {"name":"OldPad","type":"PartDesign::Pad","shape":surface}
        ]});
        assert_eq!(display_objects(&state), ["Surface", "Wire", "Vertex"]);
        state["objects"][2]["properties"]["Visibility"] = json!(false);
        state["objects"][3]["shape"]["bounds"]["max"][0] = json!(-3);
        state["objects"][4]["shape"]["valid"] = json!(false);
        assert!(display_objects(&state).is_empty());
        state["objects"][3]["shape"]["bounds"] = Value::Null;
        assert!(display_objects(&state).is_empty());
        state["objects"][3]["shape"]["bounds"] = json!({"min":[0,0,0],"max":[1e100,3,0]});
        assert!(display_objects(&state).is_empty());
    }

    #[test]
    fn separately_visible_native_binders_are_pickable_without_resurrecting_body_history() {
        let face = json!({"null":false,"valid":true,"solids":0,"faces":1,"edges":4,"vertices":4,
            "bounds":{"min":[0,0,3],"max":[10,8,3]}});
        let wire = json!({"null":false,"valid":true,"solids":0,"faces":0,"edges":2,"vertices":3,
            "bounds":{"min":[0,0,0],"max":[5,7,0]}});
        let state = json!({"objects":[
            {"name":"Body","type":"PartDesign::Body","properties":{"Visibility":true},
                "body_state":{"display_object":"Pad"}},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body","shape":{"solids":1}},
            {"name":"Binder","type":"PartDesign::ShapeBinder","body":"Body","shape":face,
                "properties":{"Visibility":true},"effective_visibility":true},
            {"name":"SubBinder","type":"PartDesign::SubShapeBinder","body":"Body","shape":wire,
                "properties":{"Visibility":true},"effective_visibility":true},
            {"name":"OldPad","type":"PartDesign::Pad","body":"Body","shape":face,
                "properties":{"Visibility":true},"effective_visibility":true},
            {"name":"HiddenBody","type":"PartDesign::Body","properties":{"Visibility":false},
                "body_state":{"display_object":null}},
            {"name":"HiddenAncestorBinder","type":"PartDesign::ShapeBinder","body":"HiddenBody",
                "shape":face,"properties":{"Visibility":true}},
            {"name":"HiddenBinder","type":"PartDesign::SubShapeBinder","body":"Body","shape":wire,
                "properties":{"Visibility":false}},
            {"name":"HiddenPartBinder","type":"PartDesign::ShapeBinder","body":"Body","shape":face,
                "properties":{"Visibility":true},"effective_visibility":false},
            {"name":"DuplicateTip","type":"PartDesign::Body","body_state":{"display_object":"Binder"}}
        ]});
        assert_eq!(display_objects(&state), ["Pad", "Binder", "SubBinder"]);
    }

    #[test]
    fn boolean_builds_native_body_links_and_rejects_self_or_duplicate_tools() {
        let edits =
            boolean_edits("Union", "Target", &["ToolA".into(), "ToolB".into()], "Fuse").unwrap();
        assert_eq!(
            edits[0],
            json!({"op":"add_object","type":"PartDesign::Boolean","name":"Union","body":"Target"})
        );
        assert_eq!(
            edits[1]["value"],
            json!([{"link":"ToolA"},{"link":"ToolB"}])
        );
        for (name, tools, operation) in [
            ("Union", vec![], "Fuse"),
            ("Union", vec!["Target".into()], "Fuse"),
            ("Union", vec!["Tool".into(), "Tool".into()], "Fuse"),
            ("Target", vec!["Tool".into()], "Fuse"),
            ("Union", vec!["Tool".into()], "Unsupported"),
        ] {
            assert!(boolean_edits(name, "Target", &tools, operation).is_err());
        }
        let registry = crate::packages::Registry::builtin().unwrap();
        for operation in ["Fuse", "Cut", "Common"] {
            let package = registry.build("body.boolean",&json!({"name":"Boolean","target":"Target","tool":"Tool","operation":operation})).unwrap();
            assert_eq!(
                package,
                boolean_edits("Boolean", "Target", &["Tool".into()], operation).unwrap()
            );
        }
    }
}
