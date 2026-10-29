use super::*;

fn idle(n: &mut Native) -> Vec<String> {
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

fn open(n: &mut Native) {
    n.click("Document");
    n.click("Workspace units...");
    assert!(n.editor.units_ui.workspace && n.editor.units_ui.active());
    idle(n);
}

#[test]
#[ignore = "independent actual Editor/native Workspace dropdown Escape owner"]
fn review_workspace_dropdown_escape_keeps_parent_dialog() {
    let mut n = Native::case("review-workspace-popup");
    n.fixture("ReviewWorkspacePopup");
    n.save("popup-before.FCStd");
    let before = n.editor.state.clone();
    let path = n.editor.path.clone();
    open(&mut n);
    n.click("Millimeter");
    n.click("Inch");
    let selected = n.editor.units_ui.selection;
    n.click("Inch");
    assert!(egui::Popup::is_any_open(&n.ctx));
    n.key(Key::Escape, egui::Modifiers::NONE);
    let actual = json!({"parent_active":n.editor.units_ui.active(),"workspace":n.editor.units_ui.workspace,
        "selection":n.editor.units_ui.selection,"expected_selection":selected,"popup_open":egui::Popup::is_any_open(&n.ctx),
        "state_exact":n.editor.state==before,"path_exact":n.editor.path==path,"dirty":n.editor.dirty,
        "native_queue_empty":n.raw.try_recv().is_err()});
    fs::write(
        n.output.join("escape-observation.json"),
        serde_json::to_vec_pretty(&actual).unwrap(),
    )
    .unwrap();
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.path, path);
    assert!(!n.editor.dirty);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(
        n.editor.units_ui.active(),
        "Escape from open Length dropdown must retain Workspace units parent"
    );
    assert_eq!(n.editor.units_ui.selection, selected);
    n.click("Cancel workspace units");
}

#[test]
#[ignore = "independent actual native same-name Close/New reply owner"]
fn review_workspace_same_name_new_reply_discards_old_dialog() {
    let mut n = Native::case("review-workspace-same-name");
    n.send("new", json!({"name":"ReviewWorkspaceSameName"}));
    let original = n.editor.state.clone();
    open(&mut n);
    n.click("Millimeter");
    n.click("Inch");
    n.editor.jobs.try_send(Job{op:"close".into(),parameters:json!({"document":original["document"],"expected_revision":original["revision"]})}).unwrap();
    n.receive("close", true);
    n.editor
        .jobs
        .try_send(Job {
            op: "new".into(),
            parameters: json!({"name":"ReviewWorkspaceSameName"}),
        })
        .unwrap();
    n.receive("new", true);
    let replacement = n.editor.state.clone();
    idle(&mut n);
    let actual = json!({"original":original,"replacement":replacement,"dialog_active":n.editor.units_ui.active(),
        "selection":n.editor.units_ui.selection,"dialog_document":n.editor.units_ui.document,"dialog_revision":n.editor.units_ui.revision});
    fs::write(
        n.output.join("same-name-observation.json"),
        serde_json::to_vec_pretty(&actual).unwrap(),
    )
    .unwrap();
    assert_eq!(n.editor.state["document"], original["document"]);
    assert_eq!(n.editor.state, replacement);
    assert!(n.raw.try_recv().is_err());
    assert!(
        !n.editor.units_ui.active(),
        "a new native same-name owner must discard the previous owner's units draft"
    );
}

#[test]
#[ignore = "independent actual Editor/native unsupported schema and old Units route"]
fn review_workspace_other_schema_cancel_and_old_units_route() {
    let mut n = Native::case("review-workspace-other-schema");
    n.fixture("ReviewWorkspaceOtherSchema");
    let candidate = n.editor.state["units"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            let label = c["label"].as_str().unwrap();
            !label.starts_with("Standard (") && !label.starts_with("Imperial decimal (")
        })
        .unwrap()
        .clone();
    n.units(candidate["index"].as_u64().unwrap() as usize);
    n.save("other-schema-before.FCStd");
    let before = n.editor.state.clone();
    open(&mut n);
    assert_eq!(n.editor.units_ui.selection, candidate["index"].as_u64());
    n.click("Cancel workspace units");
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.dirty);
    n.click("Units");
    n.click("Document units...");
    let shown = idle(&mut n);
    for choice in before["units"]["choices"].as_array().unwrap() {
        assert!(
            shown
                .iter()
                .any(|text| text == choice["label"].as_str().unwrap())
        );
    }
    n.click("Cancel units");
    assert_eq!(n.editor.state, before);
    assert!(n.raw.try_recv().is_err());
}
