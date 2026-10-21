//! Atomic native MultiTransform tasks. Geometry, ownership and recomputation
//! remain in the native document; this module only compiles guarded edits.
use crate::property_commands;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const STEP_TYPES: &[&str] = &[
    "PartDesign::LinearPattern",
    "PartDesign::PolarPattern",
    "PartDesign::Mirrored",
    "PartDesign::Scaled",
];

pub fn is_step(type_id: &str) -> bool {
    STEP_TYPES.contains(&type_id)
}

fn identifier<'a>(value: &'a Value, key: &str, description: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|name| {
            !name.is_empty()
                && name.chars().enumerate().all(|(index, ch)| {
                    ch == '_' || ch.is_ascii_alphabetic() || index > 0 && ch.is_ascii_digit()
                })
        })
        .ok_or_else(|| format!("Choose a valid {description} name"))
}

/// `transformations` is the complete ordered task list, with stable names and
/// explicit `create` flags captured before preview. `removed_transformations`
/// contains only children retired by this task. A task that edits an existing
/// feature must capture `restore_tip` from the original native Body snapshot.
/// Preview snapshots must never determine child creation or deletion.
pub fn build(parameters: &Value) -> Result<Vec<Value>, String> {
    if parameters["type"] != "PartDesign::MultiTransform" {
        return Err("Transformation steps require a Multiple transforms feature".into());
    }
    let parent = identifier(parameters, "name", "feature")?;
    let body = identifier(parameters, "body", "Body")?;
    let create_parent = parameters["create"]
        .as_bool()
        .ok_or("Specify whether the feature is new")?;
    let restore_tip = if create_parent && parameters["restore_tip"].is_null() {
        parent
    } else {
        identifier(parameters, "restore_tip", "original Body Tip")?
    };
    if create_parent && restore_tip != parent {
        return Err("A new Multiple transforms feature must become the Body Tip".into());
    }
    let steps = parameters["transformations"]
        .as_array()
        .filter(|steps| !steps.is_empty())
        .ok_or("Add at least one transformation step")?;
    let mut names = BTreeSet::new();
    for step in steps {
        let name = identifier(step, "name", "step")?;
        if name == parent || name == body || !names.insert(name) {
            return Err("Transformation steps need distinct names".into());
        }
        if !step["type"].as_str().is_some_and(is_step) {
            return Err("Choose a linear, circular, mirror or scaled transformation".into());
        }
        if step["create"].as_bool().is_none() {
            return Err("Specify whether each transformation step is new".into());
        }
        let properties = step["properties"]
            .as_object()
            .ok_or("Missing transformation step property changes")?;
        let expressions = step["expressions"].as_object();
        for reserved in [
            "Originals",
            "OriginalSubs",
            "BaseFeature",
            "Transformations",
        ] {
            if properties.contains_key(reserved)
                || expressions.is_some_and(|values| {
                    values
                        .keys()
                        .any(|path| path == reserved || path.starts_with(&format!("{reserved}.")))
                })
            {
                return Err(format!(
                    "{reserved} belongs to the Multiple transforms feature, not an individual step"
                ));
            }
        }
    }
    if names.contains(restore_tip) {
        return Err("A transformation step cannot be the Body Tip".into());
    }
    let visible = if create_parent {
        true
    } else {
        parameters["restore_tip_visibility"]
            .as_bool()
            .ok_or("Capture the original Body Tip visibility before editing")?
    };
    let removed = match parameters.get("removed_transformations") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(values)) => values.as_slice(),
        _ => return Err("Removed transformation steps must be a list of names".into()),
    };
    let mut retired = BTreeSet::new();
    for value in removed {
        let name = value.as_str().ok_or("Choose a retired step name")?;
        if name.is_empty()
            || name == body
            || name == parent
            || name == restore_tip
            || names.contains(name)
            || !retired.insert(name)
        {
            return Err("Only distinct retired transformation steps can be removed".into());
        }
    }
    let mut parent_parameters = parameters.clone();
    parent_parameters
        .as_object_mut()
        .unwrap()
        .remove("transformations");
    parent_parameters["properties"]
        .as_object_mut()
        .ok_or("Missing feature property changes")?
        .remove("Transformations");
    if parent_parameters["expressions"]
        .get("Transformations")
        .is_some()
    {
        return Err("The ordered transformation steps cannot be an expression".into());
    }
    // An unchanged existing parent is still a valid task when its children change.
    let mut edits = if !create_parent
        && parent_parameters["properties"]
            .as_object()
            .unwrap()
            .is_empty()
        && parent_parameters["expressions"]
            .as_object()
            .is_none_or(|values| values.is_empty())
    {
        Vec::new()
    } else {
        property_commands::build(&parent_parameters)?
    };
    for step in steps {
        let mut child = step.clone();
        child["body"] = json!(body);
        let unchanged = child["create"] == false
            && child["properties"].as_object().unwrap().is_empty()
            && child["expressions"]
                .as_object()
                .is_none_or(|values| values.is_empty());
        if !unchanged {
            edits.extend(property_commands::build(&child)?);
        }
    }
    edits.push(
        json!({"op":"set","object":parent,"property":"Transformations",
        "value":steps.iter().map(|step|json!({"link":step["name"]})).collect::<Vec<_>>()}),
    );
    for name in retired {
        edits.push(json!({"op":"remove_object","object":name}));
    }
    // Adding a native Body child changes Tip. Restore the captured Tip, including
    // a downstream feature when an earlier MultiTransform is being edited.
    edits.push(json!({"op":"set","object":body,"property":"Tip","value":{"link":restore_tip}}));
    edits.push(json!({"op":"set","object":restore_tip,"property":"Visibility","value":visible}));
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Value {
        json!({"type":"PartDesign::MultiTransform","name":"Result","body":"Body",
            "create":true,"properties":{"Originals":[{"link":"Protrusion"}]},
            "transformations":[{"type":"PartDesign::LinearPattern","name":"Linear",
                "create":true,"properties":{"Direction":{"link":"X_Axis","subelements":[""]},
                    "Length":4,"Occurrences":2}}]})
    }

    #[test]
    fn creates_parent_before_child_and_links_before_restoring_tip() {
        let edits = build(&task()).unwrap();
        assert_eq!(edits[0]["name"], "Result");
        let originals = edits
            .iter()
            .position(|edit| edit["property"] == "Originals")
            .unwrap();
        let child = edits
            .iter()
            .position(|edit| edit["name"] == "Linear")
            .unwrap();
        assert!(originals < child);
        assert_eq!(edits[child]["body"], "Body");
        assert_eq!(edits[edits.len() - 3]["property"], "Transformations");
        assert_eq!(edits[edits.len() - 2]["value"]["link"], "Result");
        assert_eq!(edits.last().unwrap()["value"], true);
        assert!(
            !edits
                .iter()
                .any(|e| e["object"] == "Linear" && e["property"] == "Originals")
        );
    }

    #[test]
    fn editing_reorders_unlinks_before_delete_and_preserves_downstream_tip() {
        let mut p = task();
        p["create"] = json!(false);
        p["properties"] = json!({});
        p["restore_tip"] = json!("AfterResult");
        p["restore_tip_visibility"] = json!(false);
        p["transformations"][0]["create"] = json!(false);
        p["transformations"][0]["properties"] = json!({});
        p["removed_transformations"] = json!(["Mirror"]);
        let edits = build(&p).unwrap();
        assert_eq!(edits.len(), 4);
        assert_eq!(edits[0]["property"], "Transformations");
        assert_eq!(edits[1], json!({"op":"remove_object","object":"Mirror"}));
        assert_eq!(edits[2]["value"]["link"], "AfterResult");
        assert_eq!(edits[3]["object"], "AfterResult");
        assert_eq!(edits[3]["value"], false);
        p.as_object_mut().unwrap().remove("restore_tip");
        assert!(build(&p).unwrap_err().contains("original Body Tip"));
    }

    #[test]
    fn refuses_independent_originals_duplicate_names_and_retiring_live_steps() {
        let mut p = task();
        p["transformations"][0]["properties"]["Originals"] = json!([]);
        assert!(build(&p).unwrap_err().contains("Originals"));
        p["transformations"][0]["properties"]
            .as_object_mut()
            .unwrap()
            .remove("Originals");
        let duplicate = p["transformations"][0].clone();
        p["transformations"].as_array_mut().unwrap().push(duplicate);
        assert!(build(&p).unwrap_err().contains("distinct names"));
        p["transformations"].as_array_mut().unwrap().pop();
        p["removed_transformations"] = json!(["Linear"]);
        assert!(build(&p).unwrap_err().contains("retired"));
    }
}
