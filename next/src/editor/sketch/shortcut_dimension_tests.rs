use super::*;

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}

fn pointer(n: &mut Native, p: egui::Pos2) {
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
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn shortcut_dimension_click_drives_native_parallel_edges() {
    let mut n = Native::new("shortcut-dimension");
    n.send("new", json!({"name":"ShortcutDimension"}), true);
    let mut edits = vec![
        json!({"op":"add_object","type":"PartDesign::Body","name":"Body"}),
        json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}),
    ];
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
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    let before = object(&n.editor.state, "Sketch")["sketch"].clone();
    assert_eq!(before["native_xml"]["persistent_ids"], json!([1, 2, 3, 4]));
    assert_eq!(before["native_xml"]["dof"], 2);
    let session = n.editor.sketch_ui.session.clone();

    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    settle(&mut n);
    let area = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
        .unwrap();
    let frame = n.frame(vec![]);
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Dimension" => {
                let r = t.galley.rect.translate(t.pos.to_vec2());
                (area.contains(r.center()) && s.clip_rect.contains(r.center()))
                    .then_some(r.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1);
    pointer(&mut n, targets[0]);
    assert!(!egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    assert!(n.editor.sketch_ui.dimension_tool.is_some());
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(object(&n.editor.state, "Sketch")["sketch"], before);

    for index in [1, 3] {
        settle(&mut n);
        let sketch = &object(&n.editor.state, "Sketch")["sketch"];
        let g = sketch["geometry"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["index"] == index)
            .unwrap();
        let local = std::array::from_fn(|i| {
            g["start"][i].as_f64().unwrap() * 0.6 + g["end"][i].as_f64().unwrap() * 0.4
        });
        let screen = n.editor.viewport.sketch_screen_point(local);
        let expected = SketchElement { index, point: 0 };
        assert_eq!(n.editor.viewport.sketch_hit(screen), Some(expected));
        pointer(&mut n, screen);
        assert!(n.editor.viewport.sketch_selection.contains(&expected));
        assert!(n.editor.sketch_ui.drag.is_none());
    }
    settle(&mut n);
    let canvas = n.editor.viewport.response.as_ref().unwrap().rect;
    let place = [
        egui::pos2(canvas.right() - 30., canvas.bottom() - 40.),
        egui::pos2(canvas.left() + 40., canvas.bottom() - 40.),
    ]
    .into_iter()
    .find(|p| {
        n.editor.viewport.sketch_hit(*p).is_none() && n.editor.viewport.dimension_hit(*p).is_none()
    })
    .unwrap();
    pointer(&mut n, place);
    assert_eq!(
        n.editor.sketch_ui.dimension.as_ref().unwrap().kind,
        "Distance"
    );
    n.receive("quantities", true);
    settle(&mut n);
    n.frame(vec![egui::Event::Text("80".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", true);
    settle(&mut n);
    let sketch = &object(&n.editor.state, "Sketch")["sketch"];
    assert_eq!(sketch["native_xml"]["persistent_ids"], json!([1, 2, 3, 4]));
    assert_eq!(sketch["native_xml"]["dof"], 1);
    assert_eq!(sketch["constraint_count"], 11);
    let constraint = sketch["constraints"].as_array().unwrap().last().unwrap();
    assert_eq!(constraint["type"], "Distance");
    assert_eq!(constraint["first"], 1);
    assert_eq!(constraint["second"], 3);
    assert_eq!(constraint["driving"], true);
    assert!((constraint["value"].as_f64().unwrap() - 80.).abs() < 1e-8);
    assert_eq!(n.editor.sketch_ui.session, session);
    n.events.push(json!({"action":"S, click Dimension, select two native vertical lines, place horizontal dimension, type 80 and Enter","state":n.editor.state,"ui":n.ui_state()}));
    n.record();
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn shortcut_dimension_clears_eight_selected_native_curves() {
    let mut n = Native::new("shortcut-dimension-eight");
    n.send("new", json!({"name":"ShortcutDimensionEight"}), true);
    let mut edits = vec![
        json!({"op":"add_object","type":"PartDesign::Body","name":"Body"}),
        json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}),
    ];
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
    for x in [15., 20., 25., 30.] {
        edits.push(json!({"op":"geometry","object":"Sketch","kind":"line","start":[x,-3.,0.],"end":[x,3.,0.],"construction":true}));
    }
    n.transaction(json!(edits));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    n.editor.viewport.fit_sketch();
    settle(&mut n);
    let before = object(&n.editor.state, "Sketch")["sketch"].clone();
    assert_eq!(
        before["native_xml"]["persistent_ids"],
        json!([1, 2, 3, 4, 5, 6, 7, 8])
    );
    let initial_dof = before["native_xml"]["dof"].as_i64().unwrap();
    let session = n.editor.sketch_ui.session.clone();

    // D keeps its existing selection-first behavior on a valid native pair.
    n.editor.viewport.sketch_selection = vec![
        SketchElement { index: 1, point: 0 },
        SketchElement { index: 3, point: 0 },
    ];
    tutorial_key(&mut n, Key::D, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.dimension.as_ref().unwrap().kind,
        "Distance"
    );
    assert!(n.editor.sketch_ui.dimension_tool.is_none());
    n.receive("quantities", true);
    settle(&mut n);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert!(n.editor.sketch_ui.dimension.is_none());
    assert_eq!(object(&n.editor.state, "Sketch")["sketch"], before);
    // Recreate the retained eight-curve selection and bind each selected index
    // to its current native persistent ID before invoking the palette.
    n.editor.viewport.sketch_selection = (0..8)
        .map(|index| SketchElement { index, point: 0 })
        .collect();
    let geometry = before["geometry"].as_array().unwrap();
    let selected_ids: Vec<_> = n
        .editor
        .viewport
        .sketch_selection
        .iter()
        .map(|element| {
            assert_eq!(element.point, 0);
            geometry
                .iter()
                .find(|g| g["index"] == element.index)
                .unwrap()["persistent_id"]
                .clone()
        })
        .collect();
    assert_eq!(
        selected_ids,
        vec![
            json!(1),
            json!(2),
            json!(3),
            json!(4),
            json!(5),
            json!(6),
            json!(7),
            json!(8)
        ]
    );
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    settle(&mut n);
    let area = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
        .unwrap();
    let frame = n.frame(vec![]);
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Dimension" => {
                let r = t.galley.rect.translate(t.pos.to_vec2());
                (area.contains(r.center()) && s.clip_rect.contains(r.center()))
                    .then_some(r.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1);
    pointer(&mut n, targets[0]);
    assert!(!egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    assert!(n.editor.sketch_ui.dimension_tool.is_some());
    assert!(n.editor.viewport.sketch_selection.is_empty());
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(object(&n.editor.state, "Sketch")["sketch"], before);

    for index in [1, 3] {
        settle(&mut n);
        let sketch = &object(&n.editor.state, "Sketch")["sketch"];
        let g = sketch["geometry"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["index"] == index)
            .unwrap();
        let local = std::array::from_fn(|i| {
            g["start"][i].as_f64().unwrap() * 0.6 + g["end"][i].as_f64().unwrap() * 0.4
        });
        let screen = n.editor.viewport.sketch_screen_point(local);
        let expected = SketchElement { index, point: 0 };
        assert_eq!(n.editor.viewport.sketch_hit(screen), Some(expected));
        pointer(&mut n, screen);
        assert!(n.editor.viewport.sketch_selection.contains(&expected));
        assert!(n.editor.sketch_ui.drag.is_none());
    }
    settle(&mut n);
    let canvas = n.editor.viewport.response.as_ref().unwrap().rect;
    let place = [
        egui::pos2(canvas.right() - 30., canvas.bottom() - 40.),
        egui::pos2(canvas.left() + 40., canvas.bottom() - 40.),
    ]
    .into_iter()
    .find(|p| {
        n.editor.viewport.sketch_hit(*p).is_none() && n.editor.viewport.dimension_hit(*p).is_none()
    })
    .unwrap();
    pointer(&mut n, place);
    assert_eq!(
        n.editor.sketch_ui.dimension.as_ref().unwrap().kind,
        "Distance"
    );
    n.receive("quantities", true);
    settle(&mut n);
    n.frame(vec![egui::Event::Text("80".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", true);
    settle(&mut n);
    let sketch = &object(&n.editor.state, "Sketch")["sketch"];
    assert_eq!(
        sketch["native_xml"]["persistent_ids"],
        json!([1, 2, 3, 4, 5, 6, 7, 8])
    );
    assert_eq!(
        sketch["native_xml"]["dof"].as_i64().unwrap(),
        initial_dof - 1
    );
    assert_eq!(sketch["constraint_count"], 11);
    let constraint = sketch["constraints"].as_array().unwrap().last().unwrap();
    assert_eq!(constraint["type"], "Distance");
    assert_eq!(constraint["first"], 1);
    assert_eq!(constraint["second"], 3);
    assert_eq!(constraint["driving"], true);
    assert!((constraint["value"].as_f64().unwrap() - 80.).abs() < 1e-8);
    assert_eq!(n.editor.sketch_ui.session, session);
    n.events.push(json!({"action":"S, click Dimension, select two native vertical lines, place horizontal dimension, type 80 and Enter","state":n.editor.state,"ui":n.ui_state()}));
    n.record();
}
