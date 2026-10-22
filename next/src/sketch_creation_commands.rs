//! Create sketches on native supports without predicting origin object names.
//! FreeCAD owns attachment frames, planarity validation and recomputation.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    name: String,
    body: String,
    create_body: bool,
    support: Support,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Support {
    Origin {
        role: String,
    },
    Reference {
        object: String,
        subelements: Vec<String>,
        mode: Mode,
    },
}

#[derive(Deserialize)]
enum Mode {
    FlatFace,
    ObjectXY,
}

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn object<'a>(objects: &BTreeMap<&str, &'a Value>, name: &str) -> Result<&'a Value, String> {
    objects
        .get(name)
        .copied()
        .ok_or_else(|| format!("Native object {name} is absent; refresh the document"))
}

/// Compile one guarded creation transaction. Origin attachment is resolved by
/// native Role after Body creation, so new Bodies and sketches share one Undo.
/// Face planarity remains subject to the native FlatFace attachment engine.
pub fn build(state: &Value, request: &Value) -> Result<Vec<Value>, String> {
    let request: Request = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
    if state["available"] == false || !state["preview"].is_null() || !state["sketch_edit"].is_null()
    {
        return Err("Finish the current native edit before creating another sketch".into());
    }
    if !identifier(&request.name) || !identifier(&request.body) || request.name == request.body {
        return Err("Choose distinct native sketch and Body names".into());
    }
    let entries = state["objects"]
        .as_array()
        .ok_or("Read the native document before creating a sketch")?;
    let mut objects = BTreeMap::new();
    for entry in entries {
        let name = entry["name"]
            .as_str()
            .ok_or("Native object is missing its name")?;
        if objects.insert(name, entry).is_some() {
            return Err("The snapshot contains duplicate native object names".into());
        }
    }
    if objects.contains_key(request.name.as_str()) {
        return Err("Sketch name already exists".into());
    }
    let body = if request.create_body {
        if objects.contains_key(request.body.as_str()) {
            return Err("New Body name already exists".into());
        }
        None
    } else {
        let body = object(&objects, &request.body)?;
        if body["type"] != "PartDesign::Body" || !body["body_state"].is_object() {
            return Err("Choose an existing native PartDesign Body".into());
        }
        Some(body)
    };
    let mut edits = Vec::new();
    if request.create_body {
        edits.push(json!({"op":"add_object","type":"PartDesign::Body","name":request.body}));
    }
    let mut create = json!({"op":"add_object","type":"Sketcher::SketchObject","name":request.name,"body":request.body});
    match &request.support {
        Support::Origin { role } => {
            if !matches!(role.as_str(), "XY_Plane" | "XZ_Plane" | "YZ_Plane") {
                return Err("Choose a native XY, XZ or YZ origin plane".into());
            }
            if let Some(body) = body {
                let origin = object(
                    &objects,
                    body["properties"]["Origin"]["link"]
                        .as_str()
                        .ok_or("Body origin is missing")?,
                )?;
                let features = origin["properties"]["OriginFeatures"]
                    .as_array()
                    .ok_or("Read native origin roles before creating a sketch")?;
                let matches = features
                    .iter()
                    .filter_map(|link| link["link"].as_str())
                    .filter_map(|name| objects.get(name).copied())
                    .filter(|feature| {
                        feature["properties"]["Role"] == *role
                            && feature["type"] == "App::Plane"
                            && feature["body"] == request.body
                    })
                    .count();
                if matches != 1 {
                    return Err(
                        "Body origin must contain one native plane with the requested role".into(),
                    );
                }
            }
            create["origin_plane"] = json!(role);
            edits.push(create);
        }
        Support::Reference {
            object: source,
            subelements,
            mode,
        } => {
            if request.create_body {
                return Err("Choose an origin plane for a newly created Body".into());
            }
            if !identifier(source) || subelements.len() != 1 {
                return Err("Choose one native sketch support".into());
            }
            let mut source = source.as_str();
            let mut support = object(&objects, source)?;
            if source == request.body {
                source = support["body_state"]["tip"]
                    .as_str()
                    .ok_or("Empty Body has no supporting face")?;
                support = object(&objects, source)?;
            }
            if support["body"] != request.body {
                return Err(
                    "Support belongs outside the target Body; choose the reference policy first"
                        .into(),
                );
            }
            let subelement = &subelements[0];
            let mode = match mode {
                Mode::ObjectXY
                    if support["type"] == "Sketcher::SketchObject" && subelement.is_empty() =>
                {
                    "ObjectXY"
                }
                Mode::FlatFace
                    if subelement.is_empty() && support["reference_display"]["kind"] == "plane" =>
                {
                    "FlatFace"
                }
                Mode::FlatFace
                    if subelement
                        .strip_prefix("Face")
                        .and_then(|n| n.parse::<u64>().ok())
                        .is_some_and(|n| {
                            n > 0 && n <= support["shape"]["faces"].as_u64().unwrap_or(0)
                        }) =>
                {
                    "FlatFace"
                }
                _ => {
                    return Err(
                        "Choose a native plane/face with FlatFace or a whole sketch with ObjectXY"
                            .into(),
                    );
                }
            };
            edits.push(create);
            edits.push(
                json!({"op":"set","object":request.name,"property":"AttachmentSupport",
                              "value":[{"link":source,"subelements":subelements}]}),
            );
            edits.push(json!({"op":"set","object":request.name,"property":"MapMode","value":mode}));
        }
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        json!({"available":true,"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body","body_state":{"tip":"Pad"},
             "properties":{"Origin":{"link":"Origin7"}}},
            {"name":"Origin7","properties":{"OriginFeatures":[{"link":"XY_Plane9"}]}},
            {"name":"XY_Plane9","type":"App::Plane","body":"Body","properties":{"Role":"XY_Plane"},"reference_display":{"kind":"plane"}},
            {"name":"Datum","type":"PartDesign::Plane","body":"Body","reference_display":{"kind":"plane"},"shape":{"faces":1}},
            {"name":"Profile","type":"Sketcher::SketchObject","body":"Body"},
            {"name":"Pad","type":"PartDesign::Pad","body":"Body","shape":{"faces":6}},
            {"name":"Foreign","type":"PartDesign::Plane","body":"OtherBody","reference_display":{"kind":"plane"}},
            {"name":"Axis","type":"PartDesign::Line","body":"Body","shape":{"faces":0}}
        ]})
    }
    fn request(support: Value) -> Value {
        json!({"name":"NewSketch","body":"Body","create_body":false,"support":support})
    }

    #[test]
    fn origin_roles_resolve_after_new_body_creation_without_predicting_names() {
        for role in ["XY_Plane", "XZ_Plane", "YZ_Plane"] {
            let request = json!({"name":"NewSketch","body":"NewBody","create_body":true,"support":{"kind":"origin","role":role}});
            assert_eq!(
                build(&state(), &request).unwrap(),
                vec![
                    json!({"op":"add_object","type":"PartDesign::Body","name":"NewBody"}),
                    json!({"op":"add_object","type":"Sketcher::SketchObject","name":"NewSketch","body":"NewBody","origin_plane":role})
                ]
            );
        }
        let edits = build(
            &state(),
            &request(json!({"kind":"origin","role":"XY_Plane"})),
        )
        .unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["origin_plane"], "XY_Plane");
    }

    #[test]
    fn native_plane_sketch_and_body_tip_faces_keep_their_native_supports() {
        for (source, subelement, mode, expected) in [
            ("Datum", "", "FlatFace", "Datum"),
            ("Profile", "", "ObjectXY", "Profile"),
            ("Pad", "Face6", "FlatFace", "Pad"),
            ("Body", "Face6", "FlatFace", "Pad"),
        ] {
            let edits = build(&state(), &request(json!({"kind":"reference","object":source,"subelements":[subelement],"mode":mode}))).unwrap();
            assert_eq!(edits.len(), 3);
            assert_eq!(
                edits[1]["value"],
                json!([{"link":expected,"subelements":[subelement]}])
            );
            assert_eq!(edits[2]["value"], mode);
        }
    }

    #[test]
    fn unsupported_selections_and_foreign_ownership_never_fall_back_to_xy() {
        for (source, subelement, mode) in [
            ("Foreign", "", "FlatFace"),
            ("Axis", "", "FlatFace"),
            ("Datum", "", "ObjectXY"),
            ("Profile", "", "FlatFace"),
            ("Pad", "Face0", "FlatFace"),
            ("Pad", "Face7", "FlatFace"),
        ] {
            assert!(build(&state(), &request(json!({"kind":"reference","object":source,"subelements":[subelement],"mode":mode}))).is_err());
        }
    }

    #[test]
    fn creation_requires_current_names_roles_and_an_idle_document() {
        let valid = request(json!({"kind":"origin","role":"XY_Plane"}));
        for (field, value) in [
            ("name", json!("Pad")),
            ("name", json!("Body")),
            ("body", json!("Missing")),
            ("create_body", json!(true)),
            ("extra", json!(1)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(build(&state(), &invalid).is_err());
        }
        assert!(
            build(
                &state(),
                &request(json!({"kind":"origin","role":"XY_Plane9"}))
            )
            .is_err()
        );
        for field in ["preview", "sketch_edit"] {
            let mut busy = state();
            busy[field] = json!({"token":"another-native-edit"});
            assert!(build(&busy, &valid).is_err());
        }
        let mut unavailable = state();
        unavailable["available"] = json!(false);
        assert!(build(&unavailable, &valid).is_err());
        let mut wrong_owner = state();
        wrong_owner["objects"][2]["body"] = json!("OtherBody");
        assert!(build(&wrong_owner, &valid).is_err());
    }
}
