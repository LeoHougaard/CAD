// Focused implementation follow-up; actual widgets and native actor.
fn fixed_shape(actual: &Value, expected: &Value) {
    for field in ["null", "valid", "solids", "faces", "edges", "vertices"] {
        assert_eq!(actual[field], expected[field], "shape.{field}");
    }
    for field in ["volume", "area"] {
        assert!((actual[field].as_f64().unwrap() - expected[field].as_f64().unwrap()).abs() < 1e-8, "shape.{field}");
    }
    for bound in ["min", "max"] {
        for axis in 0..3 {
            assert!((actual["bounds"][bound][axis].as_f64().unwrap() - expected["bounds"][bound][axis].as_f64().unwrap()).abs() < 1e-8, "shape.bounds.{bound}[{axis}]");
        }
    }
}
fn fixed_holes(h: &NativeEditor, expected: &Value) {
    let actual: Vec<_> = objects(&h.editor.state)
        .iter()
        .filter(|o| o["type"] == "PartDesign::Hole")
        .collect();
    assert_eq!(actual.len(), expected["holes"].as_object().unwrap().len());
    for (name, goal) in expected["holes"].as_object().unwrap() {
        let o = object(&h.editor.state, name);
        assert_eq!(o["label"], goal["label"]);
        for (field, value) in goal["properties"].as_object().unwrap() {
            assert_eq!(o["properties"][field], *value, "{name}.{field}");
        }
        assert_eq!(o["expressions"], goal["expressions"]);
        assert_eq!(o["properties"]["Profile"]["link"], goal["profile"]);
        assert_eq!(o["properties"]["BaseFeature"]["link"], goal["base"]);
        assert_eq!(o["body"], goal["body"]);
        assert!(
            (o["shape"]["volume"].as_f64().unwrap() - goal["volume"].as_f64().unwrap()).abs()
                < 1e-7,
            "{name} volume"
        );
        assert!(
            (o["shape"]["area"].as_f64().unwrap() - goal["area"].as_f64().unwrap()).abs() < 1e-7,
            "{name} area"
        );
        assert_eq!(o["shape"]["solids"], goal["solids"]);
    }
    for (name, body) in expected["bodies"].as_object().unwrap() {
        let o = object(&h.editor.state, name);
        assert_eq!(o["body_state"]["tip"], body["tip"]);
        let group: Vec<_> = o["properties"]["Group"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["link"].clone())
            .collect();
        assert_eq!(json!(group), body["group"]);
    }
}
fn fixed_click_finish(h: &mut NativeEditor, commit: bool) {
    let mut f = Form::new(true);
    f.frame(&mut h.editor, vec![]);
    f.frame(&mut h.editor, vec![]);
    f.click(&mut h.editor, if commit { "Apply" } else { "Cancel" });
    if h.editor.busy {
        h.receive(
            if commit {
                "preview_commit"
            } else {
                "preview_cancel"
            },
            true,
        );
    }
}
#[test]
#[ignore = "requires focused direct-native controls"]
fn fix_r1_open_regeneration_all_holes_and_repeated_cleanup() {
    let root = input_file("CAD_HOLE_FIXED_FIXTURES");
    let controls: Value =
        serde_json::from_str(&fs::read_to_string(root.join("controls.json")).unwrap()).unwrap();
    let mut h = open_hole("fix-r1-rejections");
    let mut f = Form::new(false);
    quantity(&mut h, &mut f, "Depth", "2 mm");
    h.preview(true);
    fixed_click_finish(&mut h, true);
    let accepted = h.editor.state.clone();
    h.send("undo", json!({}));
    let before = h.editor.state.clone();
    assert_eq!(before["redo_count"], 1);
    for case in controls["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["accepted"] == false)
    {
        for _ in 0..2 {
            h.editor.send("open", json!({"path":case["input"]}));
            h.receive("open", false);
            assert!(h.editor.error.is_some());
            assert!(!h.editor.error.as_ref().unwrap().contains("already_open"));
            for field in ["document", "revision", "undo_count", "redo_count"] {
                assert_eq!(h.editor.state[field], before[field], "{field}");
            }
            for name in ["Hole", "Profile", "Base"] {
                for field in ["properties", "expressions", "shape"] {
                    assert_eq!(
                        object(&h.editor.state, name)[field],
                        object(&before, name)[field],
                        "{name}.{field}"
                    );
                }
            }
        }
    }
    h.send("redo", json!({}));
    fixed_shape(&object(&h.editor.state, "Hole")["shape"], &object(&accepted, "Hole")["shape"]);
    drop(h);
    let mut h = NativeEditor::new("fix-r1-valid-opens");
    let mut records = vec![];
    for case in controls["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["accepted"] == true)
    {
        h.send("open", json!({"path":case["input"]}));
        fixed_holes(&h, &case["expected"]);
        assert_eq!(h.editor.state["undo_count"], 0);
        let saved =
            acceptance_dir().join(format!("fixed-{}.FCStd", case["name"].as_str().unwrap()));
        h.send("save", json!({"path":saved}));
        records.push(json!({"name":case["name"],"saved":saved,"control":case["control"],"state":h.editor.state}));
        write_json("fixed-open-results.json", &json!(records));
    }
}
#[test]
#[ignore = "requires focused direct-native controls"]
fn fix_r1_dependent_expression_history_and_reopen() {
    let root = input_file("CAD_HOLE_FIXED_FIXTURES");
    let controls: Value =
        serde_json::from_str(&fs::read_to_string(root.join("controls.json")).unwrap()).unwrap();
    let find = |name: &str| {
        controls["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone()
    };
    let valid = find("multi-valid");
    // A native Depth setter also refreshes computed ThreadDepth. The stale
    // serialization control intentionally has no setter callback, so use the
    // separately generated native interactive-edit control for this lifecycle.
    let changed = find("multi-edited");
    let mut h = NativeEditor::new("fix-r1-history");
    h.send("open", json!({"path":valid["input"]}));
    fixed_holes(&h, &valid["expected"]);
    h.editor.edit_native_properties("HoleA");
    let mut f = Form::new(false);
    quantity(&mut h, &mut f, "Depth", "8 mm");
    h.preview(true);
    fixed_holes(&h, &changed["expected"]);
    fixed_click_finish(&mut h, true);
    h.send("undo", json!({}));
    fixed_holes(&h, &valid["expected"]);
    h.send("redo", json!({}));
    fixed_holes(&h, &changed["expected"]);
    let mut saved = acceptance_dir().join("fixed-multi-cycle0.FCStd");
    h.send("save", json!({"path":saved}));
    drop(h);
    for cycle in 1..=3 {
        let mut h = NativeEditor::new(&format!("fix-r1-cycle{cycle}"));
        h.send("open", json!({"path":saved}));
        fixed_holes(&h, &changed["expected"]);
        saved = acceptance_dir().join(format!("fixed-multi-cycle{cycle}.FCStd"));
        h.send("save", json!({"path":saved}));
    }
}
fn fixed_fill_cut(h: &mut NativeEditor, f: &mut Form) {
    toggle(h, f, "HoleCutCustomValues");
    quantity(h, f, "HoleCutDepth", "0.25 mm");
    quantity(h, f, "HoleCutDiameter", "5.5 mm");
}
fn fixed_remembered_cut(h: &NativeEditor) {
    assert_eq!(h.params()["properties"]["HoleCutDepth"]["quantity_input"], "0.25 mm");
    assert_eq!(h.params()["properties"]["HoleCutDiameter"]["quantity_input"], "5.5 mm");
}
#[test]
#[ignore = "requires pinned native runtime"]
fn fix_p2_widget_round_trip_before_and_after_failure() {
    for failed in [false, true] {
        let mut h = open_hole(if failed {
            "fix-p2-failed"
        } else {
            "fix-p2-first"
        });
        let baseline = h.editor.state.clone();
        let mut f = Form::new(false);
        choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
        if failed {
            h.preview(false);
        }
        fixed_fill_cut(&mut h, &mut f);
        choose(&mut h, &mut f, "HoleCutType", "Counterbore", "None");
        let projected =
            crate::hole_commands::active_parameters(&h.editor.property_ui.schema, h.params());
        for field in ["HoleCutDepth", "HoleCutDiameter", "HoleCutCustomValues"] {
            assert!(projected["properties"].get(field).is_none());
        }
        h.preview(true);
        fixed_remembered_cut(&h);
        assert_eq!(
            volume(&h),
            object(&baseline, "Hole")["shape"]["volume"]
                .as_f64()
                .unwrap()
        );
        choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
        fixed_remembered_cut(&h);
        h.preview(true);
        assert!((volume(&h) - 82.96498535308116).abs() < 1e-7);
        fixed_click_finish(&mut h, false);
        for field in ["properties", "expressions", "shape"] {
            assert_eq!(
                object(&h.editor.state, "Hole")[field],
                object(&baseline, "Hole")[field]
            );
        }
        f.save(if failed {
            "p2-after-failure-widgets.json"
        } else {
            "p2-before-preview-widgets.json"
        });
    }
}
#[test]
#[ignore = "requires pinned native runtime"]
fn fix_p2_widget_round_trip_deferred_redo() {
    let mut h = open_hole("fix-p2-deferred");
    let mut f = Form::new(false);
    choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
    fixed_fill_cut(&mut h, &mut f);
    h.preview(true);
    fixed_click_finish(&mut h, true);
    let accepted = h.editor.state.clone();
    h.send("undo", json!({}));
    let undone = h.editor.state.clone();
    h.editor.edit_native_properties("Hole");
    assert!(h.editor.draft.as_ref().unwrap().deferred);
    choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
    fixed_fill_cut(&mut h, &mut f);
    choose(&mut h, &mut f, "HoleCutType", "Counterbore", "None");
    h.editor.preview();
    assert!(h.raw.try_recv().is_err());
    assert_eq!(h.editor.state, undone);
    fixed_remembered_cut(&h);
    choose(&mut h, &mut f, "HoleCutType", "None", "Counterbore");
    fixed_remembered_cut(&h);
    h.editor.preview();
    assert!(h.raw.try_recv().is_err());
    fixed_click_finish(&mut h, false);
    assert_eq!(h.editor.state, undone);
    h.send("redo", json!({}));
    assert_eq!(
        object(&h.editor.state, "Hole")["shape"],
        object(&accepted, "Hole")["shape"]
    );
    f.save("p2-deferred-redo-widgets.json");
}
