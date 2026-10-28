//! Pointer and keyboard routes through the real Editor and native model actor.
use super::*;

fn begin(n: &mut Native) {
    n.send("new", json!({"name":"LinePlaneFixture"}), true);
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.receive("sketch_begin", true);
    n.frame(vec![]);
    n.frame(vec![]);
}

fn point(n: &mut Native, p: [f64; 3]) {
    // Layout must settle before converting native coordinates into a click.
    n.frame(vec![]);
    n.frame(vec![]);
    tutorial_click(n, p);
}

fn model(n: &Native) -> Value {
    object(&n.editor.state, "Sketch")["sketch"].clone()
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn line_chain_eight_segments_native_history_and_reopen() {
    let mut n = Native::new("line-chain-eight");
    begin(&mut n);
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    let points = [
        [0., 0., 0.],
        [20., 0., 0.],
        [20., 12., 0.],
        [14., 12., 0.],
        [14., 5., 0.],
        [6., 5., 0.],
        [6., 12., 0.],
        [0., 12., 0.],
        [0., 0., 0.],
    ];
    point(&mut n, points[0]);
    for (i, p) in points[1..].iter().enumerate() {
        point(&mut n, *p);
        n.receive("transaction", true);
        assert_eq!(model(&n)["geometry_count"], i + 1);
        assert_eq!(
            n.editor.sketch_ui.points.len(),
            1,
            "Line continues from accepted endpoint"
        );
        assert_eq!(
            n.editor.sketch_ui.snaps[0],
            Some(SketchElement {
                index: i as i32,
                point: 2
            })
        );
    }
    let before = model(&n);
    let geometry = before["geometry"].as_array().unwrap();
    for i in 0..geometry.len() {
        assert_eq!(
            geometry[i]["end"],
            geometry[(i + 1) % geometry.len()]["start"]
        );
        assert_eq!(geometry[i]["kind"], "line");
        assert_eq!(geometry[i]["construction"], false);
    }
    let constraints = before["constraints"].as_array().unwrap();
    assert!(
        constraints
            .iter()
            .filter(|c| c["type"] == "Coincident")
            .count()
            >= 8
    );
    assert!(constraints.iter().any(|c| c["type"] == "Horizontal"));
    assert!(constraints.iter().any(|c| c["type"] == "Vertical"));
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.points.is_empty());
    n.send("undo", json!({}), true);
    assert_eq!(model(&n)["geometry_count"], 7);
    n.send("redo", json!({}), true);
    assert_eq!(model(&n)["native_xml"], before["native_xml"]);
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    let saved = object(&n.editor.state, "Sketch")["sketch"].clone();
    let path = n.save("eight-segment-profile.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    let reopened = &object(&n.editor.state, "Sketch")["sketch"];
    assert_eq!(reopened["native_xml"], saved["native_xml"]);
    assert_eq!(reopened["geometry"], saved["geometry"]);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn line_toolbar_toggle_escape_and_whole_cancel() {
    let mut n = Native::new("line-toolbar-cancel");
    begin(&mut n);
    let baseline = model(&n);
    n.click("Line");
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    point(&mut n, [3., 4., 0.]);
    point(&mut n, [12., 4., 0.]);
    n.receive("transaction", true);
    point(&mut n, [12., 10., 0.]);
    n.receive("transaction", true);
    assert_eq!(model(&n)["geometry_count"], 2);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.sketch_ui.points.is_empty());
    n.click("Line");
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    n.click("Line");
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    n.receive("undo", true); // Existing new-sketch cleanup policy, not new history semantics.
    assert!(objects(&n.editor.state).is_empty());
    assert_eq!(baseline["geometry_count"], 0);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn plane_shortcut_hides_native_planes_and_guides_without_document_edits() {
    let mut n = Native::new("plane-visibility");
    n.send("new", json!({"name":"PlaneVisibility"}), true);
    n.frame(vec![]);
    n.frame(vec![]);
    let guide = n
        .editor
        .viewport
        .standard_guide_probe(crate::viewport::StandardPlane::Top)
        .unwrap();
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(!n.editor.viewport.planes_visible());
    assert!(
        n.editor
            .viewport
            .standard_guide_probe(crate::viewport::StandardPlane::Top)
            .is_none()
    );
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(guide),
            egui::Event::PointerButton {
                pos: guide,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert!(n.editor.viewport.selected_standard_plane().is_none());
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(n.editor.viewport.planes_visible());
    n.transaction(json!([{ "op":"add_object","type":"PartDesign::Body","name":"Body" },{ "op":"add_object","type":"PartDesign::Plane","name":"Datum","body":"Body" }]));
    assert_eq!(
        object(&n.editor.state, "Datum")["reference_display"]["kind"],
        "plane"
    );
    assert!(!n.editor.viewport.topology_references("Datum").is_empty());
    n.click("Datum");
    assert_eq!(n.editor.selected_object.as_deref(), Some("Datum"));
    assert_eq!(n.editor.body_ui.selected, ["Datum"]);
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(!n.editor.viewport.planes_visible());
    assert!(n.editor.viewport.topology_references("Datum").is_empty());
    assert!(n.editor.viewport.selected.is_none() && n.editor.viewport.selections.is_empty());
    assert!(n.editor.selected_object.is_none() && n.editor.body_ui.selected.is_empty());
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(n.editor.viewport.planes_visible());
    assert_eq!(n.editor.state, before);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn line_and_plane_shortcuts_keep_popup_help_busy_and_modifier_owners() {
    let mut n = Native::new("line-plane-owners");
    begin(&mut n);
    for key in [Key::L, Key::P] {
        tutorial_key(&mut n, key, egui::Modifiers::SHIFT);
        tutorial_key(&mut n, key, egui::Modifiers::CTRL);
    }
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.viewport.planes_visible());
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.viewport.planes_visible());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Select);
    assert!(n.editor.viewport.planes_visible());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    n.editor.send("inspect", json!({}));
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(
        n.editor.viewport.planes_visible(),
        "completed reply frame retains old busy owner"
    );
    assert!(!n.editor.busy);
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Line);
    let before = n.editor.state.clone();
    tutorial_key(&mut n, Key::P, egui::Modifiers::NONE);
    assert!(!n.editor.viewport.planes_visible());
    assert_eq!(
        n.editor.sketch_ui.tool,
        Tool::Line,
        "P must not select Polyline"
    );
    assert_eq!(n.editor.state, before);
    let axis = n.editor.viewport.sketch_screen_point([0., 7., 0.]);
    assert_eq!(
        n.editor.viewport.sketch_hit(axis),
        Some(SketchElement {
            index: -2,
            point: 0
        })
    );
}
