use super::*;

#[test]
#[ignore = "source-bound Receiver shortcut-prefix native Editor check"]
fn receiver_s_opens_sketch_shortcut_menu_from_empty_studio() {
    let mut n = Native::new("receiver-s-empty-studio");
    // Empty document setup only. No source geometry, selection or tools seeded.
    // Cloud document-creation input trace is separate from this bounded probe.
    n.send("new", json!({"name":"Receiver"}), true);
    assert!(objects(&n.editor.state).is_empty());
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let before = n.editor.state.clone();
    let p = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Front)
        .expect("visible Front plane target");
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
        n.editor.viewport.selected_standard_plane(),
        Some(crate::viewport::StandardPlane::Front)
    );
    n.frame(vec![egui::Event::PointerMoved(p)]);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    let frame = n.frame(vec![]);
    let labels: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    let opened = egui::Popup::is_any_open(&n.ctx);
    n.events
        .push(json!({"source_video":"pcU5-8PkhGg","source_seconds":466.8,
        "observed_input":"S, then click Sketch in shortcut menu",
        "stage":"plain S from genuinely empty document","menu_open":opened,
        "visible_labels":labels,"native_state_unchanged":n.editor.state == before,
        "full_tutorial_credit":0,"cloud_creation_not_replayed":true}));
    n.record();
    assert_eq!(
        n.editor.state, before,
        "opening tool menu may not mutate native document"
    );
    assert!(
        opened,
        "Receiver source requires plain S shortcut menu before Sketch; later source inputs untested"
    );
}

