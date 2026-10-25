//! Actual editor form actions against the pinned native worker, without a window.
use super::super::*;
use std::{fs, path::PathBuf, thread::JoinHandle};

struct Native {
    editor: Editor,
    raw: Receiver<Reply>,
    forward: mpsc::Sender<Reply>,
    actor: Option<JoinHandle<()>>,
    ctx: egui::Context,
    time: f64,
    folder: PathBuf,
    events: Vec<Value>,
}

impl Native {
    fn new(name: &str) -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(
                std::env::var_os("CAD_SKETCH_ATTACHMENT_RUNTIME")
                    .expect("CAD_SKETCH_ATTACHMENT_RUNTIME"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            runtime["script"].as_str().unwrap(),
        );
        config.args = runtime["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        config.response_timeout = Duration::from_secs(15);
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, raw) = mpsc::channel();
        let (forward, replies) = mpsc::channel();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        let model_ctx = ctx.clone();
        let actor = std::thread::spawn(move || model_thread(config, input, output, model_ctx));
        let editor = Editor {
            jobs,
            replies,
            busy: false,
            registry: Registry::builtin().unwrap(),
            state: Value::Null,
            viewport: Viewport::default(),
            sketch: None,
            selected_object: None,
            sketch_ui: SketchUi::default(),
            sketch_creation: None,
            body_ui: BodyUi::default(),
            property_ui: properties::PropertyUi::default(),
            measurement_ui: measurements::MeasurementUi::default(),
            transform_ui: transforms::TransformUi::default(),
            exchange_ui: exchange::ExchangeUi::default(),
            draft: None,
            sequence: 0,
            error: None,
            status: String::new(),
            path: String::new(),
            file_input: String::new(),
            file_dialog: None,
            file_browser: file_browser::FileBrowser::default(),
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
            fit_next: false,
        };
        let folder = PathBuf::from(
            std::env::var_os("CAD_SKETCH_ATTACHMENT_OUTPUT").expect("CAD_SKETCH_ATTACHMENT_OUTPUT"),
        )
        .join(name);
        fs::create_dir(&folder).unwrap();
        let mut native = Self {
            editor,
            raw,
            forward,
            actor: Some(actor),
            ctx,
            time: 0.,
            folder,
            events: vec![],
        };
        native.editor.send("new", json!({"name":name}));
        native.receive("new", true);
        native
    }

