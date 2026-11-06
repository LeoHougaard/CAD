//! Source-prescribed View -> Trimetric through the actual Editor menu.
use super::*;

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn trimetric_view_menu_preserves_native_document_history_and_selection() {
    let mut n = Native::new("trimetric-view-native");
    n.send("new", json!({"name":"TrimetricNative"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Near"},
        {"op":"set","object":"Near","property":"Length","value":20},
        {"op":"set","object":"Near","property":"Width","value":10},
        {"op":"set","object":"Near","property":"Height","value":10},
        {"op":"set","object":"Near","property":"Placement","value":{"position":[100,200,30],"rotation":[0,0,0,1]}},
        {"op":"add_object","type":"Part::Box","name":"Far"},
        {"op":"set","object":"Far","property":"Placement","value":{"position":[1000,200,0],"rotation":[0,0,0,1]}}
    ]));
    let saved_path = n.save("before.FCStd");
    let saved_bytes = fs::read(&saved_path).unwrap();
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
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
    let selected_object = n.editor.selected_object.clone();
    let before_center = n.editor.viewport.sketch_screen_point([110., 205., 40.]);
    let before_scale =
        (n.editor.viewport.sketch_screen_point([111., 205., 40.]) - before_center).length();
    let requests = n.events.len();
    n.click("View");
    n.click("Trimetric");
    n.frame(vec![]);
    let origin = n.editor.viewport.sketch_screen_point([100., 200., 30.]);
    let axis_lengths = [[101., 200., 30.], [100., 201., 30.], [100., 200., 31.]]
        .map(|axis| (n.editor.viewport.sketch_screen_point(axis) - origin).length());
    assert!(
        axis_lengths
            .iter()
            .all(|length| length.is_finite() && *length > 0.)
    );
    assert!(axis_lengths[0] > axis_lengths[1] && axis_lengths[1] > axis_lengths[2]);
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.path, path);
    assert_eq!(n.editor.selected_object, selected_object);
    assert_eq!(n.editor.viewport.selections, selected);
    assert!(!n.editor.busy && !n.editor.dirty);
    assert_eq!(n.events.len(), requests);
    assert!(n.raw.try_recv().is_err());
    assert_eq!(fs::read(&saved_path).unwrap(), saved_bytes);
    n.send("inspect", json!({}), true);
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.viewport.selections, selected);
    fs::write(
        n.folder.join("trimetric-projection.json"),
        serde_json::to_vec_pretty(&json!({
            "axis_lengths":axis_lengths,"top_unit_scale":before_scale,
            "selected_object":selected_object,
            "selection":selected.iter().map(|s|json!({"object":s.object,"subelement":s.subelement})).collect::<Vec<_>>(),
            "native_state":native,"saved_file_unchanged":true,
            "view_action_native_jobs":0,"fresh_inspect_exact":true,
            "onshape_exact_camera_quaternion_claimed":false
        }))
        .unwrap(),
    )
    .unwrap();
}
