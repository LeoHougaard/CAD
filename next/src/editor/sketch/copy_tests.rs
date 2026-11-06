//! Actual editor menus, task widgets and persistent worker, without a desktop.
use super::*;
use crate::sketch_commands::SketchElement;

fn setup(case: &str) -> Native {
    let mut n = Native::new(case);
    n.send("new", json!({"name":case.replace('-',"_")}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"},
        {"op":"geometry","object":"Sketch","kind":"circle","center":[5,5,0],"radius":2},
        {"op":"constraint","object":"Sketch","kind":"Radius","args":[0,2]},
        {"op":"constraint_state","object":"Sketch","index":0,"name":"SourceRadius"},
        {"op":"expression","object":"Sketch","property":"Constraints.SourceRadius","expression":"2 mm"},
        {"op":"geometry","object":"Sketch","kind":"point","position":[2,3,0],"construction":true},
        {"op":"add_object","type":"PartDesign::Pad","name":"Pad","body":"Body"},
        {"op":"set","object":"Body","property":"AllowCompound","value":true},
        {"op":"set","object":"Pad","property":"Profile","value":{"link":"Sketch"}},
        {"op":"set","object":"Pad","property":"Length","value":5}
    ]));
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower"},
        {"op":"external_geometry","object":"Follower","source":"Sketch","subelement":"Edge1"}
    ]));
    n.save("baseline.FCStd");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n
}
fn start(n: &mut Native, name: &str) {
    n.editor.viewport.sketch_selection = vec![SketchElement { index: 0, point: 0 }];
    n.click("Edit tools");
    n.click(name);
    assert!(n.editor.sketch_ui.task.is_some());
    settle(n);
}
fn preview(n: &mut Native, valid: bool) {
    settle(n);
    assert_eq!(task_state(n)["valid"], valid);
}
fn xml(n: &Native) -> Value {
    object(&n.editor.state, "Sketch")["sketch"]["native_xml"].clone()
}
fn count(n: &Native) -> u64 {
    object(&n.editor.state, "Sketch")["sketch"]["geometry_count"]
        .as_u64()
        .unwrap()
}

fn pending(n: &mut Native) {
    let reply = n
        .raw
        .recv_timeout(Duration::from_secs(25))
        .expect("native task reply");
    assert!(reply.accepted, "{:?}", reply.error);
    assert!(matches!(reply.op.as_str(), "quantities" | "preview"));
    n.events.push(
        json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,
        "state":reply.state,"scenes":reply.scenes}),
    );
    n.record();
    n.forward.send(reply).unwrap();
    n.editor.poll();
    let state = n.ui_state();
    n.events.last_mut().unwrap()["editor"] = state;
    n.record();
}

fn task_state(n: &Native) -> Value {
    n.editor.sketch_ui.task.as_ref().unwrap().test_state()
}

