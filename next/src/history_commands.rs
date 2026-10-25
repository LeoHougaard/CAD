//! Compile native Body history actions without copying or rewriting its graph.
//! Native metadata decides eligibility; the worker owns recomputation, dependency
//! validation, display changes and atomic rollback under the caller's revision.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn name<'a>(value: &'a Value, label: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .filter(|value| identifier(value))
        .ok_or_else(|| format!("Choose a native {label} name"))
}

struct Snapshot<'a> {
    objects: BTreeMap<&'a str, &'a Value>,
}
impl<'a> Snapshot<'a> {
    fn read(state: &'a Value) -> Result<Self, String> {
        if state["available"] == false {
            return Err(
                "The native document is unavailable; reopen it before editing history".into(),
            );
        }
        let entries = state["objects"]
            .as_array()
            .ok_or("Read the native document before editing history")?;
        let mut objects = BTreeMap::new();
        for entry in entries {
            let key = name(&entry["name"], "object")?;
            if objects.insert(key, entry).is_some() {
                return Err("The snapshot contains duplicate native object names".into());
            }
        }
        Ok(Self { objects })
    }

    fn object(&self, name: &str) -> Result<&'a Value, String> {
        self.objects
            .get(name)
            .copied()
            .ok_or_else(|| format!("Native object {name} is absent; refresh the document"))
    }

    fn body(&self, name: &str) -> Result<&'a Value, String> {
        let body = self.object(name)?;
        if body["type"] != "PartDesign::Body" || !body["body_state"].is_object() {
            return Err("Choose a native PartDesign Body".into());
        }
        Ok(body)
    }

    fn members(&self, body: &Value) -> Result<BTreeSet<&'a str>, String> {
        let group = body["properties"]["Group"]
            .as_array()
            .ok_or("Read the native Body Group before editing history")?;
        let body_name = name(&body["name"], "Body")?;
        let mut members = BTreeSet::new();
        for link in group {
            let member = self.object(name(&link["link"], "Body member")?)?;
            let member_name = name(&member["name"], "Body member")?;
            if member_name == body_name || member["body"] != body_name {
                return Err(
                    "Native Body membership and ownership disagree; refresh the document".into(),
                );
            }
            if !members.insert(member_name) {
                return Err("The snapshot contains duplicate native Body members".into());
            }
        }
        Ok(members)
    }

    fn tip(&self, value: &Value, members: &BTreeSet<&str>) -> Result<(), String> {
        if value.is_null() {
            return Ok(());
        }
        let tip = name(value, "Tip")?;
        if !members.contains(tip) {
            return Err("Choose a Tip belonging to this native Body".into());
        }
        if self.object(tip)?["history_flags"]["can_tip"] != true {
            return Err(
                "This object is not a native Tip candidate; refresh or choose an eligible feature"
                    .into(),
            );
        }
        Ok(())
    }
}

fn required<'a>(request: &'a Map<String, Value>, field: &str) -> Result<&'a Value, String> {
    request
        .get(field)
        .ok_or_else(|| format!("History command requires {field}"))
}

