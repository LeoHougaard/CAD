//! Compile explicit native profile copies. The document graph remains native;
//! cross-references use the original link and need no copy edit.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Independent,
    Dependent,
    CrossReference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    source: String,
    #[serde(default)]
    name: Option<String>,
    body: String,
    mode: Mode,
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Return a single edit to compose before the feature's Profile assignment in
/// the same revision-guarded native transaction or preview.
pub fn build(state: &Value, request: &Value) -> Result<Vec<Value>, String> {
    let request: Request = serde_json::from_value(request.clone())
        .map_err(|error| format!("Invalid profile reference: {error}"))?;
    if ![&request.source, &request.body]
        .into_iter()
        .all(|name| identifier(name))
    {
        return Err("Profile references require native object names".into());
    }
    if state["available"] == false {
        return Err("Reopen the unavailable native document before copying a profile".into());
    }
    let entries = state["objects"]
        .as_array()
        .ok_or("Read the native document before copying a profile")?;
    let mut objects = BTreeMap::new();
    for entry in entries {
        let name = entry["name"]
            .as_str()
            .filter(|name| identifier(name))
            .ok_or("Native object names are unavailable; refresh the document")?;
        if objects.insert(name, entry).is_some() {
            return Err("Native object names are duplicated; refresh the document".into());
        }
    }
    let source = objects
        .get(request.source.as_str())
        .ok_or("The selected profile no longer exists")?;
    let body = objects
        .get(request.body.as_str())
        .ok_or("Choose an existing target Body")?;
    if body["type"] != "PartDesign::Body" || !body["body_state"].is_object() {
        return Err("Choose a native PartDesign Body as the reference target".into());
    }
    if source["valid"] == false || source["type"] == "PartDesign::Body" {
        return Err("Choose a valid native profile feature".into());
    }
    let mode = match request.mode {
        Mode::CrossReference => return Ok(Vec::new()),
        Mode::Independent => {
            if !source["sketch"].is_object() {
                return Err("Independent profile copying requires a native Sketch".into());
            }
            "independent"
        }
        Mode::Dependent => {
            if !source["shape"].is_object() {
                return Err("Dependent profile copying requires native feature geometry".into());
            }
            "dependent"
        }
    };
    let name = copy_name(&request)?;
    if objects.contains_key(name) {
        return Err("The reference name already exists; choose a new native object name".into());
    }
    Ok(vec![json!({"op":"reference_copy","source":request.source,
        "name":name,"body":request.body,"mode":mode})])
}

fn copy_name(request: &Request) -> Result<&str, String> {
    request
        .name
        .as_deref()
        .filter(|name| identifier(name))
        .ok_or_else(|| "Choose a new native name for the profile copy".into())
}

/// Resolve the link after `build` has validated the authoritative snapshot.
pub fn result_profile(request: &Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(request.clone())
        .map_err(|error| format!("Invalid profile reference: {error}"))?;
    let name = match request.mode {
        Mode::CrossReference => request.source.as_str(),
        _ => copy_name(&request)?,
    };
    if !identifier(name) {
        return Err("Choose a native profile object".into());
    }
    Ok(json!({"link": name}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        json!({"available":true,"objects":[
            {"name":"SourceBody","type":"PartDesign::Body","body_state":{"tip":null}},
            {"name":"TargetBody","type":"PartDesign::Body","body_state":{"tip":"Pad"}},
            {"name":"Source","type":"Sketcher::SketchObject","body":"SourceBody","valid":true,"shape":{"null":false},"sketch":{}},
            {"name":"Surface","type":"Part::Feature","body":null,"valid":true,"shape":{"null":false}},
            {"name":"Pad","type":"PartDesign::Pad","body":"TargetBody","valid":true,"shape":{"null":false}}
        ]})
    }

    fn request() -> Value {
        json!({"source":"Source","name":"CopySource","body":"TargetBody","mode":"independent"})
    }

    #[test]
    fn explicit_target_outranks_source_ownership_without_history_mutation() {
        let original = state();
        assert_eq!(
            build(&original, &request()).unwrap(),
            vec![
                json!({"op":"reference_copy","source":"Source","name":"CopySource","body":"TargetBody","mode":"independent"})
            ]
        );
        assert_eq!(original, state());
    }

    #[test]
    fn dependent_surface_uses_native_feature_metadata() {
        let mut request = request();
        request["source"] = json!("Surface");
        assert!(build(&state(), &request).is_err());
        request["mode"] = json!("dependent");
        assert_eq!(build(&state(), &request).unwrap()[0]["source"], "Surface");
    }

    #[test]
    fn cross_reference_preserves_original_link_and_needs_no_copy_name() {
        let cross = json!({"source":"Source","body":"TargetBody","mode":"cross_reference"});
        assert!(build(&state(), &cross).unwrap().is_empty());
        assert_eq!(result_profile(&cross).unwrap(), json!({"link":"Source"}));
        assert_eq!(
            result_profile(&request()).unwrap(),
            json!({"link":"CopySource"})
        );
    }

    #[test]
    fn rejects_conflicts_stale_targets_unknown_options_and_fake_cross_reference() {
        for (key, value) in [
            ("name", json!("Pad")),
            ("name", json!("bad.name")),
            ("body", json!("Missing")),
            ("body", json!("Source")),
            ("source", json!("TargetBody")),
            ("mode", json!("xref")),
            ("keep_external", json!(true)),
        ] {
            let mut input = request();
            input[key] = value;
            assert!(build(&state(), &input).is_err(), "{input}");
        }
        let mut stale = state();
        let duplicate = stale["objects"][0].clone();
        stale["objects"].as_array_mut().unwrap().push(duplicate);
        assert!(build(&stale, &request()).is_err());
    }
}
