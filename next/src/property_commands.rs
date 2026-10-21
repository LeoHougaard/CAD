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
    let mut fields: Vec<_> = properties.iter().collect();
    fields.sort_by_key(|(property, value)| {
        let controllers = [
            "ThreadType",
            "Threaded",
            "ThreadSize",
            "ThreadClass",
            "DepthType",
            "ThreadDepthType",
            "HoleCutType",
            "HoleCutCustomValues",
            "DrillPoint",
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
        (!is_reference(value), control)
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

pub fn selected_links(selections: &[(String, String)], subelements: bool) -> Value {
    let mut links: Vec<Value> = Vec::new();
    for (object, subelement) in selections {
        if let Some(existing) = links.iter_mut().find(|v| v["link"] == *object) {
            if subelements && !subelement.is_empty() {
                let names = existing["subelements"].as_array_mut().unwrap();
                if !names.iter().any(|v| v == subelement) {
                    names.push(json!(subelement));
                }
            }
        } else {
            let mut link = json!({"link":object});
            if subelements {
                link["subelements"] = json!(if subelement.is_empty() {
                    vec![]
                } else {
                    vec![subelement]
                });
            }
            links.push(link);
        }
    }
    json!(links)
}

#[cfg(test)]
mod tests {
    use super::*;
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
