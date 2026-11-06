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
fn front(n: &mut Native) {
    settle(n);
    let p = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Front)
        .expect("Front guide target");
    pointer(n, p);
    assert_eq!(
        n.editor.viewport.selected_standard_plane(),
        Some(crate::viewport::StandardPlane::Front)
    );
}
fn menu_click(n: &mut Native, label: &str) {
    settle(n);
    let rect = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
        .expect("shortcut menu area");
    let frame = n.frame(vec![]);
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                let r = t.galley.rect.translate(t.pos.to_vec2());
                rect.contains(r.center()).then_some(r.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1, "one scoped shortcut action {label}");
    pointer(n, targets[0]);
}
fn start(n: &mut Native) {
    front(n);
    tutorial_key(n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    menu_click(n, "Sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    settle(n);
}

#[test]
#[ignore = "source-bound Receiver Front/S/Sketch/N/S/Line native Editor route"]
fn receiver_front_s_sketch_n_s_line_uses_native_front_support() {
    let mut n = Native::new("receiver-front-s-sketch-line");
    n.send("new", json!({"name":"ReceiverShortcut"}), true);
    assert!(objects(&n.editor.state).is_empty());
    start(&mut n);
    let name = n.editor.sketch.clone().expect("native sketch");
    let support = object(&n.editor.state, &name)["properties"]["AttachmentSupport"][0]["link"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        object(&n.editor.state, &support)["properties"]["Role"],
        "XZ_Plane"
    );
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    let before = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    menu_click(&mut n, "Line");
    assert_eq!(n.editor.sketch_ui.tool, crate::sketch_commands::Tool::Line);
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    n.events.push(json!({"source_video":"pcU5-8PkhGg","source_steps":"M07..M12","actual":"Front click,S,Sketch click,N,S,Line click","native_front_support":support,"full_tutorial_credit":0}));
    n.record();
}

#[test]
#[ignore = "source-bound S menu input ownership and Escape check"]
fn receiver_s_menu_escape_and_input_owners_preserve_native_session() {
    let mut n = Native::new("receiver-s-menu-owners");
    n.send("new", json!({"name":"ReceiverOwners"}), true);
    start(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    settle(&mut n);
    let p = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
    pointer(&mut n, p);
    assert_eq!(n.editor.sketch_ui.points.len(), 1);
    let before = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    let points = n.editor.sketch_ui.points.clone();
    let selection = n.editor.viewport.sketch_selection.clone();
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert_eq!(n.editor.sketch_ui.points, points);
    assert_eq!(n.editor.viewport.sketch_selection, selection);
    assert_eq!(n.editor.sketch_ui.tool, crate::sketch_commands::Tool::Line);
    for mods in [
        egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        tutorial_key(&mut n, Key::S, mods);
        assert!(!egui::Popup::is_any_open(&n.ctx));
    }
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(super::super::super::workflow_guide::active(&n.ctx));
    assert!(!egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert_eq!(n.editor.sketch_ui.points, points);
    n.events.push(json!({"source_video":"pcU5-8PkhGg","scope":"menu Escape,modified S and Help keep actual partial line/session","full_tutorial_credit":0}));
    n.record();
}

#[test]
#[ignore = "S menu ownership across fields, read panels and real pending native replies"]
fn receiver_s_menu_blocks_fields_units_measure_popups_and_reply_frames() {
    let mut n = Native::new("receiver-s-all-owners");
    n.send("new", json!({"name":"ReceiverAllOwners"}), true);
    settle(&mut n);
    let initial = n.editor.state.clone();
    n.click("Document");
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    assert!(egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    n.click("Document");
    n.click("Workspace units...");
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(n.editor.units_ui.active());
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert_eq!(n.editor.state, initial);
    n.click("Cancel workspace units");
    settle(&mut n);
    assert!(!n.editor.units_ui.active());
    tutorial_key(&mut n, Key::OpenBracket, egui::Modifiers::NONE);
    assert!(n.editor.measurement_is_open());
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(n.editor.measurement_is_open());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert!(!n.editor.measurement_is_open());
    assert_eq!(n.editor.state, initial);
    start(&mut n);
    let before = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    let field = n
        .ctx
        .read_response(egui::Id::new("native_sketch_name"))
        .expect("real sketch label field");
    pointer(&mut n, field.rect.center());
    assert!(n.ctx.wants_keyboard_input());
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    n.frame(vec![egui::Event::Text("s".into())]);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    n.click("View");
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_any_open(&n.ctx));
    assert!(!egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    // A genuine outstanding inspect reply must not grant the same-frame S event.
    n.editor.send("inspect", json!({}));
    assert!(n.editor.busy);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    let reply = n
        .raw
        .recv_timeout(Duration::from_secs(10))
        .expect("real inspect reply");
    assert!(reply.accepted);
    n.events
        .push(json!({"guard":"native reply held for S frame","op":reply.op,"state":reply.state}));
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_any_open(&n.ctx));
    // Busy invalidates an already visible owner even when inspect keeps revision.
    n.editor.send("inspect", json!({}));
    assert!(n.editor.busy);
    let reply = n.raw.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(reply.accepted);
    n.forward.send(reply).unwrap();
    n.frame(vec![]);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert_eq!(n.editor.state, before);
    n.events.push(json!({"scope":"actual fields,Workspace units,Measure,ordinary popup,busy,completed reply and existing popup owner","full_tutorial_credit":0}));
    n.record();
}

#[test]
#[ignore = "Receiver M16 native line length with passive optional input"]
fn receiver_next_step_m16_62mm_line_input_diagnostic() {
    let mut n = Native::new("receiver-next-m16-line62-diagnostic");
    n.send("new", json!({"name":"ReceiverLine62"}), true);
    start(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    menu_click(&mut n, "Line");
    settle(&mut n);
    assert_eq!(n.editor.sketch_ui.tool, crate::sketch_commands::Tool::Line);
    let name = n.editor.sketch.clone().unwrap();
    // Source41.38mm is provisional, not a typed design value. The native
    // drawing field/window layout may occlude the source rough upper vertex.
    // Keep origin and left-axis vertex; choose one visible point on that same
    // vertical inference with no added Fit/pan and no alternate modeling action.
    let source_upper = [-41.38, 41.38 * 125. / 138., 0.];
    for i in 0..3 {
        settle(&mut n);
        let response = n.editor.viewport.response.as_ref().unwrap();
        let area = response.rect;
        let canvas_layer = response.layer_id;
        let mut windows = n.ctx.memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| *l != canvas_layer)
                .filter_map(|l| m.area_rect(l.id).map(|r| (format!("{l:?}"), r)))
                .collect::<Vec<_>>()
        });
        windows.sort_by(|a, b| a.0.cmp(&b.0));
        let unobstructed = |local: [f64; 3]| {
            let p = n.editor.viewport.sketch_screen_point(local);
            area.shrink(6.).contains(p)
                && n.ctx.layer_id_at(p) == Some(canvas_layer)
                && !windows.iter().any(|(_, r)| r.contains(p))
        };
        let local = match i {
            0 => [0., 0., 0.],
            1 => [-41.38, 0., 0.],
            _ => [source_upper[1], 30., 20., 15., 10., 5.]
                .into_iter()
                .map(|y| [-41.38, y, 0.])
                .find(|p| unobstructed(*p))
                .unwrap_or(source_upper),
        };
        let p = n.editor.viewport.sketch_screen_point(local);
        let pickable = unobstructed(local);
        let upper_pixel = n.editor.viewport.sketch_screen_point(source_upper);
        n.events.push(json!({"source_video":"pcU5-8PkhGg","step":(["M13","M14","M15"][i]),
            "source_provisional_upper":source_upper,"source_provisional_screen":[upper_pixel.x,upper_pixel.y],
            "actual_local_rough_input":local,"screen":[p.x,p.y],"viewport":[area.min.x,area.min.y,area.max.x,area.max.y],
            "canvas_layer":format!("{canvas_layer:?}"),"actual_layer":format!("{:?}",n.ctx.layer_id_at(p)),
            "source_provisional_actual_layer":format!("{:?}",n.ctx.layer_id_at(upper_pixel)),
            "visible_other_areas":windows.iter().map(|(l,r)|json!({"layer":l,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]})).collect::<Vec<_>>(),
            "sketch_controls_rect":n.ctx.memory(|m|m.area_rect(egui::Id::new("native_sketch_controls"))).map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),
            "pickable":pickable,"adaptation":"semantic rough vertical point nearest source provisionalY from finite descending list, outside actual UI windows; same one M1 click, no camera edit; not exact source coordinates",
            "full_tutorial_credit":0}));
        n.record();
        assert!(
            pickable,
            "no unobstructed source-semantic rough point; no hidden Fit inserted"
        );
        pointer(&mut n, p);
        n.events.push(json!({"step":(["M13","M14","M15"][i]),"phase":"after actual pointer",
            "busy":n.editor.busy,"dimension":n.editor.sketch_ui.dimension.is_some(),"keyboard_owner":n.ctx.wants_keyboard_input(),"ui":n.ui_state()}));
        n.record();
        if i > 0 {
            assert!(
                n.editor.busy,
                "rough line click did not issue a native transaction; later source value input untested"
            );
            n.receive("transaction", true);
        }
    }
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["geometry_count"],
        2
    );
    let field_before = n.ctx.wants_keyboard_input();
    let dimension_before = n.editor.sketch_ui.dimension.is_some();
    let before = n.editor.state.clone();
    let before_sketch = object(&before, &name)["sketch"].clone();
    for (key, digit) in [(Key::Num6, "6"), (Key::Num2, "2")] {
        n.frame(vec![
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(digit.into()),
        ]);
        if key == Key::Num6 {
            assert!(
                n.ctx.wants_keyboard_input(),
                "first digit must give the real input field keyboard ownership"
            );
            assert!(
                n.editor
                    .sketch_ui
                    .inline_line
                    .as_ref()
                    .is_some_and(|p| p.editing)
            );
        }
        n.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    settle(&mut n);
    if n.editor.busy {
        n.receive("transaction", true);
    }
    let g = object(&n.editor.state, &name)["sketch"]["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["index"] == 1)
        .unwrap();
    let start = g["start"].as_array().unwrap();
    let end = g["end"].as_array().unwrap();
    let length = (0..3)
        .map(|i| (end[i].as_f64().unwrap() - start[i].as_f64().unwrap()).powi(2))
        .sum::<f64>()
        .sqrt();
    n.events.push(json!({"source_video":"pcU5-8PkhGg","step":"M16","seconds":[485.333,486.167],
        "actual_input":"separate Num6 and Num2 key-down/Text/key-up, then Enter; keypad digits and NumEnter normalized by egui", "inline_keyboard_owner_before":field_before,
        "dimension_editor_before":dimension_before,"native_length_after":length,"native_state_unchanged_after_text":n.editor.state==before,
        "state":n.editor.state,"full_tutorial_credit":0,"later_source_steps":"untested"}));
    n.record();
    assert!(!field_before, "idle input must not own keyboard shortcuts");
    assert!(
        (length - 62.).abs() < 1e-7,
        "M16 native vertical length: {length}"
    );
    let after = object(&n.editor.state, &name)["sketch"].clone();
    assert_eq!(
        after["geometry"][0], before_sketch["geometry"][0],
        "horizontal source line moved"
    );
    assert_eq!(
        after["native_xml"]["persistent_ids"],
        before_sketch["native_xml"]["persistent_ids"]
    );
    assert_eq!(
        after["constraint_count"].as_u64(),
        before_sketch["constraint_count"].as_u64().map(|v| v + 1)
    );
    assert_eq!(
        after["degrees_of_freedom"].as_i64(),
        before_sketch["degrees_of_freedom"].as_i64().map(|v| v - 1)
    );
    let solved_end = after["geometry"][1]["end"].clone();
    // Establish the UI seed before the next native solve. Free geometry may
    // move during that solve; native Coincident must retain the chain.
    settle(&mut n);
    assert_eq!(json!(n.editor.sketch_ui.points[0]), solved_end);
    assert_eq!(
        n.editor.sketch_ui.snaps[0],
        Some(SketchElement { index: 1, point: 2 })
    );
    // Separate continuing-chain control, not another claimed source step.
    let next = n.editor.viewport.sketch_screen_point([-20., 20., 0.]);
    pointer(&mut n, next);
    assert!(n.editor.busy, "one canvas click must draw the next segment");
    n.receive("transaction", true);
    let continued = object(&n.editor.state, &name)["sketch"].clone();
    assert_eq!(continued["geometry_count"], 3);
    assert_eq!(
        continued["geometry"][2]["start"], solved_end,
        "chain must start at the solved endpoint"
    );
    assert_eq!(
        continued["geometry"][2]["start"], continued["geometry"][1]["end"],
        "native chain endpoints must remain exactly coincident after solve"
    );
    assert!(
        continued["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Coincident"
                && c["first"] == 2
                && c["first_pos"] == 1
                && c["second"] == 1
                && c["second_pos"] == 2)
    );
    n.events
        .push(json!({"control":"one next canvas click after M16", "sketch":continued}));
    // Separate history/persistence controls after the source prefix.
    tutorial_key(&mut n, Key::Z, egui::Modifiers::COMMAND);
    n.receive("undo", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["native_xml"],
        after["native_xml"]
    );
    tutorial_key(&mut n, Key::Z, egui::Modifiers::COMMAND);
    n.receive("undo", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["native_xml"],
        before_sketch["native_xml"]
    );
    let redo = egui::Modifiers {
        shift: true,
        ..egui::Modifiers::COMMAND
    };
    tutorial_key(&mut n, Key::Z, redo);
    n.receive("redo", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["native_xml"],
        after["native_xml"]
    );
    tutorial_key(&mut n, Key::Z, redo);
    n.receive("redo", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["native_xml"],
        continued["native_xml"]
    );
    n.click("Finish");
    n.receive("sketch_finish", true);
    let saved = object(&n.editor.state, &name)["sketch"].clone();
    let path = n.save("inline-length.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["native_xml"],
        saved["native_xml"]
    );
    n.events
        .push(json!({"control":"native undo/redo and save/reopen after M16"}));
    n.record();
}

fn inline_control_start(n: &mut Native) -> String {
    n.send("new", json!({"name":"InlineControl"}), true);
    start(n);
    tutorial_key(n, Key::N, egui::Modifiers::NONE);
    settle(n);
    tutorial_key(n, Key::S, egui::Modifiers::NONE);
    menu_click(n, "Line");
    settle(n);
    let first = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
    let second = n.editor.viewport.sketch_screen_point([-35., 0., 0.]);
    pointer(n, first);
    pointer(n, second);
    n.receive("transaction", true);
    settle(n);
    n.editor.sketch.clone().unwrap()
}

#[test]
#[ignore = "native optional line prompt, shortcut and one-click controls"]
fn receiver_inline_passive_shortcuts_and_canvas_click() {
    let mut n = Native::new("inline-passive-controls");
    let name = inline_control_start(&mut n);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    assert!(!n.ctx.wants_keyboard_input());
    let before = n.editor.state.clone();
    tutorial_key(&mut n, Key::Q, egui::Modifiers::NONE);
    assert!(n.editor.sketch_ui.construction);
    assert_eq!(n.editor.state, before);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_id_open(
        &n.ctx,
        egui::Id::new("tool_shortcut_menu")
    ));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.state, before);
    let p = n.editor.viewport.sketch_screen_point([-35., 20., 0.]);
    pointer(&mut n, p);
    assert!(n.editor.busy, "one canvas click must draw");
    n.receive("transaction", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["geometry_count"],
        2
    );
    let after = n.editor.state.clone();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.session.is_some());
    assert_eq!(n.editor.state, after);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.session.is_some());
    assert_eq!(n.editor.state, after);
    n.events
        .push(json!({"control":"Q, S, one click, two Escape presses", "state":after}));
    n.record();
}

