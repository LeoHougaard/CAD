//! Actual Editor/model thread/native measurement of an obscured picked face.
use super::*;

fn pointer(n: &mut Native, position: egui::Pos2, button: egui::PointerButton) {
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
}

fn open_chooser(n: &mut Native) -> egui::FullOutput {
    n.frame(vec![]);
    let point = n.editor.viewport.sketch_screen_point([5., 5., 0.]);
    pointer(n, point, egui::PointerButton::Secondary);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.frame(vec![])
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn select_other_native_editor_picks_and_measures_obscured_face_read_only() {
    let mut n = Native::new("select-other-native");
    n.send("new", json!({"name":"SelectOtherNative"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Rear"},
        {"op":"set","object":"Rear","property":"Length","value":10},
        {"op":"set","object":"Rear","property":"Width","value":10},
        {"op":"set","object":"Rear","property":"Height","value":10},
        {"op":"add_object","type":"Part::Box","name":"Front"},
        {"op":"set","object":"Front","property":"Length","value":10},
        {"op":"set","object":"Front","property":"Width","value":10},
        {"op":"set","object":"Front","property":"Height","value":10},
        {"op":"set","object":"Front","property":"Placement","value":{"position":[0,0,15],"rotation":[0,0,0,1]}}
    ]));
    n.save("baseline.FCStd");
    let original = n.editor.state.clone();
    let path = n.editor.path.clone();
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let point = n.editor.viewport.sketch_screen_point([5., 5., 0.]);
    pointer(&mut n, point, egui::PointerButton::Primary);
    assert_eq!(n.editor.selected_object.as_deref(), Some("Front"));
    let front = n.editor.viewport.selected.clone();
    let output = open_chooser(&mut n);
    let choices: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text().starts_with("Rear / Face") => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        choices.len(),
        2,
        "Both native rear-box intersections must be selectable"
    );
    assert_eq!(n.editor.viewport.selected, front);
    let (label, rect) = &choices[0];
    n.frame(vec![egui::Event::PointerMoved(rect.center())]);
    assert_eq!(n.editor.viewport.selected, front);
    assert_eq!(n.editor.state, original);
    assert_eq!(n.editor.path, path);
    assert!(!n.editor.dirty && !n.editor.busy);
    pointer(&mut n, rect.center(), egui::PointerButton::Primary);
    let selected = n
        .editor
        .viewport
        .selected
        .clone()
        .expect("Chosen native face");
    assert_eq!(selected.object, "Rear");
    assert_eq!(
        format!("{} / {}", selected.object, selected.subelement),
        *label
    );
    assert!(
        n.editor
            .viewport
            .topology_references("Rear")
            .contains(&selected.subelement)
    );
    assert_eq!(n.editor.selected_object.as_deref(), Some("Rear"));
    assert_eq!(n.editor.state, original);
    n.click("Measure");
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    assert_eq!(reply.op, "measure");
    assert!(reply.accepted, "{:?}", reply.error);
    let measurement = reply.data.clone();
    n.events
        .push(json!({"op":reply.op,"data":measurement,"state":reply.state}));
    n.record();
    n.forward.send(reply).unwrap();
    n.editor.poll();
    let selection = &measurement["selections"][0];
    assert_eq!(selection["reference"]["object"], "Rear");
    assert_eq!(selection["reference"]["subelement"], selected.subelement);
    assert_eq!(selection["shape_type"], "Face");
    let area = selection["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["key"] == "area")
        .unwrap();
    assert!((area["value"].as_f64().unwrap() - 100.).abs() < 1e-9);
    assert!((selection["center"]["value"][2].as_f64().unwrap() - 10.).abs() < 1e-9);
    assert_eq!(n.editor.state, original);
    assert_eq!(n.editor.path, path);
    assert!(!n.editor.dirty && !n.editor.busy);
    fs::write(
        n.folder.join("choice-and-measurement.json"),
        serde_json::to_vec_pretty(
            &json!({"selection":{"object":selected.object,"subelement":selected.subelement},
                "measurement":measurement,"state":original}),
        )
        .unwrap(),
    )
    .unwrap();
    n.editor.measurement_escape();
    n.save("selected.FCStd");
}
