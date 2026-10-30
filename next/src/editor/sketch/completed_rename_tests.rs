//! Completed sketch rename through the real Editor and native worker.
use super::*;

fn settle(n: &mut Native) {
    n.frame(vec![]);
    n.frame(vec![]);
}
fn seed(case: &str) -> Native {
    let mut n = Native::new(case);
    n.send("new", json!({"name":"RenameFixture"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"set","object":"Sketch","property":"Label","value":"Original profile"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[12,0,0]}
    ]));
    settle(&mut n);
    n.click("Original profile");
    assert_eq!(n.editor.tree_selections(), &["Sketch".to_owned()]);
    n
}
fn open(n: &mut Native) {
    tutorial_key(n, Key::N, egui::Modifiers::SHIFT);
    assert!(super::super::super::rename::is_open(&n.ctx));
    assert!(
        n.ctx
            .memory(|m| m.has_focus(egui::Id::new("completed_sketch_rename_field")))
    );
    assert!(n.editor.sketch_ui.session.is_none());
}
fn typed(n: &mut Native, text: &str) {
    n.frame(vec![egui::Event::Text(text.into())]);
}
fn sketch_model(n: &Native) -> Value {
    let s = object(&n.editor.state, "Sketch");
    json!({"name":s["name"],"type":s["type"],"sketch":s["sketch"],"placement":s["properties"]["Placement"]})
}

#[test]
#[ignore = "actual Editor tree, shortcut, native Label history and Save/Open"]
fn completed_sketch_shift_n_label_history_and_save_open() {
    let mut n = seed("completed-sketch-shift-n-history");
    let before = sketch_model(&n);
    open(&mut n);
    typed(&mut n, "MASTER LAYO");
    tutorial_key(&mut n, Key::Backspace, egui::Modifiers::NONE);
    typed(&mut n, "OUT");
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(n.editor.busy, "Enter submits native Label");
    n.receive("transaction", true);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "MASTER LAYOUT");
    assert_eq!(sketch_model(&n), before);
    assert!(n.editor.sketch_ui.session.is_none());
    assert_eq!(n.editor.state["sketch_edit"], Value::Null);
    settle(&mut n);
    tutorial_key(&mut n, Key::Z, egui::Modifiers::COMMAND);
    n.receive("undo", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["label"],
        "Original profile"
    );
    assert_eq!(sketch_model(&n), before);
    settle(&mut n);
    tutorial_key(
        &mut n,
        Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    n.receive("redo", true);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "MASTER LAYOUT");
    assert_eq!(sketch_model(&n), before);
    let path = n.save("master-layout.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "MASTER LAYOUT");
    assert_eq!(sketch_model(&n), before);
    n.events.push(json!({"source":"pcU5-8PkhGg 609.067-614.533", "scope":"tree pick, Shift+N, type, Enter, Undo/Redo/Save/Open", "full_tutorial_credit":0}));
    n.record();
}

#[test]
#[ignore = "actual Editor Escape, exact modifiers, stale owner and busy reply"]
fn completed_sketch_rename_cancel_scope_and_stale_owner() {
    let mut n = seed("completed-sketch-shift-n-scope");
    let before = n.editor.state.clone();
    for mods in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
        egui::Modifiers {
            shift: true,
            ctrl: true,
            ..egui::Modifiers::NONE
        },
    ] {
        tutorial_key(&mut n, Key::N, mods);
        assert!(!super::super::super::rename::is_open(&n.ctx));
        assert_eq!(n.editor.state, before);
    }
    open(&mut n);
    typed(&mut n, "Cancelled");
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    assert_eq!(n.editor.state, before);
    settle(&mut n);
    open(&mut n);
    typed(&mut n, "Stale draft");
    n.transaction(json!([{ "op":"remove_object", "object":"Sketch" }]));
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"set","object":"Sketch","property":"Label","value":"Replacement"}
    ]));
    let replacement = n.editor.state.clone();
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, replacement);
    assert_eq!(object(&n.editor.state, "Sketch")["label"], "Replacement");
    assert!(!super::super::super::rename::is_open(&n.ctx));
    settle(&mut n);
    n.click("Replacement");
    n.editor.send("inspect", json!({}));
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(
        !super::super::super::rename::is_open(&n.ctx),
        "reply frame owns the input"
    );
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, replacement);
    n.events.push(json!({"scope":"exact modifiers, Escape, same-name replacement and native reply frame", "full_tutorial_credit":0}));
    n.record();
}