#[test]
#[ignore = "source-bound Receiver workspace units menu check"]
fn receiver_document_menu_can_reopen_above_workspace_units() {
    let mut n = Native::new("receiver-units-document-menu");
    n.send("new", json!({"name":"ReceiverUnits"}), true);
    assert!(objects(&n.editor.state).is_empty());
    n.click("Document");
    n.click("Workspace units...");
    assert!(n.editor.units_ui.active());
    let before = n.editor.state.clone();
    n.click("Document");
    let opened = egui::Popup::is_any_open(&n.ctx);
    n.events
        .push(json!({"source_video":"pcU5-8PkhGg","source_seconds":459.2,
        "observed_input":"M1 Document menu while Workspace units stays open",
        "menu_open":opened,"units_still_open":n.editor.units_ui.active(),
        "native_state_unchanged":n.editor.state==before,"full_tutorial_credit":0}));
    n.record();
    assert_eq!(n.editor.state, before);
    assert!(n.editor.units_ui.active());
    assert!(
        opened,
        "Receiver M04 requires Document menu above existing Workspace units panel"
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn receiver_line_infers_horizontal_with_endpoint_and_axis_snaps() {
    let mut n = Native::new("receiver-axis-snap-horizontal");
    n.send("new", json!({"name":"ReceiverAxisSnap"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[-20,0,0],"end":[-20,40,0]},
        {"op":"constraint","object":"Sketch","kind":"Vertical","args":[0]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.editor.viewport.fit_sketch();
    for _ in 0..3 {
        n.frame(vec![]);
    }
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_click(&mut n, [-20., 40., 0.]);
    assert_eq!(
        n.editor.sketch_ui.snaps,
        vec![Some(SketchElement { index: 0, point: 2 })]
    );
    tutorial_click(&mut n, [0., 40., 0.]);
    n.receive("transaction", true);
    let sketch = &object(&n.editor.state, "Sketch")["sketch"];
    assert_eq!(sketch["native_xml"]["persistent_ids"], json!([1, 2]));
    let constraints = sketch["constraints"].as_array().unwrap();
    assert!(
        constraints
            .iter()
            .any(|c| c["type"] == "Coincident" && c["first"] == 1 && c["second"] == 0)
    );
    assert!(
        constraints
            .iter()
            .any(|c| c["type"] == "PointOnObject" && c["first"] == 1 && c["second"] == -2)
    );
    assert!(
        constraints
            .iter()
            .any(|c| c["type"] == "Horizontal" && c["first"] == 1)
    );
    n.events.push(
        json!({"case":"endpoint and native axis snapped horizontal line","state":n.editor.state}),
    );
    n.record();
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn axis_snap_alignment_respects_pixel_radius_and_native_vertices() {
    for (case, offset, vertex, horizontal) in [
        ("near-above", -4., false, true),
        ("near-below", 4., false, true),
        ("outside", 8., false, false),
        ("vertex", 0., true, false),
    ] {
        let mut n = Native::new(&format!("axis-snap-{case}"));
        n.send(
            "new",
            json!({"name":format!("AxisSnap{}", case.replace('-', ""))}),
            true,
        );
        let mut edits = vec![
            json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}),
            json!({"op":"geometry","object":"Sketch","kind":"line","start":[-20,0,0],"end":[-20,40,0]}),
            json!({"op":"constraint","object":"Sketch","kind":"Vertical","args":[0]}),
        ];
        if vertex {
            edits.push(json!({"op":"geometry","object":"Sketch","kind":"line","start":[0,40.1,0],"end":[8,40.1,0]}));
        }
        n.transaction(json!(edits));
        n.editor.request_sketch("Sketch");
        n.receive("sketch_begin", true);
        for _ in 0..3 {
            n.frame(vec![]);
        }
        n.editor.viewport.fit_sketch();
        for _ in 0..3 {
            n.frame(vec![]);
        }
        tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
        tutorial_click(&mut n, [-20., 40., 0.]);
        assert_eq!(
            n.editor.sketch_ui.snaps,
            vec![Some(SketchElement { index: 0, point: 2 })]
        );
        let target = if vertex {
            n.editor.viewport.sketch_screen_point([0., 40.1, 0.])
        } else {
            n.editor.viewport.sketch_screen_point([0., 40., 0.]) + egui::vec2(0., offset)
        };
        let expected = if vertex {
            SketchElement { index: 1, point: 1 }
        } else {
            SketchElement {
                index: -2,
                point: 0,
            }
        };
        assert_eq!(n.editor.viewport.sketch_hit(target), Some(expected));
        if vertex {
            let first = n.editor.viewport.sketch_screen_point([-20., 40., 0.]);
            assert!((first.y - target.y).abs() < 6.);
        }
        for pressed in [true, false] {
            n.frame(vec![
                egui::Event::PointerMoved(target),
                egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        n.receive("transaction", true);
        let sketch = &object(&n.editor.state, "Sketch")["sketch"];
        let index = if vertex { 2 } else { 1 };
        let constraints = sketch["constraints"].as_array().unwrap();
        assert_eq!(
            constraints
                .iter()
                .any(|c| c["type"] == "Horizontal" && c["first"] == index),
            horizontal,
            "{case}"
        );
        if vertex {
            assert!(constraints.iter().any(|c| c["type"] == "Coincident"
                && c["first"] == index
                && c["second"] == 1
                && c["second_pos"] == 1));
            let geometry = &sketch["geometry"][index as usize];
            assert_eq!(geometry["end"][1], 40.1);
        } else {
            assert!(
                constraints.iter().any(|c| c["type"] == "PointOnObject"
                    && c["first"] == index
                    && c["second"] == -2)
            );
        }
        n.events.push(
            json!({"case":case,"offset_pixels":offset,"vertex":vertex,"state":n.editor.state}),
        );
        n.record();
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn axis_snap_guard_detects_indirect_axis_endpoint() {
    let mut n = Native::new("axis-snap-indirect-axis");
    n.send("new", json!({"name":"IndirectAxis"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,10,0],"end":[0,30,0]},
        {"op":"constraint","object":"Sketch","kind":"Vertical","args":[0]},
        {"op":"constraint","object":"Sketch","kind":"PointOnObject","args":[0,2,-2]}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.editor.viewport.fit_sketch();
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let before = object(&n.editor.state, "Sketch");
    assert!(crate::sketch_commands::native_point_on_axis(
        before,
        SketchElement { index: 0, point: 1 },
        -2
    ));
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_click(&mut n, [0., 10., 0.]);
    assert_eq!(
        n.editor.sketch_ui.snaps,
        vec![Some(SketchElement { index: 0, point: 1 })]
    );
    let target = n.editor.viewport.sketch_screen_point([0., 10., 0.]) + egui::vec2(4.9, 5.5);
    assert_eq!(
        n.editor.viewport.sketch_hit(target),
        Some(SketchElement {
            index: -2,
            point: 0
        })
    );
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    n.receive("transaction", true);
    let sketch = &object(&n.editor.state, "Sketch")["sketch"];
    assert_eq!(sketch["native_xml"]["persistent_ids"], json!([1, 2]));
    assert!(
        !sketch["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Horizontal" && c["first"] == 1)
    );
    let line = &sketch["geometry"][1];
    assert_ne!(line["start"], line["end"]);
    n.events.push(json!({"case":"indirect axis membership prevents horizontal zero-length inference","state":n.editor.state}));
    n.record();
}
