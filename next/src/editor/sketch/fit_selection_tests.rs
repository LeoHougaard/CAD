//! Actual View menu with native display geometry, not a camera-only mock.
use super::*;

fn guide_click(n: &mut Native, label: &str) {
    n.frame(vec![]);
    let output = n.frame(vec![]);
    let bounds = n
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("workflow_guide")))
        .expect("visible workflow guide");
    let choices: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                (text.galley.text() == label && bounds.contains(rect.center())).then_some(rect)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(choices.len(), 1, "one workflow-guide control {label}");
    let point = choices[0].center();
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn fit_selection_native_editor_view_menu_preserves_document_and_selection() {
    let mut n = Native::new("fit-selection-native");
    n.send("new", json!({"name":"FitSelectionNative"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Near"},
        {"op":"set","object":"Near","property":"Length","value":20},
        {"op":"set","object":"Near","property":"Width","value":10},
        {"op":"set","object":"Near","property":"Height","value":10},
        {"op":"set","object":"Near","property":"Placement","value":{"position":[100,200,30],"rotation":[0,0,0,1]}},
        {"op":"add_object","type":"Part::Box","name":"Far"},
        {"op":"set","object":"Far","property":"Placement","value":{"position":[1000,200,0],"rotation":[0,0,0,1]}}
    ]));
    n.save("before.FCStd");
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let point = n.editor.viewport.sketch_screen_point([110., 205., 40.]);
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    let initial_pick = n.editor.viewport.selected.clone().unwrap();
    fs::write(
        n.folder.join("initial-default-pick.json"),
        serde_json::to_vec_pretty(
            &json!({"object":initial_pick.object,"subelement":initial_pick.subelement}),
        )
        .unwrap(),
    )
    .unwrap();
    n.editor.viewport.selection_mode = crate::viewport::SelectionMode::Faces;
    n.frame(vec![]);
    let point = n.editor.viewport.sketch_screen_point([110., 205., 40.]);
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(n.editor.selected_object.as_deref(), Some("Near"));
    assert_eq!(
        n.editor.viewport.selected.as_ref().unwrap().subelement,
        "Face6"
    );
    let native = n.editor.state.clone();
    let path = n.editor.path.clone();
    let selected = n.editor.viewport.selections.clone();
    let before_width = (n.editor.viewport.sketch_screen_point([120., 205., 40.])
        - n.editor.viewport.sketch_screen_point([100., 205., 40.]))
    .length();
    n.click("View");
    n.click("Fit selection");
    n.frame(vec![]);
    let center = n.editor.viewport.sketch_screen_point([110., 205., 40.]);
    let rect = n.editor.viewport.response.as_ref().unwrap().rect;
    fs::write(
        n.folder.join("fit-projection.json"),
        serde_json::to_vec_pretty(
            &json!({"screen_center":[center.x,center.y],"rect_min":[rect.min.x,rect.min.y],
            "rect_max":[rect.max.x,rect.max.y],"selection":selected.iter().map(|s|
                json!({"object":s.object,"subelement":s.subelement})).collect::<Vec<_>>()}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        (center - rect.center()).length() < 0.01,
        "{center:?} {rect:?} {selected:?}"
    );
    let after_width = (n.editor.viewport.sketch_screen_point([120., 205., 40.])
        - n.editor.viewport.sketch_screen_point([100., 205., 40.]))
    .length();
    assert!(after_width > before_width * 10.);
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.path, path);
    assert_eq!(n.editor.viewport.selections, selected);
    assert!(!n.editor.dirty && !n.editor.busy);
    n.click("View");
    n.click("Fit all");
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.viewport.selections, selected);
    n.click("Help");
    guide_click(&mut n, "External assembly components");
    guide_click(&mut n, "Fit selection");
    let instructions = n.frame(vec![]);
    assert!(
        instructions
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text().contains("Select a visible face")))
    );
    n.click("Close guide");
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.viewport.selections, selected);
    n.save("after.FCStd");
    fs::write(
        n.folder.join("camera-check.json"),
        serde_json::to_vec_pretty(&json!({"state":native,"selection":selected.iter().map(|s|
            json!({"object":s.object,"subelement":s.subelement})).collect::<Vec<_>>(),
            "before_width":before_width,"after_width":after_width}))
        .unwrap(),
    )
    .unwrap();
}
