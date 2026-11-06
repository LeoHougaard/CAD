//! Actual event-driven Editor checks. Fixture construction is explicitly typed
//! native setup; every spacing pick, placement, value, Cancel and Finish is UI.
use super::*;

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}
fn key(n: &mut Native, key: Key) {
    tutorial_key(n, key, egui::Modifiers::NONE);
}
fn sk(n: &Native) -> Value {
    object(&n.editor.state, "Sketch")["sketch"].clone()
}
fn native_facts(n: &Native) -> Value {
    let s = sk(n);
    json!({"geometry":s["native_xml"]["geometry"],"constraints":s["native_xml"]["constraints"],
        "ids":s["native_xml"]["persistent_ids"],"dof":s["native_xml"]["dof"]})
}
fn note(n: &Native, label: &str) {
    fs::write(
        n.folder.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&json!({
        "state":n.editor.state,"ui":n.ui_state(),"native":native_facts(n),
        "selection":n.editor.viewport.sketch_selection}))
        .unwrap(),
    )
    .unwrap();
}
fn click(n: &mut Native, p: [f64; 3]) {
    settle(n);
    tutorial_click(n, p);
}
fn pick_line(n: &mut Native, index: i32) {
    settle(n);
    let s = sk(n);
    let g = s["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["index"] == index)
        .unwrap();
    let p = std::array::from_fn(|i| {
        g["start"][i].as_f64().unwrap() * 0.6 + g["end"][i].as_f64().unwrap() * 0.4
    });
    let screen = n.editor.viewport.sketch_screen_point(p);
    let hit = n
        .editor
        .viewport
        .sketch_hit(screen)
        .expect("real line must be pickable");
    assert_eq!(hit, SketchElement { index, point: 0 });
    click(n, p);
    assert!(n.editor.viewport.sketch_selection.contains(&hit));
    assert!(n.editor.sketch_ui.drag.is_none());
}
fn place(n: &mut Native) {
    settle(n);
    let area = n.editor.viewport.response.as_ref().unwrap().rect;
    let positions = [
        egui::pos2(area.right() - 30., area.bottom() - 40.),
        egui::pos2(area.left() + 40., area.bottom() - 40.),
        egui::pos2(area.right() - 40., area.top() + 70.),
    ];
    let p = positions
        .into_iter()
        .find(|p| {
            n.editor.viewport.sketch_hit(*p).is_none()
                && n.editor.viewport.dimension_hit(*p).is_none()
        })
        .unwrap();
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(
        n.editor.sketch_ui.dimension.as_ref().unwrap().kind,
        "Distance"
    );
    n.receive("quantities", true);
    settle(n);
}
fn value(n: &mut Native, value: &str, accepted: bool) {
    n.frame(vec![egui::Event::Text(value.into())]);
    key(n, Key::Enter);
    n.receive("transaction", accepted);
    settle(n);
}
fn start_pair(n: &mut Native, pair: [i32; 2]) {
    assert!(n.editor.viewport.sketch_selection.is_empty());
    if n.editor.sketch_ui.dimension_tool.is_none() {
        key(n, Key::D);
    }
    pick_line(n, pair[0]);
    pick_line(n, pair[1]);
    place(n);
}
fn seed_rectangle(label: &str, rotated: bool) -> Native {
    let mut n = Native::new(label);
    n.send("new", json!({"name":label.replace('-',"")}), true);
    let mut edits = vec![
        json!({"op":"add_object","type":"PartDesign::Body","name":"Body"}),
        json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}),
    ];
    if rotated {
        edits.push(json!({"op":"set","object":"Body","property":"Placement","value":{"position":[17,-23,31],"rotation":[0.5,0.5,0.5,0.5]}}));
    }
    for (a, b) in [
        ([-5., -4., 0.], [5., -4., 0.]),
        ([5., -4., 0.], [5., 4., 0.]),
        ([5., 4., 0.], [-5., 4., 0.]),
        ([-5., 4., 0.], [-5., -4., 0.]),
    ] {
        edits.push(json!({"op":"geometry","object":"Sketch","kind":"line","start":a,"end":b}));
    }
    for i in 0..4 {
        edits.push(
            json!({"op":"constraint","object":"Sketch","kind":"Coincident","args":[i,2,(i+1)%4,1]}),
        );
        edits.push(json!({"op":"constraint","object":"Sketch","kind":if i%2==0 {"Horizontal"}else{"Vertical"},"args":[i]}));
    }
    edits.push(json!({"op":"constraint","object":"Sketch","kind":"DistanceX","args":[0,1,-5.]}));
    edits.push(json!({"op":"constraint","object":"Sketch","kind":"DistanceY","args":[0,1,-4.]}));
    n.transaction(json!(edits));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    key(&mut n, Key::N);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    assert_eq!(sk(&n)["native_xml"]["dof"], 2);
    assert_eq!(sk(&n)["native_xml"]["persistent_ids"], json!([1, 2, 3, 4]));
    note(&n, "fixture");
    n
}
fn rectangle_flow(label: &str, rotated: bool, reverse: bool) {
    let mut n = seed_rectangle(label, rotated);
    let initial = native_facts(&n);
    start_pair(&mut n, if reverse { [3, 1] } else { [1, 3] });
    value(&mut n, "12 in", true);
    assert_eq!(sk(&n)["native_xml"]["dof"], 1);
    assert_eq!(
        sk(&n)["constraint_count"],
        11,
        "no redundant Parallel may be added to H/V sides"
    );
    note(&n, "width");
    let width = native_facts(&n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    start_pair(&mut n, if reverse { [2, 0] } else { [0, 2] });
    value(&mut n, "50 mm", true);
    assert_eq!(sk(&n)["native_xml"]["dof"], 0);
    assert_eq!(sk(&n)["constraint_count"], 12);
    assert_eq!(sk(&n)["native_xml"]["persistent_ids"], json!([1, 2, 3, 4]));
    note(&n, "fully-constrained");
    let complete = native_facts(&n);
    n.send("undo", json!({}), true);
    assert_eq!(native_facts(&n), width);
    n.send("undo", json!({}), true);
    assert_eq!(native_facts(&n), initial);
    n.send("redo", json!({}), true);
    assert_eq!(native_facts(&n), width);
    n.send("redo", json!({}), true);
    assert_eq!(native_facts(&n), complete);
    key(&mut n, Key::Escape);
    n.click("Finish");
    n.receive("sketch_finish", true);
    let path = n.save("parallel-rectangle.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(native_facts(&n), complete);
    note(&n, "reopened");
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn parallel_whole_edges_grounded_rectangle_history_and_reopen() {
    rectangle_flow("parallel-rectangle", false, false);
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn parallel_whole_edges_reversed_rotated_body_history_and_reopen() {
    rectangle_flow("parallel-rotated", true, true);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn parallel_whole_edges_wrong_units_cancel_and_recovery() {
    let mut n = seed_rectangle("parallel-reject-cancel", false);
    let before = native_facts(&n);
    let history = n.editor.state["undo_count"].clone();
    start_pair(&mut n, [1, 3]);
    value(&mut n, "1 s", false);
    assert_eq!(native_facts(&n), before);
    assert_eq!(n.editor.state["undo_count"], history);
    key(&mut n, Key::Escape);
    assert!(n.editor.sketch_ui.dimension.is_none());
    key(&mut n, Key::Escape);
    n.editor.viewport.clear_sketch_selection();
    settle(&mut n);
    start_pair(&mut n, [3, 1]);
    value(&mut n, "30 mm", true);
    assert_eq!(sk(&n)["native_xml"]["dof"], 1);
    note(&n, "recovered");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn parallel_unconstrained_lines_gain_native_parallel_relation() {
    let mut n = Native::new("parallel-free-lines");
    n.send("new", json!({"name":"ParallelFreeLines"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[2,3,0],"end":[12,13,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[2,8,0],"end":[12,18,0]},
        {"op":"constraint","object":"Sketch","kind":"Block","args":[0]},
        {"op":"constraint","object":"Sketch","kind":"Distance","args":[1,14.142135623730951]},
        {"op":"constraint","object":"Sketch","kind":"DistanceX","args":[1,1,2.]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    assert_eq!(sk(&n)["native_xml"]["dof"], 2);
    start_pair(&mut n, [0, 1]);
    value(&mut n, "10 mm", true);
    assert_eq!(sk(&n)["native_xml"]["dof"], 0);
    assert_eq!(
        sk(&n)["constraints"].as_array().unwrap().last().unwrap()["type"],
        "Parallel"
    );
    assert_eq!(sk(&n)["native_xml"]["persistent_ids"], json!([1, 2]));
    note(&n, "native-parallel");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirrored_whole_edges_use_existing_symmetry_without_redundant_parallel() {
    let mut n = Native::new("parallel-mirrored-edges");
    n.send("new", json!({"name":"ParallelMirroredEdges"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[-41.38,0,0],"end":[-41.38,62,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[41.38,0,0],"end":[41.38,62,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[0,90,0],"construction":true},
        {"op":"constraint","object":"Sketch","kind":"Vertical","args":[0]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[2,1,-1,1]},
        {"op":"constraint","object":"Sketch","kind":"PointOnObject","args":[2,2,-2]},
        {"op":"constraint","object":"Sketch","kind":"Symmetric","args":[0,1,1,1,2]},
        {"op":"constraint","object":"Sketch","kind":"Symmetric","args":[0,2,1,2,2]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    let before = sk(&n);
    let ids = before["native_xml"]["persistent_ids"].clone();
    let dof = before["native_xml"]["dof"].as_i64().unwrap();
    assert_eq!(ids, json!([1, 2, 3]));
    start_pair(&mut n, [0, 1]);
    value(&mut n, "80", true);
    let after = sk(&n);
    assert_eq!(after["native_xml"]["persistent_ids"], ids);
    assert_eq!(after["native_xml"]["dof"].as_i64().unwrap(), dof - 1);
    assert_eq!(
        after["constraint_count"].as_i64().unwrap(),
        before["constraint_count"].as_i64().unwrap() + 1
    );
    let last = after["constraints"].as_array().unwrap().last().unwrap();
    assert_eq!(last["type"], "Distance");
    assert_eq!(last["first"], 0);
    assert_eq!(last["second"], 1);
    assert_eq!(last["driving"], true);
    assert_eq!(last["value"], 80.0);
    note(&n, "mirrored-width");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_shared_axis_endpoint_keeps_linear_dimension_without_parallel() {
    let mut n = Native::new("parallel-mirror-shared-endpoint");
    n.send("new", json!({"name":"ParallelMirrorSharedEndpoint"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[-6,10,0],"end":[0,10,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[6,10,0],"end":[0,10,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[0,20,0],"construction":true},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[6,0,0]},
        {"op":"constraint","object":"Sketch","kind":"Horizontal","args":[0]},
        {"op":"constraint","object":"Sketch","kind":"Horizontal","args":[3]},
        {"op":"constraint","object":"Sketch","kind":"Vertical","args":[2]},
        {"op":"constraint","object":"Sketch","kind":"Symmetric","args":[0,1,1,1,2]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[0,2,1,2]},
        {"op":"constraint","object":"Sketch","kind":"PointOnObject","args":[0,2,2]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    let before = sk(&n);
    let ids = before["native_xml"]["persistent_ids"].clone();
    let dof = before["native_xml"]["dof"].as_i64().unwrap();
    start_pair(&mut n, [3, 1]);
    value(&mut n, "182", true);
    let after = sk(&n);
    assert_eq!(after["native_xml"]["persistent_ids"], ids);
    assert_eq!(after["native_xml"]["dof"].as_i64().unwrap(), dof - 1);
    assert_eq!(
        after["constraint_count"].as_i64().unwrap(),
        before["constraint_count"].as_i64().unwrap() + 1
    );
    let last = after["constraints"].as_array().unwrap().last().unwrap();
    assert_eq!(last["type"], "Distance");
    assert_eq!(last["value"], 182.0);
    note(&n, "shared-endpoint");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn placed_angle_retains_native_rays_display_and_edit() {
    let mut n = Native::new("angle-placed-native-rays");
    n.send("new", json!({"name":"AnglePlacedNativeRays"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[-40,0,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[-40,10,0],"end":[-30,27.32050807568877,0]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    key(&mut n, Key::D);
    pick_line(&mut n, 0);
    pick_line(&mut n, 1);
    let pos = n.editor.viewport.sketch_screen_point([-20., 5., 0.]);
    assert!(n.editor.viewport.sketch_hit(pos).is_none());
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
    assert_eq!(n.editor.sketch_ui.dimension.as_ref().unwrap().kind, "Angle");
    n.receive("quantities", true);
    settle(&mut n);
    value(&mut n, "45", true);
    let sketch = sk(&n);
    assert_eq!(sketch["native_xml"]["persistent_ids"], json!([1, 2]));
    let c = &sketch["constraints"][0];
    assert_eq!(
        (
            c["type"].as_str(),
            c["first"].as_i64(),
            c["first_pos"].as_i64(),
            c["second"].as_i64(),
            c["second_pos"].as_i64()
        ),
        (Some("Angle"), Some(0), Some(2), Some(1), Some(1))
    );
    assert_eq!(c["quantity"]["value"], 45.0);
    let label = c["quantity"]["display"].as_str().unwrap().to_owned();
    key(&mut n, Key::Escape);
    settle(&mut n);
    let frame = n.frame(vec![]);
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                let center = text.galley.rect.translate(text.pos.to_vec2()).center();
                (shape.clip_rect.contains(center)
                    && n.editor.viewport.dimension_hit(center) == Some(0))
                .then_some(center)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        targets.len(),
        1,
        "native 45-degree annotation is visible and pickable"
    );
    for _ in 0..2 {
        for pressed in [true, false] {
            n.frame(vec![
                egui::Event::PointerMoved(targets[0]),
                egui::Event::PointerButton {
                    pos: targets[0],
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
    }
    assert_eq!(
        n.editor.sketch_ui.dimension.as_ref().unwrap().index,
        Some(0)
    );
    settle(&mut n);
    value(&mut n, "50", true);
    let edited = sk(&n);
    let c = &edited["constraints"][0];
    assert_eq!(
        (
            c["first"].as_i64(),
            c["first_pos"].as_i64(),
            c["second"].as_i64(),
            c["second_pos"].as_i64()
        ),
        (Some(0), Some(2), Some(1), Some(1))
    );
    assert_eq!(c["quantity"]["value"], 50.0);
    assert_eq!(edited["native_xml"]["persistent_ids"], json!([1, 2]));
    note(&n, "placed-angle-edited");
}
