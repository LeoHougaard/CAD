use super::*;

fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}
fn snapshot(n: &mut Native, step: &str) {
    let frame = n.frame(vec![]);
    let text: Vec<_> = frame.shapes.iter().filter_map(|s| match &s.shape {
        egui::epaint::Shape::Text(t) => { let r=t.galley.rect.translate(t.pos.to_vec2());
            Some(json!({"text":t.galley.text(),"rect":[r.min.x,r.min.y,r.max.x,r.max.y],"clip":[s.clip_rect.min.x,s.clip_rect.min.y,s.clip_rect.max.x,s.clip_rect.max.y]})) },
        _ => None,
    }).collect();
    fs::write(
        n.folder.join(format!("{step}-actual-ui-text.json")),
        serde_json::to_vec_pretty(&text).unwrap(),
    )
    .unwrap();
    fs::write(
        n.folder.join(format!("{step}-actual-frame-shapes.txt")),
        format!("{:#?}", frame.shapes),
    )
    .unwrap();
}
fn pointer(n: &mut Native, p: egui::Pos2, step: &str) {
    let layer = n.ctx.layer_id_at(p);
    n.events.push(json!({"step":step,"input":"actual egui M1 down/up","screen":[p.x,p.y],"actual_layer":format!("{layer:?}"),"keyboard_focus":format!("{:?}",n.ctx.memory(|m|m.focused())),"ui":n.ui_state()}));
    n.record();
    assert!(layer.is_some(), "no actual visible layer at click");
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
fn label_click(n: &mut Native, label: &str, step: &str, menu: bool) {
    settle(n);
    let frame = n.frame(vec![]);
    let area = if menu {
        Some(
            n.ctx
                .memory(|m| m.area_rect(egui::Id::new("tool_shortcut_menu")))
                .expect("actual shortcut area"),
        )
    } else {
        None
    };
    let targets: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                let r = t.galley.rect.translate(t.pos.to_vec2());
                (s.clip_rect.contains(r.center()) && area.is_none_or(|a| a.contains(r.center())))
                    .then_some(r)
            }
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1, "visible scoped action {label}");
    let r = targets[0];
    n.events.push(json!({"step":step,"target":label,"actual_rect":[r.min.x,r.min.y,r.max.x,r.max.y],"menu_area":area.map(|a|[a.min.x,a.min.y,a.max.x,a.max.y])}));
    snapshot(n, step);
    pointer(n, r.center(), step);
}
fn canvas_probe(
    n: &mut Native,
    step: &str,
    local: [f64; 3],
    metadata: Value,
) -> (egui::Pos2, bool) {
    settle(n);
    let p = n.editor.viewport.sketch_screen_point(local);
    let response = n
        .editor
        .viewport
        .response
        .as_ref()
        .expect("actual canvas response");
    let area = response.rect;
    let canvas = response.layer_id;
    let mut windows = n.ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|l| *l != canvas)
            .filter_map(|l| m.area_rect(l.id).map(|r| (format!("{l:?}"), r)))
            .collect::<Vec<_>>()
    });
    windows.sort_by(|a, b| a.0.cmp(&b.0));
    let actual = n.ctx.layer_id_at(p);
    let blockers: Vec<_> = windows
        .iter()
        .filter(|(_, r)| r.contains(p))
        .map(|(l, r)| json!({"layer":l,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]}))
        .collect();
    let pickable = area.shrink(6.).contains(p) && actual == Some(canvas) && blockers.is_empty();
    n.events.push(json!({"step":step,"phase":"before click, no input injected","actual_local_rough_target":local,"screen":[p.x,p.y],"viewport":[area.min.x,area.min.y,area.max.x,area.max.y],"canvas_layer":format!("{canvas:?}"),"actual_layer":format!("{actual:?}"),"pickable":pickable,"blocking_windows":blockers,"visible_other_areas":windows.iter().map(|(l,r)|json!({"layer":l,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]})).collect::<Vec<_>>(),"metadata":metadata,"state":n.editor.state,"ui":n.ui_state(),"full_tutorial_credit":0}));
    n.record();
    snapshot(n, step);
    (p, pickable)
}
fn stopped(n: &mut Native, step: &str, reason: &str, later: Value) {
    n.events.push(json!({"first_mismatch":step,"reason":reason,"stopped_before_further_source_input":true,"later_source_actions_untested":later,"prefix_native_state":n.editor.state,"full_tutorial_credit":0,"units_panel_full_parity":"not claimed","source_context_not_replayed":"cloud naming and presentation framing"}));
    n.record();
}

