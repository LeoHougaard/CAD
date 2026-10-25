//! New R2 tests. Uses the frozen real Editor/model-thread/native helper.
use super::*;

fn labels(out: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect()
}
fn frame_mod(
    n: &mut Native,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    n.time += 0.1;
    let mut frame = eframe::Frame::_new_kittest();
    n.ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 850.),
            )),
            time: Some(n.time),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| eframe::App::update(&mut n.editor, ctx, &mut frame),
    )
}
fn pointer(n: &mut Native, p: egui::Pos2, button: egui::PointerButton, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        frame_mod(
            n,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
        );
    }
}
fn escape(n: &mut Native) -> egui::FullOutput {
    for pressed in [true, false] {
        n.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    n.frame(vec![])
}
fn open(n: &mut Native, point: [f64; 3]) -> egui::FullOutput {
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let p = n.editor.viewport.sketch_screen_point(point);
    pointer(n, p, egui::PointerButton::Secondary, egui::Modifiers::NONE);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let out = n.frame(vec![]);
    assert!(
        labels(&out).iter().any(|(s, _)| s == "Select other"),
        "chooser must really be open"
    );
    out
}
fn choose(n: &mut Native, name: &str, modifiers: egui::Modifiers) {
    let out = open(
        n,
        if name.contains("Face") {
            [5., 5., 0.]
        } else {
            [5., 0., 0.]
        },
    );
    let p = labels(&out)
        .into_iter()
        .find(|(s, _)| s == name)
        .expect("exact native menu row")
        .1
        .center();
    pointer(n, p, egui::PointerButton::Primary, modifiers);
}
fn report(n: &Native, name: &str, value: Value) {
    fs::write(
        n.folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
fn fixture(case: &str) -> Native {
    let mut n = Native::new(case);
    n.send("new", json!({"name":case.replace('-',"_")}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Rear"},
        {"op":"add_object","type":"Part::Box","name":"Front"},
        {"op":"set","object":"Front","property":"Placement","value":{"position":[0,0,15],"rotation":[0,0,0,1]}},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}
    ]));
    n.save("baseline.FCStd");
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    n
}
fn projection(n: &mut Native) {
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    n.click("External");
    n.click("Project edge or vertex");
    assert!(n.editor.sketch_ui.external);
    // Collapse the actual toolbar menu before opening the viewport menu.
    let out = n.frame(vec![]);
    if labels(&out)
        .iter()
        .any(|(s, _)| s == "Project edge or vertex")
    {
        let p = labels(&out)
            .into_iter()
            .find(|(s, _)| s.starts_with("External") && s != "External geometry")
            .unwrap()
            .1
            .center();
        pointer(n, p, egui::PointerButton::Primary, egui::Modifiers::NONE);
    }
}

fn forward_native_reply_on_next_update(n: &mut Native, op: &str) -> Value {
    let reply = n
        .raw
        .recv_timeout(Duration::from_secs(25))
        .expect("native reply");
    assert_eq!(reply.op, op);
    assert!(reply.accepted, "{:?}", reply.error);
    let state = reply.state.clone();
    n.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,"state":reply.state,"scenes":reply.scenes,
        "r2_observation":"Unmodified completed native reply queued for actual Editor.poll on the Escape frame"}));
    n.record();
    n.forward.send(reply).unwrap();
    state
}
fn popup_open(n: &Native) -> bool {
    egui::Popup::is_id_open(
        &n.ctx,
        egui::Popup::default_response_id(n.editor.viewport.response.as_ref().unwrap()),
    )
}

#[test]
#[ignore = "R2 pinned native runtime"]
fn r2_native_preview_reply_same_escape_frame_preserves_popup_owner() {
    let mut n = fixture("r2-preview-reply-escape");
    n.editor.edit_native_properties("Rear");
    n.editor.draft.as_mut().unwrap().parameters["properties"]["Length"] = json!(12);
    n.editor.preview();
    assert!(n.editor.busy);
    let out = open(&mut n, [9., 0.5, 0.]);
    assert!(labels(&out).iter().any(|(s, _)| s == "Select other"));
    assert!(popup_open(&n));
    let expected = forward_native_reply_on_next_update(&mut n, "preview");
    assert!(popup_open(&n));
    // Actual update polls the completed preview and then sees this Escape.
    let out = escape(&mut n);
    let cancelled = n.editor.busy;
    let after_poll = n.editor.state.clone();
    if cancelled {
        n.receive("preview_cancel", true);
    }
    report(
        &n,
        "result",
        json!({"menu_visible_before_escape":true,"own_popup_open_before_escape":true,
        "unmodified_native_preview_reply_consumed_in_escape_update":true,
        "native_preview_cancel_submitted":cancelled,"menu_open_after_escape":labels(&out).iter().any(|(s,_)|s=="Select other"),
        "preview_reply_revision":expected["revision"],"preview_reply_length":object(&expected,"Rear")["properties"]["Length"],
        "state_after_poll_before_cancel_reply":after_poll,"final_state":n.editor.state,"final_draft":n.ui_state()["draft"]}),
    );
    assert!(
        !cancelled,
        "A completed native preview clears picker before Escape and the displayed chooser loses ownership"
    );
    assert_eq!(n.editor.state, expected);
}

#[test]
#[ignore = "R2 pinned native runtime"]
fn r2_native_projection_reply_same_escape_frame_retains_tool() {
    let mut n = fixture("r2-projection-reply-escape");
    projection(&mut n);
    n.editor.viewport.selection_mode = crate::viewport::SelectionMode::Edges;
    choose(&mut n, "Rear / Edge10", egui::Modifiers::NONE);
    assert!(n.editor.busy);
    let out = open(&mut n, [5., 0., 0.]);
    assert!(labels(&out).iter().any(|(s, _)| s == "Select other"));
    assert!(popup_open(&n));
    let expected = forward_native_reply_on_next_update(&mut n, "transaction");
    assert!(popup_open(&n));
    let out = escape(&mut n);
    let retained = n.editor.sketch_ui.external;
    report(
        &n,
        "result",
        json!({"menu_visible_before_escape":true,"own_popup_open_before_escape":true,
        "unmodified_native_transaction_reply_consumed_in_escape_update":true,
        "tool_retained":retained,"native_state_equals_completed_projection":n.editor.state==expected,
        "menu_open_after_escape":labels(&out).iter().any(|(s,_)|s=="Select other"),"externals":object(&n.editor.state,"Sketch")["sketch"]["externals"]}),
    );
    assert_eq!(n.editor.state, expected);
    assert!(!n.editor.busy);
    assert!(
        retained,
        "A completed native projection clears picker before Escape and the chooser also exits the tool"
    );
}
