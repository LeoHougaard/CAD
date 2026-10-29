use super::*;

fn visible(n: &mut Native) -> Vec<String> {
    n.frame(vec![]);
    n.frame(vec![]);
    n.frame(vec![])
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn workspace(n: &mut Native) {
    n.click("Document");
    n.click("Workspace units...");
    assert!(n.editor.units_ui.workspace && n.editor.units_ui.active());
    assert!(visible(n).iter().any(|s| s == "Workspace units"));
}

fn inch(n: &mut Native) {
    n.click("Millimeter");
    n.click("Inch");
    let choice = n.editor.state["units"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["index"].as_u64() == n.editor.units_ui.selection)
        .unwrap();
    assert!(
        choice["label"]
            .as_str()
            .unwrap()
            .starts_with("Imperial decimal (")
    );
    assert_eq!(choice["workspace_defaults"]["length"]["default_unit"], "in");
    let shown = visible(n);
    assert!(shown.iter().any(|s| s == "Inch"));
    assert!(shown.iter().any(|s| s == "Degree"));
    assert!(shown.iter().any(|s| s == "Pound"));
}

#[test]
fn workspace_length_mapping_uses_native_ids_and_requires_native_metadata() {
    let state = json!({"units":{"choices":[
        {"index":41,"label":"Imperial decimal (in, lb)","workspace_defaults":{"length":{"default_unit":"in"}}},
        {"index":13,"label":"Standard (mm, kg, s)","workspace_defaults":{"length":{"default_unit":"mm"}}},
        {"index":3,"label":"Unrelated schema","workspace_defaults":{"length":{"default_unit":"in"}}},
        {"index":0,"label":"Standard (mm, kg, s)"}
    ]}});
    let choices = super::super::workspace_length_choices(&state);
    assert_eq!(
        choices
            .iter()
            .map(|c| c["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![41, 13]
    );
    assert_eq!(super::super::workspace_unit_name("in"), "Inch");
    assert_eq!(super::super::workspace_unit_name("mm"), "Millimeter");
    assert!(super::super::workspace_length_choices(&json!({"units":{"choices":[]}})).is_empty());
}

#[test]
#[ignore = "pinned native; actual Document Workspace units, input and persisted setting"]
fn workspace_units_document_inch_input_and_save_open() {
    let mut n = Native::case("workspace-inch");
    n.send("new", json!({"name":"WorkspaceInch"}));
    let before = n.editor.state.clone();
    assert!(objects(&before).is_empty());
    workspace(&mut n);
    inch(&mut n);
    let native_index = n.editor.units_ui.selection.unwrap();
    n.click("\u{2713}");
    n.receive("document_settings", true);
    assert_eq!(n.editor.state["units"]["index"], native_index);
    assert_eq!(
        n.editor.state["units"]["defaults"]["mm"]["default_unit"],
        "in"
    );
    assert!(n.editor.dirty && !n.editor.units_ui.active());
    assert!(objects(&n.editor.state).is_empty());
    for key in ["undo_count", "redo_count"] {
        assert_eq!(n.editor.state[key], before[key]);
    }

    // This later native fixture verifies the selected document unit context.
    // It is not substituted for the exercise's actual sketch or cube.
    n.send(
        "transaction",
        json!({"label":"Length input fixture","edits":[
            {"op":"add_object","type":"Part::Box","name":"Box"},
            {"op":"set","object":"Box","property":"Length","value":25.4}
        ]}),
    );
    n.editor.edit_native_properties("Box");
    n.text("1 in", "2");
    n.editor.preview();
    n.receive("preview", true);
    n.send("quantities", json!({"items":[{"unit":"mm","text":"2 in"}]}));
    assert_eq!(
        n.object("Box")["properties"]["Length"],
        n.events.last().unwrap()["data"]["items"][0]["base_value"]
    );
    assert_eq!(n.object("Box")["properties"]["Length"], 50.8);
    n.click("Apply");
    n.receive("preview_commit", true);
    let accepted = n.object("Box").clone();
    let saved = n.save("workspace-inch.FCStd");
    let bytes = fs::read(&saved).unwrap();
    n.send("new", json!({"name":"WorkspaceOther"}));
    n.send("open", json!({"path":saved}));
    assert_eq!(n.editor.state["units"]["index"], native_index);
    assert_eq!(
        n.editor.state["units"]["defaults"]["mm"]["default_unit"],
        "in"
    );
    for key in [
        "name",
        "type",
        "label",
        "properties",
        "expressions",
        "shape",
    ] {
        assert_eq!(n.object("Box")[key], accepted[key], "{key}");
    }
    assert!(!n.editor.dirty);
    assert_eq!(
        fs::read(saved).unwrap(),
        bytes,
        "fresh Open does not save the input"
    );
}

#[test]
#[ignore = "pinned native; actual Workspace units Cancel, Help and stale owner"]
fn workspace_units_cancel_help_stale_revision_and_new_owner() {
    let mut n = Native::case("workspace-owners");
    n.fixture("WorkspaceOwners");
    n.send(
        "transaction",
        json!({"edits":[{"op":"set","object":"Box","property":"Length","value":30}]}),
    );
    n.send("undo", json!({}));
    n.save("workspace-owner-baseline.FCStd");
    let before = n.editor.state.clone();
    let path = n.editor.path.clone();
    workspace(&mut n);
    inch(&mut n);
    let selected = n.editor.units_ui.selection;
    n.key(Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    assert!(n.editor.units_ui.workspace && n.editor.units_ui.active());
    assert_eq!(n.editor.units_ui.selection, selected);
    assert_eq!(n.editor.state, before);
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.units_ui.active());
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.path, path);
    assert!(!n.editor.dirty);
    n.send("redo", json!({}));
    assert_eq!(n.object("Box")["properties"]["Length"], 30.);
    n.send("undo", json!({}));

    workspace(&mut n);
    inch(&mut n);
    n.editor.jobs.try_send(Job{op:"transaction".into(),parameters:json!({
        "document":n.editor.state["document"],"expected_revision":n.editor.state["revision"],
        "edits":[{"op":"set","object":"Box","property":"Length","value":31}]
    })}).unwrap();
    n.receive("transaction", true);
    let changed = n.editor.state.clone();
    n.click("\u{2713}");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, changed);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("Document changed")
    );
    n.click("Cancel workspace units");
    assert!(!n.editor.units_ui.active());

    workspace(&mut n);
    inch(&mut n);
    n.key(Key::F1, egui::Modifiers::NONE);
    n.editor
        .jobs
        .try_send(Job {
            op: "new".into(),
            parameters: json!({"name":"WorkspaceReplacement"}),
        })
        .unwrap();
    n.receive("new", true);
    let replacement = n.editor.state.clone();
    n.key(Key::Escape, egui::Modifiers::NONE);
    visible(&mut n);
    assert!(!n.editor.units_ui.active());
    assert_eq!(n.editor.state, replacement);
    assert!(n.raw.try_recv().is_err());
}