#[test]
#[ignore = "native invalid units and editable recovery for line prompt"]
fn receiver_inline_invalid_units_remain_editable() {
    let mut n = Native::new("inline-invalid-units");
    let name = inline_control_start(&mut n);
    let baseline = n.editor.state.clone();
    n.frame(vec![egui::Event::Text("17 xyz".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", false);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| p.editing)
    );
    assert_eq!(
        object(&n.editor.state, &name)["sketch"],
        object(&baseline, &name)["sketch"]
    );
    assert_eq!(n.editor.state["undo_count"], baseline["undo_count"]);
    assert_eq!(n.editor.state["redo_count"], baseline["redo_count"]);
    settle(&mut n);
    assert!(n.ctx.wants_keyboard_input());
    tutorial_key(&mut n, Key::A, egui::Modifiers::COMMAND);
    n.frame(vec![egui::Event::Text("25 mm".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", true);
    let sketch = &object(&n.editor.state, &name)["sketch"];
    assert!(
        sketch["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Distance" && c["value"] == 25.)
    );
    n.events
        .push(json!({"control":"invalid unit rejected, corrected explicit mm", "sketch":sketch}));
    // This click snaps the second endpoint onto the vertical axis. The
    // other endpoint is fixed at x=-25, so 20 mm cannot be solved.
    let axis_point = n.editor.viewport.sketch_screen_point([0., 10., 0.]);
    pointer(&mut n, axis_point);
    n.receive("transaction", true);
    let fixed = n.editor.state.clone();
    let constraints = object(&fixed, &name)["sketch"]["constraints"]
        .as_array()
        .unwrap();
    assert!(
        constraints.iter().any(|c| c["type"] == "PointOnObject"
            && c["first"] == 1
            && c["first_pos"] == 2
            && c["second"] == -2),
        "conflict setup requires endpoint on native vertical axis"
    );
    n.frame(vec![egui::Event::Text("20".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", false);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| p.editing)
    );
    assert_eq!(
        object(&n.editor.state, &name)["sketch"],
        object(&fixed, &name)["sketch"]
    );
    assert_eq!(n.editor.state["undo_count"], fixed["undo_count"]);
    assert_eq!(n.editor.state["redo_count"], fixed["redo_count"]);
    n.events.push(json!({"control":"native conflicting length rejected with prompt retained", "state":n.editor.state}));
    n.record();
}

fn inline_modified_text(n: &mut Native, modifiers: egui::Modifiers) {
    n.time += 0.1;
    let mut frame = eframe::Frame::_new_kittest();
    let _ = n.ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 850.),
            )),
            time: Some(n.time),
            modifiers,
            events: vec![
                egui::Event::Key {
                    key: Key::Num6,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Text("6".into()),
            ],
            ..Default::default()
        },
        |ctx| eframe::App::update(&mut n.editor, ctx, &mut frame),
    );
}

#[test]
#[ignore = "native inline owner blocked by other UI owners, modifiers and busy replies"]
fn receiver_inline_other_input_owners_and_busy() {
    let mut n = Native::new("inline-other-owners");
    let _name = inline_control_start(&mut n);
    let baseline = n.editor.state.clone();
    let passive = |n: &Native| {
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| !p.editing)
    };
    assert!(passive(&n));
    for mods in [
        egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        inline_modified_text(&mut n, mods);
        assert!(passive(&n), "modified digit must not acquire input");
        assert_eq!(n.editor.state, baseline);
        settle(&mut n);
    }
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    n.frame(vec![egui::Event::Text("7".into())]);
    assert!(passive(&n));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    assert!(super::super::super::workflow_guide::active(&n.ctx));
    n.frame(vec![egui::Event::Text("7".into())]);
    assert!(passive(&n));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    let field = n
        .ctx
        .read_response(egui::Id::new("native_sketch_name"))
        .unwrap();
    pointer(&mut n, field.rect.center());
    assert!(n.ctx.wants_keyboard_input());
    n.frame(vec![egui::Event::Text("7".into())]);
    assert!(passive(&n));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    n.events.push(json!({"checkpoint":"after Escape from sketch-name field", "passive":passive(&n), "inline_present":n.editor.sketch_ui.inline_line.is_some(), "focus":format!("{:?}", n.ctx.memory(|m| m.focused())), "name_owner":n.editor.sketch_ui.name_owner, "ui":n.ui_state()}));
    n.record();
    assert!(
        passive(&n),
        "sketch-name Escape must preserve the optional line owner"
    );
    assert!(!n.editor.busy, "Escape must not issue a sketch rename");
    assert_eq!(n.editor.state, baseline);
    // Workspace units is disabled while a sketch session is active.
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(passive(&n));
    settle(&mut n);
    assert_eq!(n.editor.state, baseline);
    n.editor.send("inspect", json!({}));
    assert!(n.editor.busy);
    n.frame(vec![egui::Event::Text("7".into())]);
    assert!(passive(&n));
    let reply = n.raw.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(reply.accepted);
    n.forward.send(reply).unwrap();
    n.frame(vec![egui::Event::Text("7".into())]);
    assert!(
        !n.editor.busy && passive(&n),
        "reply completion frame must not steal input"
    );
    assert_eq!(n.editor.state, baseline);
    n.events.push(json!({"control":"modified digits, popup, Help, sketch-name field, disabled Units entry, busy and native completion frame"}));
    n.record();
}

#[test]
#[ignore = "native inline owner rejects changed document, revision, session and geometry ID"]
fn receiver_inline_wrong_owner_invalidated() {
    let mut n = Native::new("inline-wrong-owner");
    let name = inline_control_start(&mut n);
    for (case, target) in [
        ("document", [-60., 50., 0.]),
        ("revision", [-20., 90., 0.]),
        ("session", [20., 50., 0.]),
        ("geometry_id", [60., 90., 0.]),
    ] {
        let state = n.editor.state.clone();
        let session = n.editor.sketch_ui.session.clone();
        match case {
            "document" => n.editor.state["document"] = json!("different document"),
            "revision" => n.editor.state["revision"] = json!(999999),
            "session" => n.editor.sketch_ui.session = Some("different session".into()),
            "geometry_id" => {
                let index = object(&state, &name)["sketch"]["geometry_count"]
                    .as_u64()
                    .unwrap() as usize
                    - 1;
                let object_index = n.editor.state["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|o| o["name"] == name)
                    .unwrap();
                n.editor.state["objects"][object_index]["sketch"]["geometry"][index]["persistent_id"] =
                    json!(999999);
            }
            _ => unreachable!(),
        }
        n.frame(vec![egui::Event::Text("7".into())]);
        assert!(
            n.editor.sketch_ui.inline_line.is_none(),
            "{case} must invalidate owner"
        );
        assert!(!n.editor.busy, "{case} must not send a model transaction");
        n.editor.state = state.clone();
        n.editor.sketch_ui.session = session;
        settle(&mut n);
        assert_eq!(n.editor.state, state);
        let before_sketch = object(&state, &name)["sketch"].clone();
        let count = before_sketch["geometry_count"].as_u64().unwrap() as usize;
        let previous_end = before_sketch["geometry"][count - 1]["end"].clone();
        let previous = std::array::from_fn(|i| previous_end[i].as_f64().unwrap());
        let p = n.editor.viewport.sketch_screen_point(target);
        let previous_screen = n.editor.viewport.sketch_screen_point(previous);
        let delta = p - previous_screen;
        let response = n.editor.viewport.response.as_ref().unwrap();
        let hit = n.editor.viewport.sketch_hit(p);
        let unprojected = n.editor.viewport.sketch_point(p).unwrap();
        n.events.push(json!({"control":"wrong-owner filler before one click","case":case,
            "intended_local":target,"unprojected_native_target":unprojected,
            "screen":[p.x,p.y],"previous_screen":[previous_screen.x,previous_screen.y],
            "pixel_delta":[delta.x,delta.y],"native_hit":hit,
            "viewport":[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y],
            "canvas_layer":format!("{:?}",response.layer_id),
            "actual_layer":format!("{:?}",n.ctx.layer_id_at(p)),
            "state":n.editor.state,"session":n.editor.sketch_ui.session}));
        n.record();
        assert!(response.rect.shrink(8.).contains(p));
        assert_eq!(n.ctx.layer_id_at(p), Some(response.layer_id));
        assert!(hit.is_none() && n.editor.viewport.dimension_hit(p).is_none());
        assert!(
            delta.x.abs() > 12. && delta.y.abs() > 12.,
            "filler must be outside handle and H/V inference thresholds"
        );
        pointer(&mut n, p);
        assert!(
            n.editor.busy,
            "control must create next line via one canvas click"
        );
        n.receive("transaction", true);
        settle(&mut n);
        assert!(n.editor.sketch_ui.inline_line.is_some());
        let after_sketch = object(&n.editor.state, &name)["sketch"].clone();
        assert_eq!(after_sketch["geometry_count"], json!(count + 1));
        assert_eq!(after_sketch["geometry"][count]["start"], previous_end);
        let requested_point = json!(unprojected);
        // The native observer proves the raw requested point survives native
        // construction exactly. Public reply numbers have passed this same
        // serde JSON parser; compare that phase, retaining the raw distinction.
        let public_reply_point: Value =
            serde_json::from_str(&serde_json::to_string(&requested_point).unwrap()).unwrap();
        n.events.push(
            json!({"control":"filler coordinate transport phase","case":case,
            "raw_requested_point":requested_point,"expected_public_reply_point":public_reply_point,
            "actual_public_reply_point":after_sketch["geometry"][count]["end"],
            "raw_to_public_exact":requested_point==after_sketch["geometry"][count]["end"]}),
        );
        n.record();
        assert_eq!(after_sketch["geometry"][count]["end"], public_reply_point);
        assert_eq!(
            after_sketch["native_xml"]["persistent_ids"]
                .as_array()
                .unwrap()[..count],
            before_sketch["native_xml"]["persistent_ids"]
                .as_array()
                .unwrap()[..]
        );
        n.events.push(
            json!({"control":"wrong-owner filler after native click","case":case,
            "native_geometry":after_sketch["geometry"][count],"native_sketch":after_sketch}),
        );
        n.record();
        n.events
            .push(json!({"control":"wrong owner invalidated before typing", "case":case}));
        n.record();
    }
}

#[test]
#[ignore = "native reference dimension follows new inline line length"]
fn receiver_inline_reference_dimension_moves_with_native_solve() {
    let mut n = Native::new("inline-reference-movement");
    let name = inline_control_start(&mut n);
    n.frame(vec![egui::Event::Text("25".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", true);
    settle(&mut n);
    let sketch = &object(&n.editor.state, &name)["sketch"];
    assert_eq!(sketch["constraint_count"], 3);
    assert_eq!(sketch["constraints"][2]["type"], "Distance");
    assert_eq!(sketch["constraints"][2]["value"], 25.);
    n.click("Constraints (3)");
    settle(&mut n);
    let ui = n.frame(vec![]);
    let rows: Vec<_> = ui
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with("3  Distance") => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 1, "one visible native distance row");
    pointer(&mut n, rows[0]);
    n.click("Driving");
    n.receive("transaction", true);
    settle(&mut n);
    let reference = &object(&n.editor.state, &name)["sketch"]["constraints"][2];
    assert_eq!(reference["driving"], false);
    assert_eq!(reference["value"], 25.);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    let origin = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
    let canvas = n.editor.viewport.response.as_ref().unwrap().layer_id;
    let target = (-5..=5)
        .flat_map(|dy| (-5..=5).map(move |dx| egui::vec2(dx as f32, dy as f32)))
        .map(|offset| origin + offset)
        .find(|p| {
            n.ctx.layer_id_at(*p) == Some(canvas)
                && n.editor.viewport.sketch_hit(*p) == Some(SketchElement { index: 0, point: 1 })
        })
        .expect("finite unoccluded hit for existing origin endpoint");
    pointer(&mut n, target);
    n.receive("transaction", true);
    let before = object(&n.editor.state, &name)["sketch"].clone();
    assert_eq!(before["constraints"][2]["value"], 25.);
    assert_eq!(
        before["constraints"].as_array().unwrap().last().unwrap()["type"],
        "Coincident"
    );
    n.frame(vec![egui::Event::Text("30".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", true);
    let after = object(&n.editor.state, &name)["sketch"].clone();
    assert_eq!(after["constraints"][2]["driving"], false);
    assert!((after["constraints"][2]["value"].as_f64().unwrap() - 30.).abs() < 1e-7);
    assert_eq!(
        after["native_xml"]["persistent_ids"],
        before["native_xml"]["persistent_ids"]
    );
    let end = after["geometry"][1]["end"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        n.editor.sketch_ui.points[0].to_vec(),
        end,
        "continuation must use the solved endpoint"
    );
    n.events.push(json!({"control":"existing reference value follows new native line length", "before":before,"after":after}));
    n.record();
}

#[test]
#[ignore = "passive Line and Polyline Escape through actual Editor and native worker"]
fn receiver_passive_escape_line_and_polyline_one_key() {
    let mut n = Native::new("passive-escape-line-polyline");
    let name = inline_control_start(&mut n);
    let first = n.editor.state.clone();
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.points.is_empty() && n.editor.sketch_ui.snaps.is_empty());
    assert!(n.editor.sketch_ui.inline_line.is_none() && n.editor.sketch_ui.session.is_some());
    assert_eq!(n.editor.state, first);
    n.click("Polyline");
    assert_eq!(n.editor.sketch_ui.tool, Tool::Polyline);
    settle(&mut n);
    let p = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
    pointer(&mut n, p);
    let p = n.editor.viewport.sketch_screen_point([-20., 15., 0.]);
    pointer(&mut n, p);
    n.receive("transaction", true);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["geometry_count"],
        2
    );
    let after = n.editor.state.clone();
    assert!(n.editor.sketch_ui.inline_line.is_some());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.points.is_empty() && n.editor.sketch_ui.snaps.is_empty());
    assert!(n.editor.sketch_ui.inline_line.is_none() && n.editor.sketch_ui.session.is_some());
    assert_eq!(n.editor.state, after);
    n.events.push(
        json!({"control":"one Escape ends passive Line and Polyline without native mutation"}),
    );
    n.record();
}

#[test]
#[ignore = "editing Escape retains line and sketch after rejected native units"]
fn receiver_passive_escape_editing_cancel_preserves_chain() {
    let mut n = Native::new("passive-escape-editing");
    let name = inline_control_start(&mut n);
    let before = n.editor.state.clone();
    n.frame(vec![egui::Event::Text("17 xyz".into())]);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("transaction", false);
    settle(&mut n);
    let rejected = n.editor.state.clone();
    assert_eq!(
        object(&rejected, &name)["sketch"],
        object(&before, &name)["sketch"]
    );
    assert_eq!(rejected["undo_count"], before["undo_count"]);
    assert_eq!(rejected["redo_count"], before["redo_count"]);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| p.editing)
    );
    let points = n.editor.sketch_ui.points.clone();
    let snaps = n.editor.sketch_ui.snaps.clone();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(n.editor.sketch_ui.inline_line.is_none());
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert_eq!(n.editor.sketch_ui.points, points);
    assert_eq!(n.editor.sketch_ui.snaps, snaps);
    assert!(n.editor.sketch_ui.session.is_some());
    assert_eq!(n.editor.state, rejected);
    n.events.push(
        json!({"control":"invalid units, editing Escape cancels only prompt", "state":rejected}),
    );
    n.record();
}

#[test]
#[ignore = "popup, sketch-name and busy native reply own Escape before passive line"]
fn receiver_passive_escape_other_owners_and_busy() {
    let mut n = Native::new("passive-escape-owners");
    let _name = inline_control_start(&mut n);
    let before = n.editor.state.clone();
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    assert!(egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    assert_eq!(n.editor.state, before);
    let field = n
        .ctx
        .read_response(egui::Id::new("native_sketch_name"))
        .unwrap();
    pointer(&mut n, field.rect.center());
    n.frame(vec![egui::Event::Text("7".into())]);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    n.editor.send("inspect", json!({}));
    assert!(n.editor.busy);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    let reply = n.raw.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(reply.accepted);
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(n.editor.sketch_ui.inline_line.is_some());
    assert_eq!(n.editor.state, before);
    n.events.push(json!({"control":"popup and name own Escape; native busy and completion frame preserve passive chain"}));
    n.record();
}

fn receiver_constraint_owner_capture(
    n: &mut Native,
    phase: &str,
    frame: egui::FullOutput,
    field: Option<egui::Id>,
) -> Vec<(String, egui::Rect)> {
    let text: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect();
    fs::write(
        n.folder.join(format!("{phase}-actual-frame-shapes.txt")),
        format!("{:#?}", frame.shapes),
    )
    .unwrap();
    fs::write(
        n.folder.join(format!("{phase}-actual-ui-text.json")),
        serde_json::to_vec_pretty(
            &text
                .iter()
                .map(|(t, r)| json!({"text":t,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]}))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    n.events.push(json!({"phase":phase,"focused":format!("{:?}",n.ctx.memory(|m|m.focused())),"wants_keyboard_input":n.ctx.wants_keyboard_input(),"field_id":field.map(|id|format!("{id:?}")),"field_has_focus":field.map(|id|n.ctx.memory(|m|m.has_focus(id))),"field_had_focus_last_frame":field.map(|id|n.ctx.memory(|m|m.had_focus_last_frame(id))),"tool":format!("{:?}",n.editor.sketch_ui.tool),"inline_prompt":n.editor.sketch_ui.inline_line.is_some(),"inline_editing":n.editor.sketch_ui.inline_line.as_ref().map(|p|p.editing),"constraint_name_draft":n.editor.sketch_ui.constraint_name,"selected_constraint":n.editor.sketch_ui.selected_constraint,"chain_points":n.editor.sketch_ui.points,"session":n.editor.sketch_ui.session,"ui":n.ui_state(),"state":n.editor.state}));
    n.record();
    text
}

#[test]
#[ignore = "one actual constraint Name TextEdit Escape ownership probe, not tutorial replay"]
fn receiver_constraint_name_escape_preserves_passive_line_owner() {
    let mut n = Native::new("constraint-name-escape-owner");
    let name = inline_control_start(&mut n);
    assert_eq!(
        object(&n.editor.state, &name)["sketch"]["constraint_count"],
        2
    );
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| !p.editing)
    );
    let baseline = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    let points = n.editor.sketch_ui.points.clone();
    let frame = n.frame(vec![]);
    receiver_constraint_owner_capture(&mut n, "passive-line-before-controls", frame, None);
    n.click("Constraints (2)");
    settle(&mut n);
    let frame = n.frame(vec![]);
    receiver_constraint_owner_capture(&mut n, "actual-constraints-open", frame, None);
    n.click("1  Coincident");
    settle(&mut n);
    assert_eq!(n.editor.sketch_ui.selected_constraint, Some(0));
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| !p.editing)
    );
    assert_eq!(n.editor.state, baseline);
    let frame = n.frame(vec![]);
    let text =
        receiver_constraint_owner_capture(&mut n, "actual-constraint-row-selected", frame, None);
    let labels: Vec<_> = text.iter().filter(|(t, _)| t == "Name").collect();
    let rename: Vec<_> = text.iter().filter(|(t, _)| t == "Rename").collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(rename.len(), 1);
    let left = labels[0].1;
    let right = rename[0].1;
    let p = egui::pos2((left.max.x + right.min.x) * 0.5, left.center().y);
    let panel = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("native_sketch_controls")))
        .unwrap();
    assert!(panel.contains(p));
    n.events.push(json!({"action":"pointer to actual Name TextEdit between visible Name and Rename controls","point":[p.x,p.y],"name_label_rect":[left.min.x,left.min.y,left.max.x,left.max.y],"rename_rect":[right.min.x,right.min.y,right.max.x,right.max.y],"panel":[panel.min.x,panel.min.y,panel.max.x,panel.max.y],"actual_layer":format!("{:?}",n.ctx.layer_id_at(p))}));
    n.record();
    pointer(&mut n, p);
    let field = n
        .ctx
        .memory(|m| m.focused())
        .expect("actual clicked Name field owns focus");
    assert_ne!(field, egui::Id::new("inline_line_length"));
    assert_ne!(field, egui::Id::new("native_sketch_name"));
    let response = n
        .ctx
        .read_response(field)
        .expect("real focused TextEdit response");
    assert!(response.rect.contains(p));
    assert!(n.ctx.wants_keyboard_input());
    n.events.push(json!({"action":"actual focused field verified","field":format!("{field:?}"),"response_rect":[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y],"has_focus":response.has_focus()}));
    n.record();
    let draft = "UnsavedConstraintDraft";
    let frame = n.frame(vec![egui::Event::Text(draft.into())]);
    let text =
        receiver_constraint_owner_capture(&mut n, "typed-real-foreign-field", frame, Some(field));
    assert_eq!(n.editor.sketch_ui.constraint_name, draft);
    assert!(text.iter().any(|(t, _)| t == draft));
    assert!(n.ctx.memory(|m| m.has_focus(field)));
    assert!(n.ctx.wants_keyboard_input());
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    assert!(
        n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| !p.editing)
    );
    assert_eq!(n.editor.sketch_ui.session, token);
    assert_eq!(n.editor.state, baseline);
    let frame = n.frame(vec![]);
    receiver_constraint_owner_capture(&mut n, "before-one-Escape", frame, Some(field));
    let frame = n.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    receiver_constraint_owner_capture(&mut n, "after-Escape-down", frame, Some(field));
    let frame = n.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    receiver_constraint_owner_capture(&mut n, "after-Escape-up", frame, Some(field));
    settle(&mut n);
    let frame = n.frame(vec![]);
    receiver_constraint_owner_capture(&mut n, "after-Escape-settled", frame, Some(field));
    let preserved = n.editor.sketch_ui.tool == Tool::Line
        && n.editor
            .sketch_ui
            .inline_line
            .as_ref()
            .is_some_and(|p| !p.editing)
        && n.editor.sketch_ui.points == points
        && n.editor.sketch_ui.session == token
        && n.editor.state == baseline;
    n.events.push(json!({"finding":if preserved {"PASS: actual foreign Name field Escape preserves passive Line owner"} else {"P2: actual foreign Name field Escape changed passive Line/session/native owner"},"owner_preserved":preserved,"tool":format!("{:?}",n.editor.sketch_ui.tool),"prompt":n.editor.sketch_ui.inline_line.is_some(),"chain_points":n.editor.sketch_ui.points,"session_unchanged":n.editor.sketch_ui.session==token,"native_unchanged":n.editor.state==baseline,"field_focus_after":n.ctx.memory(|m|m.has_focus(field)),"draft_after":n.editor.sketch_ui.constraint_name,"draft_reset_semantics_asserted":false,"tutorial_claim":false,"source_and_product_changes":0}));
    n.record();
    assert!(
        preserved,
        "one Escape from the actual foreign constraint Name TextEdit must not end Line, alter its prompt/chain/session, or mutate native state"
    );
}
