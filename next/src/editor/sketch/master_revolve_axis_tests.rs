use super::*;

fn click_popup(n: &mut Native, label: &str) {
    let area = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
        .unwrap();
    let frame = n.frame(vec![]);
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                let p = t.galley.rect.translate(t.pos.to_vec2()).center();
                area.contains(p).then_some(p)
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1);
    let pos = targets[0];
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
}

fn fixture_and_pick(n: &mut Native, doc: &str) -> (String, i64) {
    n.send("new", json!({"name":doc}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Master","body":"Body"},
        {"op":"geometry","object":"Master","kind":"line","start":[10,0,0],"end":[10,120,0]},
        {"op":"geometry","object":"Master","kind":"line","start":[0,0,0],"end":[0,182,0],"construction":true},
        {"op":"add_object","type":"PartDesign::Line","name":"ExistingAxis","body":"Body"},
        {"op":"set","object":"ExistingAxis","property":"Visibility","value":false}
    ]));
    let id = object(&n.editor.state, "Master")["sketch"]["geometry"][1]["persistent_id"]
        .as_i64()
        .unwrap();
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Profile".into());
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Profile","body":"Body"},
        {"op":"set","object":"Master","property":"Visibility","value":true}
    ]));
    n.receive("sketch_begin", true);
    n.transaction(json!([
        {"op":"geometry","object":"Profile","kind":"line","start":[0,0,0],"end":[-40,0,0]},
        {"op":"geometry","object":"Profile","kind":"line","start":[-40,0,0],"end":[-40,62,0]},
        {"op":"geometry","object":"Profile","kind":"line","start":[-40,62,0],"end":[-27.5,74.5,0]},
        {"op":"geometry","object":"Profile","kind":"line","start":[-27.5,74.5,0],"end":[-27.5,100,0]},
        {"op":"geometry","object":"Profile","kind":"line","start":[-27.5,100,0],"end":[0,100,0]},
        {"op":"geometry","object":"Profile","kind":"line","start":[0,100,0],"end":[0,0,0]}
    ]));
    n.frame(vec![]);
    tutorial_key(n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    click_popup(n, "Revolve");
    n.receive("sketch_finish", true);
    n.receive("feature_schema", true);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    if n.editor.busy {
        n.receive("preview", true);
    }
    tutorial_key(n, Key::Tab, egui::Modifiers::NONE);
    assert!(n.editor.property_ui.axis_pick);
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    tutorial_key(n, Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(n.editor.property_ui.axis_pick);
    // Same XY placement as the profile. Use an interior point of the actual
    // displayed master construction line, away from the profile endpoint.
    let point = [0., 90., 0.];
    let screen = n.editor.viewport.sketch_screen_point(point);
    assert_eq!(
        n.editor
            .viewport
            .construction_axis_hit(&n.editor.state, "Profile", screen)
            .unwrap(),
        Some(json!({"master":"Master","geometry_id":id}))
    );
    tutorial_click(n, point);
    let draft = n.editor.draft.as_ref().unwrap();
    let axis = &draft.parameters["construction_axis"];
    assert_eq!(axis["master"], "Master");
    assert_eq!(axis["geometry_id"], id);
    let helper = axis["name"].as_str().unwrap().to_owned();
    assert!(!n.editor.property_ui.axis_pick);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    assert_eq!(object(&n.editor.state, &helper)["type"], "PartDesign::Line");
    assert_eq!(
        object(&n.editor.state, &helper)["properties"]["Visibility"],
        false
    );
    assert_eq!(
        object(&n.editor.state, &helper)["properties"]["AttachmentSupport"][0]["link"],
        "Master"
    );
    (helper, id)
}

#[test]
#[ignore = "requires the pinned master-axis observer and CAD_LIFECYCLE_OUTPUT"]
fn actual_master_axis_picker_owns_preview_history_and_reopened_geometry() {
    let mut n = Native::new("master-revolve-axis");
    let (cancelled, id) = fixture_and_pick(&mut n, "AxisCancel");
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == cancelled)
    );
    assert_eq!(
        object(&n.editor.state, "ExistingAxis")["type"],
        "PartDesign::Line"
    );

    let stale = json!([
        {"op":"create_revolve_axis","master":"Master","geometry_id":id+100000,"body":"Body","profile":"Profile","name":"StaleAxis"},
        {"op":"add_object","type":"PartDesign::Revolution","name":"StaleRevolve","body":"Body"},
        {"op":"set","object":"StaleRevolve","property":"Profile","value":{"link":"Profile"}},
        {"op":"set","object":"StaleRevolve","property":"ReferenceAxis","value":{"link":"StaleAxis","subelements":[""]}}
    ]);
    n.send(
        "preview",
        json!({"token":"stale-axis","label":"Reject stale","edits":stale}),
        false,
    );
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == "StaleAxis")
    );
    n.send("close", json!({}), true);

    let (helper, id) = fixture_and_pick(&mut n, "AxisCommit");
    let rev = objects(&n.editor.state)
        .iter()
        .find(|o| o["type"] == "PartDesign::Revolution")
        .unwrap();
    let name = rev["name"].as_str().unwrap().to_owned();
    assert_eq!(rev["shape"]["solids"], 1);
    assert_eq!(rev["shape"]["valid"], true);
    assert!((rev["shape"]["volume"].as_f64().unwrap() - 417471.8487692187).abs() < 1e-4);
    assert!(rev["test_csg"]["wrong_delta"].as_f64().unwrap() > 1000.);
    let native_id = rev["test_native_id"].as_i64().unwrap();
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("preview_commit", true);
    n.send("undo", json!({}), true);
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == name || o["name"] == helper)
    );
    assert_eq!(
        object(&n.editor.state, "ExistingAxis")["type"],
        "PartDesign::Line"
    );
    n.send("redo", json!({}), true);
    assert_eq!(object(&n.editor.state, &name)["test_native_id"], native_id);
    let path = n.save("master-axis.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(object(&n.editor.state, &name)["test_native_id"], native_id);
    n.transaction(json!([{"op":"delete_geometry","object":"Master","index":0}]));
    let selected = &object(&n.editor.state, "Master")["sketch"]["geometry"][0];
    assert_eq!(selected["persistent_id"], id);
    n.transaction(json!([
        {"op":"move_geometry","object":"Master","index":0,"point":1,"position":[3,0,0],"relative":false},
        {"op":"move_geometry","object":"Master","index":0,"point":2,"position":[3,182,0],"relative":false}
    ]));
    let moved = object(&n.editor.state, &name);
    assert_eq!(moved["shape"]["valid"], true);
    assert_eq!(moved["shape"]["solids"], 1);
    assert!((moved["test_axis"]["base"][0].as_f64().unwrap() - 3.).abs() < 1e-6);
    assert!((moved["shape"]["volume"].as_f64().unwrap() - 485389.15494901297).abs() < 1e-4);

    // A later invalid preview may create its own helper before recompute, but
    // abort must remove only that helper and keep all existing native datums.
    let edits = json!([
        {"op":"create_revolve_axis","master":"Master","geometry_id":id,"body":"Body","profile":"Profile","name":"FailedAxis"},
        {"op":"add_object","type":"PartDesign::Revolution","name":"FailedRevolve","body":"Body"},
        {"op":"set","object":"FailedRevolve","property":"Profile","value":{"link":"Profile"}},
        {"op":"set","object":"FailedRevolve","property":"ReferenceAxis","value":{"link":"FailedAxis","subelements":[""]}},
        {"op":"set","object":"FailedRevolve","property":"Angle","value":0}
    ]);
    n.send(
        "preview",
        json!({"token":"failed-axis","label":"Fail axis","edits":edits}),
        false,
    );
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == "FailedAxis" || o["name"] == "FailedRevolve")
    );
    assert_eq!(object(&n.editor.state, &helper)["type"], "PartDesign::Line");
    assert_eq!(
        object(&n.editor.state, "ExistingAxis")["type"],
        "PartDesign::Line"
    );

    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"OtherBody"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Foreign","body":"OtherBody"},
        {"op":"geometry","object":"Foreign","kind":"line","start":[0,0,0],"end":[0,182,0],"construction":true}
    ]));
    let foreign = object(&n.editor.state, "Foreign")["sketch"]["geometry"][0]["persistent_id"]
        .as_i64()
        .unwrap();
    let bad = json!([
        {"op":"create_revolve_axis","master":"Foreign","geometry_id":foreign,"body":"Body","profile":"Profile","name":"WrongOwnerAxis"},
        {"op":"add_object","type":"PartDesign::Revolution","name":"WrongOwnerRevolve","body":"Body"},
        {"op":"set","object":"WrongOwnerRevolve","property":"Profile","value":{"link":"Profile"}},
        {"op":"set","object":"WrongOwnerRevolve","property":"ReferenceAxis","value":{"link":"WrongOwnerAxis","subelements":[""]}}
    ]);
    n.send(
        "preview",
        json!({"token":"wrong-owner","label":"Reject owner","edits":bad}),
        false,
    );
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == "WrongOwnerAxis")
    );
    assert_eq!(object(&n.editor.state, &helper)["type"], "PartDesign::Line");
}