#[test]
#[ignore = "pinned native; actual Workspace units busy, reply, Measure, SaveAs and model owners"]
fn workspace_units_busy_reply_and_modal_model_owners() {
    let mut n = Native::case("workspace-blocked");
    n.fixture("WorkspaceBlocked");
    let before = n.editor.state.clone();
    let output = n.frame(vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "Document" => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .expect("actual Document menu");
    n.editor.send("inspect", json!({}));
    let reply = n.raw.recv_timeout(Duration::from_secs(40)).unwrap();
    assert!(reply.accepted && reply.op == "inspect");
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    n.events.push(json!({"op":reply.op,"accepted":reply.accepted,"state":reply.state,"data":reply.data,"error":reply.error,"same_frame_workspace_input":true}));
    n.forward.send(reply).unwrap();
    n.frame(vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    n.frame(vec![egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(!n.editor.busy && !n.editor.units_ui.active());
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(n.raw.try_recv().is_err());
    assert_eq!(n.editor.state, before);

    n.click("Measure");
    assert!(n.editor.measurement_is_open());
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    assert!(n.editor.measurement_is_open());
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.measurement_is_open());

    n.editor.file_dialog(true);
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    assert!(n.editor.file_dialog.is_some());
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(n.editor.file_dialog.is_none());

    n.editor.edit_native_properties("Box");
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    assert!(n.editor.draft.is_some());
    n.editor.finish_preview(false);
    if n.editor.busy {
        n.receive("preview_cancel", true);
    }

    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let session = n.editor.sketch_ui.session.clone();
    let sketch = n.editor.state.clone();
    n.click("Document");
    assert!(!n.editor.units_ui.active());
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(n.editor.state, sketch);
    assert!(n.raw.try_recv().is_err());
    n.send("sketch_cancel", json!({"token":session.unwrap()}));
}
