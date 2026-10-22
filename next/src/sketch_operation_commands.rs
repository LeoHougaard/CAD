//! Native Sketcher task commands. Geometry, units and solving remain native.
use crate::sketch_commands::SketchElement;
use serde_json::{Value, json};

fn finite_vector(value: &Value) -> bool {
    value.as_array().is_some_and(|v| {
        v.len() == 3
            && v.iter().all(|n| n.as_f64().is_some_and(f64::is_finite))
            && v[2].as_f64().is_some_and(|z| z.abs() < 1e-9)
    })
}
fn scalar(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
        || value.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn integer(value: &Value, min: i64, max: i64) -> bool {
    value.as_i64().is_some_and(|n| (min..=max).contains(&n))
}
fn geometry(snapshot: &Value, index: i32) -> Option<&Value> {
    snapshot["sketch"]["geometry"]
        .as_array()?
        .iter()
        .find(|g| g["index"].as_i64() == Some(index as i64))
}

/// Compile against the native snapshot and selections captured when the task
/// began. Replacing a preview must use that baseline, not its generated IDs.
pub fn build(
    snapshot: &Value,
    kind: &str,
    selection: &[SketchElement],
    parameters: &Value,
) -> Result<Vec<Value>, String> {
    let object = snapshot["name"]
        .as_str()
        .filter(|name| {
            !name.is_empty()
                && name.chars().enumerate().all(|(i, c)| {
                    c.is_ascii_alphabetic() || c == '_' || i > 0 && c.is_ascii_digit()
                })
        })
        .ok_or("Choose a native sketch")?;
    if !snapshot["sketch"]["geometry"].is_array() {
        return Err("Choose a native sketch".into());
    }
    let params = parameters
        .as_object()
        .ok_or("Task parameters must be an object")?;
    let allowed: &[&str] = match kind {
        "fillet" | "chamfer" => &["radius", "trim", "preserve_corner", "positions"],
        "split" => &["position"],
        "extend" => &["increment"],
        "carbon_copy" => &["source", "construction"],
        "spline_degree" => &["change"],
        "spline_knot" => &["parameter", "multiplicity", "knot", "change"],
        "spline_pole" => &["pole", "position"],
        "spline_weight" => &["pole", "value", "release_constraints"],
        _ => return Err("Unsupported native sketch task".into()),
    };
    if params.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unsupported task parameter".into());
    }
    let native = match kind {
        "fillet" | "chamfer" => "fillet_geometry",
        "split" => "split_geometry",
        "extend" => "extend_geometry",
        other => other,
    };
    let mut edit = json!({"op":native,"object":object});
    for (key, value) in params {
        edit[key] = value.clone();
    }
    if kind == "carbon_copy" {
        if parameters["source"]
            .as_str()
            .is_none_or(|source| source.is_empty() || source == object)
            || parameters
                .get("construction")
                .is_some_and(|v| !v.is_boolean())
        {
            return Err("Choose another native sketch and a construction state".into());
        }
        return Ok(vec![edit]);
    }
    if selection.is_empty()
        || selection
            .iter()
            .any(|s| s.index < 0 || geometry(snapshot, s.index).is_none())
    {
        return Err("Select internal native sketch geometry".into());
    }
    if matches!(kind, "fillet" | "chamfer") {
        if !scalar(&parameters["radius"])
            || ["trim", "preserve_corner"]
                .iter()
                .any(|key| parameters.get(key).is_some_and(|v| !v.is_boolean()))
        {
            return Err("Enter a native radius quantity and corner options".into());
        }
        edit["chamfer"] = json!(kind == "chamfer");
        if selection.len() == 1 && matches!(selection[0].point, 1 | 2) {
            edit["index"] = json!(selection[0].index);
            edit["point"] = json!(selection[0].point);
        } else if selection.len() == 2
            && selection[0].index != selection[1].index
            && selection.iter().all(|s| s.point == 0)
        {
            if !parameters["positions"].as_array().is_some_and(|positions| {
                positions.len() == 2 && positions.iter().all(finite_vector)
            }) {
                return Err("Select two curves with their sketch-local pick positions".into());
            }
            edit["indices"] = json!([selection[0].index, selection[1].index]);
        } else {
            return Err("Select one corner endpoint or two whole curves".into());
        }
        // Single-corner native fillet has no positions argument.
        if selection.len() == 1 {
            edit.as_object_mut().unwrap().remove("positions");
        }
        return Ok(vec![edit]);
    }
    if selection.len() != 1 {
        return Err("Select one native curve".into());
    }
    let selected = selection[0];
    let curve = geometry(snapshot, selected.index).unwrap();
    edit["index"] = json!(selected.index);
    match kind {
        "split" if !finite_vector(&parameters["position"]) => {
            return Err("Pick a sketch-local split position".into());
        }
        "extend" => {
            if !matches!(selected.point, 1 | 2)
                || !["line", "arc"].contains(&curve["kind"].as_str().unwrap_or_default())
                || !scalar(&parameters["increment"])
            {
                return Err(
                    "Select a line or circular arc endpoint and enter an extension quantity".into(),
                );
            }
            edit["point"] = json!(selected.point);
        }
        "spline_degree" | "spline_knot" | "spline_pole" | "spline_weight" => {
            if curve["kind"] != "bspline" {
                return Err("Select a native B-spline".into());
            }
            match kind {
                "spline_degree" => {
                    if !integer(&parameters["change"], -24, 24) || parameters["change"] == 0 {
                        return Err("Enter a nonzero native degree change".into());
                    }
                }
                "spline_knot" => {
                    if parameters.get("parameter").is_some() {
                        if !parameters["parameter"].as_f64().is_some_and(f64::is_finite)
                            || parameters.get("knot").is_some()
                            || parameters.get("change").is_some()
                            || parameters
                                .get("multiplicity")
                                .is_some_and(|v| !integer(v, 1, 25))
                        {
                            return Err("Enter a native knot parameter and multiplicity".into());
                        }
                    } else if !integer(&parameters["knot"], 1, i32::MAX as i64)
                        || !integer(&parameters["change"], -25, 25)
                        || parameters["change"] == 0
                        || parameters.get("multiplicity").is_some()
                    {
                        return Err(
                            "Choose a native one-based knot and nonzero multiplicity change".into(),
                        );
                    }
                }
                "spline_pole" | "spline_weight" => {
                    let count = curve["poles"].as_array().map_or(0, Vec::len);
                    if count == 0 || !integer(&parameters["pole"], 0, count as i64 - 1) {
                        return Err("Choose a native zero-based spline pole".into());
                    }
                    if kind == "spline_pole" && !finite_vector(&parameters["position"]) {
                        return Err("Enter a sketch-local pole position".into());
                    }
                    if kind == "spline_weight" {
                        if !scalar(&parameters["value"]) {
                            return Err("Enter a native dimensionless weight".into());
                        }
                        if let Some(releases) = parameters.get("release_constraints") {
                            let indices = releases
                                .as_array()
                                .ok_or("Choose explicit native Equal constraints to release")?;
                            let mut unique = std::collections::BTreeSet::new();
                            for value in indices {
                                let index = value
                                    .as_u64()
                                    .filter(|n| *n <= i32::MAX as u64)
                                    .ok_or("Invalid native constraint index")?;
                                if !unique.insert(index) {
                                    return Err("Release each native constraint once".into());
                                }
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        _ => {}
    }
    Ok(vec![edit])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Value {
        json!({"name":"Sketch","sketch":{"geometry":[{"index":0,"kind":"line"},{"index":1,"kind":"arc"},{"index":2,"kind":"bspline","poles":[[0,0,0],[1,2,0],[3,0,0]]}]}})
    }
    fn select(index: i32, point: i32) -> Vec<SketchElement> {
        vec![SketchElement { index, point }]
    }
    #[test]
    fn native_task_indices_units_and_explicit_releases_survive_compilation() {
        let s = snapshot();
        let f = build(
            &s,
            "chamfer",
            &select(0, 2),
            &json!({"radius":"2 mm","trim":true}),
        )
        .unwrap();
        assert_eq!(
            f[0],
            json!({"op":"fillet_geometry","object":"Sketch","index":0,"point":2,"radius":"2 mm","trim":true,"chamfer":true})
        );
        assert_eq!(
            build(&s, "extend", &select(1, 1), &json!({"increment":"-30 deg"})).unwrap()[0]["increment"],
            "-30 deg"
        );
        let p = json!({"pole":1,"value":"0.75","release_constraints":[12,14]});
        assert_eq!(
            build(&s, "spline_weight", &select(2, 0), &p).unwrap()[0]["release_constraints"],
            json!([12, 14])
        );
    }
    #[test]
    fn stale_external_and_ambiguous_tasks_are_rejected_without_model_writes() {
        let s = snapshot();
        assert!(build(&s, "extend", &select(-3, 1), &json!({"increment":1})).is_err());
        assert!(build(&s, "fillet", &select(0, 0), &json!({"radius":1})).is_err());
        assert!(
            build(
                &s,
                "spline_pole",
                &select(2, 0),
                &json!({"pole":3,"position":[0,0,0]})
            )
            .is_err()
        );
        assert!(
            build(
                &s,
                "spline_knot",
                &select(2, 0),
                &json!({"parameter":0.5,"knot":1,"change":1})
            )
            .is_err()
        );
        assert!(build(&s, "carbon_copy", &[], &json!({"source":"Sketch"})).is_err());
        assert!(build(&s, "spline_degree", &select(2, 0), &json!({"change":0})).is_err());
        assert!(
            build(
                &s,
                "spline_weight",
                &select(2, 0),
                &json!({"pole":0,"value":1,"release_constraints":[4,4]})
            )
            .is_err()
        );
    }
}
