//! Headless actual Editor/egui/native checks. Physical GUI acceptance is separate.
use super::*;

fn open_hole(case: &str) -> NativeEditor {
    let mut h = NativeEditor::new(case);
    h.send("open", json!({"path":input_file("CAD_HOLE_INPUT")}));
    h.editor.edit_native_properties("Hole");
    h
}

fn choose(h: &mut NativeEditor, form: &mut Form, property: &str, old: &str, new: &str) {
    h.editor.property_ui.filter = property.into();
    form.frame(&mut h.editor, vec![]);
    form.frame(&mut h.editor, vec![]);
    form.click(&mut h.editor, old);
    form.frame(&mut h.editor, vec![]);
    for _ in 0..50 {
        if form.last.iter().any(|t| t.text == new) {
            break;
        }
        form.frame(
            &mut h.editor,
            vec![
                egui::Event::PointerMoved(egui::pos2(80., 180.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -120.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..3 {
            form.frame(&mut h.editor, vec![]);
        }
    }
    form.click(&mut h.editor, new);
    assert_eq!(h.params()["properties"][property], new);
}

fn row(h: &mut NativeEditor, form: &mut Form, property: &str) {
    h.editor.property_ui.filter = property.into();
    form.frame(&mut h.editor, vec![]);
    form.frame(&mut h.editor, vec![]);
}

fn quantity(h: &mut NativeEditor, form: &mut Form, property: &str, value: &str) {
    row(h, form, property);
    let current = h.params()["properties"]
        .get(property)
        .unwrap_or(&h.editor.property_ui.schema["properties"][property]);
    let metadata = &h.editor.property_ui.schema["property_metadata"][property]["quantity"];
    let old = current["quantity_input"]
        .as_str()
        .or_else(|| {
            (current == &metadata["source"]
                || current == &metadata["value"]
                || current == &metadata["base_value"])
                .then(|| metadata["input"].as_str())
                .flatten()
        })
        .or_else(|| current.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| current.to_string());
    form.click(&mut h.editor, &old);
    form.frame(
        &mut h.editor,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
            },
            egui::Event::Text(value.into()),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(h.params()["properties"][property]["quantity_input"], value);
}

fn toggle(h: &mut NativeEditor, form: &mut Form, property: &str) {
    row(h, form, property);
    let label = form
        .last
        .iter()
        .rfind(|text| text.text == property)
        .unwrap()
        .rect
        .center();
    // The native form draws its checkbox on the next row after the label.
    form.click_at(&mut h.editor, egui::pos2(16., label.y + 21.));
}

fn volume(h: &NativeEditor) -> f64 {
    object(&h.editor.state, "Hole")["shape"]["volume"]
        .as_f64()
        .unwrap()
}

fn replace_text(h: &mut NativeEditor, form: &mut Form, old: &str, text: &str) {
    let pos = form
        .last
        .iter()
        .rfind(|t| t.text == old)
        .unwrap_or_else(|| panic!("missing input {old}: {:?}", form.last))
        .rect
        .center();
    form.click_at(&mut h.editor, pos);
    form.frame(
        &mut h.editor,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
            },
            egui::Event::Text(text.into()),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
}

fn expression(h: &mut NativeEditor, form: &mut Form, property: &str, value: &str) {
    row(h, form, property);
    if !form.last.iter().any(|t| t.text == "Set expression") {
        form.click(&mut h.editor, "Expression paths");
    }
    form.frame(&mut h.editor, vec![]);
    let path = h
        .editor
        .property_ui
        .buffers
        .get("@path")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "Property or component path".into());
    let old = h
        .editor
        .property_ui
        .buffers
        .get("@expression")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "Expression".into());
    replace_text(h, form, &path, property);
    replace_text(h, form, &old, value);
    form.click(&mut h.editor, "Set expression");
    assert_eq!(h.params()["expressions"][property], value);
}

#[test]
#[ignore = "requires independently generated pinned native controls"]
fn hole_editor_native_matrix() {
    let root = input_file("CAD_HOLE_MATRIX");
    let controls: Value =
        serde_json::from_str(&fs::read_to_string(root.join("controls.json")).unwrap()).unwrap();
    let mut records = vec![];
    for case in controls["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut h = NativeEditor::new(name);
        h.send("open", json!({"path":root.join("input.FCStd")}));
        h.editor.edit_native_properties("Hole");
        let initial = h.editor.state.clone();
        let mut form = Form::new(false);
        // Controller expressions must be entered before their dependent input
        // widgets become available. The native command compiler orders their
        // evaluation before dependent literal writes as well.
        for (field, value) in case["expressions"].as_object().unwrap() {
            expression(&mut h, &mut form, field, value.as_str().unwrap());
        }
        let controllers = [
            "ThreadType",
            "Threaded",
            "ThreadSize",
            "ThreadClass",
            "DepthType",
            "ThreadDepthType",
            "ModelThread",
            "UseCustomThreadClearance",
            "HoleCutType",
            "HoleCutCustomValues",
            "DrillPoint",
            "Tapered",
        ];
        let mut fields: Vec<_> = case["properties"].as_object().unwrap().iter().collect();
        fields.sort_by_key(|(key, _)| {
            controllers
                .iter()
                .position(|c| *c == key.as_str())
                .unwrap_or(controllers.len())
        });
        for (field, value) in fields {
            let old = h.params()["properties"]
                .get(field)
                .unwrap_or(&h.editor.property_ui.schema["properties"][field])
                .clone();
            if &old == value && !(case["name"] == "bad-standard-size" && field == "ThreadSize") {
                continue;
            }
            let kind = h.editor.property_ui.schema["property_types"][field]
                .as_str()
                .unwrap()
                .to_owned();
            if kind == "App::PropertyEnumeration" {
                if case["name"] == "bad-standard-size" && field == "ThreadSize" {
                    // Negative native enum input through the shared compiler. It
                    // is deliberately absent from the actual native combo.
                    row(&mut h, &mut form, field);
                    let selected_bottom = form
                        .last
                        .iter()
                        .find(|t| t.text == old.as_str().unwrap())
                        .unwrap()
                        .rect
                        .max
                        .y;
                    form.click(&mut h.editor, old.as_str().unwrap());
                    form.frame(&mut h.editor, vec![]);
                    assert!(!form.last.iter().any(
                        |t| t.rect.min.y > selected_bottom && t.text == value.as_str().unwrap()
                    ));
                    form.frame(
                        &mut h.editor,
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                    h.editor.draft.as_mut().unwrap().parameters["properties"][field] =
                        value.clone();
                } else {
                    choose(
                        &mut h,
                        &mut form,
                        field,
                        old.as_str().unwrap(),
                        value.as_str().unwrap(),
                    );
                }
            } else if kind == "App::PropertyBool" {
                toggle(&mut h, &mut form, field);
                assert_eq!(h.params()["properties"][field], *value, "{name}: {field}");
            } else {
                quantity(&mut h, &mut form, field, &display(value));
            }
        }
        let expected = case["accepted"] == true;
        let no_changes = feature_edits(
            &h.editor.registry,
            h.editor.draft.as_ref().unwrap(),
            &h.editor.property_ui.schema,
        )
        .is_err();
        if no_changes {
            assert_eq!(name, "fit-Medium");
        } else {
            h.preview(expected);
        }
        if expected {
            let actual = object(&h.editor.state, "Hole");
            let target = &case["result"];
            assert!(
                (volume(&h) - target["volume"].as_f64().unwrap()).abs() < 1e-7,
                "{name}"
            );
            assert!(
                (actual["shape"]["area"].as_f64().unwrap() - target["area"].as_f64().unwrap())
                    .abs()
                    < 1e-7,
                "{name}"
            );
            for (field, value) in target["properties"].as_object().unwrap() {
                assert_eq!(actual["properties"][field], *value, "{name}: {field}");
            }
            if no_changes {
                h.editor.finish_preview(false);
            } else {
                h.editor.finish_preview(true);
                h.receive("preview_commit", true);
            }
            h.send(
                "save",
                json!({"path":acceptance_dir().join(format!("{name}.FCStd"))}),
            );
        } else {
            assert!(h.editor.error.is_some());
            assert!(h.editor.state["preview"].is_null());
            assert_eq!(
                object(&h.editor.state, "Hole")["shape"],
                object(&initial, "Hole")["shape"]
            );
            h.editor.finish_preview(false);
        }
        form.save(&format!("{name}-widgets.json"));
        records.push(json!({"name":name,"passed":true,"native_accepted":expected,"final":object(&h.editor.state,"Hole")}));
        write_json("hole-native-matrix-result.json", &json!(records));
    }
}

#[test]
#[ignore = "requires pinned native runtime and private output"]
fn hole_editor_recovery_lifecycle() {
    let mut h = open_hole("hole-recovery");
    let baseline = h.editor.state.clone();
    let initial_volume = volume(&h);
    let mut form = Form::new(false);
    choose(&mut h, &mut form, "HoleCutType", "None", "Counterbore");
    h.preview(false);
    assert!(h.editor.state["preview"].is_null());
    assert_eq!(volume(&h), initial_volume);
    toggle(&mut h, &mut form, "HoleCutCustomValues");
    assert_eq!(h.params()["properties"]["HoleCutCustomValues"], true);
    quantity(&mut h, &mut form, "HoleCutDepth", "0.25 mm");
    quantity(&mut h, &mut form, "HoleCutDiameter", "5.5 mm");
    h.preview(true);
    assert!((volume(&h) - 82.96498535308116).abs() < 1e-8);
    assert_eq!(object(&h.editor.state, "Hole")["properties"]["Depth"], 1.);
    let corrected = object(&h.editor.state, "Hole").clone();
    // A bad replacement keeps the accepted candidate, lease and editable draft.
    quantity(&mut h, &mut form, "HoleCutDepth", "4 mm");
    h.preview(false);
    assert_eq!(object(&h.editor.state, "Hole")["shape"], corrected["shape"]);
    assert!(!h.editor.state["preview"].is_null());
    quantity(&mut h, &mut form, "HoleCutDepth", "0.25 mm");
    h.preview(true);
    let mut panels = Form::new(true);
    panels.frame(&mut h.editor, vec![]);
    panels.frame(&mut h.editor, vec![]);
    panels.click(&mut h.editor, "Apply");
    h.receive("preview_commit", true);
    assert!(h.editor.draft.is_none());
    assert_eq!(
        h.editor.state["undo_count"].as_u64().unwrap(),
        baseline["undo_count"].as_u64().unwrap() + 1
    );
    h.send("undo", json!({}));
    assert!((volume(&h) - initial_volume).abs() < 1e-8);
    // All controller-dependent widgets remain reachable during deferred Redo.
    let undone = h.editor.state.clone();
    h.editor.edit_native_properties("Hole");
    assert!(h.editor.draft.as_ref().unwrap().deferred);
    choose(&mut h, &mut form, "HoleCutType", "None", "Counterbore");
    quantity(&mut h, &mut form, "HoleCutDepth", "0.2 mm");
    h.editor.preview();
    assert!(h.raw.try_recv().is_err());
    panels.frame(&mut h.editor, vec![]);
    panels.frame(&mut h.editor, vec![]);
    panels.click(&mut h.editor, "Cancel");
    assert_eq!(h.editor.state, undone);
    assert!(h.editor.draft.is_none());
    h.send("redo", json!({}));
    assert_eq!(object(&h.editor.state, "Hole")["shape"], corrected["shape"]);
    // Cancel an accepted native candidate, not only an unopened deferred task.
    let before_cancel = h.editor.state.clone();
    h.editor.edit_native_properties("Hole");
    quantity(&mut h, &mut form, "HoleCutDepth", "0.2 mm");
    h.preview(true);
    panels.frame(&mut h.editor, vec![]);
    panels.frame(&mut h.editor, vec![]);
    panels.click(&mut h.editor, "Cancel");
    h.receive("preview_cancel", true);
    assert_eq!(object(&h.editor.state, "Hole")["shape"], corrected["shape"]);
    assert_eq!(h.editor.state["undo_count"], before_cancel["undo_count"]);
    let path = acceptance_dir().join("Hole-recovered.FCStd");
    h.send("save", json!({"path":path}));
    let mut fresh = NativeEditor::new("hole-fresh");
    fresh.send("open", json!({"path":path}));
    assert_eq!(
        object(&fresh.editor.state, "Hole")["shape"],
        corrected["shape"]
    );
    assert_eq!(
        object(&fresh.editor.state, "Hole")["properties"]["Profile"]["link"],
        "Profile"
    );
    assert_eq!(
        object(&fresh.editor.state, "Hole")["properties"]["BaseFeature"]["link"],
        "Base"
    );
    assert_eq!(
        object(&fresh.editor.state, "Body")["body_state"]["tip"],
        "Hole"
    );
    form.save("hole-recovery-rendered-widgets.json");
    panels.save("hole-recovery-panels.json");
    write_json(
        "hole-recovery-result.json",
        &json!({"passed":true,"baseline_volume":initial_volume,"corrected":corrected,"fresh":fresh.editor.state}),
    );
}

#[test]
#[ignore = "requires pinned native runtime and private output"]
fn hole_editor_computed_protection_and_error_correction() {
    let root = input_file("CAD_HOLE_MATRIX");
    let mut h = NativeEditor::new("hole-protection");
    h.send("open", json!({"path":root.join("input.FCStd")}));
    let baseline = h.editor.state.clone();
    let mut form = Form::new(false);
    for field in ["Diameter", "ThreadDiameter", "Shape"] {
        h.editor.edit_native_properties("Hole");
        row(&mut h, &mut form, field);
        let presented = crate::hole_commands::task_schema(&h.editor.property_ui.schema, h.params());
        assert!(!crate::property_commands::writable(&presented, field));
        assert!(!h.editor.property_ui.buffers.contains_key(field));
        h.properties(json!({(field):7}), json!({}));
        h.preview(false);
        assert!(
            h.editor
                .error
                .as_ref()
                .unwrap()
                .contains("read_only_property")
        );
        assert_eq!(
            object(&h.editor.state, "Hole")["shape"],
            object(&baseline, "Hole")["shape"]
        );
        h.editor.finish_preview(false);
    }
    h.editor.edit_native_properties("Hole");
    quantity(&mut h, &mut form, "Depth", "3 deg");
    h.preview(false);
    quantity(&mut h, &mut form, "Depth", "5 mm");
    h.preview(true);
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    h.editor.edit_native_properties("Hole");
    expression(&mut h, &mut form, "Depth", "Missing.Length");
    h.preview(false);
    expression(&mut h, &mut form, "Depth", "Base.Height / 2");
    h.preview(true);
    assert_eq!(
        object(&h.editor.state, "Hole")["expressions"]["Depth"],
        "Base.Height / 2"
    );
    row(&mut h, &mut form, "Depth");
    form.click(&mut h.editor, "fx");
    h.preview(true);
    assert!(
        object(&h.editor.state, "Hole")["expressions"]
            .get("Depth")
            .is_none()
    );
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    assert_eq!(
        object(&h.editor.state, "Hole")["shape"],
        object(&baseline, "Hole")["shape"]
    );
    form.save("hole-protection-widgets.json");
    write_json(
        "hole-protection-result.json",
        &json!({"passed":true,"state":h.editor.state}),
    );
}

#[test]
#[ignore = "requires pinned native runtime and private output"]
fn hole_editor_depth_controller_round_trip() {
    let root = input_file("CAD_HOLE_MATRIX");
    let mut h = NativeEditor::new("hole-depth-round-trip");
    h.send("open", json!({"path":root.join("input.FCStd")}));
    h.editor.edit_native_properties("Hole");
    let mut form = Form::new(false);
    quantity(&mut h, &mut form, "Depth", "5 mm");
    h.preview(true);
    choose(&mut h, &mut form, "DepthType", "Dimension", "ThroughAll");
    h.preview(true);
    choose(&mut h, &mut form, "DepthType", "ThroughAll", "Dimension");
    h.preview(true);
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    form.save("hole-depth-round-trip-widgets.json");
}

#[test]
#[ignore = "requires pinned native runtime and private output"]
fn hole_editor_controller_expression_atomic_recovery() {
    let root = input_file("CAD_HOLE_MATRIX");
    let mut h = NativeEditor::new("hole-controller-expression-recovery");
    h.send("open", json!({"path":root.join("input.FCStd")}));
    let baseline = h.editor.state.clone();
    h.editor.edit_native_properties("Hole");
    let mut form = Form::new(false);
    expression(&mut h, &mut form, "Threaded", "1");
    // Invalid enum input follows a real native controller recompute in the
    // same candidate. It must abort every intermediate native change.
    h.editor.draft.as_mut().unwrap().parameters["properties"]["ThreadSize"] =
        json!("not-a-native-thread-size");
    h.preview(false);
    assert!(h.editor.state["preview"].is_null());
    let restored = object(&h.editor.state, "Hole");
    let original = object(&baseline, "Hole");
    for field in ["properties", "expressions", "shape"] {
        assert_eq!(restored[field], original[field], "{field}");
    }
    assert!(h.editor.state["revision"].as_u64().unwrap() > baseline["revision"].as_u64().unwrap());
    assert_eq!(h.editor.state["undo_count"], baseline["undo_count"]);
    choose(
        &mut h,
        &mut form,
        "ThreadSize",
        "not-a-native-thread-size",
        "M4x0.7",
    );
    expression(&mut h, &mut form, "Threaded", "0");
    h.preview(true);
    assert_eq!(
        object(&h.editor.state, "Hole")["properties"]["Threaded"],
        false
    );
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    for field in ["properties", "expressions", "shape"] {
        assert_eq!(
            object(&h.editor.state, "Hole")[field],
            original[field],
            "{field}"
        );
    }
    form.save("hole-controller-expression-recovery-widgets.json");
    write_json(
        "hole-controller-expression-recovery-result.json",
        &json!({"passed":true,"state":h.editor.state}),
    );
}

fn contract(h: &NativeEditor, expected: &Value) {
    let hole = object(&h.editor.state, "Hole");
    for (field, value) in expected["properties"].as_object().unwrap() {
        assert_eq!(hole["properties"][field], *value, "{field}");
    }
    assert_eq!(hole["expressions"], expected["expressions"]);
    assert!(
        (hole["shape"]["volume"].as_f64().unwrap() - expected["volume"].as_f64().unwrap()).abs()
            < 1e-7
    );
    assert_eq!(hole["properties"]["Profile"]["link"], "Profile");
    assert_eq!(hole["properties"]["BaseFeature"]["link"], "Base");
    assert_eq!(object(&h.editor.state, "Body")["body_state"]["tip"], "Hole");
}

#[test]
#[ignore = "requires saved matrix documents and pinned native runtime"]
fn hole_editor_fresh_reopen_matrix() {
    let controls: Value = serde_json::from_str(
        &fs::read_to_string(input_file("CAD_HOLE_MATRIX").join("controls.json")).unwrap(),
    )
    .unwrap();
    let saved = input_file("CAD_HOLE_SAVED");
    let mut h = NativeEditor::new("hole-fresh-matrix");
    let mut records = vec![];
    let mut form = Form::new(false);
    for case in controls["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["accepted"] == true)
    {
        let name = case["name"].as_str().unwrap();
        h.send("open", json!({"path":saved.join(format!("{name}.FCStd"))}));
        contract(&h, &case["result"]);
        assert_eq!(h.editor.state["undo_count"], 0);
        if matches!(name, "modeled-thread" | "custom-thread-clearance") {
            h.editor.edit_native_properties("Hole");
            toggle(&mut h, &mut form, "ModelThread");
            h.preview(true);
            assert_eq!(
                object(&h.editor.state, "Hole")["properties"]["ModelThread"],
                false
            );
            h.editor.finish_preview(false);
            h.receive("preview_cancel", true);
            contract(&h, &case["result"]);
        }
        records.push(json!({"name":name,"passed":true,"hole":object(&h.editor.state,"Hole")}));
        write_json("hole-fresh-matrix-result.json", &json!(records));
    }
}

#[test]
#[ignore = "requires native persistence fixtures and pinned runtime"]
fn hole_editor_persistence_parent_profile_history_and_failed_open() {
    let fixture = input_file("CAD_HOLE_PERSISTENCE");
    let controls: Value =
        serde_json::from_str(&fs::read_to_string(fixture.join("controls.json")).unwrap()).unwrap();
    let mut h = NativeEditor::new("hole-persistence");
    h.send(
        "open",
        json!({"path":fixture.join("expression-input.FCStd")}),
    );
    contract(&h, &controls["expression-input"]);
    h.editor.edit_native_properties("Base");
    let mut form = Form::new(false);
    quantity(&mut h, &mut form, "Height", "22 mm");
    h.preview(true);
    contract(&h, &controls["parent-control"]);
    let mut panels = Form::new(true);
    panels.frame(&mut h.editor, vec![]);
    panels.frame(&mut h.editor, vec![]);
    panels.click(&mut h.editor, "Apply");
    h.receive("preview_commit", true);
    assert_eq!(h.editor.state["undo_count"], 1);
    h.send("undo", json!({}));
    contract(&h, &controls["expression-input"]);
    h.send("redo", json!({}));
    contract(&h, &controls["parent-control"]);
    h.send(
        "sketch_begin",
        json!({"object":"Profile","token":"hole-profile-edit"}),
    );
    h.editor.sketch_ui.session = Some("hole-profile-edit".into());
    h.send("transaction",json!({"label":"Move consumed Hole locating circle","edits":[{"op":"move_geometry","object":"Profile","index":0,"point":3,"position":[1,0,0]}]}));
    h.send("sketch_finish", json!({"token":"hole-profile-edit"}));
    h.editor.sketch_ui.session = None;
    contract(&h, &controls["profile-control"]);
    let mut saved = acceptance_dir().join("Hole-persistence-0.FCStd");
    h.send("save", json!({"path":saved}));
    drop(h);
    for cycle in 1..=3 {
        let mut fresh = NativeEditor::new(&format!("hole-reopen-{cycle}"));
        fresh.send("open", json!({"path":saved}));
        contract(&fresh, &controls["profile-control"]);
        assert_eq!(fresh.editor.state["undo_count"], 0);
        let before = fresh.editor.state.clone();
        for _ in 0..2 {
            fresh
                .editor
                .send("open", json!({"path":fixture.join("bad-open.FCStd")}));
            fresh.receive("open", false);
            assert!(
                fresh
                    .editor
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("invalid_model")
            );
            assert_eq!(fresh.editor.state["document"], before["document"]);
            assert_eq!(fresh.editor.state["revision"], before["revision"]);
            contract(&fresh, &controls["profile-control"]);
        }
        fresh.editor.edit_native_properties("Hole");
        let mut fields = Form::new(false);
        quantity(&mut fresh, &mut fields, "HoleCutDiameter", "6.2 mm");
        fresh.preview(true);
        fresh.editor.finish_preview(false);
        fresh.receive("preview_cancel", true);
        contract(&fresh, &controls["profile-control"]);
        saved = acceptance_dir().join(format!("Hole-persistence-{cycle}.FCStd"));
        fresh.send("save", json!({"path":saved}));
    }
    write_json(
        "hole-persistence-result.json",
        &json!({"passed":true,"final":saved,"expected":controls["profile-control"]}),
    );
}

#[test]
#[ignore = "requires native missing-preset control and pinned runtime"]
fn hole_editor_missing_preset_forced_custom_recovery_and_protection() {
    let controls: Value = serde_json::from_str(
        &fs::read_to_string(input_file("CAD_HOLE_MISSING_PRESET").join("controls.json")).unwrap(),
    )
    .unwrap();
    let expected = &controls["cases"][0]["result"];
    let mut h = open_hole("hole-missing-preset");
    let mut form = Form::new(false);
    choose(&mut h, &mut form, "ThreadSize", "M4x0.7", "M1.2x0.25");
    choose(&mut h, &mut form, "HoleCutType", "None", "ISO 4762");
    toggle(&mut h, &mut form, "HoleCutCustomValues");
    quantity(&mut h, &mut form, "HoleCutDepth", "0.25 mm");
    quantity(&mut h, &mut form, "HoleCutDiameter", "1.8 mm");
    h.preview(true);
    contract(&h, expected);
    let shown = crate::hole_commands::task_schema(&h.editor.property_ui.schema, h.params());
    assert!(!crate::property_commands::writable(
        &shown,
        "HoleCutCustomValues"
    ));
    row(&mut h, &mut form, "HoleCutCustomValues");
    let before = h.params().clone();
    form.click(&mut h.editor, "true");
    assert_eq!(*h.params(), before);
    h.editor.finish_preview(true);
    h.receive("preview_commit", true);
    let saved = acceptance_dir().join("Hole-missing-preset.FCStd");
    h.send("save", json!({"path":saved}));
    h.editor.edit_native_properties("Hole");
    h.properties(json!({"HoleCutCustomValues":false}), json!({}));
    h.preview(false);
    assert!(
        h.editor
            .error
            .as_ref()
            .unwrap()
            .contains("read_only_property")
    );
    contract(&h, expected);
    h.editor.finish_preview(false);
    drop(h);
    let mut fresh = NativeEditor::new("hole-missing-preset-fresh");
    fresh.send("open", json!({"path":saved}));
    contract(&fresh, expected);
    fresh.editor.edit_native_properties("Hole");
    quantity(&mut fresh, &mut form, "HoleCutDepth", "0.3 mm");
    fresh.preview(true);
    fresh.editor.finish_preview(false);
    fresh.receive("preview_cancel", true);
    contract(&fresh, expected);
    form.save("hole-missing-preset-widgets.json");
    write_json(
        "hole-missing-preset-result.json",
        &json!({"passed":true,"saved":saved,"fresh":fresh.editor.state}),
    );
}

#[test]
#[ignore = "requires pinned native runtime and private output"]
fn q06_baseline_actual_editor_reproduction() {
    let mut h = open_hole("q06-baseline");
    let baseline = h.editor.state.clone();
    let mut form = Form::new(false);
    choose(&mut h, &mut form, "HoleCutType", "None", "Counterbore");
    h.preview(false);
    assert!(
        h.editor
            .error
            .as_ref()
            .unwrap()
            .contains("Hole cut depth must be less than hole depth")
    );
    assert_eq!(
        object(&h.editor.state, "Hole")["properties"]["HoleCutType"],
        "None"
    );
    assert!(h.editor.state["preview"].is_null());
    assert_eq!(
        object(&h.editor.state, "Hole")["shape"],
        object(&baseline, "Hole")["shape"]
    );
    for field in ["HoleCutDepth", "HoleCutDiameter"] {
        h.editor.property_ui.filter = field.into();
        form.frame(&mut h.editor, vec![]);
        form.frame(&mut h.editor, vec![]);
        assert!(!crate::property_commands::writable(
            &h.editor.property_ui.schema,
            field
        ));
        assert!(!h.editor.property_ui.buffers.contains_key(field));
        assert!(form.last.iter().any(|t| t.text == field));
    }
    form.save("q06-baseline-rendered-widgets.json");
    write_json(
        "q06-baseline-finding.json",
        &json!({"reproduced":true,"route":"Editor edit_native_properties; actual HoleCutType ComboBox; native failed preview; actual readonly dependent rows","parameters":h.params(),"error":h.editor.error,"state":h.editor.state}),
    );
    h.editor.finish_preview(false);
    assert!(h.editor.draft.is_none());
    assert!(h.raw.try_recv().is_err());
}

include!("review_r1_tests.rs");
include!("fixed_r1_tests.rs");

include!("hole_review_r2_tests.rs");
include!("hole_units_tests.rs");
