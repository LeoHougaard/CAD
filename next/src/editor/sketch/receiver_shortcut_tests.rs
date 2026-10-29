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
#[ignore = "diagnostic next Receiver M16 source step; expected unsupported inline line-length input is not repaired by S menu"]
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
    n.frame(vec![egui::Event::Text("62".into())]);
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
        "actual_input":"Text 62 then Enter, source NumEnter normalized to platform Enter", "inline_keyboard_owner_before":field_before,
        "dimension_editor_before":dimension_before,"native_length_after":length,"native_state_unchanged_after_text":n.editor.state==before,
        "state":n.editor.state,"full_tutorial_credit":0,"later_source_steps":"untested"}));
    n.record();
    assert!(
        field_before,
        "Receiver M16 requires automatic line-length input after rough second vertex; S menu does not repair this next gap"
    );
    assert_eq!(
        length, 62.,
        "Receiver source entered62mm must drive the actual native vertical line"
    );
}
