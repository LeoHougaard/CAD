//! Bounded selected-line Slot tests against the actual Editor and native journal.
use super::*;

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}
fn sk(n: &Native) -> Value {
    object(&n.editor.state, "Sketch")["sketch"].clone()
}
fn diameter(s: &Value) -> &Value {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "Diameter")
        .expect("native diameter")
}
fn setup(name: &str) -> Native {
    let mut n = Native::new(name);
    n.send("new", json!({"name":name.replace('-',"")}), true);
    // Native fixture: approximate free line and the two axis snaps the Editor
    // emits for the source clicks. The length is not a tutorial dimension.
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[-61.23,0,0],"end":[61.47,0,0]},
        {"op":"constraint","object":"Sketch","kind":"PointOnObject","args":[0,1,-1]},
        {"op":"constraint","object":"Sketch","kind":"PointOnObject","args":[0,2,-1]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    n
}
fn select_line(n: &mut Native) {
    let p = [-25., 0., 0.];
    let hit = n
        .editor
        .viewport
        .sketch_hit(n.editor.viewport.sketch_screen_point(p));
    assert_eq!(hit, Some(SketchElement { index: 0, point: 0 }));
    tutorial_click(n, p);
    settle(n);
    assert_eq!(
        n.editor.viewport.sketch_selection,
        vec![SketchElement { index: 0, point: 0 }]
    );
    assert!(n.editor.mirror_selection_owned());
}
fn slot(n: &mut Native) {
    n.click("Offset");
    n.click("Slot");
    n.receive("transaction", true);
    settle(n);
}
fn open_width(n: &mut Native) {
    settle(n);
    let index = diameter(&sk(n))["index"].as_u64().unwrap() as usize;
    let label = format!(
        "\u{2300}{}",
        diameter(&sk(n))["quantity"]["display"].as_str().unwrap()
    );
    let frame = n.frame(vec![]);
    let mut candidates = Vec::new();
    let positions: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => {
                let p = text.galley.rect.translate(text.pos.to_vec2()).center();
                if shape.clip_rect.contains(p) && n.editor.viewport.dimension_hit(p) == Some(index)
                {
                    candidates.push(json!({"text":text.galley.text(),"position":[p.x,p.y]}));
                    (text.galley.text() == label).then_some(p)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    fs::write(
        n.folder.join("diameter-hit-candidates.json"),
        serde_json::to_vec_pretty(&json!({"index":index,"expected":label,"candidates":candidates}))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        positions.len(),
        1,
        "native diameter annotation must be visible and pickable"
    );
    for _ in 0..2 {
        for pressed in [true, false] {
            n.frame(vec![
                egui::Event::PointerMoved(positions[0]),
                egui::Event::PointerButton {
                    pos: positions[0],
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
    }
    assert!(
        n.editor
            .sketch_ui
            .dimension
            .as_ref()
            .is_some_and(|d| d.index == Some(index))
    );
    settle(n);
}
fn enter_value(n: &mut Native, text: &str, accepted: bool) {
    n.frame(vec![egui::Event::Text(text.into())]);
    tutorial_key(n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", accepted);
    settle(n);
}
fn native_slot(s: &Value, width: f64) {
    assert_eq!(s["geometry_count"], 5);
    assert_eq!(s["native_xml"]["persistent_ids"], json!([1, 2, 3, 4, 5]));
    let geometry = s["geometry"].as_array().unwrap();
    assert_eq!(
        geometry
            .iter()
            .map(|g| g["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["line", "arc", "arc", "line", "line"]
    );
    assert_eq!(geometry[0]["construction"], false);
    assert!(geometry[1..].iter().all(|g| g["construction"] == false));
    assert_eq!(diameter(s)["driving"], true);
    assert!((diameter(s)["value"].as_f64().unwrap() - width).abs() < 1e-6);
    for i in [1, 2] {
        assert!((geometry[i]["radius"].as_f64().unwrap() - width / 2.).abs() < 1e-6);
        let source = if i == 1 { "start" } else { "end" };
        for (center, endpoint) in geometry[i]["center"]
            .as_array()
            .unwrap()
            .iter()
            .zip(geometry[0][source].as_array().unwrap())
            .take(2)
        {
            assert!((center.as_f64().unwrap() - endpoint.as_f64().unwrap()).abs() < 1e-6);
        }
    }
    for (arc, arc_end, line, line_end) in [
        (1, "start", 3, "start"),
        (1, "end", 4, "start"),
        (2, "end", 3, "end"),
        (2, "start", 4, "end"),
    ] {
        let xy = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
        let p = xy(&geometry[arc][arc_end]);
        let q = xy(&geometry[line][line_end]);
        let center = xy(&geometry[arc]["center"]);
        let a = xy(&geometry[line]["start"]);
        let b = xy(&geometry[line]["end"]);
        assert!(
            (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6,
            "outer wire is open"
        );
        let dot = (p[0] - center[0]) * (b[0] - a[0]) + (p[1] - center[1]) * (b[1] - a[1]);
        assert!(
            dot.abs() / ((width / 2.) * (b[0] - a[0]).hypot(b[1] - a[1])) < 1e-6,
            "join is not tangent"
        );
    }
    assert_eq!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["type"] == "Tangent")
            .count(),
        4
    );
}

#[test]
#[ignore = "requires frozen actual Editor and native runtime grant"]
fn selected_line_slot_menu_existing_diameter_journal_and_reopen() {
    let mut n = setup("selected-line-slot-editor");
    let baseline = sk(&n);
    select_line(&mut n);
    slot(&mut n);
    let twenty = sk(&n);
    native_slot(&twenty, 20.);
    open_width(&mut n);
    enter_value(&mut n, "70", true);
    let seventy = sk(&n);
    native_slot(&seventy, 70.);
    for endpoint in ["start", "end"] {
        for (a, b) in baseline["geometry"][0][endpoint]
            .as_array()
            .unwrap()
            .iter()
            .zip(seventy["geometry"][0][endpoint].as_array().unwrap())
            .take(2)
        {
            assert!(
                (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-6,
                "source axis endpoint moved on width edit"
            );
        }
    }
    n.send("undo", json!({}), true);
    native_slot(&sk(&n), 20.);
    n.send("undo", json!({}), true);
    assert_eq!(sk(&n)["geometry_count"], 1);
    n.send("redo", json!({}), true);
    native_slot(&sk(&n), 20.);
    n.send("redo", json!({}), true);
    native_slot(&sk(&n), 70.);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("slot.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    native_slot(&sk(&n), 70.);
}

#[test]
#[ignore = "requires frozen actual Editor and native runtime grant"]
fn selected_line_slot_invalid_width_cancel_and_owner_guards() {
    let mut n = setup("selected-line-slot-rejected");
    let baseline = sk(&n);
    n.click("Offset");
    n.click("Slot");
    assert!(!n.editor.busy);
    assert_eq!(sk(&n), baseline);
    select_line(&mut n);
    slot(&mut n);
    let twenty = sk(&n);
    native_slot(&twenty, 20.);
    n.send("inspect", json!({"lifecycle_profile":"capture"}), true);
    open_width(&mut n);
    n.click("Offset");
    assert!(n.editor.sketch_ui.dimension.is_some());
    assert!(!n.editor.busy);
    native_slot(&sk(&n), 20.);
    n.click("20 mm");
    tutorial_key(&mut n, Key::A, egui::Modifiers::COMMAND);
    n.frame(vec![egui::Event::Text("0".into())]);
    assert_eq!(n.editor.sketch_ui.dimension.as_ref().unwrap().value, "0");
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.events.push(json!({"phase":"zero-width-submit", "busy":n.editor.busy,
        "editor_error":n.editor.error,"value":n.editor.sketch_ui.dimension.as_ref().map(|d|d.value.clone()),
        "units":n.editor.state["units"],"state":n.editor.state}));
    n.record();
    if n.editor.busy {
        n.receive("transaction", false);
    } else {
        assert!(
            n.editor
                .error
                .as_deref()
                .is_some_and(|error| !error.is_empty()),
            "no native request and no local rejection was reported"
        );
    }
    settle(&mut n);
    native_slot(&sk(&n), 20.);
    assert_eq!(sk(&n)["native_xml"], twenty["native_xml"]);
    assert!(n.editor.sketch_ui.dimension.is_some());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert!(n.editor.sketch_ui.dimension.is_none());
    n.send("undo", json!({}), true);
    assert_eq!(sk(&n)["geometry_count"], 1);
    n.send("redo", json!({}), true);
    native_slot(&sk(&n), 20.);
    for field in [
        "Constraints",
        "ExpressionEngine",
        "constraints",
        "expressions",
        "persistent_ids",
        "element_map",
        "dof",
    ] {
        assert_eq!(
            sk(&n)["native_xml"].get(field).expect("Redo native field"),
            twenty["native_xml"]
                .get(field)
                .expect("accepted native field")
        );
    }
    n.send("inspect", json!({"lifecycle_profile":"compare"}), true);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(sk(&n)["geometry_count"], 1);
    assert_eq!(sk(&n)["native_xml"], baseline["native_xml"]);
}
