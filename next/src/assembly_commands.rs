//! Assembly UI and agent commands share native object and reference identities.
//! FreeCAD's Assembly module owns connector frames, solving and history.
use serde_json::{Value, json};

pub const JOINT_TYPES: &[&str] = &[
    "Fixed",
    "Revolute",
    "Cylindrical",
    "Slider",
    "Ball",
    "Distance",
    "Parallel",
    "Perpendicular",
    "Angle",
    "RackPinion",
    "Screw",
    "Gears",
    "Belt",
];

pub fn objects(state: &Value) -> &[Value] {
    state["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn name(value: &Value) -> Result<&str, String> {
    let value = value.as_str().ok_or("Choose a native object name")?;
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Native names must be ASCII identifiers".into());
    }
    Ok(value)
}

fn object<'a>(state: &'a Value, value: &Value) -> Result<&'a Value, String> {
    let key = name(value)?;
    objects(state)
        .iter()
        .find(|o| o["name"] == key)
        .ok_or_else(|| format!("Native object {key} is absent; refresh the document"))
}

fn fresh(state: &Value, value: &Value) -> Result<(), String> {
    let key = name(value)?;
    if objects(state).iter().any(|o| o["name"] == key) {
        return Err(format!("Object name already exists: {key}"));
    }
    Ok(())
}

fn instance<'a>(state: &'a Value, value: &Value, assembly: &Value) -> Result<&'a Value, String> {
    let obj = object(state, value)?;
    if obj["type"] != "App::Link" || obj["assembly"] != *assembly {
        return Err("Choose a native component instance in this assembly".into());
    }
    Ok(obj)
}

fn placement(value: &Value) -> Result<(), String> {
    let p = value["position"]
        .as_array()
        .filter(|p| p.len() == 3)
        .ok_or("Placement requires three position values")?;
    let q = value["rotation"]
        .as_array()
        .filter(|p| p.len() == 4)
        .ok_or("Placement requires four quaternion values")?;
    if p.iter().any(|v| !crate::document_units::scalar(v))
        || q.iter().any(|v| !v.as_f64().is_some_and(f64::is_finite))
        || q.iter().map(|v| v.as_f64().unwrap().powi(2)).sum::<f64>() < 1e-24
    {
        return Err("Placement requires finite values and a nonzero quaternion".into());
    }
    Ok(())
}

