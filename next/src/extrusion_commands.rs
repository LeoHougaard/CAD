//! Pad/Pocket task availability follows the pending values, even when an
//! incomplete target cannot yet produce a successful native preview.
use serde_json::{Value, json};

pub fn supported(type_id: &str) -> bool {
    matches!(type_id, "PartDesign::Pad" | "PartDesign::Pocket")
}

pub fn reference(property: &str) -> bool {
    matches!(
        property,
        "UpToFace" | "UpToFace2" | "UpToShape" | "UpToShape2" | "ReferenceAxis"
    )
}

/// Preserve native object/subelement identities. A singular field must never
/// silently discard the other selected targets. FreeCAD validates geometry.
pub fn selection(property: &str, selected: &[(String, String)]) -> Result<Value, String> {
    if !reference(property) || selected.is_empty() {
        return Err("Select a native termination target or direction reference".into());
    }
    let links = crate::property_commands::selected_links(selected, true);
    if property.starts_with("UpToShape") {
        return Ok(links);
    }
    if selected.len() != 1 {
        return Err("Select exactly one native target for this field".into());
    }
    Ok(links[0].clone())
}

/// Only task presentation is derived here. Native enums, quantities, defaults,
/// reference resolution, expression evaluation and modeling stay in FreeCAD.
pub fn task_schema(schema: &Value, parameters: &Value) -> Value {
    let mut result = schema.clone();
    if !supported(parameters["type"].as_str().unwrap_or_default()) {
        return result;
    }
    if let Some(changes) = parameters["properties"].as_object() {
        for (key, value) in changes {
            result["properties"][key] = value.clone();
        }
    }
    // A rejected preview restores the native baseline, so its values cannot
    // resolve pending controller expressions. Keep dependent task inputs
    // reachable for any possible result. FreeCAD remains the only evaluator.
    // Merge exact native paths and explicit clears without interpreting text.
    // Self-relative and non-self-relative paths need not be equivalent clears.
    let mut expressions = schema["expressions"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(changes) = parameters["expressions"].as_object() {
        for (path, expression) in changes {
            expressions.remove(path);
            if expression.is_string() {
                expressions.insert(path.clone(), expression.clone());
            }
        }
    }
    let controlled = |property: &str| {
        expressions.iter().any(|(path, expression)| {
            expression.is_string()
                && path.trim_start_matches('.').split(['.', '[']).next() == Some(property)
        })
    };
    let side_expression = controlled("SideType");
    let first_expression = controlled("Type");
    let second_expression = controlled("Type2");
    let custom_expression = controlled("UseCustomVector");
    let sides = result["properties"]["SideType"]
        .as_str()
        .unwrap_or("One side")
        .to_owned();
    let custom = result["properties"]["UseCustomVector"] == true;
    let axis = !result["properties"]["ReferenceAxis"].is_null();
    let mode1 = result["properties"]["Type"]
        .as_str()
        .unwrap_or("Length")
        .to_owned();
    let mode2 = result["properties"]["Type2"]
        .as_str()
        .unwrap_or("Length")
        .to_owned();
    let two_sides_possible = sides == "Two sides" || side_expression;
    let length_possible = mode1 == "Length"
        || first_expression
        || two_sides_possible && (mode2 == "Length" || second_expression);
    let mut availability = vec![
        ("Type2".to_owned(), two_sides_possible),
        ("Direction".to_owned(), custom || custom_expression),
        ("ReferenceAxis".to_owned(), !custom || custom_expression),
        (
            "AlongSketchNormal".to_owned(),
            (custom || axis || custom_expression || controlled("ReferenceAxis")) && length_possible,
        ),
        (
            "Reversed".to_owned(),
            sides != "Symmetric" || mode1 != "Length" || side_expression || first_expression,
        ),
    ];
    for (suffix, mode, enabled, mode_expression) in [
        ("", mode1.as_str(), true, first_expression),
        ("2", mode2.as_str(), two_sides_possible, second_expression),
    ] {
        for (name, active) in [
            ("Length", mode == "Length"),
            ("TaperAngle", mode == "Length" || mode == "ThroughAll"),
            ("UpToFace", mode == "UpToFace"),
            ("UpToShape", mode == "UpToShape"),
            (
                "Offset",
                matches!(mode, "UpToFace" | "UpToShape" | "UpToFirst" | "UpToLast"),
            ),
        ] {
            availability.push((
                format!("{name}{suffix}"),
                enabled && (active || mode_expression),
            ));
        }
    }
    for (name, active) in availability {
        let metadata = &mut result["property_metadata"][&name];
        metadata["task_editable"] = json!(active);
        metadata["read_only"] = json!(!active);
        metadata["hidden"] = json!(!active);
    }
    for name in ["Type", "Type2"] {
        if let Some(values) = result["property_metadata"][name]["enumerations"].as_array_mut() {
            values.retain(|v| v.as_str().is_none_or(|s| !s.starts_with('?')));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expression_controllers_keep_dependent_form_fields_reachable_without_evaluation() {
        let schema = json!({"properties":{"SideType":"One side","Type":"Length","Type2":"Length", "UseCustomVector":false,"ReferenceAxis":null,"Length2":17},
            "expressions":{".Type2":"SomeParent.Mode"},"property_types":{
                "UpToFace":"App::PropertyLinkSub", "UpToShape":"App::PropertyLinkSubList",
                "Type2":"App::PropertyEnumeration", "UpToFace2":"App::PropertyLinkSub",
                "UpToShape2":"App::PropertyLinkSubList", "Direction":"App::PropertyVector", "ReferenceAxis":"App::PropertyLinkSub"}});
        for (controller, fields) in [
            (".Type", vec!["UpToFace", "UpToShape"]),
            ("SideType", vec!["Type2", "UpToFace2", "UpToShape2"]),
            (".UseCustomVector", vec!["Direction", "ReferenceAxis"]),
        ] {
            let parameters = json!({"type":"PartDesign::Pad", "properties":{},"expressions":{controller:"Parent.Width > 5 mm ? 3 : 0"}});
            let task = task_schema(&schema, &parameters);
            for field in fields {
                assert_eq!(task["property_metadata"][field]["hidden"], false, "{field}");
                assert!(crate::property_commands::writable(&task, field), "{field}");
            }
            assert_eq!(task["properties"]["Length2"], 17);
            assert_eq!(
                parameters["expressions"][controller],
                "Parent.Width > 5 mm ? 3 : 0"
            );
        }
        let cleared = task_schema(
            &schema,
            &json!({"type":"PartDesign::Pocket", "properties":{"SideType":"Two sides"},"expressions":{".Type2":null}}),
        );
        assert_eq!(cleared["property_metadata"]["UpToFace2"]["hidden"], true);
        assert!(!crate::property_commands::writable(&cleared, "UpToFace2"));
        let other_path_clear = task_schema(
            &schema,
            &json!({"type":"PartDesign::Pocket", "properties":{"SideType":"Two sides"},"expressions":{"Type2":null}}),
        );
        assert_eq!(
            other_path_clear["property_metadata"]["UpToFace2"]["hidden"],
            false
        );
        let other_type =
            json!({"type":"Part::Box", "properties":{},"expressions":{"Type":"3","SideType":"1"}});
        assert_eq!(task_schema(&schema, &other_type), schema);
        // A dimensional expression does not make any controller uncertain.
        let dimension = task_schema(
            &schema,
            &json!({"type":"PartDesign::Pad", "properties":{},"expressions":{"Length":"Parent.Width"}}),
        );
        assert_eq!(dimension["property_metadata"]["UpToFace"]["hidden"], true);
    }

    #[test]
    fn pending_modes_unlock_targets_before_preview_and_preserve_native_values() {
        let schema = json!({"properties":{"Length":5,"SideType":"One side","Type":"Length","Type2":"Length","ReferenceAxis":null},"property_metadata":{"Type":{"enumerations":["Length","?TwoLengths","UpToShape"]}}});
        let task = task_schema(
            &schema,
            &json!({"type":"PartDesign::Pocket","properties":{"SideType":"Two sides","Type":"UpToFace","Type2":"UpToShape","UseCustomVector":true}}),
        );
        for field in [
            "UpToFace",
            "UpToShape2",
            "Offset",
            "Offset2",
            "Direction",
            "Type2",
        ] {
            assert_eq!(
                task["property_metadata"][field]["read_only"], false,
                "{field}"
            );
        }
        assert_eq!(task["property_metadata"]["Length"]["read_only"], true);
        assert_eq!(task["properties"]["Length"], 5);
        assert_eq!(
            task["property_metadata"]["Type"]["enumerations"],
            json!(["Length", "UpToShape"])
        );
        assert!(
            selection(
                "UpToFace",
                &[
                    ("Target".into(), "Face4".into()),
                    ("Other".into(), "Face2".into())
                ]
            )
            .is_err()
        );
        assert_eq!(
            selection("UpToShape", &[("Target".into(), String::new())]).unwrap(),
            json!([{"link":"Target","subelements":[]}])
        )
    }
}
