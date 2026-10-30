//! Native property values and references cross the same guarded edit boundary as tools.
use serde_json::{Map, Value, json};

pub fn supported(type_id: &str) -> bool {
    let suffix = type_id.strip_prefix("App::Property").unwrap_or("");
    type_id == "Part::PropertyFilletEdges"
        || matches!(
            suffix,
            "String"
                | "UUID"
                | "Color"
                | "Map"
                | "VectorList"
                | "Bool"
                | "Integer"
                | "IntegerConstraint"
                | "Float"
                | "FloatConstraint"
                | "Enumeration"
                | "Length"
                | "Distance"
                | "Angle"
                | "Area"
                | "Volume"
                | "Quantity"
                | "QuantityConstraint"
                | "Percent"
                | "Precision"
                | "Speed"
                | "Acceleration"
                | "StringList"
                | "IntegerList"
                | "FloatList"
                | "BoolList"
                | "Vector"
                | "VectorDistance"
                | "Position"
                | "Placement"
        )
        || suffix.starts_with("Link")
        || suffix.starts_with("XLink")
}

pub fn writable(schema: &Value, property: &str) -> bool {
    property != "Visibility"
        && schema["property_types"][property]
            .as_str()
            .is_some_and(supported)
        && (schema["property_metadata"][property]["task_editable"] == true
            || !schema["property_metadata"][property]["read_only"]
                .as_bool()
                .unwrap_or(false))
}

