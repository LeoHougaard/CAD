//! Actual active-sketch name input over genuine native labels and history.
use super::*;

fn settle(n: &mut Native) {
    n.frame(vec![]);
    n.frame(vec![]);
}

fn seed(case: &str) -> Native {
    let mut n = Native::new(case);
    n.send("new", json!({"name":"SketchNameFixture"}), true);
    let mut edits = vec![
        json!({"op":"add_object","type":"PartDesign::Body","name":"Body"}),
        json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body","origin_plane":"XY_Plane"}),
        json!({"op":"set","object":"Sketch","property":"Label","value":"Original profile"}),
    ];
    edits.extend(
        crate::sketch_commands::draw_snapped_edits(
            "Sketch",
            Tool::Rectangle,
            &[[0., 0., 0.], [12., 10., 0.]],
            0,
            false,
            &[
                Some(SketchElement {
                    index: -1,
                    point: 1,
                }),
                None,
            ],
        )
        .unwrap(),
    );
    edits.extend([
        json!({"op":"constraint","object":"Sketch","kind":"Distance","args":[0,12.]}),
        json!({"op":"constraint","object":"Sketch","kind":"Distance","args":[1,10.]}),
        json!({"op":"add_object","type":"PartDesign::Pad","name":"ProfileLink","body":"Body"}),
        json!({"op":"set","object":"ProfileLink","property":"Profile","value":{"link":"Sketch"}}),
        json!({"op":"set","object":"ProfileLink","property":"Length","value":7.}),
    ]);
    n.transaction(json!(edits));
    assert_eq!(
        object(&n.editor.state, "ProfileLink")["type"],
        "PartDesign::Pad"
    );
    assert_eq!(
        object(&n.editor.state, "ProfileLink")["properties"]["Profile"]["link"],
        "Sketch"
    );
    assert_eq!(object(&n.editor.state, "ProfileLink")["shape"]["solids"], 1);
    assert_eq!(
        object(&n.editor.state, "ProfileLink")["shape"]["valid"],
        true
    );
    let volume = object(&n.editor.state, "ProfileLink")["shape"]["volume"]
        .as_f64()
        .unwrap();
    assert!(volume.is_finite() && volume > 0.);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["degrees_of_freedom"],
        0
    );
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    assert_eq!(n.editor.sketch_ui.name_text, "Original profile");
    n
}

fn model(n: &Native) -> Value {
    let s = object(&n.editor.state, "Sketch");
    json!({"name":s["name"],"type":s["type"],"body":s["body"],
        "support":s["properties"]["AttachmentSupport"],"placement":s["properties"]["Placement"],
        "offset":s["properties"]["AttachmentOffset"],"sketch":s["sketch"],
        "group":object(&n.editor.state,"Body")["properties"]["Group"],
        "tip":object(&n.editor.state,"Body")["properties"]["Tip"],
        "link":object(&n.editor.state,"ProfileLink")["properties"]["Profile"],
        "follower_shape":object(&n.editor.state,"ProfileLink")["shape"]})
}

fn focus_name(n: &mut Native) {
    settle(n);
    let response = n
        .ctx
        .read_response(egui::Id::new("native_sketch_name"))
        .expect("actual name field rendered");
    assert!(response.enabled());
    let pos = response.rect.center();
    fs::write(n.folder.join("name-field-target.json"),serde_json::to_vec_pretty(&json!({"target":[pos.x,pos.y],"rect":[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y]})).unwrap()).unwrap();
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
    assert!(n.editor.sketch_name_focused(&n.ctx));
    tutorial_key(n, Key::A, egui::Modifiers::COMMAND);
}

fn text(n: &mut Native, value: &str) {
    n.frame(vec![egui::Event::Text(value.into())]);
    assert_eq!(n.editor.sketch_ui.name_text, value);
}

