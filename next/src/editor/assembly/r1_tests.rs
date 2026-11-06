use super::external_tests::directory_bytes;
use super::*;

fn content(state: &Value) -> Vec<Value> {
    objects(state).iter().map(|o|json!({"name":o["name"],"type":o["type"],"properties":o["properties"],"expressions":o["expressions"],"shape":o["shape"],"global_placement":o["global_placement"]})).collect()
}
fn key(n: &mut Native, key: Key) {
    n.frame(vec![
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
    ]);
}

fn pending(n: &Native, before: &Value, unresolved: &[&str]) {
    assert_eq!(n.editor.state["available"], false);
    assert_eq!(n.editor.state["recovery_only"], true);
    assert_eq!(n.editor.state["recovery_available"], true);
    assert!(n.editor.dirty);
    assert_eq!(n.editor.path, n.editor.state["path"].as_str().unwrap());
    assert_eq!(n.editor.file_input, n.editor.path);
    assert!(n.editor.state["revision"].as_u64() > before["revision"].as_u64());
    for name in ["Instance", "Second", "AffineNested"] {
        let old = objects(before).iter().find(|o| o["name"] == name).unwrap();
        let now = n.obj(name);
        assert_eq!(
            now["instance_state"]["external_source"],
            old["instance_state"]["external_source"]
        );
        assert_eq!(
            now["properties"]["CadExternalSource"],
            old["properties"]["CadExternalSource"]
        );
        assert_eq!(
            now["properties"]["LinkPlacement"],
            old["properties"]["LinkPlacement"]
        );
        if unresolved.contains(&name) {
            assert!(now["properties"]["LinkedObject"].is_null());
            assert_eq!(
                now["instance_state"]["source_status"],
                "assembly_recovery_required"
            );
        } else {
            assert_eq!(
                now["properties"]["LinkedObject"],
                old["properties"]["LinkedObject"]
            );
            assert_eq!(now["instance_state"]["source_status"], "current");
        }
    }
}

#[test]
fn complete_v1_descriptor_compiler_contract() {
    let state = json!({"available":true,"path":"Root.FCStd","objects":[{"name":"Assembly","type":"Assembly::AssemblyObject"}]});
    let value = json!({"version":1,"path":"Source.FCStd","sha256":"a".repeat(64),"document_uid":"source-uid","object":"Box","object_id":1,"type":"Part::Box","dependencies":[]});
    let request = json!({"op":"assembly_insert","assembly":"Assembly","name":"Instance","external_source":value});
    assert!(
        assembly_commands::build(&state, &request).is_ok(),
        "complete legacy v1 needs no count"
    );
    let mut missing = request.clone();
    missing["external_source"]
        .as_object_mut()
        .unwrap()
        .remove("dependencies");
    assert!(assembly_commands::build(&state, &missing).is_err());
    let mut bad_count = request.clone();
    bad_count["external_source"]["dependency_count"] = json!(1);
    assert!(assembly_commands::build(&state, &bad_count).is_err());
    let revision = json!({"path":"Leaf.FCStd","sha256":"b".repeat(64),"document_uid":"leaf-uid"});
    let mut complete = request.clone();
    complete["external_source"]["dependencies"] = json!([revision]);
    assert!(assembly_commands::build(&state, &complete).is_ok());
    complete["external_source"]["dependencies"] = json!([revision, revision]);
    assert!(assembly_commands::build(&state, &complete).is_err());
    let mut unavailable = state.clone();
    unavailable["available"] = json!(false);
    assert!(assembly_commands::build(&unavailable, &request).is_err());
    unavailable["recovery_only"] = json!(true);
    assert!(assembly_commands::build(&unavailable, &request).is_err());
}

