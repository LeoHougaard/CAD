use super::*;

fn recovery_native_receipt(n: &mut Native) -> Value {
    n.send("_native_facts", json!({}));
    let files: Vec<_> = fs::read_dir(std::env::var_os("R7_BINDINGS").unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    assert_eq!(files.len(), 1);
    let binding: Value = serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    let event = binding["events"].as_array().unwrap().last().unwrap();
    assert_eq!(event["request"]["op"], "_native_facts");
    event["after"].clone()
}

fn browser_click(n: &mut Native, label: &str) {
    // Let the closing Discard modal and the new browser finish their layer and
    // layout passes before locating a control. Never accept a guessed click.
    for _ in 0..3 {
        n.frame(vec![]);
    }
    let mut target = None;
    let mut matched = String::new();
    let mut window = egui::Rect::NOTHING;
    for _ in 0..40 {
        let frame = n.frame(vec![]);
        let (_, enabled, rect, icon, open_button) =
            n.editor.file_browser.test_selection(&n.ctx).unwrap();
        window = rect;
        let exact = if label == "Open" {
            assert!(enabled, "The dialog Open control must be enabled");
            open_button
        } else {
            format!("{icon} {label}")
        };
        let hits: Vec<_> = frame
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape {
                    let center = text.galley.rect.translate(text.pos.to_vec2()).center();
                    if text.galley.text() == exact
                        && rect.contains(center)
                        && shape.clip_rect.contains(center)
                    {
                        Some(center)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        assert!(hits.len() <= 1, "Ambiguous file-list row/button: {exact}");
        target = hits.first().copied();
        matched = exact;
        if target.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let position =
        target.unwrap_or_else(|| panic!("Visible icon-prefixed browser control missing: {label}"));
    let file = n.output.join("browser-clicks.json");
    let mut clicks: Vec<Value> = fs::read(&file)
        .ok()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
        .unwrap_or_default();
    let dialog_layer = n.editor.file_browser.test_layer().unwrap();
    let press_layer = n.ctx.layer_id_at(position);
    clicks.push(json!({"label":label,"matched":matched,"x":position.x,"y":position.y,"browser_rect":[window.min.x,window.min.y,window.max.x,window.max.y],"expected_layer":format!("{dialog_layer:?}"),"press_layer":format!("{press_layer:?}"),"browser_active":n.editor.file_browser.active()}));
    fs::write(file, serde_json::to_vec_pretty(&clicks).unwrap()).unwrap();
    assert_eq!(
        press_layer,
        Some(dialog_layer),
        "Press must reach the front dialog"
    );
    n.frame(vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let release_layer = n.ctx.layer_id_at(position);
    assert_eq!(
        release_layer,
        Some(dialog_layer),
        "Release must reach the same front dialog"
    );
    n.frame(vec![egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    clicks.last_mut().unwrap()["release_layer"] = json!(format!("{release_layer:?}"));
    clicks.last_mut().unwrap()["selected_after"] = json!(
        n.editor
            .file_browser
            .test_selection(&n.ctx)
            .and_then(|state| state.0)
    );
    fs::write(
        n.output.join("browser-clicks.json"),
        serde_json::to_vec_pretty(&clicks).unwrap(),
    )
    .unwrap();
}

fn ordinary_open(n: &mut Native, source: &std::path::Path) {
    n.click("Open");
    assert!(n.editor.pending_file.is_some());
    n.click("Discard");
    assert!(n.editor.file_browser.active());
    browser_click(n, "editor-failed-open.FCStd");
    let (selected, enabled, ..) = n.editor.file_browser.test_selection(&n.ctx).unwrap();
    assert!(enabled);
    assert_eq!(
        selected.unwrap().canonicalize().unwrap(),
        source.canonicalize().unwrap(),
        "Selected path must match before confirmation"
    );
    browser_click(n, "Open");
    assert!(n.editor.busy, "The real file browser must send Open");
}

#[test]
#[ignore = "pinned native runtime; actual Editor failed Open recovery through its file browser"]
fn native_failed_open_cleanup_retries_from_reachable_editor_open() {
    let mut n = Native::case("open-cleanup");
    n.fixture("OpenSource");
    let source = n.save("editor-failed-open.FCStd");
    let source_bytes = fs::read(&source).unwrap();
    n.send("close", json!({}));
    n.fixture("OpenKeeper");
    let keeper_path = n.save("keeper.FCStd");
    n.send("transaction", json!({"label":"Keeper redo","edits":[{"op":"set","object":"Box","property":"Length","value":42.0}]}));
    n.send("undo", json!({}));
    let keeper = n.editor.state.clone();
    let path = n.editor.path.clone();
    let dirty = n.editor.dirty;
    assert!(dirty);
    assert_eq!(keeper["redo_count"], 1);
    let plan = json!({"path":source,"attempts":[
        {"validate":1,"close":1,"validation_code":"invalid_model"},
        {"close":1},
        {}
    ]});
    fs::write(
        std::env::var_os("R7_EDITOR_FAULT_PLAN").unwrap(),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    for attempt in 0..2 {
        ordinary_open(&mut n, &source);
        n.receive("open", false);
        assert_eq!(n.editor.state, keeper);
        assert_eq!(n.editor.path, path);
        assert_eq!(n.editor.dirty, dirty);
        assert_eq!(n.editor.state["redo_count"], 1);
        let error = n.editor.error.as_ref().unwrap();
        assert!(
            error.contains("invalid_model") && error.contains("r7 injected validation failure"),
            "{error}"
        );
        assert!(
            error.contains("cleanup_errors") && error.contains("r7 injected native Close failure"),
            "{error}"
        );
        fs::write(n.output.join(format!("failure-{attempt}.json")), serde_json::to_vec_pretty(&json!({"state":n.editor.state,"path":n.editor.path,"dirty":n.editor.dirty,"error":error})).unwrap()).unwrap();
    }
    // Redo is still usable in the actual Editor before a successful replacement.
    n.send("redo", json!({}));
    assert_eq!(n.object("Box")["properties"]["Length"], 42.0);
    n.send("undo", json!({}));
    assert_eq!(
        n.object("Box")["properties"]["Length"],
        keeper["objects"][0]["properties"]["Length"]
    );
    ordinary_open(&mut n, &source);
    n.receive("open", true);
    assert!(n.editor.error.is_none());
    assert!(!n.editor.unavailable());
    assert!(!n.editor.dirty);
    assert_eq!(n.editor.path, source.to_string_lossy());
    assert_eq!(n.object("Box")["properties"]["Length"], 25.4);
    n.send(
        "transaction",
        json!({"edits":[{"op":"set","object":"Box","property":"Length","value":31.0}]}),
    );
    n.send("undo", json!({}));
    n.send("redo", json!({}));
    assert_eq!(n.object("Box")["properties"]["Length"], 31.0);
    assert_eq!(fs::read(source).unwrap(), source_bytes);
    assert!(keeper_path.is_file());
    n.send("close", json!({}));
}

#[test]
#[ignore = "pinned native runtime; confirmed dependent Close retains terminal owner"]
fn native_terminal_dependent_close_confirmation_retry_and_reopen() {
    let mut n = Native::case("terminal-recovery");
    n.fixture("TerminalEditorOwner");
    let saved = n.save("terminal-owner.FCStd");
    let saved_bytes = fs::read(&saved).unwrap();
    n.send(
        "transaction",
        json!({"edits":[{"op":"set","object":"Box","property":"Length","value":42.0}]}),
    );
    n.send("undo", json!({}));
    let path = n.editor.path.clone();
    assert!(n.editor.dirty);
    assert_eq!(n.editor.state["redo_count"], 1);
    n.send(
        "_terminal_editor_dependency",
        json!({"path":n.output.join("editor-dependent.FCStd")}),
    );
    n.send(
        "sketch_begin",
        json!({"object":"Sketch","token":"terminal-editor-sketch"}),
    );
    n.send("_poison_owner", json!({}));
    n.send("inspect", json!({}));
    let owner = n.editor.state.clone();
    let local_session = n.editor.sketch_ui.session.clone();
    let local_sketch = n.editor.sketch.clone();
    assert!(n.editor.unavailable());
    assert_eq!(owner["sketch_edit"]["token"], "terminal-editor-sketch");
    let native_before = recovery_native_receipt(&mut n);
    let owner_name = owner["document"].as_str().unwrap();
    // During sketch editing the public counters describe the sketch journal.
    // The banked document Redo remains separate and must survive recovery.
    assert_eq!(owner["redo_count"], 0);
    assert_eq!(native_before["documents"][owner_name]["redo"], 1);
    assert_eq!(owner["close_dependents"].as_array().unwrap().len(), 1);
    let dependent = owner["close_dependents"][0]["document"]
        .as_str()
        .unwrap()
        .to_owned();
    let button = format!("Close dependent {dependent}...");
    let receipt = n.output.join("recovery-controls.json");
    let mut receipts = vec![];
    // The current owner's confirmation does not consent to dependent loss.
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", false);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("document_referenced")
    );
    assert_eq!(n.editor.state, owner);
    n.click(&button);
    let frame = n.frame(vec![]);
    let texts: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::epaint::Shape::Text(t) = &s.shape {
                Some(t.galley.text().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t == &format!("Close dependent {dependent}?"))
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("This dependent's unsaved changes will be lost"))
    );
    assert!(
        texts
            .iter()
            .any(|t| t == owner["close_dependents"][0]["path"].as_str().unwrap())
    );
    n.click("Cancel");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, owner);
    assert_eq!(n.editor.path, path);
    assert!(n.editor.dirty);
    assert_eq!(n.editor.sketch_ui.session, local_session);
    assert_eq!(n.editor.sketch, local_sketch);
    let cancel_native = recovery_native_receipt(&mut n);
    assert_eq!(cancel_native, native_before);
    receipts.push(json!({"step":"cancel","state":n.editor.state,"path":n.editor.path,"dirty":n.editor.dirty,"session":n.editor.sketch_ui.session,"modal_text":texts}));
    for attempt in 0..3 {
        n.click(&button);
        n.click("Close document and discard");
        n.receive("close_dependent", attempt == 2);
        let mut expected = owner.clone();
        if attempt == 2 {
            expected["close_dependents"] = json!([]);
        }
        assert_eq!(n.editor.state, expected);
        assert_eq!(n.editor.path, path);
        assert!(n.editor.dirty);
        assert_eq!(n.editor.state["redo_count"], owner["redo_count"]);
        assert_eq!(n.editor.sketch_ui.session, local_session);
        assert_eq!(n.editor.sketch, local_sketch);
        assert!(n.editor.unavailable());
        assert_eq!(n.events.last().unwrap()["scene_count"], 0);
        if attempt < 2 {
            assert!(
                n.editor
                    .error
                    .as_ref()
                    .unwrap()
                    .contains(&format!("independent Close fault {dependent}"))
            );
        } else {
            assert!(n.editor.error.is_none());
        }
        let native_after = recovery_native_receipt(&mut n);
        assert_eq!(
            native_after["documents"][owner_name],
            native_before["documents"][owner_name]
        );
        assert_eq!(native_after["active"], native_before["active"]);
        assert_eq!(
            native_after["native_active"],
            native_before["native_active"]
        );
        if attempt < 2 {
            assert_eq!(
                native_after["documents"][&dependent],
                native_before["documents"][&dependent]
            );
        } else {
            assert!(native_after["documents"].get(&dependent).is_none());
        }
        receipts.push(json!({"step":format!("attempt-{attempt}"),"state":n.editor.state,"path":n.editor.path,"dirty":n.editor.dirty,"session":n.editor.sketch_ui.session,"native":native_after}));
        fs::write(&receipt, serde_json::to_vec_pretty(&receipts).unwrap()).unwrap();
    }
    assert_eq!(fs::read(&saved).unwrap(), saved_bytes);
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", true);
    assert!(n.editor.state.is_null());
    assert!(!n.editor.dirty);
    assert!(n.editor.path.is_empty());
    n.click("Open");
    assert!(n.editor.file_browser.active());
    browser_click(&mut n, "terminal-owner.FCStd");
    let (selected, enabled, ..) = n.editor.file_browser.test_selection(&n.ctx).unwrap();
    assert!(enabled);
    assert_eq!(
        selected.unwrap().canonicalize().unwrap(),
        saved.canonicalize().unwrap()
    );
    browser_click(&mut n, "Open");
    n.receive("open", true);
    assert!(!n.editor.unavailable());
    assert!(n.editor.error.is_none());
    assert_eq!(n.object("Box")["properties"]["Length"], 25.4);
    n.send("close", json!({}));
}

#[test]
#[ignore = "pinned native runtime; isolated native Link recovery confirmation"]
fn native_terminal_cycle_reference_removal_cancel_failure_retry() {
    let mut n = Native::case("terminal-cycle");
    n.fixture("TerminalCycleOwner");
    let saved = n.save("terminal-cycle-owner.FCStd");
    let saved_bytes = fs::read(&saved).unwrap();
    n.send(
        "_terminal_editor_dependency",
        json!({"path":n.output.join("cycle-dependent.FCStd"),"cycle":true}),
    );
    n.send(
        "transaction",
        json!({"edits":[{"op":"set","object":"Box","property":"Length","value":42.0}]}),
    );
    n.send("undo", json!({}));
    n.send(
        "sketch_begin",
        json!({"object":"Sketch","token":"cycle-editor-sketch"}),
    );
    n.send("_poison_owner", json!({}));
    n.send("inspect", json!({}));
    let owner = n.editor.state.clone();
    let path = n.editor.path.clone();
    let session = n.editor.sketch_ui.session.clone();
    let sketch = n.editor.sketch.clone();
    assert!(n.editor.unavailable() && n.editor.dirty);
    assert_eq!(owner["redo_count"], 0);
    let native_before = recovery_native_receipt(&mut n);
    let owner_name = owner["document"].as_str().unwrap();
    assert_eq!(native_before["documents"][owner_name]["redo"], 1);
    let dependent = owner["close_dependents"][0]["document"]
        .as_str()
        .unwrap()
        .to_owned();
    let edits = owner["close_dependents"][0]["reference_removals"]
        .as_array()
        .unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["object"], "Target");
    assert!(edits[0]["native_id"].as_u64().is_some());
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", false);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("document_referenced")
    );
    n.click(&format!("Close dependent {dependent}..."));
    n.click("Close document and discard");
    n.receive("close_dependent", false);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("document_referenced")
    );
    assert_eq!(n.editor.state, owner);
    let button = format!("Remove references in {dependent}...");
    n.click(&button);
    let frame = n.frame(vec![]);
    let texts: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::epaint::Shape::Text(t) = &s.shape {
                Some(t.galley.text().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t == &format!("Remove references in {dependent}?"))
    );
    assert!(
        texts
            .iter()
            .any(|t| t == &format!("Target (native ID {})", edits[0]["native_id"]))
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("Both documents stay open; other unsaved work is retained"))
    );
    n.click("Cancel");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, owner);
    assert_eq!(n.editor.path, path);
    assert!(n.editor.dirty);
    assert_eq!(n.editor.sketch_ui.session, session);
    assert_eq!(n.editor.sketch, sketch);
    assert_eq!(recovery_native_receipt(&mut n), native_before);
    let mut receipts = vec![
        json!({"step":"cancel","state":n.editor.state,"path":n.editor.path,"dirty":n.editor.dirty,"modal_text":texts}),
    ];
    for attempt in 0..2 {
        n.click(&button);
        n.click("Remove listed references");
        n.receive("remove_dependent_references", attempt == 1);
        let mut actual = n.editor.state.clone();
        let mut expected = owner.clone();
        actual.as_object_mut().unwrap().remove("close_dependents");
        expected.as_object_mut().unwrap().remove("close_dependents");
        assert_eq!(actual, expected);
        assert_eq!(n.editor.path, path);
        assert!(n.editor.dirty);
        assert_eq!(n.editor.sketch_ui.session, session);
        assert_eq!(n.editor.sketch, sketch);
        assert_eq!(n.events.last().unwrap()["scene_count"], 0);
        if attempt == 0 {
            assert!(
                n.editor
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("independent reference removal validation fault")
            );
            assert_ne!(
                n.editor.state["close_dependents"][0]["revision"],
                owner["close_dependents"][0]["revision"]
            );
            assert_eq!(
                n.editor.state["close_dependents"][0]["reference_removals"],
                owner["close_dependents"][0]["reference_removals"]
            );
        } else {
            assert!(n.editor.error.is_none());
            assert_eq!(n.editor.state["close_dependents"], json!([]));
        }
        let native_after = recovery_native_receipt(&mut n);
        assert_eq!(
            native_after["documents"][owner_name],
            native_before["documents"][owner_name]
        );
        assert_eq!(native_after["active"], native_before["active"]);
        assert_eq!(
            native_after["native_active"],
            native_before["native_active"]
        );
        assert_eq!(
            native_after["documents"][&dependent]["uuid"],
            native_before["documents"][&dependent]["uuid"]
        );
        assert_eq!(
            native_after["documents"][&dependent]["objects"]["Box"],
            native_before["documents"][&dependent]["objects"]["Box"]
        );
        if attempt == 0 {
            assert_eq!(
                native_after["documents"][&dependent]["objects"]["Target"]["id"],
                native_before["documents"][&dependent]["objects"]["Target"]["id"]
            );
            assert_eq!(
                native_after["documents"][&dependent]["objects"]["Target"]["link"],
                native_before["documents"][&dependent]["objects"]["Target"]["link"]
            );
        } else {
            assert!(
                native_after["documents"][&dependent]["objects"]
                    .get("Target")
                    .is_none()
            );
        }
        receipts.push(json!({"step":format!("attempt-{attempt}"),"state":n.editor.state,"path":n.editor.path,"dirty":n.editor.dirty,"native":native_after}));
        fs::write(
            n.output.join("cycle-controls.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(fs::read(saved).unwrap(), saved_bytes);
    // Both native owners remain registered. This focused action removes only
    // the selected Link; it never silently closes either document.
}