fn rename(n: &mut Native, value: &str) {
    focus_name(n);
    text(n, value);
    let token = n.editor.sketch_ui.session.clone();
    tutorial_key(n, Key::Enter, egui::Modifiers::NONE);
    assert!(n.editor.busy, "Enter submits native Label edit");
    n.receive("transaction", true);
    assert_eq!(
        n.editor.sketch_ui.session, token,
        "name Enter must not Finish"
    );
    assert_eq!(object(&n.editor.state, "Sketch")["label"], value);
    settle(n);
    assert_eq!(n.editor.sketch_ui.name_text, value);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn active_sketch_name_enter_undo_redo_and_whole_cancel() {
    let mut n = seed("sketch-name-history");
    let before = model(&n);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    rename(&mut n, "Base");
    assert_eq!(model(&n), before);
    n.send("undo", json!({}), true);
    settle(&mut n);
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Original profile"
    );
    assert_eq!(n.editor.sketch_ui.name_text, "Original profile");
    assert_eq!(model(&n), before);
    n.send("redo", json!({}), true);
    settle(&mut n);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "Base");
    assert_eq!(model(&n), before);
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Original profile"
    );
    assert!(n.editor.sketch_ui.session.is_none());
    assert_eq!(n.editor.state["sketch_edit"], Value::Null);
    assert_eq!(model(&n), before);
    n.events
        .push(json!({"name_session_token":token,"name_cancel_model":model(&n)}));
    n.record();
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn active_sketch_name_finish_save_reopen_keeps_native_identity_and_refs() {
    let mut n = seed("sketch-name-reopen");
    let before = model(&n);
    rename(&mut n, "Base");
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    assert_eq!(model(&n), before);
    let path = n.save("named-profile.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "Base");
    assert_eq!(
        object(&n.editor.state, "Sketch")["properties"]["Label"],
        "Base"
    );
    assert_eq!(model(&n), before);
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    assert_eq!(n.editor.sketch_ui.name_text, "Base");
    rename(&mut n, "Temporary name");
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "Base");
    assert_eq!(model(&n), before);
    assert!(
        !n.editor.dirty,
        "cancel existing edit restores clean saved document"
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn active_sketch_name_escape_text_busy_popup_and_stale_reply_owners() {
    let mut n = seed("sketch-name-owners");
    let before = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Select,
        "fixture starts with Select"
    );
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Rectangle,
        "one G activates rectangle"
    );
    focus_name(&mut n);
    text(&mut n, "Uncommitted");
    tutorial_key(&mut n, Key::Q, egui::Modifiers::NONE);
    assert!(!n.editor.sketch_ui.construction, "text owns Q");
    assert!(n.editor.sketch_name_focused(&n.ctx));
    assert_eq!(n.editor.sketch_ui.tool, Tool::Rectangle);
    fs::write(n.folder.join("before-name-escape.json"), serde_json::to_vec_pretty(&json!({
        "current_name_focus":n.editor.sketch_name_focused(&n.ctx),
        "previous_name_focus":n.ctx.memory(|memory|memory.had_focus_last_frame(egui::Id::new("native_sketch_name"))),
        "tool":"Rectangle", "draft":n.editor.sketch_ui.name_text,"owner":n.editor.sketch_ui.name_owner,
        "session":n.editor.sketch_ui.session
    })).unwrap()).unwrap();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Rectangle,
        "Escape cancels only local name"
    );
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert!(!n.editor.busy);
    assert!(!n.editor.sketch_name_focused(&n.ctx));
    assert!(n.editor.sketch_ui.name_owner.is_none());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Select,
        "second Escape reaches sketch once"
    );
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert!(!n.editor.busy);
    // A legitimate own-sketch transaction changes revision while the name draft
    // is focused. The old draft must never silently bind to the new revision.
    focus_name(&mut n);
    text(&mut n, "Stale name");
    n.transaction(
        json!([{"op":"set","object":"Sketch","property":"Label","value":"External accepted"}]),
    );
    assert!(n.editor.sketch_name_focused(&n.ctx));
    let after = n.editor.state.clone();
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, after);
    assert!(
        n.editor
            .error
            .as_ref()
            .is_some_and(|e| e.contains("sketch changed"))
    );
    settle(&mut n);
    assert_eq!(n.editor.sketch_ui.name_text, "External accepted");
    focus_name(&mut n);
    tutorial_key(&mut n, Key::Backspace, egui::Modifiers::NONE);
    assert!(n.editor.sketch_ui.name_text.is_empty());
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, after);
    assert!(
        n.editor
            .error
            .as_ref()
            .is_some_and(|e| e.contains("cannot be empty"))
    );
    // Hold a real completed native reply until the Enter frame. The pre-poll
    // busy owner must consume that frame, even though poll makes the UI idle.
    focus_name(&mut n);
    text(&mut n, "Busy draft");
    n.editor.send("transaction",json!({"label":"Legitimate owner reply","edits":[{"op":"set","object":"Sketch","property":"Label","value":"Accepted reply"}]}));
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    assert!(reply.accepted);
    assert_eq!(reply.op, "transaction");
    n.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,"state":reply.state,"scenes":reply.scenes,"same_frame_name_input":true}));
    n.record();
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "Accepted reply");
    assert_eq!(n.editor.sketch_ui.session, token);
    assert!(
        n.raw.try_recv().is_err(),
        "name Enter sent no extra job after reply"
    );
    settle(&mut n);
    let after = n.editor.state.clone();
    // Popup is the frame owner. Neither Enter nor text may rename underneath it.
    focus_name(&mut n);
    text(&mut n, "Popup draft");
    egui::Popup::open_id(&n.ctx, egui::Id::new("name-owner-popup"));
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, after);
    egui::Popup::close_all(&n.ctx);
    settle(&mut n);
    n.editor.busy = true;
    let response = n
        .ctx
        .read_response(egui::Id::new("native_sketch_name"))
        .unwrap();
    let pos = response.rect.center();
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
    assert!(!n.editor.sketch_name_focused(&n.ctx));
    n.editor.busy = false;
    settle(&mut n);
    assert_eq!(n.editor.state, after);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn review_finish_pointer_applies_focused_name_before_finishing() {
    let mut n = seed("review-name-focus-finish");
    let before = model(&n);
    focus_name(&mut n);
    text(&mut n, "Name entered before Finish");
    assert!(n.editor.sketch_name_focused(&n.ctx));
    assert!(n.editor.sketch_ui.name_owner.is_some());
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Original profile"
    );
    n.events
        .push(json!({"review_phase":"before_actual_finish_click",
        "focused":n.editor.sketch_name_focused(&n.ctx),
        "draft":n.editor.sketch_ui.name_text,"owner":n.editor.sketch_ui.name_owner,
        "native_label":object(&n.editor.state,"Sketch")["label"]}));
    n.record();
    n.click("Finish");
    assert!(
        n.editor.busy,
        "actual pointer Finish submitted its native job"
    );
    n.receive("transaction", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Name entered before Finish"
    );
    assert!(n.editor.sketch_ui.session.is_some());
    n.receive("sketch_finish", true);
    assert!(n.editor.sketch_ui.session.is_none());
    assert_eq!(
        model(&n),
        before,
        "name interaction preserves the modeled result"
    );
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Name entered before Finish",
        "Finish must retain the focused name draft rather than silently discard it"
    );
}