#[test]
#[ignore = "Requires isolated native setter-fault wrapper and generic destinations"]
fn editor_restoring_setter_recovery() {
    let mut n = Native::new();
    let scene = PathBuf::from(std::env::var_os("CAD_EXTERNAL_SCENE").unwrap());
    let inputs = PathBuf::from(std::env::var_os("CAD_FIX_EDITOR_INPUTS").unwrap());
    let control = PathBuf::from(std::env::var_os("CAD_FIX_CONTROL").unwrap());
    for (mode, route) in [
        ("once", "save"),
        ("persistent", "save"),
        ("persistent", "relink"),
    ] {
        fs::write(&control, b"{\"mode\":\"off\"}").unwrap();
        n.editor
            .browser_picked(file_browser::Purpose::Open, scene.clone());
        n.receive(true);
        n.send("transaction",json!({"label":"Dirty content","edits":[{"op":"add_object","type":"Part::Box","name":"DirtyContent"}]}));
        n.send("transaction",json!({"label":"Redo marker","edits":[{"op":"add_object","type":"Part::Box","name":"RedoMarker"}]}));
        n.send("undo", json!({}));
        let before = n.editor.state.clone();
        assert_eq!(before["available"], true);
        assert_eq!(before["redo_count"], 1);
        let label = format!("{mode}-{route}");
        let folder = inputs.join(&label);
        let destination = folder.join("Destination.FCStd");
        let files = directory_bytes(&folder);
        fs::write(
            &control,
            serde_json::to_vec(
                &json!({"mode":mode,"links":["Instance","Second"],"generation":label}),
            )
            .unwrap(),
        )
        .unwrap();
        n.editor.file_dialog(true);
        n.editor
            .browser_picked(file_browser::Purpose::Save, destination.clone());
        n.receive(false);
        assert_eq!(directory_bytes(&folder), files);
        assert!(n.editor.dirty);
        assert_eq!(n.editor.path, n.editor.state["path"].as_str().unwrap());
        assert_eq!(n.editor.file_input, n.editor.path);
        assert_eq!(n.editor.state["undo_count"], before["undo_count"]);
        assert_eq!(n.editor.state["redo_count"], before["redo_count"]);
        if mode == "once" {
            assert_eq!(content(&n.editor.state), content(&before));
            assert_eq!(n.editor.state["recovery_only"], false);
        } else {
            pending(&n, &before, &["Instance", "Second"]);
            assert_eq!(n.editor.state["recovery_only"], true);
            assert_eq!(
                n.editor.state["save_recovery"]["pending_edges"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            let blocked = n.editor.state.clone();
            assert!(
                n.editor.unavailable(),
                "normal document availability remains false"
            );
            assert!(
                n.editor.source_recovery(),
                "captured-edge repair UI remains reachable"
            );
            n.editor.state["recovery_available"] = json!(false);
            assert!(
                !n.editor.source_recovery(),
                "poisoned/unavailable lease cannot admit repair UI"
            );
            n.editor.state = blocked.clone();
            n.editor.state["save_recovery"] = Value::Null;
            assert!(
                !n.editor.source_recovery(),
                "unrelated unavailable state remains closed"
            );
            n.editor.state = blocked.clone();
            for op in [
                "transaction",
                "redo",
                "mesh",
                "topology",
                "measure",
                "export_exchange",
            ] {
                n.editor.send(op, json!({}));
                assert!(!n.editor.busy);
                assert_eq!(n.editor.state, blocked);
                assert!(n.editor.error.as_ref().unwrap().contains("recovery"));
            }
            n.click_any("Undo");
            assert!(!n.editor.busy);
            assert_eq!(n.editor.state, blocked);
            assert!(n.editor.error.as_ref().unwrap().contains("recovery"));
            for action in ["New", "Open", "Redo"] {
                n.click_any(action);
                assert!(!n.editor.busy);
                assert_eq!(n.editor.state, blocked);
                assert!(n.editor.draft.is_none() && n.editor.file_dialog.is_none());
            }
            let frame = n.frame(vec![]);
            assert!(!frame.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(t) if ["Extrude", "Pocket", "Sketch"].contains(&t.galley.text()))));
            key(&mut n, Key::S);
            assert!(n.editor.busy);
            n.receive(false);
            pending(&n, &before, &["Instance", "Second"]);
            assert_eq!(directory_bytes(&folder), files);
            fs::write(
                &control,
                serde_json::to_vec(
                    &json!({"mode":"persistent","links":["Second"],"generation":label}),
                )
                .unwrap(),
            )
            .unwrap();
            key(&mut n, Key::S);
            n.receive(false);
            pending(&n, &before, &["Second"]);
            assert_eq!(
                n.editor.state["save_recovery"]["pending_edges"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                n.editor.state["save_recovery"]["pending_edges"][0]["name"],
                "Second"
            );
            assert_eq!(
                n.obj("Instance")["properties"],
                objects(&before)
                    .iter()
                    .find(|o| o["name"] == "Instance")
                    .unwrap()["properties"]
            );
            assert_eq!(directory_bytes(&folder), files);
        }
        fs::write(&control, b"{\"mode\":\"off\"}").unwrap();
        if route == "relink" {
            n.editor.selected_object = Some("Second".into());
            n.click_any("Assembly");
            n.click_any("Update / relink external source...");
            n.click_text("Load source");
            n.receive(true);
            n.editor.preview();
            n.click("Apply");
            n.receive(true);
            assert!(n.editor.draft.is_none());
            assert_eq!(n.editor.state["recovery_only"], false);
            assert_eq!(n.editor.state["available"], true);
            assert_eq!(content(&n.editor.state), content(&before));
            assert!(n.editor.dirty);
            assert_eq!(n.editor.state["undo_count"], before["undo_count"]);
            assert_eq!(n.editor.state["redo_count"], before["redo_count"]);
        }
        key(&mut n, Key::S);
        assert!(n.editor.busy);
        n.receive(true);
        assert!(!n.editor.dirty);
        assert!(n.editor.state["save_recovery"].is_null());
        assert_eq!(n.editor.state["recovery_only"], false);
        assert_eq!(n.editor.state["available"], true);
        assert_eq!(n.editor.path, n.editor.state["path"].as_str().unwrap());
        assert_eq!(n.editor.file_input, n.editor.path);
        assert!(n.editor.state["revision"].as_u64() > before["revision"].as_u64());
        assert_eq!(content(&n.editor.state), content(&before));
        for (name, bytes) in &files {
            if name != std::path::Path::new("Destination.FCStd") {
                assert_eq!(&fs::read(folder.join(name)).unwrap(), bytes);
            }
        }
        n.send("redo", json!({}));
        assert!(
            objects(&n.editor.state)
                .iter()
                .any(|o| o["name"] == "RedoMarker")
        );
        n.send("undo", json!({}));
        assert_eq!(content(&n.editor.state), content(&before));
        n.send("close", json!({}));
        n.editor
            .browser_picked(file_browser::Purpose::Open, destination);
        n.receive(true);
        assert_eq!(content(&n.editor.state), content(&before));
        n.send("close", json!({}));
    }
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor":true,"restoring_native_setter_faults":true,"persistent_partial_two_edges":true,"save_and_relink_recovery":true,"redo_and_backup_bytes_preserved":true,"physical_gui":false})).unwrap()).unwrap();
}

#[test]
#[ignore = "Requires fresh generic nested source input and pinned native runtime"]
fn editor_nested_source_revision_guard() {
    let mut n = Native::new();
    let inputs = PathBuf::from(std::env::var_os("CAD_FIX_EDITOR_INPUTS").unwrap());
    let leaf = inputs.join("fixtures/Leaf.FCStd");
    let revision = fs::read(inputs.join("fixtures/LeafRevision.FCStd")).unwrap();
    let original = fs::read(&leaf).unwrap();
    n.send("new", json!({"name":"NestedEditor"}));
    n.editor.file_input = n
        .output
        .join("NestedEditor.FCStd")
        .to_string_lossy()
        .into_owned();
    n.send("save", json!({"path":n.editor.file_input}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","name":"NestedInstance","external_source":null,"placement":identity_placement()}),"Insert external component");
    n.editor.assembly_source_path(
        inputs
            .join("fixtures/Wrapper.FCStd")
            .to_string_lossy()
            .into_owned(),
    );
    n.receive(true);
    let canonical = n.request()["external_source"].clone();
    assert_eq!(canonical["dependencies"].as_array().unwrap().len(), 1);
    assert!(canonical.get("dependency_count").is_none());
    let before = n.editor.state.clone();
    fs::write(&leaf, &revision).unwrap();
    n.editor.preview();
    n.click("Apply");
    n.receive(false);
    assert_eq!(n.editor.state, before);
    assert!(n.editor.draft.is_some());
    n.click_text("Load source");
    n.receive(true);
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    assert!((n.obj("NestedInstance")["shape"]["volume"].as_f64().unwrap() - 2000.).abs() < 2e-7);
    let saved = n.output.join("CompleteNested.FCStd");
    n.editor.file_input = saved.to_string_lossy().into_owned();
    fs::write(&leaf, &original).unwrap();
    n.editor.send("save", json!({"path":saved}));
    n.receive(false);
    assert!(!saved.exists());
    assert!(n.editor.dirty);
    assert_eq!(
        n.obj("NestedInstance")["instance_state"]["source_status"],
        "source_changed"
    );
    fs::write(&leaf, &revision).unwrap();
    n.editor.send("save", json!({"path":saved}));
    n.receive(true);
    assert!(!n.editor.dirty);
    n.send("close", json!({}));
    n.editor.browser_picked(file_browser::Purpose::Open, saved);
    n.receive(true);
    assert!((n.obj("NestedInstance")["shape"]["volume"].as_f64().unwrap() - 2000.).abs() < 2e-7);
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor":true,"complete_v1_without_count":true,"source_changed_correction":true,"changed_reverted_save_denied":true,"fresh_open_matches":true,"physical_gui":false})).unwrap()).unwrap();
}

#[test]
#[ignore = "Requires Windows generic hardlink fixture support and pinned native runtime"]
fn editor_hardlink_source_ownership() {
    let mut n = Native::new();
    n.send("new", json!({"name":"AliasEditor"}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    n.send(
        "transaction",
        json!({"label":"Root part","edits":[{"op":"add_object","type":"Part::Box","name":"Box"}]}),
    );
    let original = n.output.join("OwnedRoot.FCStd");
    n.editor
        .browser_picked(file_browser::Purpose::Save, original.clone());
    n.receive(true);
    let alias = n.output.join("Alias.FCStd");
    fs::hard_link(&original, &alias).unwrap();
    n.send("transaction",json!({"label":"Dirty root","edits":[{"op":"set","object":"Box","property":"Length","value":47}]}));
    let before = n.editor.state.clone();
    let bytes = (fs::read(&original).unwrap(), fs::read(&alias).unwrap());
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","name":"Instance","external_source":null,"placement":identity_placement()}),"Insert external component");
    n.click_text("Browse...");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, alias.clone());
    n.receive(false);
    assert_eq!(n.editor.state, before);
    assert_eq!(
        (fs::read(&original).unwrap(), fs::read(&alias).unwrap()),
        bytes
    );
    assert!(n.editor.dirty);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("source_ownership")
    );
    n.click("Cancel");
    key(&mut n, Key::S);
    n.receive(true);
    assert!(!n.editor.dirty);
    assert_eq!(n.obj("Box")["properties"]["Length"], 47.);
    assert_eq!(n.editor.state["undo_count"], before["undo_count"]);
    assert_eq!(n.editor.state["redo_count"], before["redo_count"]);
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor":true,"hardlink_browse_denied":true,"own_save_not_revoked":true,"content_history_preserved":true,"physical_gui":false})).unwrap()).unwrap();
}
