//! Actual native owner underneath an ordinary View menu.
use super::*;

fn escape(n: &mut Native) {
    for pressed in [true, false] {
        n.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    n.frame(vec![]);
}

#[test]
#[ignore = "requires pinned native runtime"]
fn native_view_popup_escape_keeps_preview_owner() {
    let mut n = Native::new("view-popup-preview");
    n.send("new", json!({"name":"ViewPopupPreview"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    n.editor.edit_native_properties("Box");
    n.editor.draft.as_mut().unwrap().parameters["properties"]["Length"] = json!(12);
    n.editor.preview();
    n.receive("preview", true);
    let before = n.editor.state.clone();
    let token = n.editor.draft.as_ref().unwrap().token.clone();
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    escape(&mut n);
    let sent = n.editor.busy;
    if sent {
        n.receive("preview_cancel", true);
    }
    fs::write(
        n.folder.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "own_chooser_open":false,"ordinary_view_popup_was_open":true,
            "preview_cancel_submitted":sent,"before":before,"after":n.editor.state,
            "draft_kept":n.editor.draft.as_ref().is_some_and(|draft|draft.token==token)
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        !sent,
        "Escape from View submitted native preview cancellation"
    );
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.draft.as_ref().unwrap().token, token);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    escape(&mut n);
    assert!(n.editor.busy);
    n.receive("preview_cancel", true);
    assert!(n.editor.draft.is_none());
    assert_eq!(
        object(&n.editor.state, "Box")["properties"]["Length"],
        json!(10.)
    );
    fs::write(
        n.folder.join("intentional-cancel.json"),
        serde_json::to_vec_pretty(&n.editor.state).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires pinned native runtime"]
fn native_view_popup_escape_keeps_measure_owner() {
    let mut n = Native::new("view-popup-measure");
    n.send("new", json!({"name":"ViewPopupMeasure"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    n.editor.selected_object = Some("Box".into());
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let p = n.editor.viewport.sketch_screen_point([5., 5., 0.]);
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
    n.click("Measure");
    n.receive("measure", true);
    let before = n.editor.state.clone();
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    escape(&mut n);
    let kept = n.editor.measurement_escape();
    fs::write(
        n.folder.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "ordinary_view_popup_was_open":true,"measurement_owner_kept":kept,
            "before":before,"after":n.editor.state
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        kept,
        "Escape from View closed the underlying Measure window"
    );
    assert_eq!(n.editor.state, before);
}