    fn receive(&mut self, op: &str, accepted: bool) {
        let reply = self
            .raw
            .recv_timeout(Duration::from_secs(25))
            .expect("native editor reply");
        self.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,"state":reply.state}));
        fs::write(
            self.folder.join("events.json"),
            serde_json::to_vec_pretty(&self.events).unwrap(),
        )
        .unwrap();
        assert_eq!(reply.op, op);
        assert_eq!(reply.accepted, accepted, "{:?}", reply.error);
        self.forward.send(reply).unwrap();
        self.editor.poll();
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut frame = eframe::Frame::_new_kittest();
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 850.),
                )),
                time: Some(self.time),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut self.editor, ctx, &mut frame),
        )
    }

    fn click(&mut self, text: &str) {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        let matches: Vec<_> = frame
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(matches.len(), 1, "one visible {text} action");
        let pos = matches[0].center();
        for pressed in [true, false] {
            self.frame(vec![
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
}

impl Drop for Native {
    fn drop(&mut self) {
        let (placeholder, receiver) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.editor.jobs, placeholder));
        drop(receiver);
        if let Some(actor) = self.actor.take() {
            let result = actor.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and a new CAD_SKETCH_ATTACHMENT_OUTPUT directory"]
fn attachment_editor_cancel_create_and_restore_native_session() {
    let mut n = Native::new("AttachmentEditorOptions");
    let before = n.editor.state.clone();
    n.editor.new_sketch_with_options();
    n.click("Cancel");
    assert!(n.editor.sketch_creation.is_none() && !n.editor.busy);
    assert_eq!(n.editor.state, before);
    assert!(n.raw.try_recv().is_err());
    n.editor.new_sketch_with_options();
    n.click("Reverse normal");
    n.click("Unattached");
    let task = n.editor.sketch_creation.as_mut().unwrap();
    assert!(task.reversed && task.detach);
    task.position = [1., 2., 3.];
    task.angle = 30.;
    let name = task.request["name"].as_str().unwrap().to_owned();
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    assert!(n.editor.sketch_creation.is_none() && n.editor.sketch_ui.session.is_some());
    let sketch = objects(&n.editor.state)
        .iter()
        .find(|o| o["name"] == name)
        .unwrap();
    assert_eq!(sketch["properties"]["MapMode"], "Deactivated");
    assert_eq!(sketch["properties"]["AttachmentSupport"], json!([]));
    assert_eq!(sketch["properties"]["MapReversed"], true);
    let p: Vec<f64> =
        serde_json::from_value(sketch["properties"]["Placement"]["position"].clone()).unwrap();
    assert!(
        p.iter()
            .zip([-1., 2., -3.])
            .all(|(a, b)| (a - b).abs() < 1e-8)
    );
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    n.receive("undo", true);
    assert!(objects(&n.editor.state).is_empty());
    assert!(!n.editor.busy && !n.editor.dirty && n.editor.sketch_ui.session.is_none());
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and a new CAD_SKETCH_ATTACHMENT_OUTPUT directory"]
fn attachment_editor_rejected_formula_retains_fields_for_typed_correction() {
    let mut n = Native::new("AttachmentEditorCorrection");
    n.editor.new_sketch_with_options();
    n.click("Normal offset expression");
    n.editor.sketch_creation.as_mut().unwrap().offset_expression = "Missing.Height".into();
    n.click("Create sketch");
    n.receive("transaction", false);
    assert!(objects(&n.editor.state).is_empty());
    let task = n.editor.sketch_creation.as_ref().unwrap();
    assert!(!task.pending && task.expression && task.offset_expression == "Missing.Height");
    n.click("Missing.Height");
    n.frame(vec![
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        },
        egui::Event::Text("1 cm".into()),
    ]);
    n.frame(vec![egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let task = n.editor.sketch_creation.as_ref().unwrap();
    assert_eq!(task.offset_expression, "1 cm");
    let name = task.request["name"].as_str().unwrap().to_owned();
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    let sketch = objects(&n.editor.state)
        .iter()
        .find(|o| o["name"] == name)
        .unwrap();
    let offset = sketch["properties"]["AttachmentOffset"]["position"][2]
        .as_f64()
        .unwrap();
    assert!((offset - 10.).abs() < 1e-8);
    n.click("Finish");
    n.receive("sketch_finish", true);
    assert!(n.editor.sketch_ui.session.is_none() && n.editor.dirty && !n.editor.busy);
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and a new CAD_SKETCH_ATTACHMENT_OUTPUT directory"]
fn workflow_help_preserves_chooser_and_native_sketch_ownership() {
    let mut n = Native::new("WorkflowHelpOwnership");
    n.click("Help");
    assert!(workflow_guide::active(&n.ctx));
    n.click("Parity checklist");
    n.click("Current instructions");
    n.click("Close guide");
    n.editor.new_sketch_with_options();
    n.editor.sketch_creation.as_mut().unwrap().offset_expression = "2 cm".into();
    let before = n.editor.state.clone();
    for pressed in [true, false] {
        n.frame(vec![egui::Event::Key {
            key: egui::Key::F1,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    assert!(workflow_guide::active(&n.ctx));
    for key in [egui::Key::Enter, egui::Key::S, egui::Key::Z] {
        let modifiers = if key == egui::Key::Enter {
            egui::Modifiers::NONE
        } else {
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            }
        };
        for pressed in [true, false] {
            n.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }]);
        }
    }
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.busy && !n.editor.dirty);
    assert_eq!(
        n.editor.sketch_creation.as_ref().unwrap().offset_expression,
        "2 cm"
    );
    n.click("Close guide");
    assert!(!workflow_guide::active(&n.ctx));
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    let before = n.editor.state.clone();
    let owner = n.editor.sketch_ui.session.clone();
    n.editor.sketch_ui.tool = crate::sketch_commands::Tool::Line;
    for pressed in [true, false] {
        n.frame(vec![egui::Event::Key {
            key: egui::Key::F1,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    n.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    n.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(!workflow_guide::active(&n.ctx));
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.busy);
    assert_eq!(n.editor.sketch_ui.tool, crate::sketch_commands::Tool::Line);
    assert!(n.raw.try_recv().is_err());
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    n.receive("undo", true);
}

// Review-only input probe. Appended only to a private copy of the frozen tests.
impl Native {
    fn review_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }]);
        }
    }

    fn review_pointer(&mut self, pos: egui::Pos2, pressed: Option<bool>) {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        self.frame(events);
    }

    fn review_geometry(&self) -> Value {
        let name = self.editor.sketch.as_ref().unwrap();
        objects(&self.editor.state)
            .iter()
            .find(|o| o["name"] == *name)
            .unwrap()["sketch"]["geometry"]
            .clone()
    }
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn review_help_during_actual_native_drag_preserves_geometry() {
    let mut n = Native::new("ReviewHelpDrag");
    n.editor.new_sketch_with_options();
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    let sketch = n.editor.sketch.clone().unwrap();
    n.editor.transaction(
        "Review unconstrained line",
        vec![json!({
            "op":"geometry", "object":sketch, "kind":"line",
            "start":[0.,0.,0.], "end":[25.,10.,0.]
        })],
    );
    n.receive("transaction", true);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let original = n.review_geometry();
    let endpoint = n.editor.viewport.sketch_screen_point([25., 10., 0.]);
    n.review_pointer(endpoint, Some(true));
    let drag_at = endpoint + egui::vec2(25., -20.);
    n.review_pointer(drag_at, None);
    assert!(
        n.editor.sketch_ui.drag.is_some(),
        "actual endpoint drag must start"
    );
    n.frame(vec![]);
    assert!(n.editor.busy, "actual drag must queue native preview");
    n.receive("preview", true);
    // The drag-start frame queues the unchanged anchor. Pump the changed
    // pointer target after that reply before opening Help.
    std::thread::sleep(Duration::from_millis(100));
    n.frame(vec![]);
    assert!(
        n.editor.busy,
        "moved endpoint must queue its native preview"
    );
    n.receive("preview", true);
    let before_help = n.review_geometry();
    assert_ne!(before_help, original, "native drag positive control");
    let before_state = n.editor.state.clone();
    let owner = n.editor.sketch_ui.session.clone();
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    let help_release = drag_at + egui::vec2(-180., 90.);
    n.review_pointer(help_release, None);
    n.review_pointer(help_release, Some(false));
    assert_eq!(
        n.editor.state, before_state,
        "native pump pauses while Help is open"
    );
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    // Complete any work triggered solely by the pointer movement/release over Help.
    let mut after_close_ops = vec![];
    for _ in 0..6 {
        if n.editor.busy {
            let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
            let op = reply.op.clone();
            after_close_ops.push(op.clone());
            n.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,"state":reply.state}));
            assert!(reply.accepted, "{:?}", reply.error);
            n.forward.send(reply).unwrap();
            n.editor.poll();
        }
        n.frame(vec![]);
        if !n.editor.busy && n.editor.sketch_ui.drag.is_none() {
            break;
        }
    }
    assert!(!n.editor.busy);
    let after_help = n.review_geometry();
    fs::write(
        n.folder.join("help-drag-observations.json"),
        serde_json::to_vec_pretty(&json!({
            "original": original, "before_help":before_help, "after_help":after_help,
            "geometry_preserved":before_help == after_help,
            "native_owner_preserved":n.editor.sketch_ui.session == owner,
            "native_state_paused_while_open":true,
            "after_close_operations":after_close_ops,
            "endpoint_screen":[endpoint.x,endpoint.y],
            "drag_screen":[drag_at.x,drag_at.y],
            "release_screen":[help_release.x,help_release.y],
            "final_state":n.editor.state,
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        n.folder.join("events.json"),
        serde_json::to_vec_pretty(&n.events).unwrap(),
    )
    .unwrap();
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    n.receive("undo", true);
    assert_eq!(
        after_help, before_help,
        "Help pointer movement must not move native sketch geometry after close"
    );
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn review_help_from_open_curves_menu_blocks_sketch_keys() {
    let mut n = Native::new("ReviewHelpMenu");
    n.editor.new_sketch_with_options();
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    n.click("Curves");
    let menu = n.frame(vec![]);
    assert!(menu.shapes.iter().any(|s| matches!(&s.shape,
        egui::epaint::Shape::Text(t) if t.galley.text() == "B-spline")));
    let before = n.editor.state.clone();
    let owner = n.editor.sketch_ui.session.clone();
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    for key in [egui::Key::L, egui::Key::Delete, egui::Key::Enter] {
        n.review_key(key, egui::Modifiers::NONE);
    }
    n.review_key(egui::Key::M, egui::Modifiers::COMMAND);
    n.review_key(egui::Key::Z, egui::Modifiers::COMMAND);
    n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert_eq!(
        n.editor.sketch_ui.tool,
        crate::sketch_commands::Tool::Select
    );
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    fs::write(
        n.folder.join("help-menu-observations.json"),
        serde_json::to_vec_pretty(&json!({
            "passed":true,"opened_over_visible_curves_menu":true,
            "native_owner_preserved":true,"state_unchanged":true,"sketch_keys_blocked":true
        }))
        .unwrap(),
    )
    .unwrap();
    n.editor
        .send("sketch_cancel", json!({"token":owner.unwrap()}));
    n.receive("sketch_cancel", true);
    n.receive("undo", true);
}

// Focused native controls for the Help gesture correction.
impl Native {
    fn gesture_fixture(name: &str, redo: bool) -> Self {
        let mut n = Self::new(name);
        n.editor.transaction("Seed existing sketch", vec![
            json!({"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}),
            json!({"op":"geometry","object":"Sketch","kind":"line","start":[0.,0.,0.],"end":[25.,10.,0.]})
        ]);
        n.receive("transaction", true);
        if redo {
            n.editor.transaction(
                "Preserved redo label",
                vec![json!({"op":"set","object":"Sketch","property":"Label","value":"Redo label"})],
            );
            n.receive("transaction", true);
            n.editor.send("undo", json!({}));
            n.receive("undo", true);
            assert_eq!(n.editor.state["redo_count"], 1);
        }
        n.editor.request_sketch("Sketch");
        n.receive("sketch_begin", true);
        n.editor.viewport.fit();
        n.frame(vec![]);
        n
    }

    fn gesture_geometry(&self) -> Value {
        objects(&self.editor.state)
            .iter()
            .find(|o| o["name"] == "Sketch")
            .unwrap()["sketch"]["geometry"]
            .clone()
    }

    fn gesture_start_drag(&mut self) -> (egui::Pos2, [f64; 3]) {
        let endpoint = self.editor.viewport.sketch_screen_point([25., 10., 0.]);
        let cursor = endpoint + egui::vec2(25., -20.);
        let origin = self.editor.viewport.sketch_point(endpoint).unwrap();
        let local = self.editor.viewport.sketch_point(cursor).unwrap();
        let target = std::array::from_fn(|i| [25., 10., 0.][i] + local[i] - origin[i]);
        self.review_pointer(endpoint, Some(true));
        self.review_pointer(cursor, None);
        assert!(self.editor.sketch_ui.drag.is_some());
        if self.editor.sketch_ui.branch_pending {
            self.frame(vec![]);
            assert!(!self.editor.busy && self.raw.try_recv().is_err());
        } else {
            self.receive("preview", true);
            std::thread::sleep(Duration::from_millis(100));
            self.frame(vec![]);
            self.receive("preview", true);
            self.gesture_near(target);
        }
        (cursor, target)
    }

    fn gesture_near(&self, expected: [f64; 3]) {
        let actual = self.gesture_geometry();
        for (index, expected) in expected.into_iter().enumerate() {
            assert!(
                (actual[0]["end"][index].as_f64().unwrap() - expected).abs() < 2e-5,
                "native endpoint {:?} != {:?}",
                actual[0]["end"],
                expected
            );
        }
    }

    fn gesture_task_click(&mut self, label: &str) {
        self.frame(vec![]);
        let output = self.frame(vec![]);
        let pos = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()).center())
                }
                _ => None,
            })
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("visible task action");
        self.review_pointer(pos, Some(true));
        self.review_pointer(pos, Some(false));
    }

    fn gesture_record(&self, name: &str, observation: Value) {
        fs::write(
            self.folder.join(name),
            serde_json::to_vec_pretty(&observation).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_held_pointer_resumes_without_jump_and_keeps_native_history() {
    let mut n = Native::gesture_fixture("HelpHeldHistory", false);
    let seed = n.gesture_geometry();
    let owner = n.editor.sketch_ui.session.clone();
    let (cursor, target) = n.gesture_start_drag();
    let before = n.editor.state.clone();
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    let help_cursor = cursor + egui::vec2(-180., 90.);
    n.review_pointer(help_cursor, None);
    n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert!(n.editor.sketch_ui.drag.is_some() && !n.editor.busy && n.raw.try_recv().is_err());
    let resumed = help_cursor + egui::vec2(12., -6.);
    let origin = n.editor.viewport.sketch_point(help_cursor).unwrap();
    let local = n.editor.viewport.sketch_point(resumed).unwrap();
    let expected = std::array::from_fn(|i| target[i] + local[i] - origin[i]);
    std::thread::sleep(Duration::from_millis(100));
    n.review_pointer(resumed, None);
    n.receive("preview", true);
    n.gesture_near(expected);
    n.review_pointer(resumed, Some(false));
    n.receive("preview_commit", true);
    assert!(n.editor.sketch_ui.drag.is_none());
    assert_eq!(n.editor.sketch_ui.session, owner);
    let accepted = n.gesture_geometry();
    n.click("Finish");
    n.receive("sketch_finish", true);
    n.editor.send("undo", json!({}));
    n.receive("undo", true);
    assert_eq!(n.gesture_geometry(), seed);
    n.editor.send("redo", json!({}));
    n.receive("redo", true);
    assert_eq!(n.gesture_geometry(), accepted);
    let path = n.folder.join("held-history.FCStd");
    n.editor.send("save", json!({"path":path}));
    n.receive("save", true);
    n.editor.send("close", json!({}));
    n.receive("close", true);
    n.editor.send("open", json!({"path":path}));
    n.receive("open", true);
    assert_eq!(n.gesture_geometry(), accepted);
    n.gesture_record(
        "held-history.json",
        json!({"passed":true,"seed":seed,"accepted":accepted,
        "last_canvas_target":target,"resumed_target":expected,"native_owner_preserved":true,
        "no_jump":true,"undo_redo_save_reopen":true}),
    );
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_pending_preview_reply_and_close_button_use_last_canvas_target() {
    let mut n = Native::gesture_fixture("HelpPendingReply", false);
    let seed = n.gesture_geometry();
    let owner = n.editor.sketch_ui.session.clone();
    let (cursor, target) = n.gesture_start_drag();
    let next = cursor + egui::vec2(15., -10.);
    let origin = n.editor.viewport.sketch_point(cursor).unwrap();
    let local = n.editor.viewport.sketch_point(next).unwrap();
    let expected = std::array::from_fn(|i| target[i] + local[i] - origin[i]);
    std::thread::sleep(Duration::from_millis(100));
    n.review_pointer(next, None);
    assert!(n.editor.busy);
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    let help_cursor = next + egui::vec2(-180., 90.);
    n.review_pointer(help_cursor, Some(false));
    n.receive("preview", true);
    assert!(workflow_guide::active(&n.ctx));
    n.gesture_near(expected);
    let before_close = n.editor.state.clone();
    n.click("Close guide");
    assert_eq!(n.editor.state, before_close);
    n.frame(vec![]);
    n.receive("preview_commit", true);
    n.gesture_near(expected);
    assert_eq!(n.editor.sketch_ui.session, owner);
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    assert_eq!(n.gesture_geometry(), seed);
    n.gesture_record("pending-reply.json", json!({"passed":true,"expected":expected,
        "reply_delivered_while_help_open":true,"close_button_used":true,"native_cancel_restored_seed":true}));
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_box_selection_cancels_only_transient_band() {
    for released_in_help in [true, false] {
        let mut n = Native::gesture_fixture(
            if released_in_help {
                "HelpBoxReleased"
            } else {
                "HelpBoxHeld"
            },
            false,
        );
        n.editor.viewport.sketch_selection =
            vec![crate::sketch_commands::SketchElement { index: 0, point: 2 }];
        let start =
            n.editor.viewport.response.as_ref().unwrap().rect.left_top() + egui::vec2(40., 40.);
        n.review_pointer(start, Some(true));
        n.review_pointer(start + egui::vec2(80., 60.), None);
        assert!(n.editor.sketch_ui.drag.is_none());
        let selection = n.editor.viewport.sketch_selection.clone();
        assert!(!selection.is_empty());
        let before = n.editor.state.clone();
        n.review_key(egui::Key::F1, egui::Modifiers::NONE);
        let end = n.editor.viewport.response.as_ref().unwrap().rect.center();
        n.review_pointer(end, if released_in_help { Some(false) } else { None });
        n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
        if !released_in_help {
            n.review_pointer(end, Some(false));
        }
        n.frame(vec![]);
        assert_eq!(n.editor.viewport.sketch_selection, selection);
        assert_eq!(n.editor.state, before);
        assert!(!n.editor.busy && n.raw.try_recv().is_err());
        let point = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
        n.review_pointer(point, Some(true));
        n.review_pointer(point, Some(false));
        assert!(
            n.editor
                .viewport
                .sketch_selection
                .iter()
                .any(|s| s.index == 0 && s.point == 1)
        );
        n.click("Cancel");
        n.receive("sketch_cancel", true);
        n.gesture_record(
            "box-selection.json",
            json!({"passed":true,"released_in_help":released_in_help,
            "accepted_selection_preserved":true,"next_normal_pick_succeeded":true}),
        );
    }
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_deferred_redo_cancel_and_release_commit_preserve_history_rules() {
    for cancel in [true, false] {
        let mut n = Native::gesture_fixture(
            if cancel {
                "HelpRedoCancel"
            } else {
                "HelpRedoCommit"
            },
            true,
        );
        let seed = n.gesture_geometry();
        let owner = n.editor.sketch_ui.session.clone();
        assert!(n.editor.sketch_ui.branch_pending);
        let (cursor, target) = n.gesture_start_drag();
        let before = n.editor.state.clone();
        n.review_key(egui::Key::F1, egui::Modifiers::NONE);
        n.review_pointer(
            cursor + egui::vec2(-180., 90.),
            if cancel { None } else { Some(false) },
        );
        assert_eq!(n.editor.state, before);
        n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(n.editor.sketch_ui.session, owner);
        if cancel {
            assert_eq!(n.editor.state, before);
            n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
            assert!(
                n.editor.sketch_ui.drag.is_none() && !n.editor.busy && n.raw.try_recv().is_err()
            );
            assert_eq!(n.editor.state["redo_count"], before["redo_count"]);
            n.review_pointer(cursor, Some(false));
            n.click("Cancel");
            n.receive("sketch_cancel", true);
            assert_eq!(n.editor.state["redo_count"], 1);
            n.editor.send("redo", json!({}));
            n.receive("redo", true);
            assert_eq!(
                objects(&n.editor.state)
                    .iter()
                    .find(|o| o["name"] == "Sketch")
                    .unwrap()["label"],
                "Redo label"
            );
        } else {
            n.receive("transaction", true);
            n.gesture_near(target);
            assert_eq!(n.editor.state["redo_count"], 0);
            let accepted = n.gesture_geometry();
            n.click("Finish");
            n.receive("sketch_finish", true);
            n.editor.send("undo", json!({}));
            n.receive("undo", true);
            assert_eq!(n.gesture_geometry(), seed);
            n.editor.send("redo", json!({}));
            n.receive("redo", true);
            assert_eq!(n.gesture_geometry(), accepted);
        }
        n.gesture_record(
            "deferred-redo.json",
            json!({"passed":true,"cancel":cancel,
            "owner_preserved":true,"native_history_preserved_until_real_commit":true}),
        );
    }
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_failed_drag_preview_keeps_owner_until_actual_cancel() {
    let mut n = Native::gesture_fixture("HelpFailedDrag", false);
    let seed = n.gesture_geometry();
    let document_state = &n.events.iter().find(|e| e["op"] == "transaction").unwrap()["state"];
    let history = (
        document_state["undo_count"].clone(),
        document_state["redo_count"].clone(),
    );
    let owner = n.editor.sketch_ui.session.clone();
    let (cursor, _) = n.gesture_start_drag();
    let token = n.editor.state["preview"]["token"].clone();
    n.editor.send(
        "preview",
        json!({"token":token,"label":"Rejected replacement","edits":[
            {"op":"set","object":"Sketch","property":"MissingHelpProperty","value":1}
        ]}),
    );
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    n.receive("preview", false);
    assert_eq!(n.editor.state["preview"]["token"], token);
    let rejected = n.editor.state.clone();
    n.review_pointer(cursor + egui::vec2(-180., 90.), None);
    n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.state, rejected);
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert!(!n.editor.busy && n.editor.sketch_ui.drag.is_some());
    n.review_key(egui::Key::Escape, egui::Modifiers::NONE);
    n.receive("preview_cancel", true);
    assert_eq!(n.gesture_geometry(), seed);
    assert_eq!(n.editor.sketch_ui.session, owner);
    n.review_pointer(cursor, Some(false));
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    assert_eq!(
        (
            n.editor.state["undo_count"].clone(),
            n.editor.state["redo_count"].clone()
        ),
        history
    );
    n.gesture_record(
        "failed-drag-cancel.json",
        json!({"passed":true,"native_rejection":true,
        "help_escape_preserved_failed_preview":true,"second_escape_cancelled_drag":true,
        "native_sketch_cancel_preserved_history":true}),
    );
}

#[test]
#[ignore = "requires CAD_SKETCH_ATTACHMENT_RUNTIME and fresh CAD_SKETCH_ATTACHMENT_OUTPUT"]
fn gesture_help_actual_extend_task_pending_reply_apply_and_history() {
    let mut n = Native::gesture_fixture("HelpNativeTask", false);
    let seed = n.gesture_geometry();
    let owner = n.editor.sketch_ui.session.clone();
    let endpoint = n.editor.viewport.sketch_screen_point([25., 10., 0.]);
    n.review_pointer(endpoint, Some(true));
    n.review_pointer(endpoint, Some(false));
    assert!(
        n.editor
            .viewport
            .sketch_selection
            .iter()
            .any(|s| s.index == 0 && s.point == 2)
    );
    n.click("Edit tools");
    n.click("Extend");
    assert!(n.editor.sketch_ui.task.is_some());
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    assert!(n.editor.busy);
    n.review_key(egui::Key::F1, egui::Modifiers::NONE);
    n.receive("preview", true);
    let accepted = n.gesture_geometry();
    assert_ne!(accepted, seed);
    let before = n.editor.state.clone();
    n.review_key(egui::Key::Z, egui::Modifiers::COMMAND);
    n.click("Close guide");
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.sketch_ui.session, owner);
    assert!(n.editor.sketch_ui.task.is_some());
    n.gesture_task_click("Apply");
    n.receive("preview_commit", true);
    assert!(n.editor.sketch_ui.task.is_none());
    n.click("Finish");
    n.receive("sketch_finish", true);
    n.editor.send("undo", json!({}));
    n.receive("undo", true);
    assert_eq!(n.gesture_geometry(), seed);
    n.editor.send("redo", json!({}));
    n.receive("redo", true);
    assert_eq!(n.gesture_geometry(), accepted);
    n.gesture_record(
        "native-task-history.json",
        json!({"passed":true,"actual_extend_menu_and_apply":true,
        "pending_preview_reply_during_help":true,"native_owner_preserved":true,"undo_redo":true}),
    );
}