fn settle(n: &mut Native) -> egui::FullOutput {
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    loop {
        assert!(std::time::Instant::now() < deadline, "task did not settle");
        if n.editor.busy {
            pending(n);
            continue;
        }
        let frame = n.frame(vec![]);
        if !n.editor.busy && task_state(n)["pending"] == false {
            assert_eq!(task_state(n)["enabled"], true);
            return frame;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn toggle(n: &mut Native, label: &str, key: &str, expected: Value) {
    settle(n);
    n.click(label);
    assert_eq!(task_state(n)["parameters"][key], expected, "{label} input");
    settle(n);
}

fn edit_field(n: &mut Native, label: &str, key: &str, text: &str) {
    let frame = settle(n);
    let area = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("native_sketch_task")))
        .unwrap();
    let texts: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .filter(|(_, r)| area.contains(r.center()))
        .collect();
    let at = texts
        .iter()
        .position(|(t, _)| t == label)
        .expect("field label");
    let pos = texts[at + 1].1.center();
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
    settle(n);
    let modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    n.frame(vec![
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
        egui::Event::Text(text.into()),
    ]);
    settle(n);
    n.frame(vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    settle(n);
    let state = task_state(n);
    let value = state["parameters"].pointer(key).unwrap();
    if let Ok(number) = text.parse::<i64>() {
        assert_eq!(value.as_i64(), Some(number), "{label} input");
    } else {
        assert_eq!(
            value["quantity_input"], text,
            "{label} native quantity input"
        );
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn copy_editor_widgets_history_replacement_and_reopen() {
    let mut n = setup("copy-editor");
    let baseline = xml(&n);
    start(&mut n, "Copy");
    preview(&mut n, true);
    assert_eq!(count(&n), 3);
    let accepted = xml(&n);
    // Failure after native mutation must recover the accepted preview exactly.
    let token = n.editor.state["preview"]["token"].clone();
    n.send(
        "preview",
        json!({"token":token,"edits":[
        {"op":"copy_geometry","object":"Sketch","indices":[0],"displacement":[20,0,0]},
        {"op":"move_geometries","object":"Sketch","indices":[-3],"displacement":[1,0,0]}]}),
        false,
    );
    assert_eq!(xml(&n), accepted);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(xml(&n), baseline);
    start(&mut n, "Clone");
    preview(&mut n, true);
    assert!(
        object(&n.editor.state, "Sketch")["sketch"]["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Equal")
    );
    n.click("Apply");
    n.receive("preview_commit", true);
    let cloned = xml(&n);
    n.send("undo", json!({}), true);
    assert_eq!(xml(&n), baseline);
    start(&mut n, "Move");
    assert_eq!(task_state(&n)["deferred"], true);
    assert!(n.editor.state["preview"].is_null());
    let redo = n.editor.state["redo_count"].clone();
    n.click("Cancel");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state["redo_count"], redo);
    n.send("redo", json!({}), true);
    assert_eq!(xml(&n), cloned);
    n.send("undo", json!({}), true);
    start(&mut n, "Move");
    toggle(&mut n, "All internal geometry", "all_geometry", json!(true));
    assert_eq!(task_state(&n)["deferred"], true);
    assert!(n.editor.state["preview"].is_null());
    assert!(!n.editor.busy);
    n.click("Apply");
    n.receive("transaction", true);
    let moved = xml(&n);
    assert_eq!(moved["persistent_ids"], baseline["persistent_ids"]);
    n.send("undo", json!({}), true);
    assert_eq!(xml(&n), baseline);
    n.send("redo", json!({}), true);
    assert_eq!(xml(&n), moved);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("copy-final.FCStd");
    let saved = xml(&n);
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(xml(&n), saved);
    assert!(
        object(&n.editor.state, "Follower")["sketch"]["externals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["missing"] != true)
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn copy_array_controls_and_session_cancel() {
    let mut n = setup("copy-array-editor");
    let baseline = xml(&n);
    start(&mut n, "Rectangular array");
    toggle(&mut n, "Clone", "clone", json!(true));
    toggle(
        &mut n,
        "Equal vertical/horizontal spacing",
        "perpendicular_scale",
        json!(1.0),
    );
    edit_field(&mut n, "Columns", "/columns", "3");
    edit_field(&mut n, "Rows", "/rows", "2");
    edit_field(&mut n, "X", "/displacement/0", "1 in");
    edit_field(&mut n, "Y", "/displacement/1", "-2 cm");
    let form = task_state(&n);
    assert_eq!(form["parameters"]["columns"], 3);
    assert_eq!(form["parameters"]["rows"], 2);
    assert_eq!(
        form["parameters"]["displacement"][0]["quantity_input"],
        "1 in"
    );
    assert_eq!(
        form["parameters"]["displacement"][1]["quantity_input"],
        "-2 cm"
    );
    fs::write(
        n.folder.join("array-input.json"),
        serde_json::to_vec_pretty(&form).unwrap(),
    )
    .unwrap();
    preview(&mut n, true);
    assert_eq!(count(&n), 12);
    toggle(
        &mut n,
        "Constrain inter-element separation",
        "constrain_displacement",
        json!(false),
    );
    preview(&mut n, true);
    assert_eq!(count(&n), 7);
    n.click("Apply");
    n.receive("preview_commit", true);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(xml(&n), baseline);
}