#[test]
#[ignore = "requires the pinned master-axis observer and CAD_LIFECYCLE_OUTPUT"]
fn overlapping_master_and_profile_axes_report_and_allow_both_choices() {
    let mut n = Native::new("master-axis-overlap-choice");
    fixture_and_pick(&mut n, "AxisOverlap");
    n.click("Revolve axis (Tab)");
    assert!(n.editor.property_ui.axis_pick);
    tutorial_click(&mut n, [0., 110., 0.]);
    assert!(n.editor.property_ui.axis_overlap);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    let helper = n.editor.draft.as_ref().unwrap().parameters["construction_axis"]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    n.click("Profile V axis");
    assert!(!n.editor.property_ui.axis_overlap);
    assert!(
        n.editor
            .draft
            .as_ref()
            .unwrap()
            .parameters
            .get("construction_axis")
            .is_none()
    );
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    assert!(!objects(&n.editor.state).iter().any(|o| o["name"] == helper));
    let profile_axis = &n.editor.draft.as_ref().unwrap().parameters["properties"]["ReferenceAxis"];
    assert_eq!(profile_axis["link"], "Profile");
    assert_eq!(profile_axis["subelements"], json!(["V_Axis"]));
    n.click("Revolve axis (Tab)");
    tutorial_click(&mut n, [0., 110., 0.]);
    assert!(n.editor.property_ui.axis_overlap);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    let axis = &n.editor.draft.as_ref().unwrap().parameters["construction_axis"];
    assert_eq!(axis["master"], "Master");
    let helper = axis["name"].as_str().unwrap().to_owned();
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("preview_commit", true);
    let revolution = objects(&n.editor.state)
        .iter()
        .find(|o| o["type"] == "PartDesign::Revolution")
        .unwrap();
    assert_eq!(revolution["properties"]["ReferenceAxis"]["link"], helper);
    assert_eq!(revolution["shape"]["valid"], true);
    assert_eq!(revolution["shape"]["solids"], 1);
}