/// Return one dedicated native edit. `body_tip.tip` is required and may be null.
/// For reorder, null target + after=true means beginning; after=false means end.
/// Omitting reorder's tip preserves the native remove/insert Tip result. An
/// explicit tip, including null, requests that final Tip instead. Compile from
/// the task baseline and submit with a revision guard; native validation remains
/// authoritative after links, attachments and expressions recompute.
pub fn build(state: &Value, request: &Value) -> Result<Vec<Value>, String> {
    let request = request
        .as_object()
        .ok_or("History command must be an object")?;
    let op = required(request, "op")?
        .as_str()
        .ok_or("Choose a native history operation")?;
    let allowed: &[&str] = match op {
        "body_tip" => &["op", "object", "tip"],
        "body_reorder" => &["op", "object", "target", "after", "tip"],
        "feature_suppression" => &["op", "object", "suppressed"],
        _ => return Err("Unsupported native history operation".into()),
    };
    if request.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unsupported history command field".into());
    }
    let snapshot = Snapshot::read(state)?;
    let object = name(required(request, "object")?, "object")?;
    if op == "feature_suppression" {
        let feature = snapshot.object(object)?;
        let suppressed = required(request, "suppressed")?
            .as_bool()
            .ok_or("Suppression must be a boolean")?;
        if feature["suppression"]["can_toggle"] != true {
            return Err(feature["suppression"]["reason"]
                .as_str()
                .unwrap_or("Read native suppression availability before editing")
                .to_owned());
        }
        if !feature["properties"]["Suppressed"].is_boolean() {
            return Err("Read the native suppression value before editing".into());
        }
        return Ok(vec![
            json!({"op":"set","object":object,"property":"Suppressed","value":suppressed}),
        ]);
    }
    if op == "body_tip" {
        let body = snapshot.body(object)?;
        let members = snapshot.members(body)?;
        let tip = required(request, "tip")?;
        snapshot.tip(tip, &members)?;
        return Ok(vec![json!({"op":op,"object":object,"tip":tip})]);
    }

    let member = snapshot.object(object)?;
    let body_name = name(&member["body"], "owner Body")?;
    let body = snapshot.body(body_name)?;
    let members = snapshot.members(body)?;
    if !members.contains(object) {
        return Err("Choose a direct member of this native Body".into());
    }
    if member["history_flags"]["can_reorder"] != true {
        return Err("This native Body member cannot be reordered".into());
    }
    let target = required(request, "target")?;
    if !target.is_null() {
        let target_name = name(target, "target member")?;
        if target_name == object {
            return Err("A feature cannot be moved relative to itself".into());
        }
        if !members.contains(target_name) {
            return Err("Choose a target belonging to the same native Body".into());
        }
    }
    let after = required(request, "after")?
        .as_bool()
        .ok_or("History after must be a boolean")?;
    let mut edit = json!({"op":op,"object":object,"target":target,"after":after});
    if let Some(tip) = request.get("tip") {
        snapshot.tip(tip, &members)?;
        edit["tip"] = tip.clone();
    }
    Ok(vec![edit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppression_uses_native_availability_and_boolean_input() {
        let mut state = json!({"available":true,"objects":[{"name":"A",
            "properties":{"Suppressed":false},"suppression":{"can_toggle":true,"reason":null}}]});
        let request = json!({"op":"feature_suppression","object":"A","suppressed":true});
        assert_eq!(
            build(&state, &request).unwrap(),
            [json!({"op":"set","object":"A","property":"Suppressed","value":true})]
        );
        let mut bad = request.clone();
        bad["suppressed"] = json!("true");
        assert!(build(&state, &bad).unwrap_err().contains("boolean"));
        bad = request.clone();
        bad["tip"] = Value::Null;
        assert!(build(&state, &bad).is_err());
        state["objects"][0]["suppression"] =
            json!({"can_toggle":false,"reason":"Original placement unavailable"});
        assert_eq!(
            build(&state, &request).unwrap_err(),
            "Original placement unavailable"
        );
        state["available"] = json!(false);
        assert!(build(&state, &request).unwrap_err().contains("unavailable"));
    }

    fn state() -> Value {
        json!({"available":true,"objects":[
            {"name":"Body","type":"PartDesign::Body","body":"Body",
             "body_state":{"tip":"Finish"},"properties":{"Group":[{"link":"Base"},{"link":"Profile"},{"link":"Finish"},{"link":"Child"}],"BaseFeature":{"link":"Base"}}},
            {"name":"Base","type":"PartDesign::Feature","body":"Body","history_flags":{"can_tip":true,"can_reorder":false,"transform_step":false}},
            {"name":"Profile","type":"Sketcher::SketchObject","body":"Body","history_flags":{"can_tip":false,"can_reorder":true,"transform_step":false}},
            {"name":"Finish","type":"PartDesign::Pad","body":"Body","history_flags":{"can_tip":true,"can_reorder":true,"transform_step":false}},
            {"name":"Child","type":"PartDesign::LinearPattern","body":"Body","history_flags":{"can_tip":false,"can_reorder":true,"transform_step":true}},
            {"name":"OtherBody","type":"PartDesign::Body","body":"OtherBody","body_state":{"tip":"OtherPad"},"properties":{"Group":[{"link":"OtherPad"}]}},
            {"name":"OtherPad","type":"PartDesign::Pad","body":"OtherBody","history_flags":{"can_tip":true,"can_reorder":true,"transform_step":false}}
        ]})
    }

    #[test]
    fn rollback_and_explicit_empty_tip_keep_native_history_intact() {
        let state = state();
        let before = state.clone();
        for tip in [json!("Base"), Value::Null] {
            let request = json!({"op":"body_tip","object":"Body","tip":tip});
            assert_eq!(build(&state, &request).unwrap(), [request]);
        }
        assert_eq!(state, before);
    }

    #[test]
    fn reorder_preserves_native_null_anchor_semantics_and_explicit_tip_choice() {
        let state = state();
        for after in [true, false] {
            let mut request =
                json!({"op":"body_reorder","object":"Finish","target":null,"after":after});
            let edits = build(&state, &request).unwrap();
            assert_eq!(edits, [request.clone()]);
            assert!(edits[0].get("tip").is_none());
            for tip in [json!("Finish"), Value::Null] {
                request["tip"] = tip;
                assert_eq!(build(&state, &request).unwrap(), [request.clone()]);
            }
        }
        // A protected base feature remains a valid insertion anchor.
        let request = json!({"op":"body_reorder","object":"Profile","target":"Base","after":true,"tip":"Finish"});
        assert_eq!(build(&state, &request).unwrap(), [request]);
    }

    #[test]
    fn native_eligibility_controls_tips_and_movement_instead_of_type_prefixes() {
        let mut state = state();
        for object in ["Profile", "Child"] {
            assert!(
                build(
                    &state,
                    &json!({"op":"body_tip","object":"Body","tip":object})
                )
                .is_err()
            );
        }
        assert!(
            build(
                &state,
                &json!({"op":"body_reorder","object":"Base","target":null,"after":true})
            )
            .is_err()
        );
        // The native GUI permits child-step reorder selection. Do not invent a
        // Rust exclusion based on transform_step; final native validation owns it.
        assert!(
            build(
                &state,
                &json!({"op":"body_reorder","object":"Child","target":"Base","after":true})
            )
            .is_ok()
        );
        state["objects"][3]["type"] = json!("Custom::NativeFeature");
        assert!(
            build(
                &state,
                &json!({"op":"body_tip","object":"Body","tip":"Finish"})
            )
            .is_ok()
        );
        state["objects"][3]["history_flags"]
            .as_object_mut()
            .unwrap()
            .remove("can_tip");
        assert!(
            build(
                &state,
                &json!({"op":"body_tip","object":"Body","tip":"Finish"})
            )
            .is_err()
        );
    }

    #[test]
    fn foreign_members_and_stale_native_ownership_are_rejected() {
        let state = state();
        for request in [
            json!({"op":"body_tip","object":"Body","tip":"OtherPad"}),
            json!({"op":"body_reorder","object":"Finish","target":"OtherPad","after":true}),
            json!({"op":"body_reorder","object":"Finish","target":"Base","after":true,"tip":"OtherPad"}),
            json!({"op":"body_reorder","object":"Finish","target":"Finish","after":true}),
        ] {
            assert!(build(&state, &request).is_err(), "{request}");
        }
        let mut stale = state.clone();
        stale["objects"][3]["body"] = json!("OtherBody");
        assert!(
            build(
                &stale,
                &json!({"op":"body_tip","object":"Body","tip":"Finish"})
            )
            .is_err()
        );
        let mut stale = state;
        stale["objects"][0]["properties"]["Group"]
            .as_array_mut()
            .unwrap()
            .remove(2);
        assert!(
            build(
                &stale,
                &json!({"op":"body_reorder","object":"Finish","target":null,"after":true})
            )
            .is_err()
        );
    }

    #[test]
    fn malformed_requests_and_ambiguous_snapshots_do_not_emit_edits() {
        let state = state();
        for request in [
            json!(null),
            json!({"op":"set","object":"Body","tip":null}),
            json!({"op":"body_tip","object":"Body"}),
            json!({"op":"body_tip","object":"Body","tip":null,"group":[]}),
            json!({"op":"body_tip","object":"Body","tip":"Feature.Label"}),
            json!({"op":"body_reorder","object":"Finish","after":true}),
            json!({"op":"body_reorder","object":"Finish","target":null,"after":1}),
            json!({"op":"body_reorder","object":"Finish","target":null,"after":"true"}),
        ] {
            assert!(build(&state, &request).is_err(), "{request}");
        }
        let request = json!({"op":"body_tip","object":"Body","tip":null});
        let mut duplicate = state.clone();
        duplicate["objects"]
            .as_array_mut()
            .unwrap()
            .push(state["objects"][1].clone());
        assert!(build(&duplicate, &request).is_err());
        let mut duplicate = state.clone();
        duplicate["objects"][0]["properties"]["Group"]
            .as_array_mut()
            .unwrap()
            .push(json!({"link":"Base"}));
        assert!(build(&duplicate, &request).is_err());
        let mut unavailable = state;
        unavailable["available"] = json!(false);
        assert!(build(&unavailable, &request).is_err());
    }
}