/// Only changed values are written. Native computed/default properties never
/// round-trip through the UI, and clearing an expression precedes its new value.
pub fn build(parameters: &Value) -> Result<Vec<Value>, String> {
    if let Some(point) = parameters.get("plane_point") {
        let plane = point["plane"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Choose a plane")?;
        let object = point["object"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Choose a vertex owner")?;
        let subelement = point["subelement"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Choose a vertex")?;
        let mapped = point["mapped_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Vertex identity is unavailable")?;
        if parameters["create"] != true
            || parameters["type"] != "PartDesign::Plane"
            || parameters["body"] != point["body"]
            || parameters["properties"]["MapMode"] != "ParallelPlane"
            || parameters["properties"]["AttachmentSupport"]
                != json!([
                {"link":plane,"subelements":[]}, {"link":object,"subelements":[subelement]}])
        {
            return Err("Plane point references changed; select them again".into());
        }
        let mut candidate = parameters.clone();
        candidate.as_object_mut().unwrap().remove("plane_point");
        let mut edits = vec![
            json!({"op":"validate_plane_point","object":object,"plane":plane,
            "body":point["body"],"subelement":subelement,"mapped_name":mapped}),
        ];
        edits.extend(build(&candidate)?);
        return Ok(edits);
    }
    if let Some(selection) = parameters.get("extrusion_selection") {
        if !crate::extrusion_commands::supported(parameters["type"].as_str().unwrap_or_default()) {
            return Err("Termination selection requires Pad or Pocket".into());
        }
        let property = selection["property"]
            .as_str()
            .ok_or("Choose a target field")?;
        let selected: Vec<(String, String)> =
            serde_json::from_value(selection["selections"].clone()).map_err(|e| e.to_string())?;
        let value = crate::extrusion_commands::selection(property, &selected)?;
        let mut candidate = parameters.clone();
        candidate
            .as_object_mut()
            .unwrap()
            .remove("extrusion_selection");
        candidate["properties"][property] = value;
        return build(&candidate);
    }
    if let Some(choice) = parameters.get("reference_choice") {
        let request = &choice["request"];
        let source = request["source"]
            .as_str()
            .ok_or("Choose a source profile")?;
        if parameters["properties"]["Profile"]["link"] != source
            || parameters["body"] != request["body"]
            || !parameters["type"]
                .as_str()
                .and_then(crate::feature_commands::get)
                .is_some_and(|definition| definition.preferred_properties.contains(&"Profile"))
        {
            return Err("Profile reference choice does not match this native feature task".into());
        }
        let mut edits = crate::profile_reference_commands::build(&choice["state"], request)?;
        let profile = crate::profile_reference_commands::result_profile(request)?;
        // Keep the task's original source stable across replacement previews.
        // Substitute only in this compiled candidate, never in the UI baseline.
        let mut candidate = parameters.clone();
        candidate
            .as_object_mut()
            .unwrap()
            .remove("reference_choice");
        candidate["properties"]["Profile"] = profile;
        edits.extend(build(&candidate)?);
        return Ok(edits);
    }
    if let Some(axis) = parameters.get("construction_axis") {
        if parameters["create"] != true
            || parameters["type"] != "PartDesign::Revolution"
            || axis["profile"] != parameters["properties"]["Profile"]["link"]
            || axis["body"] != parameters["body"]
        {
            return Err("Construction axis does not match this Revolution task".into());
        }
        let helper = axis["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Missing axis dependency name")?;
        let mut candidate = parameters.clone();
        candidate
            .as_object_mut()
            .unwrap()
            .remove("construction_axis");
        candidate["properties"]["ReferenceAxis"] = json!({"link":helper,"subelements":[""]});
        let mut edit = axis.clone();
        edit["op"] = json!("create_revolve_axis");
        let mut edits = vec![edit];
        edits.extend(build(&candidate)?);
        return Ok(edits);
    }
    if parameters["transformations"].is_array() {
        return crate::transform_commands::build(parameters);
    }

    let name = parameters["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Choose a native object")?;
    let mut edits = Vec::new();
    if parameters["create"] == true {
        let type_id = parameters["type"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Choose a native feature type")?;
        let mut edit = json!({"op":"add_object","type":type_id,"name":name});
        if let Some(body) = parameters["body"].as_str().filter(|s| !s.is_empty()) {
            edit["body"] = json!(body);
        }
        edits.push(edit);
    }
    let properties = parameters["properties"]
        .as_object()
        .ok_or("Missing native property changes")?;
    let empty = Map::new();
    let expressions = parameters["expressions"].as_object().unwrap_or(&empty);
    for (property, expression) in expressions {
        if !expression.is_null() && !expression.is_string() {
            return Err("Expressions must be text or null".into());
        }
        if expression.is_null() {
            edits.push(
                json!({"op":"expression","object":name,"property":property,"expression":null}),
            );
        }
    }
    // Native links must exist before dimensional properties and expressions use
    // them. Map order is otherwise irrelevant to native recomputation.
    let value_start = edits.len();
    let mut fields: Vec<_> = properties.iter().collect();
    fields.sort_by_key(|(property, value)| {
        let controllers = [
            "ThreadType",
            "Threaded",
            "ThreadSize",
            "ThreadClass",
            "DepthType",
            "ThreadDepthType",
            "ModelThread",
            "UseCustomThreadClearance",
            "HoleCutType",
            "HoleCutCustomValues",
            "DrillPoint",
            "Tapered",
            "SideType",
            "Type",
            "Type2",
            "Mode",
            "Mode2",
            "MapMode",
            "Transformation",
            "TransformationMode",
            "Transition",
        ];
        let control = controllers
            .iter()
            .position(|field| *field == property.as_str())
            .unwrap_or(controllers.len());
        if crate::extrusion_commands::supported(parameters["type"].as_str().unwrap_or_default()) {
            // Assemble task controllers before targets, then dimensional values.
            // Do not recompute between them: a mode can require a new target.
            let rank = match property.as_str() {
                "SideType" => 0,
                "Type" => 1,
                "Type2" => 2,
                "UseCustomVector" => 3,
                _ if is_reference(value) => 4,
                _ => 5,
            };
            (rank, 0)
        } else {
            (usize::from(!is_reference(value)), control)
        }
    });
    for (property, value) in fields {
        if property == "Support"
            && matches!(
                parameters["type"].as_str(),
                Some("PartDesign::ShapeBinder" | "PartDesign::SubShapeBinder")
            )
        {
            edits.push(json!({"op":"binder_support","object":name,"value":value}));
        } else {
            edits.push(json!({"op":"set","object":name,"property":property,"value":value}));
        }
    }
    for (property, expression) in expressions {
        if !expression.is_null() {
            edits.push(json!({"op":"expression","object":name,"property":property,"expression":expression}));
        }
    }
    if parameters["type"] == "PartDesign::Hole" {
        // Native controller expressions can change editor locks and enum
        // domains too. Assemble them before dependent values, in the same
        // order as literal controllers. FreeCAD evaluates every expression.
        edits[value_start..].sort_by_key(|edit| {
            let property = edit["property"].as_str().unwrap_or_default();
            let property = property
                .trim_start_matches('.')
                .split(['.', '['])
                .next()
                .unwrap_or_default();
            match property {
                "Profile" => 0,
                "ThreadType" => 1,
                "Threaded" => 2,
                "ThreadSize" => 3,
                "ThreadClass" => 4,
                "DepthType" => 5,
                "ThreadDepthType" => 6,
                "ModelThread" => 7,
                "UseCustomThreadClearance" => 8,
                "HoleCutType" => 9,
                "HoleCutCustomValues" => 10,
                "DrillPoint" => 11,
                "Tapered" => 12,
                _ => 13,
            }
        });
    }
    // Native profile/original/transform link setters can move the Body tip.
    // Creation publishes the new solid feature; editing preserves the tip
    // captured when its task began, including a downstream history feature.
    let type_id = parameters["type"].as_str().unwrap_or_default();
    let solid_feature = type_id.starts_with("PartDesign::")
        && !matches!(
            type_id,
            "PartDesign::Body"
                | "PartDesign::Plane"
                | "PartDesign::Line"
                | "PartDesign::Point"
                | "PartDesign::CoordinateSystem"
                | "PartDesign::ShapeBinder"
                | "PartDesign::SubShapeBinder"
        );
    if solid_feature
        && let Some(body) = parameters["body"].as_str()
        && (parameters["create"] == true || !properties.is_empty() || !expressions.is_empty())
    {
        let tip = if parameters["create"] == true {
            Some(name)
        } else {
            parameters["tip"].as_str()
        };
        if let Some(tip) = tip {
            edits.push(json!({"op":"set","object":body,"property":"Tip","value":{"link":tip}}));
            let visible = if parameters["create"] == true {
                Value::Bool(true)
            } else {
                parameters["tip_visibility"].clone()
            };
            if visible.is_boolean() {
                edits
                    .push(json!({"op":"set","object":tip,"property":"Visibility","value":visible}));
            }
        }
    }
    if edits.is_empty() {
        Err("Change a property first".into())
    } else {
        Ok(edits)
    }
}

fn is_reference(value: &Value) -> bool {
    value.get("link").is_some()
        || value
            .as_array()
            .is_some_and(|items| items.iter().any(|v| v.get("link").is_some()))
}

/// Native Link and LinkSub contain one distinct object, not necessarily one
/// viewport pick. LinkSub retains that object's complete selected subname list.
/// Keep validation separate from assignment so a rejected pick changes no task.
pub fn selected_reference(
    type_id: &str,
    objects: &[Value],
    selections: &[(String, String)],
) -> Result<Value, String> {
    let kind = type_id.strip_prefix("App::Property").unwrap_or_default();
    if !kind.starts_with("Link") && !kind.starts_with("XLink") {
        return Err("Choose a native reference field".into());
    }
    if selections.is_empty() {
        return Err("Select a native object for this field".into());
    }
    validate_selected_objects(objects, selections)?;
    let links = selected_links(selections, kind.contains("Sub"));
    if kind.contains("List") {
        Ok(links)
    } else if links.as_array().unwrap().len() == 1 {
        Ok(links[0].clone())
    } else {
        Err(
            "Select one native object for this field. Clear picks from other objects and retry."
                .into(),
        )
    }
}

/// Check every selected identity before a task narrows its reference choices.
pub fn validate_selected_objects(
    objects: &[Value],
    selections: &[(String, String)],
) -> Result<(), String> {
    for (name, _) in selections {
        if !objects.iter().any(|object| object["name"] == *name) {
            return Err(format!(
                "Selected native object {name} is unavailable. Clear the stale selection and retry."
            ));
        }
    }
    Ok(())
}

pub fn selected_links(selections: &[(String, String)], subelements: bool) -> Value {
    let mut links: Vec<Value> = Vec::new();
    for (object, subelement) in selections {
        if let Some(existing) = links.iter_mut().find(|v| v["link"] == *object) {
            if subelements {
                let names = existing["subelements"].as_array_mut().unwrap();
                if !names.iter().any(|v| v == subelement) {
                    names.push(json!(subelement));
                }
            }
        } else {
            let mut link = json!({"link":object});
            if subelements {
                link["subelements"] = json!([subelement]);
            }
            links.push(link);
        }
    }
    if subelements {
        for link in &mut links {
            // Keep the existing whole-object encoding when it is the only pick.
            // A mixed whole-object/subelement pick needs its explicit empty
            // subname so neither native reference is silently discarded.
            if link["subelements"] == json!([""]) {
                link["subelements"] = json!([]);
            }
        }
    }
    json!(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revolve_axis_dependency_is_created_before_the_revolution() {
        let p = json!({"type":"PartDesign::Revolution","name":"Revolution","body":"Body",
            "create":true,"properties":{"Profile":{"link":"Profile"},
                "ReferenceAxis":{"link":"Profile","subelements":["V_Axis"]}},"expressions":{},
            "construction_axis":{"name":"RevolveAxis","master":"Master","geometry_id":42,
                "body":"Body","profile":"Profile"}});
        let edits = build(&p).unwrap();
        assert_eq!(edits[0]["op"], "create_revolve_axis");
        assert_eq!(edits[0]["geometry_id"], 42);
        assert!(edits.iter().any(|e| e["op"] == "set"
            && e["object"] == "Revolution"
            && e["property"] == "ReferenceAxis"
            && e["value"] == json!({"link":"RevolveAxis","subelements":[""]})));
        let mut stale = p.clone();
        stale["construction_axis"]["profile"] = json!("Other");
        assert!(build(&stale).is_err());
        stale = p.clone();
        stale["create"] = json!(false);
        assert!(build(&stale).is_err());
    }
    #[test]
    fn reference_choices_compose_before_profile_assignment_and_preserve_task_source_and_tip() {
        let state = json!({"objects":[
            {"name":"Target","type":"PartDesign::Body","body_state":{"tip":"Later"}},
            {"name":"Source","type":"Sketcher::SketchObject","body":"Other","valid":true,"sketch":{},"shape":{"null":false}}
        ]});
        for mode in ["independent", "dependent", "cross_reference"] {
            let parameters = json!({"name":"Pad","type":"PartDesign::Pad","body":"Target",
                "create":false,"tip":"Later","tip_visibility":false,
                "properties":{"Profile":{"link":"Source"},"Length":"4 mm"},
                "reference_choice":{"state":state,"request":{
                    "source":"Source","body":"Target","name":"Copy","mode":mode}}});
            let edits = build(&parameters).unwrap();
            let profile = if mode == "cross_reference" {
                "Source"
            } else {
                "Copy"
            };
            if mode != "cross_reference" {
                assert_eq!(edits[0]["op"], "reference_copy");
                assert_eq!(edits[0]["mode"], mode);
            }
            assert!(
                edits.iter().any(|edit| edit["property"] == "Profile"
                    && edit["value"] == json!({"link":profile}))
            );
            assert_eq!(edits[edits.len() - 2]["value"], json!({"link":"Later"}));
            assert_eq!(edits.last().unwrap()["value"], false);
            assert_eq!(parameters["properties"]["Profile"]["link"], "Source");
            for field in ["body", "source"] {
                let mut invalid = parameters.clone();
                invalid["reference_choice"]["request"][field] = json!("Mismatch");
                assert!(build(&invalid).is_err());
            }
        }
    }
    #[test]
    fn expression_clear_precedes_value_and_links_precede_expression() {
        let edits = build(&json!({"name":"Pad","create":false,"properties":{
            "Length":"2 cm", "Profile":{"link":"Sketch"}},
            "expressions":{"Length":null,"Length2":"Sketch.Constraints.width / 2"}}))
        .unwrap();
        assert_eq!(edits[0]["op"], "expression");
        assert!(edits[0]["expression"].is_null());
        assert_eq!(edits[1]["property"], "Profile");
        assert_eq!(edits[2]["value"], "2 cm");
        assert_eq!(edits[3]["expression"], "Sketch.Constraints.width / 2");
        assert_eq!(edits.len(), 4);
    }
    #[test]
    fn grouped_selection_retains_order_and_deduplicates_native_references() {
        let selected = [
            ("Pad".into(), "Edge2".into()),
            ("Pad".into(), "Edge1".into()),
            ("Other".into(), "Face1".into()),
            ("Pad".into(), "Edge2".into()),
        ];
        assert_eq!(
            selected_links(&selected, true),
            json!([
            {"link":"Pad","subelements":["Edge2","Edge1"]},
            {"link":"Other","subelements":["Face1"]}])
        );
        assert_eq!(
            selected_links(&selected, false),
            json!([{"link":"Pad"},{"link":"Other"}])
        );
    }
    #[test]
    fn creation_without_body_and_empty_existing_edits_are_distinct() {
        let edits = build(
            &json!({"name":"Box","type":"Part::Box","create":true,"body":null,"properties":{}}),
        )
        .unwrap();
        assert_eq!(
            edits,
            vec![json!({"op":"add_object","name":"Box","type":"Part::Box"})]
        );
        assert!(build(&json!({"name":"Box","create":false,"properties":{}})).is_err());
    }
}
