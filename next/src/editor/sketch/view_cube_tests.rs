//! Source-prescribed viewport Top click, with a real active native sketch.
use super::*;

fn pointer(n: &mut Native, pos: egui::Pos2) {
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
}

fn cube_top(n: &mut Native) -> egui::Pos2 {
    n.frame(vec![]);
    let frame = n.frame(vec![]);
    let cube = n
        .editor
        .viewport
        .view_cube_rect()
        .expect("visible cube in viewport");
    let targets = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Top" => {
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                cube.contains(rect.center()).then_some(rect.center())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets.len(),
        1,
        "one actual Top label in the upper-right cube"
    );
    let pos = targets[0];
    fs::write(
        n.folder.join("actual-cube-target.json"),
        serde_json::to_vec_pretty(&json!({
            "label":"Top","target":[pos.x,pos.y],
            "cube_min":[cube.min.x,cube.min.y],"cube_max":[cube.max.x,cube.max.y],
            "source":"BLOCK PDF21; actual rendered viewport label, no View menu substitution"
        }))
        .unwrap(),
    )
    .unwrap();
    pointer(n, pos);
    pos
}

fn projections(n: &Native) -> [egui::Pos2; 4] {
    [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
        .map(|p| n.editor.viewport.sketch_screen_point(p))
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn view_cube_top_preserves_active_native_sketch_and_owns_input() {
    let mut n = Native::new("view-cube-active-sketch");
    n.send("new", json!({"name":"CubeActiveSketch"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,4,0],"end":[12,4,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[12,4,0],"end":[12,9,0]},
        {"op":"constraint","object":"Sketch","kind":"Horizontal","args":[0]},
        {"op":"constraint","object":"Sketch","kind":"Vertical","args":[1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[0,2,1,1]}
    ]));
    let saved = n.save("cube-before.FCStd");
    let bytes = fs::read(&saved).unwrap();
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n.frame(vec![]);
    n.frame(vec![]);
    tutorial_click(&mut n, [6., 4., 0.]);
    assert_eq!(
        n.editor.viewport.sketch_selection,
        [SketchElement { index: 0, point: 0 }]
    );
    let selection = n.editor.viewport.sketch_selection.clone();
    let native = n.editor.state.clone();
    let token = n.editor.sketch_ui.session.clone();
    let dirty = n.editor.dirty;
    let path = n.editor.path.clone();
    let jobs = n.events.len();
    n.editor
        .viewport
        .set_view(crate::viewport::StandardView::Trimetric);
    n.frame(vec![]);
    n.frame(vec![]);
    let oblique = projections(&n);
    assert_ne!(oblique[0], oblique[3]);
    cube_top(&mut n);
    let top = projections(&n);
    assert_eq!(
        top[0], top[3],
        "native XY sketch normal projects to zero in Top view"
    );
    assert!(top[1].x > top[0].x && top[2].y < top[0].y);
    assert_eq!(top[1].y, top[0].y);
    assert_eq!(top[2].x, top[0].x);
    assert_eq!(n.editor.viewport.sketch_selection, selection);
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert_eq!(n.editor.dirty, dirty);
    assert_eq!(n.editor.path, path);
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    assert_eq!(n.events.len(), jobs);

    // An active rectangle has one accepted local anchor. A cube click must not
    // become its second model point or submit a four-line transaction.
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Rectangle);
    n.frame(vec![]);
    n.frame(vec![]);
    tutorial_click(&mut n, [3., 2., 0.]);
    assert_eq!(n.editor.sketch_ui.points.len(), 1);
    let points = n.editor.sketch_ui.points.clone();
    n.click("View");
    n.click("Trimetric");
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    let before_popup = projections(&n);
    cube_top(&mut n);
    assert_eq!(
        projections(&n),
        before_popup,
        "the pre-existing popup owns this click"
    );
    assert_eq!(n.editor.sketch_ui.points, points);
    assert_eq!(n.editor.state, native);
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    if egui::Popup::is_any_open(&n.ctx) {
        tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    }
    n.frame(vec![]);
    n.frame(vec![]);
    cube_top(&mut n);
    assert_eq!(projections(&n)[0], projections(&n)[3]);
    assert_eq!(n.editor.sketch_ui.points, points);
    assert_eq!(n.editor.sketch_ui.tool, Tool::Rectangle);
    assert_eq!(n.editor.state, native);
    assert_eq!(n.editor.sketch_ui.session, token);
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    assert_eq!(n.events.len(), jobs);
    assert_eq!(fs::read(&saved).unwrap(), bytes);
    n.send("inspect", json!({}), true);
    assert_eq!(n.editor.state, native);
    fs::write(
        n.folder.join("cube-result.json"),
        serde_json::to_vec_pretty(&json!({
            "native_state":native,"session":token,"before_projection":oblique.map(|p|[p.x,p.y]),
            "after_projection":top.map(|p|[p.x,p.y]),"native_jobs_from_cube":0,
            "selection_preserved":true,"popup_owner_preserved":true,"drawing_points_preserved":true,
            "saved_file_unchanged":true,"fresh_native_inspect_exact":true,
            "physical_gui":false,"full_cube_parity":false
        }))
        .unwrap(),
    )
    .unwrap();
}
