use super::*;

pub(super) fn directory_bytes(
    root: &std::path::Path,
) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn collect(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    collect(root, root, &mut out);
    out
}

impl Native {
    pub(super) fn click_any(&mut self, text: &str) {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        let point = frame
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    let center = t.galley.rect.translate(t.pos.to_vec2()).center();
                    s.clip_rect.contains(center).then_some(center)
                }
                _ => None,
            })
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .unwrap_or_else(|| panic!("missing visible {text}"));
        self.click_pos(point);
    }
    fn external_menu(&mut self) {
        self.click_any("Assembly");
        self.click_any("Insert external component...");
    }
}

#[test]
#[ignore = "Requires native dependency Close fault wrapper and generic source"]
fn source_teardown_failure_reports_actual_close_and_reachable_retry() {
    let mut n = Native::new();
    let source =
        PathBuf::from(std::env::var_os("CAD_EXTERNAL_FIXTURES").unwrap()).join("component.FCStd");
    n.send("new", json!({"name":"CloseSourceRoot"}));
    n.send("save", json!({"path":n.output.join("Root.FCStd")}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","name":"Instance","external_source":null,"placement":identity_placement()}), "Insert external component");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, source.clone());
    n.receive(true);
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    n.send(
        "close",
        json!({"_independent_fault":{"close_fail_files":["component.FCStd"]}}),
    );
    assert!(n.editor.state.is_null());
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("independent Close fault")
    );
    n.send("new", json!({"name":"RetrySourceRoot"}));
    n.send("save", json!({"path":n.output.join("Retry.FCStd")}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    let before = n.editor.state.clone();
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","name":"Instance","external_source":null,"placement":identity_placement()}), "Insert external component");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, source);
    n.receive(true);
    assert_eq!(n.editor.state, before);
    assert!(n.request()["external_source"].is_object());
    n.click("Cancel");
    n.send("close", json!({}));
}

#[test]
#[ignore = "Requires pinned native runtime and immutable generic external fixtures"]
fn native_external_assembly_editor() {
    let mut n = Native::new();
    let fixtures = PathBuf::from(std::env::var_os("CAD_EXTERNAL_FIXTURES").unwrap());
    let source = fixtures.join("component.FCStd");
    let revision = fixtures.join("revision.FCStd");
    let original = fs::read(&source).unwrap();
    n.send("new", json!({"name":"ExternalEditor"}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    let baseline = n.editor.state.clone();
    // Real menu exposes SaveAs before an insertion draft can be lost.
    n.external_menu();
    assert!(n.editor.file_browser.active());
    assert!(n.editor.draft.is_none());
    assert!(n.editor.status.contains("Save the root FCStd"));
    n.editor.cancel_file_browser();
    assert_eq!(n.editor.state, baseline);
    n.external_menu();
    let root = n.output.join("ExternalEditor.FCStd");
    // Same production callback used when the shared modal browser accepts a path.
    n.editor
        .browser_picked(file_browser::Purpose::Save, root.clone());
    n.receive(true);
    assert_eq!(n.editor.state["path"], json!(root));
    n.external_menu();
    assert!(n.editor.draft.is_some());
    n.click_text("Browse...");
    assert!(n.editor.file_browser.active());
    n.editor.cancel_file_browser();
    assert!(n.editor.draft.is_some());
    n.click_text("Browse...");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, source.clone());
    n.receive(true);
    let selected = n.request()["external_source"].clone();
    assert_eq!(selected["object"], "Block");
    // Actual Apply rejects a bad native placement and keeps entered task values.
    n.editor.draft.as_mut().unwrap().parameters["request"]["placement"]["position"] =
        json!([{"quantity_input":"bad"},0,0]);
    n.editor.preview();
    n.click("Apply");
    n.receive(false);
    assert!(n.editor.draft.is_some());
    n.editor.draft.as_mut().unwrap().parameters["request"]["placement"] = identity_placement();
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    let instance = objects(&n.editor.state)
        .iter()
        .find(|o| o["instance_state"]["external_source"].is_object())
        .unwrap()["name"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(n.obj(&instance)["shape"]["volume"], 288.);
    assert!(n.editor.edit_assembly_object(&instance));
    n.editor.preview();
    n.click("Cancel");
    assert!(n.editor.draft.is_none());
    n.send("undo", json!({}));
    let redo = n.editor.state["redo_count"].clone();
    n.external_menu();
    n.click("Cancel");
    assert_eq!(n.editor.state["redo_count"], redo);
    n.send("redo", json!({}));
    // Root-owned identity-matched relink through its real menu and form.
    n.editor.selected_object = Some(instance.clone());
    n.click_any("Assembly");
    n.click_any("Update / relink external source...");
    n.click_text("Browse...");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, revision.clone());
    n.receive(true);
    assert_eq!(
        n.request()["external_source"]["object_id"],
        selected["object_id"]
    );
    n.editor.preview();
    n.click("Cancel");
    assert_eq!(n.obj(&instance)["shape"]["volume"], 288.);
    n.editor.selected_object = Some(instance.clone());
    n.click_any("Assembly");
    n.click_any("Update / relink external source...");
    n.editor
        .assembly_source_path(revision.to_string_lossy().into_owned());
    n.receive(true);
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    assert_eq!(n.obj(&instance)["shape"]["volume"], 648.);
    n.send("undo", json!({}));
    assert_eq!(n.obj(&instance)["shape"]["volume"], 288.);
    n.send("redo", json!({}));
    assert_eq!(n.obj(&instance)["shape"]["volume"], 648.);
    let saved = n.output.join("ExternalEditorSaved.FCStd");
    n.editor.file_input = saved.to_string_lossy().into_owned();
    n.send("save", json!({"path":saved}));
    n.send("close", json!({}));
    n.send("open", json!({"path":saved}));
    assert!(n.editor.edit_assembly_object(&instance));
    n.editor.preview();
    n.click("Cancel");
    assert_eq!(fs::read(&source).unwrap(), original);
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor_actor":true,"headless_egui":true,"physical_gui":false})).unwrap()).unwrap();
}

#[test]
#[ignore = "Requires the generic external affine native scene and control"]
fn native_external_affine_pick_and_measure() {
    let mut n = Native::new();
    let scene = PathBuf::from(std::env::var_os("CAD_EXTERNAL_SCENE").unwrap());
    let control: Value = serde_json::from_slice(
        &fs::read(std::env::var_os("CAD_EXTERNAL_AFFINE").unwrap()).unwrap(),
    )
    .unwrap();
    n.send("open", json!({"path":scene}));
    let baseline = n.editor.state.clone();
    let point = &control["measure"]["selections"][0]["center"]["value"];
    let world = [
        point[0].as_f64().unwrap(),
        point[1].as_f64().unwrap(),
        point[2].as_f64().unwrap(),
    ];
    n.editor.viewport.selection_mode = crate::viewport::SelectionMode::Vertices;
    n.editor.viewport.display_mode = crate::viewport::DisplayMode::Wireframe;
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let pos = n.editor.viewport.sketch_screen_point(world);
    n.click_pos(pos);
    let selected = n.editor.viewport.selected.as_ref().unwrap();
    assert_eq!(selected.object, "AffineNested");
    assert_eq!(selected.subelement, "Vertex1");
    n.frame(vec![egui::Event::Key {
        key: Key::OpenBracket,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(n.editor.busy);
    n.receive(true);
    let measured = &n.events.last().unwrap()["data"]["selections"][0]["center"]["value"];
    for (a, b) in measured.as_array().unwrap().iter().zip(world) {
        assert!((a.as_f64().unwrap() - b).abs() < 2e-7);
    }
    assert_eq!(n.editor.state, baseline);
    assert!(!n.editor.dirty);
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor_viewport_pick":true,"headless_egui":true,"physical_gui":false,"world":world,"measured":measured})).unwrap()).unwrap();
}

#[test]
#[ignore = "Requires isolated one-shot SaveAs injection worker and generic existing destinations"]
fn native_external_save_recovery() {
    let mut n = Native::new();
    let original = PathBuf::from(std::env::var_os("CAD_EXTERNAL_SCENE").unwrap());
    let destinations = PathBuf::from(std::env::var_os("CAD_SAVE_DESTINATIONS").unwrap());
    let content = |state: &Value| -> Vec<Value> {
        objects(state).iter().map(|o|json!({"name":o["name"],"type":o["type"],"properties":o["properties"],"expressions":o["expressions"],"shape":o["shape"],"global_placement":o["global_placement"]})).collect()
    };
    for stage in [
        "after_save_as",
        "clear_rebind",
        "native_save",
        "serialized_paths",
    ] {
        n.editor
            .browser_picked(file_browser::Purpose::Open, original.clone());
        n.receive(true);
        n.send("transaction",json!({"label":"Dirty root","edits":[{"op":"add_object","type":"Part::Box","name":"DirtyContent"}]}));
        let baseline = n.editor.state.clone();
        let folder = destinations.join(stage);
        let destination = folder.join("Destination.FCStd");
        let before = directory_bytes(&folder);
        assert!(
            before
                .keys()
                .any(|p| p.extension().is_some_and(|e| e == "FCBak"))
        );
        n.editor.file_dialog(true);
        n.editor
            .browser_picked(file_browser::Purpose::Save, destination.clone());
        n.receive(false);
        assert_eq!(
            directory_bytes(&folder),
            before,
            "{stage}: destination or previous native backup changed"
        );
        assert_eq!(content(&n.editor.state), content(&baseline));
        assert_eq!(n.editor.state["undo_count"], baseline["undo_count"]);
        assert_eq!(n.editor.state["redo_count"], baseline["redo_count"]);
        assert_eq!(
            n.editor.state["save_recovery"]["destination_restored"],
            true
        );
        assert_eq!(n.editor.state["save_recovery"]["dirty"], true);
        assert!(n.editor.dirty);
        assert_eq!(n.editor.path, n.editor.state["path"].as_str().unwrap());
        assert_eq!(n.editor.file_input, n.editor.path);
        assert_eq!(
            fs::canonicalize(&n.editor.path).unwrap(),
            fs::canonicalize(&destination).unwrap()
        );
        assert!(n.editor.error.as_ref().unwrap().contains("save_recovery"));
        // Real Ctrl+S retries the SAME already-moved native destination.
        n.frame(vec![egui::Event::Key {
            key: Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
        assert!(n.editor.busy);
        n.receive(true);
        assert!(!n.editor.dirty);
        assert!(n.editor.state["save_recovery"].is_null());
        n.frame(vec![egui::Event::Key {
            key: Key::S,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
        for (name, bytes) in &before {
            if name != std::path::Path::new("Destination.FCStd") {
                assert_eq!(&fs::read(folder.join(name)).unwrap(), bytes);
            }
        }
        assert_eq!(content(&n.editor.state), content(&baseline));
        n.send("close", json!({}));
        n.editor
            .browser_picked(file_browser::Purpose::Open, destination);
        n.receive(true);
        assert_eq!(content(&n.editor.state), content(&baseline));
        assert!(!n.editor.dirty);
        n.send("close", json!({}));
    }
    fs::write(n.output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"actual_editor_save_recovery":true,"four_native_stages":true,"backup_bytes_restored":true,"same_destination_ctrl_s_retry":true,"physical_gui":false})).unwrap()).unwrap();
}

fn uncertain_source_close_recovery(pre_effect: bool) {
    let mut n = Native::new();
    let source =
        PathBuf::from(std::env::var_os("CAD_EXTERNAL_FIXTURES").unwrap()).join("component.FCStd");
    let original = fs::read(&source).unwrap();
    n.send("new", json!({"name":"UncertainSourceRoot"}));
    n.send("save", json!({"path":n.output.join("Root.FCStd")}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","name":"Instance","external_source":null,"placement":identity_placement()}), "Insert external component");
    n.editor
        .browser_picked(file_browser::Purpose::AssemblySource, source.clone());
    n.receive(true);
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    n.send(
        "close",
        if pre_effect {
            json!({"_independent_fault":{"close_fail_files":["component.FCStd"]}})
        } else {
            json!({})
        },
    );
    assert!(n.editor.unavailable());
    assert_eq!(n.editor.state["objects"], json!([]));
    assert!(n.editor.state["revision"].as_u64().unwrap() > 0);
    assert_ne!(n.editor.state["document"], "UncertainSourceRoot");
    assert_eq!(
        PathBuf::from(n.editor.state["path"].as_str().unwrap()),
        source
    );
    let error = n.editor.error.as_ref().unwrap();
    assert!(error.contains("R3 source cleanup presence query failure"));
    if pre_effect {
        assert!(error.contains("independent Close fault"));
    }
    assert_eq!(n.events.last().unwrap()["display_objects"], json!([]));
    let retained = n.editor.state.clone();
    n.click_any("Close unavailable document");
    n.click_any("Cancel");
    assert_eq!(n.editor.state, retained);
    assert!(!n.editor.busy);
    n.click_any("Close unavailable document");
    n.click_any("Close document and discard");
    assert!(n.editor.busy);
    n.receive(false);
    assert!(n.editor.unavailable());
    assert_eq!(n.editor.state["objects"], json!([]));
    assert_eq!(n.editor.state["document"], retained["document"]);
    assert!(
        n.editor.state["recovery"]
            .to_string()
            .contains("R3 source cleanup presence query failure")
    );
    // Fault infrastructure only: remove the injected query failure. The
    // recovery itself still uses the ordinary visible confirmation below.
    n.editor
        .jobs
        .send(Job {
            op: "_r3_source_control".into(),
            parameters: json!({"fault":{"enabled":false,"filename":"component.FCStd"}}),
        })
        .unwrap();
    n.editor.busy = true;
    n.receive(true);
    n.click_any("Close unavailable document");
    n.click_any("Close document and discard");
    n.receive(true);
    assert!(n.editor.state.is_null());
    assert_eq!(fs::read(&source).unwrap(), original);
    n.send("new", json!({"name":"RecoveredSourceRoot"}));
    assert_eq!(n.editor.state["document"], "RecoveredSourceRoot");
    n.send("close", json!({}));
}

#[test]
#[ignore = "Requires pinned native source Close/presence fault wrapper"]
fn uncertain_absent_source_exposes_explicit_close_recovery() {
    uncertain_source_close_recovery(false);
}

#[test]
#[ignore = "Requires pinned native source Close/presence fault wrapper"]
fn uncertain_live_source_preserves_error_and_explicit_close_recovery() {
    uncertain_source_close_recovery(true);
}
