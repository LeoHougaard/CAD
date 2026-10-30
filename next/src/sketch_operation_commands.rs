//! Native Sketcher task commands. Geometry, units and solving remain native.
use crate::sketch_commands::SketchElement;
use serde_json::{Value, json};

fn finite_vector(value: &Value) -> bool {
    value.as_array().is_some_and(|v| {
        v.len() == 3
            && v.iter().all(crate::document_units::scalar)
            && v[2].as_f64().is_some_and(|z| z.abs() < 1e-9)
    })
}
fn scalar(value: &Value) -> bool {
    crate::document_units::scalar(value)
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

/// Slot around one selected native line. The original line keeps its geometry
/// ID and construction state; the generated boundary is solver-coupled to it.
pub fn selected_line_slot(
    snapshot: &Value,
    selection: &[SketchElement],
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
    let [selected] = selection else {
        return Err("Select one whole native sketch line".into());
    };
    let line = geometry(snapshot, selected.index)
        .filter(|g| {
            selected.index >= 0
                && selected.point == 0
                && g["kind"] == "line"
                && g["editable"] == true
                && !g["persistent_id"].is_null()
        })
        .ok_or("Select one whole editable native sketch line")?;
    let native_point = |p: &Value| {
        p.as_array().is_some_and(|v| {
            v.len() == 3
                && v.iter().all(|x| x.as_f64().is_some_and(f64::is_finite))
                && v[2].as_f64().is_some_and(|z| z.abs() < 1e-9)
        })
    };
    if !native_point(&line["start"]) || !native_point(&line["end"]) {
        return Err("The selected line has invalid native endpoints".into());
    }
    let a = [
        line["start"][0].as_f64().unwrap(),
        line["start"][1].as_f64().unwrap(),
    ];
    let b = [
        line["end"][0].as_f64().unwrap(),
        line["end"][1].as_f64().unwrap(),
    ];
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= 1e-9 {
        return Err("The selected line has zero length".into());
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy * 10., ux * 10.);
    let point = |p: [f64; 2], x: f64, y: f64| json!([p[0] + x, p[1] + y, 0.]);
    let first = snapshot["sketch"]["geometry_count"]
        .as_u64()
        .and_then(|n| i32::try_from(n).ok())
        .filter(|n| *n <= i32::MAX - 4)
        .ok_or("The native geometry count is unavailable")?;
    if selected.index >= first {
        return Err("The selected line changed; select it again".into());
    }
    let mut edits = vec![
        json!({"op":"geometry","object":object,"kind":"arc","start":point(a,nx,ny),"mid":point(a,-ux*10.,-uy*10.),"end":point(a,-nx,-ny)}),
        json!({"op":"geometry","object":object,"kind":"arc","start":point(b,-nx,-ny),"mid":point(b,ux*10.,uy*10.),"end":point(b,nx,ny)}),
        json!({"op":"geometry","object":object,"kind":"line","start":point(a,nx,ny),"end":point(b,nx,ny)}),
        json!({"op":"geometry","object":object,"kind":"line","start":point(a,-nx,-ny),"end":point(b,-nx,-ny)}),
    ];
    for (kind, args) in [
        ("Tangent", json!([first, 1, first + 2, 1])),
        ("Tangent", json!([first, 2, first + 3, 1])),
        ("Tangent", json!([first + 1, 2, first + 2, 2])),
        ("Tangent", json!([first + 1, 1, first + 3, 2])),
        ("Equal", json!([first, first + 1])),
        ("Coincident", json!([first, 3, selected.index, 1])),
        ("Coincident", json!([first + 1, 3, selected.index, 2])),
        ("Diameter", json!([first, 20.])),
    ] {
        edits.push(json!({"op":"constraint","object":object,"kind":kind,"args":args}));
    }
    Ok(edits)
}

fn mirror_geometry(snapshot: &Value, index: i32) -> Option<&Value> {
    geometry(snapshot, index).or_else(|| {
        snapshot["sketch"]["externals"]
            .as_array()?
            .iter()
            .find(|g| g["geometry_index"].as_i64() == Some(index as i64))
            .map(|g| &g["geometry"])
    })
}

/// Legacy native Copy/Move/array use whole internal curves and standalone
/// points. Endpoints may accompany curves but do not become source geometry.
pub fn copy_sources(snapshot: &Value, selection: &[SketchElement]) -> Result<Vec<i32>, String> {
    let sources = mirror_sources(snapshot, selection)?;
    if sources.iter().any(|index| *index < 0) {
        return Err("Copy, Clone, Move and arrays require internal native geometry".into());
    }
    Ok(sources)
}

/// Match the pinned command's selected-edge/standalone-point extraction.
/// Endpoint and axis picks do not become source geometry when mixed with edges.
pub fn mirror_sources(snapshot: &Value, selection: &[SketchElement]) -> Result<Vec<i32>, String> {
    let mut result = Vec::new();
    for selected in selection {
        if matches!((selected.index, selected.point), (-1, 0 | 1) | (-2, 0)) {
            continue;
        }
        let geo = mirror_geometry(snapshot, selected.index)
            .ok_or("Select native geometry from the captured sketch")?;
        if selected.point == 0 || geo["kind"] == "point" && selected.point == 1 {
            if result.contains(&selected.index) {
                return Err("Select each source geometry once".into());
            }
            result.push(selected.index);
        } else if !geo["handles"].as_array().is_some_and(|handles| {
            handles
                .iter()
                .any(|h| h["point"].as_i64() == Some(selected.point as i64))
        }) {
            return Err("Choose an exposed native sketch selection".into());
        }
    }
    if result.is_empty() {
        return Err("Select whole native geometries or standalone sketch points to mirror".into());
    }
    Ok(result)
}

/// Reference choices carry the native IDs from the captured snapshot.
pub fn mirror_references(snapshot: &Value) -> Vec<(SketchElement, String)> {
    let mut result = vec![
        (
            SketchElement {
                index: -1,
                point: 0,
            },
            "X axis".into(),
        ),
        (
            SketchElement {
                index: -2,
                point: 0,
            },
            "Y axis".into(),
        ),
        (
            SketchElement {
                index: -1,
                point: 1,
            },
            "Origin".into(),
        ),
    ];
    for g in snapshot["sketch"]["geometry"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            snapshot["sketch"]["externals"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| &e["geometry"]),
        )
    {
        let Some(index) = g["index"].as_i64().and_then(|i| i32::try_from(i).ok()) else {
            continue;
        };
        if g["kind"] == "line" {
            result.push((SketchElement { index, point: 0 }, format!("Line {index}")));
        }
        for h in g["handles"].as_array().into_iter().flatten() {
            if let Ok(element) = serde_json::from_value::<SketchElement>(
                json!({"index":h["index"],"point":h["point"]}),
            ) {
                result.push((
                    element,
                    format!("Geometry {} point {}", element.index, element.point),
                ));
            }
        }
    }
    result
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
        "mirror" => &[
            "reference",
            "delete_original",
            "create_symmetry_constraints",
        ],
        "copy" | "clone" | "move" => &["displacement", "all_geometry"],
        "rectangular_array" => &[
            "displacement",
            "all_geometry",
            "columns",
            "rows",
            "clone",
            "constrain_displacement",
            "perpendicular_scale",
        ],
        _ => return Err("Unsupported native sketch task".into()),
    };
    if params.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unsupported task parameter".into());
    }
    let native = match kind {
        "fillet" | "chamfer" => "fillet_geometry",
        "split" => "split_geometry",
        "extend" => "extend_geometry",
        "mirror" => "mirror_geometry",
        "copy" => "copy_geometry",
        "clone" => "clone_geometry",
        "move" => "move_geometries",
        other => other,
    };
    let mut edit = json!({"op":native,"object":object});
    for (key, value) in params {
        edit[key] = value.clone();
    }
    if matches!(kind, "copy" | "clone" | "move" | "rectangular_array") {
        if !finite_vector(&parameters["displacement"])
            || parameters["displacement"][2].as_f64() != Some(0.)
            || parameters
                .get("all_geometry")
                .is_some_and(|v| !v.is_boolean())
        {
            return Err("Enter a sketch-local displacement and geometry scope".into());
        }
        let sources = if parameters["all_geometry"] == true {
            if snapshot["sketch"]["geometry"]
                .as_array()
                .is_none_or(Vec::is_empty)
            {
                return Err("The native sketch has no internal geometry".into());
            }
            Vec::new()
        } else {
            copy_sources(snapshot, selection)?
        };
        if kind == "rectangular_array"
            && (!integer(&parameters["columns"], 2, 99)
                || !integer(&parameters["rows"], 1, 99)
                || !scalar(&parameters["perpendicular_scale"])
                || ["clone", "constrain_displacement"]
                    .iter()
                    .any(|key| parameters.get(key).is_some_and(|v| !v.is_boolean())))
        {
            return Err("Enter native array columns 2..99, rows 1..99 and spacing options".into());
        }
        edit["indices"] = json!(sources);
        return Ok(vec![edit]);
    }
    if kind == "mirror" {
        let sources = mirror_sources(snapshot, selection)?;
        let reference: SketchElement = serde_json::from_value(parameters["reference"].clone())
            .map_err(|_| "Choose a native Mirror reference")?;
        if parameters["reference"]
            .as_object()
            .is_none_or(|r| r.len() != 2)
            || !mirror_references(snapshot)
                .iter()
                .any(|(r, _)| *r == reference)
        {
            return Err("Choose a native line, axis or exposed point from this sketch".into());
        }
        if ["delete_original", "create_symmetry_constraints"]
            .iter()
            .any(|key| parameters.get(key).is_some_and(|v| !v.is_boolean()))
            || parameters["delete_original"] == true
                && parameters["create_symmetry_constraints"] == true
        {
            return Err("Choose either delete originals or create symmetry constraints".into());
        }
        if parameters["create_symmetry_constraints"] == true {
            let capability =
                &snapshot["sketch"]["task_capabilities"]["mirror"]["create_symmetry_constraints"];
            if capability["available"] != true {
                return Err(capability["reason"]
                    .as_str()
                    .unwrap_or(
                        "Constrained Mirror availability is unknown; refresh the native sketch",
                    )
                    .into());
            }
        }
        edit["indices"] = json!(sources);
        return Ok(vec![edit]);
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

    #[test]
    fn slot_uses_one_owned_native_line_and_keeps_its_mode() {
        let mut s = json!({"name":"Sketch","sketch":{"geometry_count":1,"geometry":[
            {"index":0,"persistent_id":17,"editable":true,"kind":"line",
             "start":[3.,-5.,0.],"end":[3.,25.,0.],"construction":false}]}});
        let line = SketchElement { index: 0, point: 0 };
        let edits = selected_line_slot(&s, &[line]).unwrap();
        assert_eq!(edits.len(), 12);
        assert_eq!(edits[0]["kind"], "arc");
        assert_eq!(edits[0]["start"], json!([-7., -5., 0.]));
        assert_eq!(edits[0]["mid"], json!([3., -15., 0.]));
        assert_eq!(edits[4]["kind"], "Tangent");
        assert_eq!(edits[9]["args"], json!([1, 3, 0, 1]));
        assert_eq!(edits[11]["args"], json!([1, 20.]));
        assert!(!edits.iter().any(|e| e["op"] == "construction"));
        assert!(selected_line_slot(&s, &[]).is_err());
        assert!(selected_line_slot(&s, &[SketchElement { index: 0, point: 1 }]).is_err());
        assert!(selected_line_slot(&s, &[line, line]).is_err());
        s["sketch"]["geometry"][0]["persistent_id"] = Value::Null;
        assert!(selected_line_slot(&s, &[line]).is_err());
    }
    fn snapshot() -> Value {
        json!({"name":"Sketch","sketch":{"geometry":[{"index":0,"kind":"line"},{"index":1,"kind":"arc"},{"index":2,"kind":"bspline","poles":[[0,0,0],[1,2,0],[3,0,0]]}]}})
    }
    fn select(index: i32, point: i32) -> Vec<SketchElement> {
        vec![SketchElement { index, point }]
    }
    #[test]
    fn copy_tasks_keep_native_inputs_and_reject_unsafe_sources() {
        let s = snapshot();
        let p = json!({"displacement":["1 in",{"quantity_input":"2","default_unit":"cm"},0]});
        for (kind, op) in [
            ("copy", "copy_geometry"),
            ("clone", "clone_geometry"),
            ("move", "move_geometries"),
        ] {
            let edits = build(&s, kind, &select(0, 0), &p).unwrap();
            assert_eq!(edits[0]["op"], op);
            assert_eq!(edits[0]["indices"], json!([0]));
            assert_eq!(edits[0]["displacement"], p["displacement"]);
            for pick in [select(-3, 0), select(999, 0), select(0, 1), vec![]] {
                assert!(build(&s, kind, &pick, &p).is_err());
            }
        }
        let all = json!({"displacement":[0,0,0],"all_geometry":true});
        assert_eq!(
            build(&s, "copy", &[], &all).unwrap()[0]["indices"],
            json!([])
        );
        let mut a = json!({"displacement":[1,2,0],"columns":3,"rows":2,"clone":true,
            "constrain_displacement":true,"perpendicular_scale":0.5});
        assert!(build(&s, "rectangular_array", &select(0, 0), &a).is_ok());
        for (key, value) in [
            ("columns", json!(100)),
            ("columns", json!(1)),
            ("rows", json!(0)),
            ("rows", json!(2147483648_i64)),
            ("clone", json!(1)),
            ("displacement", json!([0, 0, 1e-10])),
        ] {
            let mut invalid = a.clone();
            invalid[key] = value;
            assert!(build(&s, "rectangular_array", &select(0, 0), &invalid).is_err());
        }
        a["unknown"] = json!(1);
        assert!(build(&s, "rectangular_array", &select(0, 0), &a).is_err());
    }
    #[test]
    fn mirror_compiles_supplied_ids_and_native_selection_filtering() {
        let s = json!({"name":"Sketch","sketch":{"task_capabilities":{"mirror":{"create_symmetry_constraints":{"available":true}}},"geometry":[
            {"index":4,"kind":"line","handles":[{"index":4,"point":1},{"index":4,"point":2}]},
            {"index":8,"kind":"point","handles":[{"index":8,"point":1}]}],
            "externals":[{"geometry_index":-3,"geometry":{"index":-3,"kind":"line",
                "handles":[{"index":-3,"point":1},{"index":-3,"point":2}]}}]}});
        let selection = vec![
            SketchElement { index: 8, point: 1 },
            SketchElement { index: 4, point: 0 },
            SketchElement { index: 4, point: 2 },
            SketchElement {
                index: -2,
                point: 0,
            },
        ];
        let p = json!({"reference":{"index":-3,"point":0},"create_symmetry_constraints":true});
        assert_eq!(
            build(&s, "mirror", &selection, &p).unwrap(),
            vec![json!({
            "op":"mirror_geometry","object":"Sketch","indices":[8,4],
            "reference":{"index":-3,"point":0},"create_symmetry_constraints":true})]
        );
        assert!(build(&s, "mirror", &select(4, 1), &p).is_err());
        assert!(build(&s, "mirror", &select(0, 0), &p).is_err());
        assert!(
            build(
                &s,
                "mirror",
                &select(4, 0),
                &json!({"reference":{"index":8,"point":0}})
            )
            .is_err()
        );
        assert!(
            build(
                &s,
                "mirror",
                &select(4, 0),
                &json!({"reference":{"index":4,"point":1},
            "delete_original":true,"create_symmetry_constraints":true})
            )
            .is_err()
        );
        assert!(
            build(
                &s,
                "mirror",
                &select(4, 0),
                &json!({"reference":{"index":-1,"point":0},"offset":1})
            )
            .is_err()
        );
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
