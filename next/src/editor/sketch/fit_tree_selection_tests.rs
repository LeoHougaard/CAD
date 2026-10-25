//! Public regressions adapted from independent tree-selection review.
use super::*;

fn modified_frame(
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

fn tree_click(n: &mut Native, label: &str, ctrl: bool) {
    n.frame(vec![]);
    let frame = n.frame(vec![]);
    let bounds = n
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("features")))
        .unwrap();
    let choices: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                bounds.contains(rect.center()).then_some(rect.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(choices.len(), 1, "one tree row {label}");
    let modifiers = egui::Modifiers {
        ctrl,
        ..egui::Modifiers::NONE
    };
    for pressed in [true, false] {
        modified_frame(
            n,
            vec![
                egui::Event::PointerMoved(choices[0]),
                egui::Event::PointerButton {
                    pos: choices[0],
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
        );
    }
}

fn projection(n: &Native, point: [f64; 3]) -> Value {
    let p = n.editor.viewport.sketch_screen_point(point);
    json!([p.x, p.y])
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn fit_tree_native_multiple_tree_objects_fit_union() {
    let mut n = Native::new("review-r1-tree-union");
    n.send("new", json!({"name":"ReviewTreeUnion"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Near"},
        {"op":"set","object":"Near","property":"Placement","value":{"position":[100,200,30],"rotation":[0,0,0,1]}},
        {"op":"add_object","type":"Part::Box","name":"Far"},
        {"op":"set","object":"Far","property":"Placement","value":{"position":[1000,200,30],"rotation":[0,0,0,1]}}
    ]));
    n.save("before.FCStd");
    n.editor.viewport.top();
    n.frame(vec![]);
    tree_click(&mut n, "Near", false);
    tree_click(&mut n, "Far", true);
    assert_eq!(n.editor.body_ui.selected, vec!["Near", "Far"]);
    assert_eq!(n.editor.selected_object.as_deref(), Some("Far"));
    assert!(n.editor.viewport.selected.is_none() && n.editor.viewport.selections.is_empty());
    let before = (
        n.editor.state.clone(),
        n.editor.path.clone(),
        n.editor.dirty,
        n.editor.body_ui.selected.clone(),
    );
    n.click("View");
    n.click("Fit selection");
    n.frame(vec![]);
    let rect = n.editor.viewport.response.as_ref().unwrap().rect;
    let near = n.editor.viewport.sketch_screen_point([105., 205., 35.]);
    let far = n.editor.viewport.sketch_screen_point([1005., 205., 35.]);
    let evidence = json!({"tree_selection":n.editor.body_ui.selected,"selected_object":n.editor.selected_object,
        "near_screen":projection(&n,[105.,205.,35.]),"far_screen":projection(&n,[1005.,205.,35.]),
        "rect_min":[rect.min.x,rect.min.y],"rect_max":[rect.max.x,rect.max.y],
        "near_visible":rect.contains(near),"far_visible":rect.contains(far)});
    fs::write(
        n.folder.join("projection.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            n.editor.state.clone(),
            n.editor.path.clone(),
            n.editor.dirty,
            n.editor.body_ui.selected.clone()
        ),
        before
    );
    assert!(
        rect.contains(near) && rect.contains(far),
        "must frame both selected displayed objects: {evidence}"
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn fit_tree_native_body_display_fallback_and_hidden_noop() {
    let mut n = Native::new("review-r1-body-fallback");
    n.send("new", json!({"name":"ReviewBody"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"PartDesign::AdditiveBox","name":"Solid","body":"Body"},
        {"op":"set","object":"Solid","property":"Length","value":20},
        {"op":"set","object":"Solid","property":"Width","value":10},
        {"op":"set","object":"Solid","property":"Height","value":10},
        {"op":"set","object":"Body","property":"Placement","value":{"position":[250,100,30],"rotation":[0,0,0,1]}},
        {"op":"add_object","type":"Part::Box","name":"Distractor"},
        {"op":"set","object":"Distractor","property":"Placement","value":{"position":[2000,100,30],"rotation":[0,0,0,1]}}
    ]));
    assert_eq!(
        object(&n.editor.state, "Body")["body_state"]["display_object"],
        "Solid"
    );
    n.save("before.FCStd");
    n.editor.viewport.top();
    n.frame(vec![]);
    tree_click(&mut n, "Body", false);
    let before = (
        n.editor.state.clone(),
        n.editor.path.clone(),
        n.editor.dirty,
        n.editor.body_ui.selected.clone(),
    );
    n.click("View");
    n.click("Fit selection");
    n.frame(vec![]);
    let rect = n.editor.viewport.response.as_ref().unwrap().rect;
    let center = n.editor.viewport.sketch_screen_point([260., 105., 35.]);
    assert!(
        (rect.center() - center).length() < 0.01,
        "{rect:?} {center:?}"
    );
    assert_eq!(
        (
            n.editor.state.clone(),
            n.editor.path.clone(),
            n.editor.dirty,
            n.editor.body_ui.selected.clone()
        ),
        before
    );
    n.transaction(json!([{"op":"set","object":"Body","property":"Visibility","value":false}]));
    n.frame(vec![]);
    assert_eq!(
        object(&n.editor.state, "Body")["body_state"]["display_object"],
        Value::Null
    );
    assert!(!n.editor.viewport.can_fit_selection(Some("Body")));
    let before = projection(&n, [260., 105., 35.]);
    assert!(!n.editor.viewport.fit_selection(Some("Body")));
    assert_eq!(before, projection(&n, [260., 105., 35.]));
}
