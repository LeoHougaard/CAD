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