#[test]
#[ignore = "requires real Editor/native fixture and controlled disconnected model queue"]
fn name_finish_failed_enqueue_retains_draft_and_actual_retry() {
    let mut n = seed("name-failed-enqueue");
    let before = n.editor.state.clone();
    let geometry = model(&n);
    let token = n.editor.sketch_ui.session.clone();
    focus_name(&mut n);
    text(&mut n, "Draft retained after queue failure");
    let original_jobs = n.editor.jobs.clone();
    let (unavailable, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    n.editor.jobs = unavailable;
    n.click("Finish");
    for _ in 0..5 {
        n.frame(vec![]);
    }
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert_eq!(
        n.editor.sketch_ui.name_text,
        "Draft retained after queue failure"
    );
    assert!(n.editor.sketch_ui.name_owner.is_some());
    assert!(n.editor.sketch_ui.pending_name_finish.is_none());
    assert!(
        n.editor
            .error
            .as_deref()
            .is_some_and(|e| e.contains("Model thread stopped"))
    );
    assert!(n.raw.try_recv().is_err());
    n.editor.jobs = original_jobs;
    n.click("Finish");
    n.receive("transaction", true);
    n.receive("sketch_finish", true);
    assert!(n.editor.sketch_ui.session.is_none());
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Draft retained after queue failure"
    );
    assert_eq!(model(&n), geometry);
}