#[test]
#[ignore = "actual Editor popups, Help, units, Measure, file dialog and text ownership"]
fn completed_sketch_rename_respects_other_input_owners() {
    let mut n = seed("completed-sketch-shift-n-input-owners");
    let before = n.editor.state.clone();
    for menu in ["View", "Document"] {
        n.click(menu);
        assert!(egui::Popup::is_any_open(&n.ctx));
        tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
        assert!(!super::super::super::rename::is_open(&n.ctx));
        tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
        settle(&mut n);
    }
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    assert!(super::super::super::workflow_guide::active(&n.ctx));
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    n.click("Document");
    n.click("Workspace units...");
    assert!(n.editor.units_ui.active());
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    n.click("Cancel workspace units");
    settle(&mut n);
    tutorial_key(&mut n, Key::OpenBracket, egui::Modifiers::NONE);
    assert!(n.editor.measurement_is_open());
    if n.editor.busy {
        let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
        assert_eq!(reply.op, "measure");
        n.forward.send(reply).unwrap();
        n.frame(vec![]);
    }
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    settle(&mut n);
    n.editor.file_dialog(false);
    settle(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    assert!(n.editor.file_dialog.is_some());
    n.editor.cancel_file_browser();
    settle(&mut n);
    assert_eq!(n.editor.state, before);
    open(&mut n);
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    typed(&mut n, "Only text owns keys");
    assert!(super::super::super::rename::is_open(&n.ctx));
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.state, before);
    n.record();
}

#[test]
#[ignore = "actual active sketch session keeps its existing name control"]
fn completed_rename_does_not_enter_or_interrupt_active_sketch() {
    let mut n = seed("completed-sketch-shift-n-active-guard");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    settle(&mut n);
    let before = n.editor.state.clone();
    let session = n.editor.sketch_ui.session.clone();
    tutorial_key(&mut n, Key::N, egui::Modifiers::SHIFT);
    assert!(!super::super::super::rename::is_open(&n.ctx));
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.busy);
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    n.record();
}

#[test]
#[ignore = "actual modal validation leaves native errors and model unchanged"]
fn completed_sketch_empty_name_validation_is_local_and_cancellable() {
    for unchanged in [false, true] {
        let mut n = seed(if unchanged {
            "rename-empty-unchanged"
        } else {
            "rename-empty-cancel"
        });
        n.send("transaction", json!({"edits":[{"op":"set","object":"Sketch","property":"NotARealProperty","value":false}]}), false);
        let original_error = n.editor.error.clone();
        assert!(
            original_error.is_some(),
            "real rejected native edit supplies an unrelated diagnostic"
        );
        let before = n.editor.state.clone();
        settle(&mut n);
        open(&mut n);
        tutorial_key(&mut n, Key::Backspace, egui::Modifiers::NONE);
        tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
        settle(&mut n);
        assert!(super::super::super::rename::is_open(&n.ctx));
        assert!(!n.editor.busy);
        assert_eq!(n.editor.error, original_error);
        let output = n.frame(vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Sketch name cannot be empty" && shape.clip_rect.contains(text.pos))));
        if unchanged {
            typed(&mut n, "Original profile");
            let output = n.frame(vec![]);
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Sketch name cannot be empty")));
            tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
        } else {
            n.click("Cancel");
        }
        assert!(!super::super::super::rename::is_open(&n.ctx));
        assert!(!n.editor.busy);
        assert_eq!(n.editor.error, original_error);
        assert_eq!(n.editor.state, before);
        n.events.push(json!({"case":"empty validation local; cancel or unchanged accept preserves native diagnostic and state","unchanged_accept":unchanged}));
        n.record();
    }
}