#[test]
#[ignore = "requires the pinned master-axis observer and CAD_LIFECYCLE_OUTPUT"]
fn masked_master_axis_overlap_warns_at_origin_handle_and_curve() {
    let mut n = Native::new("master-axis-masked-overlap");
    fixture_and_pick(&mut n, "MaskedAxisOverlap");
    for point in [[0., 0., 0.], [0., 100., 0.], [0., 90., 0.]] {
        n.click("Revolve axis (Tab)");
        // Canvas installs the profile overlay on its next ordinary UI frame.
        // Resolve the hit only after that visible state has settled.
        for _ in 0..3 {
            n.frame(vec![]);
        }
        let screen = n.editor.viewport.sketch_screen_point(point);
        let ordinary = n.editor.viewport.sketch_hit(screen).unwrap();
        assert!(ordinary.point != 0 || ordinary.index >= 0);
        assert!(n.editor.viewport.sketch_axis_hit(screen).is_some());
        tutorial_click(&mut n, point);
        assert!(n.editor.property_ui.axis_overlap);
        std::thread::sleep(Duration::from_millis(200));
        n.frame(vec![]);
        n.receive("preview", true);
    }
    n.click("Profile V axis");
    assert!(!n.editor.property_ui.axis_overlap);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    n.click("Cancel");
    n.receive("preview_cancel", true);
}