fn prefix(case: &str) -> Option<Native> {
    let mut n = Native::new(case);
    assert!(n.editor.state.is_null());
    label_click(&mut n, "New", "blank-native-New", false);
    n.receive("new", true);
    assert!(objects(&n.editor.state).is_empty());
    settle(&mut n);
    let front = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Front)
        .expect("visible Front guide");
    let response = n.editor.viewport.response.as_ref().unwrap();
    assert!(response.rect.shrink(6.).contains(front));
    assert_eq!(n.ctx.layer_id_at(front), Some(response.layer_id));
    n.events.push(json!({"step":"M07","target":"visible Front guide","screen":[front.x,front.y],"viewport":[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y],"layer":format!("{:?}",response.layer_id)}));
    snapshot(&mut n, "M07");
    pointer(&mut n, front, "M07");
    assert_eq!(
        n.editor.viewport.selected_standard_plane(),
        Some(crate::viewport::StandardPlane::Front)
    );
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    label_click(&mut n, "Sketch", "M09", true);
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    settle(&mut n);
    tutorial_key(&mut n, Key::S, egui::Modifiers::NONE);
    label_click(&mut n, "Line", "M12", true);
    let name = n.editor.sketch.clone().unwrap();
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    let source_upper = [-41.38, 41.38 * 125. / 138., 0.];
    for i in 0..3 {
        let step = ["M13", "M14", "M15"][i];
        let local = if i == 0 {
            [0., 0., 0.]
        } else if i == 1 {
            [-41.38, 0., 0.]
        } else {
            let response = n.editor.viewport.response.as_ref().unwrap();
            let area = response.rect;
            let canvas = response.layer_id;
            let windows = n.ctx.memory(|m| {
                m.areas()
                    .visible_layer_ids()
                    .into_iter()
                    .filter(|l| *l != canvas)
                    .filter_map(|l| m.area_rect(l.id))
                    .collect::<Vec<_>>()
            });
            [source_upper[1], 30., 20., 15., 10., 5.]
                .into_iter()
                .map(|y| [-41.38, y, 0.])
                .find(|local| {
                    let p = n.editor.viewport.sketch_screen_point(*local);
                    area.shrink(6.).contains(p)
                        && n.ctx.layer_id_at(p) == Some(canvas)
                        && !windows.iter().any(|r| r.contains(p))
                })
                .unwrap_or(source_upper)
        };
        let (p, ok) = canvas_probe(
            &mut n,
            step,
            local,
            json!({"source_provisional_target":source_upper,"source_seconds":([473.767,477.733,483.800][i]),"adaptation":"M15 only may choose visible lower rough point on same upward vertical inference from finite descending list. Same M1, no added camera action. Source rough length is not typed."}),
        );
        if !ok {
            stopped(
                &mut n,
                step,
                "source-semantic click target outside actual canvas or behind actual UI",
                json!([
                    "M16",
                    "M17",
                    "M18",
                    "497..502 camera gap",
                    "all later tutorial inputs"
                ]),
            );
            return None;
        }
        pointer(&mut n, p, step);
        if i > 0 {
            assert!(n.editor.busy);
            n.receive("transaction", true);
        }
    }
    let before = object(&n.editor.state, &name)["sketch"].clone();
    assert_eq!(before["geometry_count"], 2);
    assert!(!n.ctx.wants_keyboard_input());
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
        n.events.push(json!({"step":"M16","phase":"after separate key-down and Text","digit":digit,"focused_widget":format!("{:?}",n.ctx.memory(|m|m.focused())),"wants_keyboard_input":n.ctx.wants_keyboard_input(),"inline_editing":n.editor.sketch_ui.inline_line.as_ref().is_some_and(|p|p.editing),"state":n.editor.state}));
        n.record();
        if key == Key::Num6 {
            assert!(n.ctx.wants_keyboard_input());
            assert!(
                n.editor
                    .sketch_ui
                    .inline_line
                    .as_ref()
                    .is_some_and(|p| p.editing)
            );
            assert!(n.ctx.memory(|m| m.focused()).is_some());
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
    assert!(n.editor.busy);
    n.receive("transaction", true);
    let after = object(&n.editor.state, &name)["sketch"].clone();
    let g = &after["geometry"][1];
    let length = (0..3)
        .map(|i| (g["end"][i].as_f64().unwrap() - g["start"][i].as_f64().unwrap()).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!((length - 62.).abs() < 1e-7);
    assert_eq!(after["geometry"][0], before["geometry"][0]);
    assert_eq!(
        after["native_xml"]["persistent_ids"],
        before["native_xml"]["persistent_ids"]
    );
    assert_eq!(
        after["constraint_count"].as_u64(),
        before["constraint_count"].as_u64().map(|v| v + 1)
    );
    assert_eq!(
        after["degrees_of_freedom"].as_i64(),
        before["degrees_of_freedom"].as_i64().map(|v| v - 1)
    );
    n.events.push(json!({"step":"M16","actual_input":"Num6 down/Text6/up; Num2 down/Text2/up; Enter down/up","native_length":length,"native_sketch":after,"state":n.editor.state,"full_tutorial_credit":0}));
    n.record();
    snapshot(&mut n, "M16");
    for (step, seconds, target) in [
        ("M17", 492.067, [-41.38 + 14.4, 62. + 14.4, 0.]),
        ("M18", 494.400, [-41.38 + 14.4, 62. + 14.4 + 33.84, 0.]),
    ] {
        let (p, ok) = canvas_probe(
            &mut n,
            step,
            target,
            json!({"source_seconds":seconds,"source_target":"M17 up/right from solved 62 mm top, M18 directly up from M17","source_world_approximation":"M17 +14.4 mm x and +14.4 mm y; M18 +33.84 mm y. Rough observed points only, no typed dimensions.","source_camera_origin":[1254,556],"source_camera_pixels_per_mm":3.335,"source_m17_screen":[1164,302],"source_m18_screen":[1164,187],"camera_input_added":false}),
        );
        if !ok {
            stopped(
                &mut n,
                step,
                "prescribed rising target is outside canvas or behind a real UI window at current initial camera. No pointer, Fit, pan, zoom or viewport mutation injected.",
                json!([
                    "this target M1",
                    "remaining M17/M18",
                    "CtrlLeft at 495.867",
                    "UNREPRESENTED 497..502 camera gesture",
                    "502.8+ rough profile",
                    "Q construction closure",
                    "box selection and Mirror",
                    "all design dimensions and later tutorial inputs"
                ]),
            );
            return None;
        }
        pointer(&mut n, p, step);
        assert!(n.editor.busy);
        n.receive("transaction", true);
    }
    Some(n)
}

#[test]
#[ignore = "exploratory modeling-only Receiver continuation; camera gaps explicitly omitted"]
fn receiver_passive_escape_source_q_closure_one_escape() {
    let Some(mut n) = prefix("receiver-passive-escape-source") else {
        panic!("prefix stopped before Escape");
    };
    let name = n.editor.sketch.clone().unwrap();
    n.events.push(json!({"branch":"EXPLORATORY MODELING ONLY","omitted_source_input":"UNREPRESENTED camera button/gesture at496.1000..496.8333, within old497..502 gap; known Ctrl overlay alone does not define it","strict_branch":"preserved existing source11 test remains stopped after M18 before unknown camera","camera_action_injected":false,"exact_full_input_replay":false,"full_tutorial_credit":0}));
    n.record();
    for (step, seconds, local, source_xy) in [
        ("M19", 502.8, [-282. / 3.335, 466. / 3.335, 0.], [979, 370]),
        (
            "M20",
            505.033,
            [-282. / 3.335, 602. / 3.335, 0.],
            [979, 234],
        ),
        ("M21", 508., [0., 602. / 3.335, 0.], [1261, 234]),
    ] {
        let (p, ok) = canvas_probe(
            &mut n,
            step,
            local,
            json!({"source_seconds":seconds,"source_screen":source_xy,"source_postpan_origin":[1261,836],"source_pixels_per_mm":3.335,"mapping":"approximate untyped source rough point mapped into actual current normal view; no source camera action replayed","camera_input_added":false}),
        );
        if !ok {
            stopped(
                &mut n,
                step,
                "known source rough modeling target unpickable in actual current view; no camera or alternative geometry input added",
                json!([
                    "remaining rough profile",
                    "Q",
                    "construction closure",
                    "Escape",
                    "box selection",
                    "Mirror",
                    "later dimensions"
                ]),
            );
            panic!("source modeling prefix stopped before single Escape control");
        }
        let old = object(&n.editor.state, &name)["sketch"].clone();
        let count = old["geometry_count"].as_u64().unwrap() as usize;
        pointer(&mut n, p, step);
        if !n.editor.busy {
            stopped(
                &mut n,
                step,
                "actual source M1 did not issue native geometry transaction",
                json!([
                    "remaining rough profile",
                    "Q",
                    "construction closure",
                    "Escape",
                    "box selection",
                    "Mirror"
                ]),
            );
            panic!("source modeling prefix stopped before single Escape control");
        }
        n.receive("transaction", true);
        let current = object(&n.editor.state, &name)["sketch"].clone();
        assert_eq!(current["geometry_count"].as_u64(), Some(count as u64 + 1));
        let g = &current["geometry"][count];
        assert_eq!(
            g["start"],
            current["geometry"][count - 1]["end"],
            "new line must use the current native previous endpoint"
        );
        assert_eq!(
            current["native_xml"]["persistent_ids"].as_array().unwrap()[..count],
            old["native_xml"]["persistent_ids"].as_array().unwrap()[..]
        );
        let a = [
            g["start"][0].as_f64().unwrap(),
            g["start"][1].as_f64().unwrap(),
        ];
        let b = [g["end"][0].as_f64().unwrap(), g["end"][1].as_f64().unwrap()];
        let direction = match step {
            "M19" => b[0] < a[0] && b[1] > a[1],
            "M20" => (b[0] - a[0]).abs() < 1e-6 && b[1] > a[1],
            _ => (b[1] - a[1]).abs() < 1e-6 && b[0] > a[0],
        };
        n.events.push(json!({"step":step,"phase":"after actual M1/native reply","direction_matches_source":direction,"native_geometry":g,"native_sketch":current,"full_tutorial_credit":0}));
        n.record();
        if !direction {
            stopped(
                &mut n,
                step,
                "native line direction or vertical/horizontal inference differs from source",
                json!([
                    "remaining rough profile",
                    "Q",
                    "construction closure",
                    "Escape",
                    "box selection",
                    "Mirror"
                ]),
            );
            panic!("source modeling prefix stopped before single Escape control");
        }
    }
    let before = n.editor.state.clone();
    tutorial_key(&mut n, Key::Q, egui::Modifiers::NONE);
    n.events.push(json!({"step":"M22-Q","source_seconds":510.4,"construction":n.editor.sketch_ui.construction,"native_state_unchanged":n.editor.state==before,"tool":format!("{:?}",n.editor.sketch_ui.tool)}));
    n.record();
    if !n.editor.sketch_ui.construction || n.editor.state != before {
        stopped(
            &mut n,
            "M22-Q",
            "source Q did not toggle next line to construction without editing existing native geometry",
            json!(["construction closure", "Escape", "box selection", "Mirror"]),
        );
        panic!("source modeling prefix stopped before single Escape control");
    }
    let (p, ok) = canvas_probe(
        &mut n,
        "M23-construction-close",
        [0., 0., 0.],
        json!({"source_seconds":513.3,"source_screen":[1261,836],"target":"actual origin, construction centerline from upper center"}),
    );
    if !ok {
        stopped(
            &mut n,
            "M23-construction-close",
            "actual origin is unpickable",
            json!(["Escape", "box selection", "Mirror"]),
        );
        panic!("source modeling prefix stopped before single Escape control");
    }
    let old = object(&n.editor.state, &name)["sketch"].clone();
    pointer(&mut n, p, "M23-construction-close");
    if !n.editor.busy {
        stopped(
            &mut n,
            "M23-construction-close",
            "one actual origin click did not commit the construction centerline",
            json!(["Escape", "box selection", "Mirror"]),
        );
        panic!("source modeling prefix stopped before single Escape control");
    }
    n.receive("transaction", true);
    let closed = object(&n.editor.state, &name)["sketch"].clone();
    let g = &closed["geometry"][7];
    assert_eq!(closed["geometry_count"], 8);
    assert_eq!(g["start"], closed["geometry"][6]["end"]);
    assert_eq!(
        closed["native_xml"]["persistent_ids"].as_array().unwrap()[..7],
        old["native_xml"]["persistent_ids"].as_array().unwrap()[..]
    );
    assert!(
        g["end"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_f64().unwrap().abs() < 1e-7),
        "construction endpoint must coincide with native origin"
    );
    assert_eq!(g["construction"], true);
    assert!(
        closed["geometry"].as_array().unwrap()[..7]
            .iter()
            .all(|g| g["construction"] == false)
    );
    let v = &closed["geometry"][1];
    let length = (0..3)
        .map(|i| (v["end"][i].as_f64().unwrap() - v["start"][i].as_f64().unwrap()).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!((length - 62.).abs() < 1e-7);
    assert_eq!(closed["constraint_count"], 15);
    assert_eq!(closed["degrees_of_freedom"], 8);
    n.events.push(json!({"step":"M23-construction-close","native_sketch":closed,"native_62_length":length,"native_geometry_identity_checked":true}));
    n.record();
    snapshot(&mut n, "M23-construction-close");
    let before = n.editor.state.clone();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    n.events.push(json!({"step":"M24-Escape","source_seconds":515.167,"expected":"Select/no active line chain as visible source event074","actual_tool":format!("{:?}",n.editor.sketch_ui.tool),"chain_points":n.editor.sketch_ui.points,"inline_prompt":n.editor.sketch_ui.inline_line.is_some(),"state_unchanged":n.editor.state==before,"ui":n.ui_state()}));
    n.record();
    snapshot(&mut n, "M24-Escape");
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Select,
        "one source Escape ends the drawing tool"
    );
    assert!(n.editor.sketch_ui.points.is_empty() && n.editor.sketch_ui.snaps.is_empty());
    assert!(n.editor.sketch_ui.inline_line.is_none() && n.editor.sketch_ui.session.is_some());
    assert_eq!(
        n.editor.state, before,
        "Escape does not mutate native document or history"
    );
    // A separate non-source control tests that the selection box can start.
    let area = n.editor.viewport.response.as_ref().unwrap().rect;
    let layer = n.editor.viewport.response.as_ref().unwrap().layer_id;
    let start = egui::pos2(area.right() - 80., area.bottom() - 80.);
    let end = egui::pos2(area.right() - 160., area.bottom() - 130.);
    assert_eq!(n.ctx.layer_id_at(start), Some(layer));
    assert_eq!(n.ctx.layer_id_at(end), Some(layer));
    assert!(n.editor.viewport.sketch_hit(start).is_none());
    n.frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    n.frame(vec![egui::Event::PointerMoved(end)]);
    assert!(
        n.editor.sketch_ui.box_start.is_some(),
        "box selection is available immediately after one Escape"
    );
    n.frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(n.editor.sketch_ui.box_start.is_none());
    assert_eq!(n.editor.state, before);
    n.events.push(json!({"control":"separate box-selection availability after single source Escape","state_unchanged":true,"later_source_actions":"untested","full_tutorial_credit":0}));
    n.record();
}