/// Compile against a captured snapshot. Submit through the ordinary guarded
/// transaction/preview boundary. Native validation remains authoritative.
pub fn build(state: &Value, request: &Value) -> Result<Vec<Value>, String> {
    let recovery_relink = state["recovery_only"] == true
        && state["recovery_available"] == true
        && state["save_recovery"]["dirty"] == true
        && state["save_recovery"]["live_graph_restored"] == false
        && request["op"] == "assembly_relink"
        && state["save_recovery"]["pending_edges"]
            .as_array()
            .is_some_and(|edges| {
                edges.iter().any(|edge| {
                    edge["name"] == request["object"]
                        && edge["native_id"].as_u64().is_some_and(|id| id > 0)
                        && edge["source"] == request["external_source"]
                })
            });
    if state["available"] == false && !recovery_relink || !state["objects"].is_array() {
        return Err("Read the available native document before authoring an assembly".into());
    }
    let fields = request
        .as_object()
        .ok_or("Assembly request must be an object")?;
    let op = request["op"]
        .as_str()
        .ok_or("Choose an assembly operation")?;
    if state["recovery_only"] == true && op != "assembly_relink" {
        return Err(
            "Recover native links through Save or Update / relink external source first".into(),
        );
    }
    let allowed: &[&str] = match op {
        "assembly_create" => &["op", "name", "joints"],
        "assembly_insert" => &[
            "op",
            "assembly",
            "name",
            "source",
            "external_source",
            "placement",
        ],
        "assembly_relink" => &[
            "op",
            "assembly",
            "object",
            "external_source",
            "solve_joints",
        ],
        "assembly_placement" => &["op", "assembly", "object", "placement"],
        "assembly_ground" => &["op", "assembly", "object", "name", "grounded"],
        "assembly_joint" => &[
            "op",
            "assembly",
            "name",
            "create",
            "joint_type",
            "reference1",
            "reference2",
            "properties",
            "expressions",
        ],
        "assembly_remove" => &["op", "assembly", "object"],
        "assembly_solve" => &["op", "assembly"],
        _ => return Err("Unknown native assembly operation".into()),
    };
    if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unknown assembly request field".into());
    }
    if op == "assembly_create" {
        fresh(state, &request["name"])?;
        fresh(state, &request["joints"])?;
        if request["name"] == request["joints"] {
            return Err("Assembly and JointGroup require different names".into());
        }
        return Ok(vec![request.clone()]);
    }
    let assembly = &request["assembly"];
    if object(state, assembly)?["type"] != "Assembly::AssemblyObject" {
        return Err("Choose a native Assembly container".into());
    }
    match op {
        "assembly_insert" => {
            fresh(state, &request["name"])?;
            if fields.contains_key("external_source") {
                external_source(&request["external_source"])?;
                if state["path"].as_str().is_none_or(str::is_empty) {
                    return Err("Save as the root assembly FCStd before external insertion".into());
                }
                if let Some(value) = fields.get("placement") {
                    placement(value)?;
                }
                return Ok(vec![request.clone()]);
            }
            let source = object(state, &request["source"])?;
            let source = if source["type"] == "App::Link" {
                if source["instance_state"]["local"] != true {
                    return Err("External component authoring is unavailable".into());
                }
                object(state, &source["instance_state"]["source"])?
            } else {
                source
            };
            if source["type"] != "PartDesign::Body"
                && !source["type"]
                    .as_str()
                    .is_some_and(|t| t.starts_with("Part::"))
            {
                return Err("Choose a local native Part feature or Body".into());
            }
            if let Some(value) = fields.get("placement") {
                placement(value)?;
            }
        }
        "assembly_relink" => {
            let instance = instance(state, &request["object"], assembly)?;
            external_source(&request["external_source"])?;
            let old = &instance["instance_state"]["external_source"];
            for key in ["document_uid", "object", "object_id", "type"] {
                if old[key] != request["external_source"][key] {
                    return Err(
                        "Relink requires the same native document UUID and object ID".into(),
                    );
                }
            }
            if request["solve_joints"] != true {
                return Err("Independent source update without solving is unavailable in the pinned native runtime".into());
            }
        }
        "assembly_placement" => {
            instance(state, &request["object"], assembly)?;
            placement(&request["placement"])?;
        }
        "assembly_ground" => {
            instance(state, &request["object"], assembly)?;
            if !request["grounded"].is_boolean() {
                return Err("Choose Ground or Unground".into());
            }
            if request["grounded"] == true {
                fresh(state, &request["name"])?;
            }
        }
        "assembly_joint" => {
            if request["create"] == true {
                fresh(state, &request["name"])?;
            } else if request["create"] == false {
                let joint = object(state, &request["name"])?;
                if joint["assembly"] != *assembly || !joint["properties"]["JointType"].is_string() {
                    return Err("Choose a native joint in this assembly".into());
                }
            } else {
                return Err("Specify whether the joint is new".into());
            }
            if !request["joint_type"]
                .as_str()
                .is_some_and(|t| JOINT_TYPES.contains(&t))
            {
                return Err("Choose a pinned native joint type".into());
            }
            for key in ["reference1", "reference2"] {
                let r = &request[key];
                instance(state, &r["link"], assembly)?;
                let subs = r["subelements"]
                    .as_array()
                    .filter(|s| s.len() == 2)
                    .ok_or("Choose the connector element and vertex or center")?;
                if subs.iter().any(|v| !v.is_string()) || (subs[0] == "") != (subs[1] == "") {
                    return Err(
                        "Choose both connector elements, or the whole component origin".into(),
                    );
                }
            }
            if request["reference1"]["link"] == request["reference2"]["link"] {
                return Err("Choose two distinct component instances".into());
            }
        }
        "assembly_remove" => {
            let obj = object(state, &request["object"])?;
            if obj["assembly"] != *assembly || obj["type"] == "Assembly::JointGroup" {
                return Err("Choose an assembly component or joint".into());
            }
        }
        _ => (),
    }
    Ok(vec![request.clone()])
}

pub fn identity_placement() -> Value {
    json!({"position":[0.,0.,0.],"rotation":[0.,0.,0.,1.]})
}

fn external_source(value: &Value) -> Result<(), String> {
    if value["version"] != 1
        || !value["object_id"].is_u64()
        || ["path", "document_uid", "object", "type"]
            .iter()
            .any(|key| value[key].as_str().is_none_or(str::is_empty))
        || !value["sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Load a source FCStd and select its native object before Apply".into());
    }
    name(&value["object"])?;
    let revisions = value["dependencies"]
        .as_array()
        .ok_or("Load the complete native dependency revisions before Apply")?;
    if value.get("dependency_count").is_some()
        && value["dependency_count"].as_u64() != Some(revisions.len() as u64)
    {
        return Err("Native dependency count differs from its complete revision list; load the source again".into());
    }
    let mut paths = std::collections::HashSet::new();
    for revision in revisions {
        if !revision.as_object().is_some_and(|o| o.len() == 3)
            || ["path", "document_uid"]
                .iter()
                .any(|key| revision[key].as_str().is_none_or(str::is_empty))
            || !revision["sha256"]
                .as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid native dependency revision".into());
        }
        let normalize = |path: &str| {
            if cfg!(target_os = "windows") {
                path.replace('\\', "/").to_ascii_lowercase()
            } else {
                path.to_owned()
            }
        };
        let path = normalize(revision["path"].as_str().unwrap());
        if path == normalize(value["path"].as_str().unwrap()) || !paths.insert(path) {
            return Err("Duplicate native dependency identity".into());
        }
    }
    Ok(())
}
