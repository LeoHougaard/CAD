//! Hole task presentation. FreeCAD owns enum domains, presets, units,
//! expressions, read-only write admission and every recomputation.
use serde_json::{Value, json};

/// Do not submit an earlier literal override after its task controller has
/// made it inactive/computed. Expressions and all unlisted properties still
/// pass through native write admission unchanged.
pub fn active_parameters(schema: &Value, parameters: &Value) -> Value {
    if parameters["type"] != "PartDesign::Hole"
        || schema["type"] != "PartDesign::Hole"
        || schema["property_metadata"]["ThreadType"]["hole_task_version"] != 1
    {
        return parameters.clone();
    }
    let inactive = inactive_literals(schema, parameters);
    let mut result = parameters.clone();
    if let Some(properties) = result["properties"].as_object_mut() {
        properties.retain(|field, _| !inactive.iter().any(|name| name == field));
    }
    result
}

fn literal_input(field: &str) -> bool {
    matches!(
        field,
        "Diameter"
            | "Threaded"
            | "ThreadSize"
            | "ThreadFit"
            | "ThreadClass"
            | "ThreadDirection"
            | "ModelThread"
            | "UseCustomThreadClearance"
            | "CustomThreadClearance"
            | "ThreadDepthType"
            | "ThreadDepth"
            | "Depth"
            | "DrillPoint"
            | "DrillPointAngle"
            | "DrillForDepth"
            | "TaperedAngle"
            | "HoleCutCustomValues"
            | "HoleCutDiameter"
            | "HoleCutDepth"
            | "HoleCutCountersinkAngle"
    )
}

/// Private UI provenance. Only an enabled native Hole input widget records a
/// literal. This never authorizes a native write or evaluates an expression.
pub fn remember_literal(schema: &Value, parameters: &mut Value, field: &str) {
    if schema["type"] == "PartDesign::Hole"
        && parameters["type"] == "PartDesign::Hole"
        && schema["property_metadata"]["ThreadType"]["hole_task_version"] == 1
        && literal_input(field)
        && schema["property_metadata"][field]["task_editable"] == true
    {
        if parameters["hole_task_literals"].is_null() {
            parameters["hole_task_literals"] = json!({"version":1,"fields":{}});
        }
        if parameters["hole_task_literals"]["version"] != 1
            || !parameters["hole_task_literals"]["fields"].is_object()
        {
            return;
        }
        parameters["hole_task_literals"]["fields"][field] = json!(true);
    }
}

/// Keep dormant draft values separate from the literal writes submitted now.
pub fn inactive_literals(schema: &Value, parameters: &Value) -> Vec<String> {
    if parameters["type"] != "PartDesign::Hole"
        || schema["type"] != "PartDesign::Hole"
        || schema["property_metadata"]["ThreadType"]["hole_task_version"] != 1
    {
        return Vec::new();
    }
    let task = task_schema(schema, parameters);
    let baseline = task_schema(schema, &json!({"type":"PartDesign::Hole"}));
    parameters["properties"]
        .as_object()
        .into_iter()
        .flat_map(|p| p.keys())
        .filter(|field| {
            let remembered = parameters["hole_task_literals"]["version"] == 1
                && parameters["hole_task_literals"]["fields"][*field] == true;
            literal_input(field)
                && task["property_metadata"][*field]["task_editable"] == false
                && (remembered || baseline["property_metadata"][*field]["task_editable"] == true)
        })
        .cloned()
        .collect()
}

