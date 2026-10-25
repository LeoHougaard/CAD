//! Public regressions adapted from independent SO-R1-01 and SO-R1-02 probes.
use super::*;

fn report(n: &Native, name: &str, value: Value) {
    fs::write(
        n.folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
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
fn pointer(n: &mut Native, p: egui::Pos2, button: egui::PointerButton) {
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
}
fn open(n: &mut Native, point: [f64; 3]) -> egui::FullOutput {
    n.frame(vec![]);
    let p = n.editor.viewport.sketch_screen_point(point);
    pointer(n, p, egui::PointerButton::Secondary);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let out = n.frame(vec![]);
    assert!(labels(&out).iter().any(|(s, _)| s == "Select other"));
    out
}
fn escape(n: &mut Native) -> egui::FullOutput {
    n.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    n.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    n.frame(vec![])
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
#[test]
#[ignore = "requires private pinned native runtime"]
fn select_other_menu_escape_preserves_active_native_preview() {
    let mut n = fixture("preview-escape");
    let original_length = object(&n.editor.state, "Rear")["properties"]["Length"].clone();
    n.editor.edit_native_properties("Rear");
    n.editor.draft.as_mut().unwrap().parameters["properties"]["Length"] = json!(12);
    n.editor.preview();
    n.receive("preview", true);
    let state = n.editor.state.clone();
    let token = n.editor.draft.as_ref().unwrap().token.clone();
    open(&mut n, [9., 0.5, 0.]);
    let out = escape(&mut n);
    let busy = n.editor.busy;
    let op = if busy {
        n.receive("preview_cancel", true);
        Some("preview_cancel")
    } else {
        None
    };
    let kept = n.editor.draft.as_ref().is_some_and(|d| d.token == token);
    report(
        &n,
        "escape-observation",
        json!({"menu_open_after_escape":labels(&out).iter().any(|(s,_)|s=="Select other"),
        "busy_after_escape":busy,"native_request_after_escape":op,"draft_kept":kept,
        "state_before":state,"state_after":n.editor.state}),
    );
    assert!(
        kept && !busy && op.is_none(),
        "Escape from Select other cancelled its underlying native modeling preview"
    );
    assert!(!labels(&out).iter().any(|(s, _)| s == "Select other"));
    assert_eq!(n.editor.state, state);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(
        object(&n.editor.state, "Rear")["properties"]["Length"],
        original_length
    );
}
#[test]
#[ignore = "requires private pinned native runtime"]
fn select_other_menu_escape_preserves_measurement_owner() {
    let mut n = fixture("measurement-escape");
    let p = n.editor.viewport.sketch_screen_point([5., 5., 0.]);
    pointer(&mut n, p, egui::PointerButton::Primary);
    n.click("Measure");
    n.receive("measure", true);
    open(&mut n, [5., 5., 0.]);
    let out = escape(&mut n);
    let menu_open = labels(&out).iter().any(|(s, _)| s == "Select other");
    let measurement_still_open = n.editor.measurement_escape();
    report(
        &n,
        "escape-observation",
        json!({"menu_open_after_escape":menu_open,"measurement_still_open":measurement_still_open}),
    );
    assert!(
        !menu_open && measurement_still_open,
        "Escape must close Select other before closing Measure"
    );
}
#[test]
#[ignore = "requires private pinned native runtime"]
fn select_other_menu_choice_projects_native_external_geometry() {
    let mut n = fixture("external-project");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    n.click("External");
    n.click("Project edge or vertex");
    assert!(n.editor.sketch_ui.external);
    let out = n.frame(vec![]);
    if labels(&out)
        .iter()
        .any(|(s, _)| s == "Project edge or vertex")
    {
        let root = labels(&out)
            .into_iter()
            .find(|(s, _)| s.starts_with("External") && s != "External geometry")
            .unwrap()
            .1
            .center();
        pointer(&mut n, root, egui::PointerButton::Primary);
    }
    n.editor.viewport.selection_mode = crate::viewport::SelectionMode::Edges;
    let before = n.editor.state.clone();
    let out = open(&mut n, [5., 0., 0.]);
    let (label, rect) = labels(&out)
        .into_iter()
        .find(|(s, _)| s.starts_with("Rear / Edge"))
        .expect("obscured native rear edge choice");
    pointer(&mut n, rect.center(), egui::PointerButton::Primary);
    let chosen = n
        .editor
        .viewport
        .selected
        .clone()
        .expect("chosen rear edge");
    assert_eq!(chosen.object, "Rear");
    let chooser_sent = n.editor.busy;
    let unchanged = n.editor.state == before;
    if chooser_sent {
        n.receive("transaction", true);
    }
    assert!(
        chooser_sent,
        "Chooser must deliver its committed native edge to projection"
    );
    let projected = object(&n.editor.state, "Sketch")["sketch"]["externals"].clone();
    assert_eq!(projected.as_array().unwrap().len(), 1);
    assert_eq!(projected[0]["source"]["link"], "Rear");
    assert_eq!(
        projected[0]["source"]["subelements"],
        json!([chosen.subelement])
    );
    // Positive control: the same real tool accepts an ordinary viewport click.
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let point = n.editor.viewport.sketch_screen_point([5., 0., 0.]);
    pointer(&mut n, point, egui::PointerButton::Primary);
    let ordinary_sent = n.editor.busy;
    let ordinary_clicked = n
        .editor
        .viewport
        .response
        .as_ref()
        .is_some_and(|r| r.clicked());
    let external_active = n.editor.sketch_ui.external;
    if ordinary_sent {
        n.receive("transaction", true);
    }
    report(
        &n,
        "projection-observation",
        json!({"chosen_menu_label":label,"selection":{"object":chosen.object,"subelement":chosen.subelement},
        "chooser_sent_native_transaction":chooser_sent,"state_unchanged_after_menu_choice":unchanged,
        "ordinary_canvas_click_sent_native_transaction":ordinary_sent,"ordinary_response_clicked":ordinary_clicked,
        "external_active":external_active,"editor_error":n.editor.error,"sketch_after_positive_control":object(&n.editor.state,"Sketch")}),
    );
    assert!(ordinary_sent, "ordinary native projection positive control");
    assert!(
        chooser_sent,
        "Selecting an obscured edge in the active projection tool never submits external_geometry"
    );
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["externals"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let both = object(&n.editor.state, "Sketch")["sketch"]["externals"].clone();
    n.send("undo", json!({}), true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["externals"],
        projected
    );
    n.send("redo", json!({}), true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["externals"],
        both
    );
    n.click("Finish");
    n.receive("sketch_finish", true);
    let expected = object(&n.editor.state, "Sketch")["sketch"]["externals"].clone();
    let path = n.save("projection.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["externals"],
        expected
    );
}
#[test]
#[ignore = "requires private pinned native runtime"]
fn select_other_help_closes_menu_and_external_escape_retains_tool() {
    let mut n = fixture("help-external-escape");
    open(&mut n, [5., 5., 0.]);
    n.frame(vec![egui::Event::Key {
        key: Key::F1,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let out = n.frame(vec![]);
    assert!(!labels(&out).iter().any(|(s, _)| s == "Select other"));
    escape(&mut n);
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n.editor.sketch_ui.external = true;
    n.editor.viewport.top();
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    open(&mut n, [5., 0., 0.]);
    let state = n.editor.state.clone();
    escape(&mut n);
    let kept = n.editor.sketch_ui.external;
    report(
        &n,
        "external-escape",
        json!({"help_closed_menu":true,"projection_tool_retained_after_menu_escape":kept,"native_state_unchanged":state==n.editor.state}),
    );
    assert!(
        kept,
        "Escape from the element chooser also exits the active projection tool"
    );
}