pub fn task_schema(schema: &Value, parameters: &Value) -> Value {
    if parameters["type"] != "PartDesign::Hole"
        || schema["type"] != "PartDesign::Hole"
        || schema["property_metadata"]["ThreadType"]["hole_task_version"] != 1
    {
        return schema.clone();
    }
    let mut result = schema.clone();
    if let Some(changes) = parameters["properties"].as_object() {
        for (name, value) in changes {
            result["properties"][name] = value.clone();
        }
    }
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
    let standard = result["properties"]["ThreadType"]
        .as_str()
        .unwrap_or("None")
        .to_owned();
    // These domains are collected by native setters before requests begin.
    // Never substitute guessed ISO/UNC/BSP names or a union of invalid choices.
    if !controlled("ThreadType")
        && let Some(domains) = schema["property_metadata"]["ThreadType"]["hole_task_enumerations"]
            [&standard]
            .as_object()
    {
        for (field, choices) in domains {
            result["property_metadata"][field]["enumerations"] = choices.clone();
        }
    }
    let possible =
        |field: &str, value: &Value| controlled(field) || result["properties"][field] == *value;
    let has_standard = standard != "None" || controlled("ThreadType");
    let threaded = has_standard && possible("Threaded", &json!(true));
    let clearance = threaded
        && possible("ModelThread", &json!(true))
        && possible("UseCustomThreadClearance", &json!(true));
    let dimension = possible("DepthType", &json!("Dimension"));
    let point = dimension && possible("DrillPoint", &json!("Angled"));
    let cut = result["properties"]["HoleCutType"]
        .as_str()
        .unwrap_or("None");
    let cut_possible = cut != "None" || controlled("HoleCutType");
    let simple_cut = matches!(cut, "Counterbore" | "Countersink" | "Counterdrill");
    let custom_cut = simple_cut || possible("HoleCutCustomValues", &json!(true));
    let angle = matches!(cut, "Countersink" | "Counterdrill")
        || controlled("HoleCutType")
        || !simple_cut
            && schema["property_metadata"]["ThreadType"]["hole_task_cut_angles"][&standard][cut]
                == true;
    let forced_custom = ["ThreadType", "ThreadSize", "HoleCutType"]
        .iter()
        .all(|field| {
            !controlled(field) && result["properties"][field] == schema["properties"][field]
        })
        && schema["properties"]["HoleCutCustomValues"] == true
        && schema["property_metadata"]["HoleCutCustomValues"]["read_only"] == true;
    // Only native TaskHoleParameters input properties participate. In
    // particular ThreadDiameter, Shape, BaseFeature and other computed data
    // retain their native metadata. Final setters still enforce native locks.
    let availability = [
        ("Diameter", standard == "None" || controlled("ThreadType")),
        ("Threaded", has_standard),
        ("ThreadSize", has_standard),
        (
            "ThreadFit",
            has_standard && possible("Threaded", &json!(false)),
        ),
        (
            "ThreadClass",
            threaded
                && (!clearance
                    || controlled("UseCustomThreadClearance")
                    || controlled("ModelThread")),
        ),
        ("ThreadDirection", threaded),
        ("ModelThread", threaded),
        (
            "UseCustomThreadClearance",
            threaded && possible("ModelThread", &json!(true)),
        ),
        ("CustomThreadClearance", clearance),
        ("ThreadDepthType", threaded),
        (
            "ThreadDepth",
            threaded && possible("ThreadDepthType", &json!("Dimension")),
        ),
        ("Depth", dimension),
        ("DrillPoint", dimension),
        ("DrillPointAngle", point),
        ("DrillForDepth", point),
        ("TaperedAngle", possible("Tapered", &json!(true))),
        ("HoleCutCustomValues", cut_possible && !forced_custom),
        ("HoleCutDiameter", cut_possible && custom_cut),
        ("HoleCutDepth", cut_possible && custom_cut),
        (
            "HoleCutCountersinkAngle",
            cut_possible && angle && custom_cut,
        ),
    ];
    for (name, active) in availability {
        if schema["property_types"].get(name).is_none() {
            continue;
        }
        let metadata = &mut result["property_metadata"][name];
        metadata["task_editable"] = json!(active);
        metadata["read_only"] = json!(!active);
        metadata["hidden"] = json!(!active);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/hole_schema.json")).unwrap()
    }
    fn task(properties: Value, expressions: Value) -> Value {
        json!({"type":"PartDesign::Hole","properties":properties,"expressions":expressions})
    }
    fn writable(schema: &Value, name: &str) -> bool {
        crate::property_commands::writable(schema, name)
    }
    #[test]
    fn version_and_feature_guards_preserve_native_schema() {
        let original = schema();
        let mut unknown = original.clone();
        unknown["property_metadata"]["ThreadType"]["hole_task_version"] = json!(2);
        assert_eq!(
            task_schema(
                &unknown,
                &task(json!({"HoleCutType":"Counterbore"}), json!({}))
            ),
            unknown
        );
        assert_eq!(
            task_schema(&original, &json!({"type":"PartDesign::Pad"})),
            original
        );
    }
    #[test]
    fn rejected_cut_repair_preserves_computed_and_standard_locks() {
        let native = schema();
        let counterbore = task_schema(
            &native,
            &task(json!({"HoleCutType":"Counterbore"}), json!({})),
        );
        assert!(writable(&counterbore, "HoleCutDepth"));
        assert!(writable(&counterbore, "HoleCutDiameter"));
        assert!(!writable(&counterbore, "HoleCutCountersinkAngle"));
        for field in ["Shape", "ThreadDiameter", "Diameter"] {
            assert!(!writable(&counterbore, field), "{field}");
        }
        for field in ["Shape", "ThreadDiameter", "BaseFeature"] {
            assert_eq!(
                counterbore["property_metadata"][field],
                native["property_metadata"][field]
            );
        }
        let preset = task_schema(&native, &task(json!({"HoleCutType":"ISO 4762"}), json!({})));
        assert!(!writable(&preset, "HoleCutDepth"));
        let sink = task_schema(
            &native,
            &task(
                json!({"HoleCutType":"ISO 10642","HoleCutCustomValues":true}),
                json!({}),
            ),
        );
        assert!(writable(&sink, "HoleCutCountersinkAngle"));
    }
    #[test]
    fn standards_use_captured_native_domains_without_geometry_or_guessed_enums() {
        let native = schema();
        for (standard, domain) in
            native["property_metadata"]["ThreadType"]["hole_task_enumerations"]
                .as_object()
                .unwrap()
        {
            let presented = task_schema(&native, &task(json!({"ThreadType":standard}), json!({})));
            for (field, choices) in domain.as_object().unwrap() {
                assert_eq!(
                    presented["property_metadata"][field]["enumerations"],
                    *choices
                );
            }
        }
    }
    #[test]
    fn controller_expressions_keep_repair_fields_available_until_native_resolution() {
        let native = schema();
        let params = task(
            json!({"DepthType":"ThroughAll","DrillPoint":"Flat"}),
            json!({"DepthType":"0","DrillPoint":"1","Threaded":"1","ThreadDepthType":"1","Tapered":"1"}),
        );
        let pending = task_schema(&native, &params);
        for field in [
            "Depth",
            "DrillPointAngle",
            "DrillForDepth",
            "ThreadDepth",
            "TaperedAngle",
        ] {
            assert!(writable(&pending, field), "{field}");
        }
        let cleared = task_schema(
            &native,
            &task(
                json!({"DepthType":"ThroughAll","DrillPoint":"Flat"}),
                json!({"DepthType":null,"DrillPoint":null}),
            ),
        );
        assert!(!writable(&cleared, "Depth"));
        assert!(!writable(&cleared, "DrillPointAngle"));
    }
    #[test]
    fn native_controller_expressions_precede_dependent_values() {
        let edits=crate::property_commands::build(&json!({"type":"PartDesign::Hole","name":"Hole","create":false,
            "properties":{"ThreadDepth":"5 mm"},"expressions":{"Threaded":"1","ThreadDepthType":"1"}})).unwrap();
        let at = |name| edits.iter().position(|e| e["property"] == name).unwrap();
        assert!(at("Threaded") < at("ThreadDepthType"));
        assert!(at("ThreadDepthType") < at("ThreadDepth"));
    }

    #[test]
    fn inactive_literal_projection_preserves_native_protection_and_expression_admission() {
        let native = schema();
        let original = task(
            json!({"DepthType":"ThroughAll","Depth":"5 mm","Shape":7}),
            json!({"Depth":"Base.Height / 2"}),
        );
        let candidate = active_parameters(&native, &original);
        assert!(candidate["properties"].get("Depth").is_none());
        assert_eq!(candidate["properties"]["Shape"], 7);
        assert_eq!(candidate["expressions"], original["expressions"]);
        assert_eq!(original["properties"]["Depth"], "5 mm");
    }

    #[test]
    fn forced_custom_mode_keeps_native_readonly_while_dimensions_remain_available() {
        let native: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/hole_missing_preset_schema.json"
        ))
        .unwrap();
        let shown = task_schema(&native, &task(json!({}), json!({})));
        assert_eq!(shown["properties"]["HoleCutCustomValues"], true);
        assert!(!writable(&shown, "HoleCutCustomValues"));
        assert!(writable(&shown, "HoleCutDepth"));
        assert!(writable(&shown, "HoleCutDiameter"));
        assert!(!writable(&shown, "Shape"));
        assert!(!writable(&shown, "ThreadDiameter"));
    }

    #[test]
    fn widget_literal_provenance_survives_controller_round_trip() {
        let native = schema();
        let mut p = task(
            json!({"HoleCutType":"Counterbore","HoleCutCustomValues":true}),
            json!({}),
        );
        let shown = task_schema(&native, &p);
        remember_literal(&shown, &mut p, "HoleCutDepth");
        p["properties"]["HoleCutDepth"] = json!("0.25 mm");
        remember_literal(&shown, &mut p, "HoleCutDiameter");
        p["properties"]["HoleCutDiameter"] = json!("5.5 mm");
        p["properties"]["HoleCutType"] = json!("None");
        let inactive = active_parameters(&native, &p);
        assert!(inactive["properties"].get("HoleCutDepth").is_none());
        assert!(inactive["properties"].get("HoleCutDiameter").is_none());
        assert_eq!(p["properties"]["HoleCutDepth"], "0.25 mm");
        p["properties"]["HoleCutType"] = json!("Counterbore");
        assert_eq!(
            active_parameters(&native, &p)["properties"]["HoleCutDepth"],
            "0.25 mm"
        );
    }

    #[test]
    fn provenance_is_bounded_and_does_not_hide_computed_writes() {
        let native = schema();
        let mut p = task(
            json!({"Shape":7,"ThreadDiameter":7,"Diameter":7}),
            json!({}),
        );
        for field in ["Shape", "ThreadDiameter", "Diameter"] {
            remember_literal(&native, &mut p, field);
        }
        assert!(p.get("hole_task_literals").is_none());
        assert_eq!(active_parameters(&native, &p), p);
        p["hole_task_literals"] =
            json!({"version":1,"fields":{"Shape":true,"ThreadDiameter":true}});
        assert_eq!(
            active_parameters(&native, &p)["properties"],
            p["properties"]
        );
        p["hole_task_literals"] = json!({"version":2,"fields":{"HoleCutDepth":true}});
        p["properties"]["HoleCutDepth"] = json!("1 mm");
        assert_eq!(
            active_parameters(&native, &p)["properties"]["HoleCutDepth"],
            "1 mm"
        );
    }
}
