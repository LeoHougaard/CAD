//! Actual Editor/update/model_thread with a native worker. Run ignored tests with
//! CAD_LIFECYCLE_RUNTIME pointing to a runtime manifest and CAD_LIFECYCLE_OUTPUT
//! pointing to a fresh writable directory. No desktop application is launched.
#[path = "fit_selection_tests.rs"]
mod fit_selection_tests;
#[path = "line_plane_tests.rs"]
mod line_plane_tests;
use super::super::*;
use super::*;
use std::{fs, thread::JoinHandle};

#[path = "mirror_tests.rs"]
mod mirror_tests;

#[path = "copy_tests.rs"]
mod copy_tests;

#[path = "select_other_tests.rs"]
mod select_other_tests;

fn test_output() -> PathBuf {
    PathBuf::from(std::env::var_os("CAD_LIFECYCLE_OUTPUT").expect("CAD_LIFECYCLE_OUTPUT"))
}
fn object<'a>(state: &'a Value, name: &str) -> &'a Value {
    objects(state).iter().find(|o| o["name"] == name).unwrap()
}
fn set_width(value: f64) -> Value {
    json!([{"op":"set","object":"Box","property":"Width","value":value}])
}

struct Native {
    editor: Editor,
    raw: Receiver<Reply>,
    forward: Sender<Reply>,
    actor: Option<JoinHandle<()>>,
    events: Vec<Value>,
    folder: PathBuf,
    ctx: egui::Context,
    time: f64,
}
impl Native {
    fn new(case: &str) -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(std::env::var_os("CAD_LIFECYCLE_RUNTIME").expect("CAD_LIFECYCLE_RUNTIME"))
                .unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            runtime["test_worker"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/lifecycle_fault_worker.py")
                }),
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
        let actor = std::thread::spawn(move || {
            model_thread(config, input, output, egui::Context::default())
        });
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
            transform_ui: transforms::TransformUi::default(),
            exchange_ui: exchange::ExchangeUi::default(),
            draft: None,
            measurement_ui: super::super::measurements::MeasurementUi::default(),
            units_ui: Default::default(),
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
        let folder = test_output().join(case);
        fs::create_dir(&folder).unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        Self {
            editor,
            raw,
            forward,
            actor: Some(actor),
            events: vec![],
            folder,
            ctx,
            time: 0.,
        }
    }
    fn record(&self) {
        fs::write(
            self.folder.join("events.json"),
            serde_json::to_vec_pretty(&self.events).unwrap(),
        )
        .unwrap();
    }
    fn receive(&mut self, op: &str, accepted: bool) {
        let reply = self
            .raw
            .recv_timeout(Duration::from_secs(25))
            .expect("native reply timed out");
        self.events.push(
            json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,
            "state":reply.state,"scenes":reply.scenes}),
        );
        self.record();
        assert_eq!(reply.op, op);
        assert_eq!(reply.accepted, accepted, "{:?}", reply.error);
        self.forward.send(reply).unwrap();
        self.editor.poll();
        let ui = self.ui_state();
        self.events.last_mut().unwrap()["editor"] = ui;
        self.record();
    }
    fn send(&mut self, op: &str, parameters: Value, accepted: bool) {
        self.editor.send(op, parameters);
        assert!(self.editor.busy);
        self.receive(op, accepted);
    }
    fn transaction(&mut self, edits: Value) {
        self.send(
            "transaction",
            json!({"label":"Lifecycle test","edits":edits}),
            true,
        );
    }
    fn save(&mut self, name: &str) -> PathBuf {
        let path = self.folder.join(name);
        self.editor.file_input = path.to_string_lossy().into_owned();
        self.send("save", json!({"path":path}), true);
        assert!(!self.editor.dirty);
        path
    }
    fn ui_state(&self) -> Value {
        json!({"busy":self.editor.busy,"dirty":self.editor.dirty,"path":self.editor.path,
            "draft":self.editor.draft.as_ref().map(|d|json!({"token":d.token,"parameters":d.parameters,"valid":d.valid})),
            "session":self.editor.sketch_ui.session,"dimension":self.editor.sketch_ui.dimension.is_some(),
            "task":self.editor.sketch_ui.task.is_some(),"cleanup":self.editor.sketch_ui.cleanup,
            "pending_begin":self.editor.sketch_ui.pending_begin,"continuation":self.editor.sketch_ui.continuation})
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let mut frame = eframe::Frame::_new_kittest();
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 850.),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut self.editor, ctx, &mut frame),
        )
    }
    fn click(&mut self, label: &str) {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        let texts: Vec<_> = frame
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) => Some((
                    t.galley.text().to_owned(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        fs::write(
            self.folder.join("last-ui-text.json"),
            serde_json::to_vec_pretty(
                &texts
                    .iter()
                    .map(|(t, r)| json!({"text":t,"min":[r.min.x,r.min.y],"max":[r.max.x,r.max.y]}))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        )
        .unwrap();
        // Scope confirmation actions to the actual modal area, not an
        // identically labelled disabled control in the sketch toolbar.
        let modal = if self.editor.pending_file.is_some() {
            Some(
                self.ctx
                    .memory(|memory| memory.area_rect(egui::Id::new("unsaved")))
                    .expect("file-action confirmation must have a modal area"),
            )
        } else if self.editor.sketch_ui.task.is_some() && matches!(label, "Apply" | "Cancel") {
            Some(
                self.ctx
                    .memory(|memory| memory.area_rect(egui::Id::new("native_sketch_task")))
                    .expect("sketch task must have a modal area"),
            )
        } else {
            None
        };
        let matches: Vec<_> = texts
            .iter()
            .filter(|(text, rect)| {
                text == label && modal.is_none_or(|area| area.contains(rect.center()))
            })
            .collect();
        assert_eq!(matches.len(), 1, "expected one scoped UI action {label}");
        let rect = matches[0].1;
        fs::write(
            self.folder.join("last-click-target.json"),
            serde_json::to_vec_pretty(&json!({
                "label":label, "target":[rect.center().x,rect.center().y],
                "modal":modal.map(|r| json!({"min":[r.min.x,r.min.y],"max":[r.max.x,r.max.y]})),
                "all_label_matches":texts.iter().filter(|(text,_)|text==label)
                    .map(|(_,r)|json!([r.center().x,r.center().y])).collect::<Vec<_>>()
            }))
            .unwrap(),
        )
        .unwrap();
        let pos = rect.center();
        self.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        self.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    fn assert_discard_guards(&mut self) {
        for action in [FileAction::New, FileAction::Close] {
            let before = self.editor.state.clone();
            self.editor.request_file(action);
            assert!(
                self.editor.pending_file.is_some(),
                "dirty change must require discard confirmation"
            );
            assert!(!self.editor.busy && !self.editor.allow_close);
            self.click("Cancel");
            assert!(self.editor.pending_file.is_none());
            assert!(!self.editor.busy);
            assert_eq!(self.editor.state, before);
            assert!(self.editor.dirty && !self.editor.allow_close);
        }
    }
    fn failed_cleanup(&mut self) {
        self.editor.prepare_new_sketch();
        self.editor.sketch = Some("Sketch".into());
        self.transaction(
            json!([{"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}]),
        );
        self.receive("sketch_begin", true);
        let token = self.editor.sketch_ui.session.clone().unwrap();
        self.send(
            "sketch_cancel",
            json!({"token":token,"lifecycle_fail_cleanup_undo":true}),
            true,
        );
        self.receive("undo", false);
        assert_eq!(self.editor.state["available"], true);
        assert!(self.editor.sketch_ui.session.is_none() && self.editor.dirty);
    }
    fn assert_reset(&self) {
        let s = &self.editor.sketch_ui;
        assert!(
            !s.cleanup
                && !s.pending_begin
                && !s.new_sketch
                && s.continuation.is_none()
                && s.after_finish.is_none()
        );
        assert!(
            s.session.is_none() && s.dimension.is_none() && s.task.is_none() && s.drag.is_none()
        );
        assert!(
            self.editor.sketch.is_none()
                && self.editor.selected_object.is_none()
                && self.editor.draft.is_none()
        );
        assert!(
            self.editor.viewport.selected.is_none()
                && self.editor.viewport.selections.is_empty()
                && self.editor.viewport.sketch_selection.is_empty()
        );
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let (sender, _) = mpsc::sync_channel(1);
        self.editor.jobs = sender;
        if let Some(actor) = self.actor.take() {
            actor.join().unwrap();
        }
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn failed_cleanup_then_same_document_undo_keeps_dirty_confirmation() {
    let mut n = Native::new("same-document");
    n.send("new", json!({"name":"SameDocument"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    n.save("baseline.FCStd");
    n.failed_cleanup();
    n.transaction(set_width(17.));
    n.save("baseline.FCStd");
    n.send("undo", json!({}), true);
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 10.0);
    assert!(n.editor.dirty, "failed cleanup must not own a later Undo");
    n.assert_discard_guards();
}

fn replacement(open: bool) {
    let mut n = Native::new(if open { "replace-open" } else { "replace-new" });
    n.send("new", json!({"name":"ReplacementTarget"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    let target = n.save("target.FCStd");
    n.send("new", json!({"name":"OldDocument"}), true);
    n.save("old.FCStd");
    n.failed_cleanup();
    n.editor.request_file(if open {
        FileAction::Open
    } else {
        FileAction::New
    });
    assert!(n.editor.pending_file.is_some());
    n.click("Discard");
    if open {
        // Browser selection is outside this test. Submit its ordinary accepted path.
        assert!(n.editor.file_dialog.is_some());
        n.editor.cancel_file_browser();
        n.editor.file_input = target.to_string_lossy().into_owned();
        n.send("open", json!({"path":target}), true);
    } else {
        n.receive("new", true);
        n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    }
    n.save("replacement.FCStd");
    n.transaction(set_width(17.));
    n.save("replacement.FCStd");
    n.send("undo", json!({}), true);
    assert!(n.editor.dirty);
    n.assert_reset();
    n.assert_discard_guards();
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn failed_cleanup_then_new_resets_document_state() {
    replacement(false);
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn failed_cleanup_then_open_resets_document_state() {
    replacement(true);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn successful_new_sketch_cancel_restores_previous_dirty_state() {
    for dirty in [false, true] {
        let mut n = Native::new(if dirty {
            "cancel-dirty"
        } else {
            "cancel-clean"
        });
        n.send("new", json!({"name":"SuccessfulCancel"}), true);
        n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
        n.save("baseline.FCStd");
        if dirty {
            n.transaction(set_width(17.));
        }
        n.editor.prepare_new_sketch();
        n.editor.sketch = Some("Sketch".into());
        n.transaction(json!([{"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}]));
        n.receive("sketch_begin", true);
        let token = n.editor.sketch_ui.session.clone().unwrap();
        n.send("sketch_cancel", json!({"token":token}), true);
        n.receive("undo", true);
        assert_eq!(n.editor.dirty, dirty);
        assert!(!n.editor.sketch_ui.cleanup);
        assert!(
            !objects(&n.editor.state)
                .iter()
                .any(|o| o["name"] == "Sketch")
        );
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn unavailable_preview_cancel_close_failure_retry_and_new_open() {
    let mut n = Native::new("preview-recovery");
    n.send("new", json!({"name":"PreviewRecovery"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    let path = n.save("baseline.FCStd");
    let disk = fs::read(&path).unwrap();
    n.transaction(set_width(17.));
    let token = "lifecycle-owner";
    n.editor.draft = Some(Draft {
        id: "native.property".into(),
        name: "Length".into(),
        token: token.into(),
        parameters: json!({"object":"Box","property":"Length","value":20}),
        changed: None,
        valid: false,
        all_parameters: false,
        deferred: false,
        committing: false,
    });
    n.editor.preview();
    n.receive("preview", true);
    n.send(
        "preview",
        json!({"token":token,"edits":[{"op":"set","object":"Box","property":"Length","value":30}],
        "lifecycle_fault":"validate_once","lifecycle_fail_close_once":true}),
        false,
    );
    assert_eq!(n.editor.state["available"], false);
    let state = n.editor.state.clone();
    let ui = n.ui_state();
    n.click("Close unavailable document");
    assert!(n.editor.pending_file.is_some());
    assert!(!n.editor.busy);
    n.click("Cancel");
    assert!(n.editor.pending_file.is_none());
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, state);
    assert_eq!(n.ui_state(), ui);
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", false);
    assert_eq!(n.editor.state, state);
    assert_eq!(n.ui_state(), ui);
    assert!(n.editor.dirty);
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", true);
    assert!(n.editor.state.is_null() && !n.editor.dirty && n.editor.path.is_empty());
    n.assert_reset();
    let close = n.events.last().unwrap();
    assert!(close["error"].is_null());
    assert_eq!(close["scenes"], json!([]));
    assert_eq!(
        fs::read(&path).unwrap(),
        disk,
        "recovery must not save or overwrite"
    );
    n.click("New");
    n.receive("new", true);
    assert_eq!(n.editor.state["available"], true);
    n.editor.file_input = path.to_string_lossy().into_owned();
    n.send("open", json!({"path":path}), true);
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 10.0);
}

fn poisoned_sketch(owner: &str) {
    let mut n = Native::new(&format!("sketch-{owner}"));
    n.send("new", json!({"name":format!("Sketch_{owner}")}), true);
    n.transaction(
        json!([{"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[10,0,0]}]),
    );
    n.save("baseline.FCStd");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let selected = crate::sketch_commands::SketchElement { index: 0, point: 0 };
    if owner == "dimension" {
        n.editor.viewport.sketch_selection = vec![selected];
        n.editor.new_dimension("Distance");
        assert!(n.editor.sketch_ui.dimension.is_some());
        n.receive("quantities", true);
    }
    if owner == "task" {
        n.editor.begin_sketch_task(
            "extend",
            Some(vec![crate::sketch_commands::SketchElement {
                index: 0,
                point: 2,
            }]),
        );
        assert!(n.editor.sketch_ui.task.is_some());
        n.receive("quantities", true);
    }
    n.send("transaction",json!({"label":"Poison sketch","edits":[{"op":"construction","object":"Sketch","index":0}],"lifecycle_fault":"validate_twice"}),false);
    assert_eq!(n.editor.state["available"], false);
    assert!(!n.editor.state["sketch_edit"].is_null());
    let state = n.editor.state.clone();
    let ui = n.ui_state();
    n.click("Close unavailable document");
    n.click("Cancel");
    assert!(n.editor.pending_file.is_none());
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state, state);
    assert_eq!(n.ui_state(), ui);
    n.click("Close unavailable document");
    n.click("Close document and discard");
    n.receive("close", true);
    n.assert_reset();
    assert!(n.editor.state.is_null() && !n.editor.dirty);
    n.click("New");
    n.receive("new", true);
    assert_eq!(n.editor.state["available"], true);
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn unavailable_sketch_owner_can_close() {
    poisoned_sketch("session");
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn unavailable_dimension_owner_can_close() {
    poisoned_sketch("dimension");
}
#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn unavailable_task_owner_can_close() {
    poisoned_sketch("task");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn ordinary_failed_open_keeps_dirty_content_and_save_retry_path() {
    let mut n = Native::new("failed-open");
    n.send("new", json!({"name":"FailedOpen"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    let path = n.save("baseline.FCStd");
    n.transaction(set_width(17.));
    n.editor.file_input = n
        .folder
        .join("missing.FCStd")
        .to_string_lossy()
        .into_owned();
    n.send("open", json!({"path":n.editor.file_input}), false);
    assert!(n.editor.dirty && n.editor.path == path.to_string_lossy());
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 17.0);
    n.editor.save();
    n.receive("save", true);
    assert!(!n.editor.dirty && n.editor.path == path.to_string_lossy());
    n.editor.request_file(FileAction::New);
    n.receive("new", true);
    n.editor.file_input = path.to_string_lossy().into_owned();
    n.send("open", json!({"path":path}), true);
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 17.0);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn document_switch_preserves_dirty_confirmation_and_each_save_path() {
    let mut n = Native::new("document-switch");
    n.send("new", json!({"name":"FirstModel"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    let first_path = n.save("First.FCStd");
    n.transaction(set_width(17.));
    n.assert_discard_guards();
    assert_eq!(n.editor.path, first_path.to_string_lossy());
    n.editor.save();
    n.receive("save", true);
    let first_bytes = fs::read(&first_path).unwrap();
    n.editor.request_file(FileAction::New);
    n.receive("new", true);
    assert!(!n.editor.dirty && n.editor.path.is_empty());
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    let second_path = n.save("Second.FCStd");
    n.transaction(set_width(23.));
    n.assert_discard_guards();
    assert_eq!(n.editor.path, second_path.to_string_lossy());
    n.editor.save();
    n.receive("save", true);
    assert_eq!(fs::read(&first_path).unwrap(), first_bytes);
    n.editor.file_input = first_path.to_string_lossy().into_owned();
    n.send("open", json!({"path":first_path}), true);
    assert!(!n.editor.dirty);
    assert_eq!(n.editor.path, first_path.to_string_lossy());
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 17.0);
    n.editor.file_input = second_path.to_string_lossy().into_owned();
    n.send("open", json!({"path":second_path}), true);
    assert!(!n.editor.dirty);
    assert_eq!(n.editor.path, second_path.to_string_lossy());
    assert_eq!(object(&n.editor.state, "Box")["properties"]["Width"], 23.0);
    assert_eq!(fs::read(&first_path).unwrap(), first_bytes);
}

#[path = "select_other_ownership_tests.rs"]
mod select_other_ownership_tests;

#[path = "select_other_reply_tests.rs"]
mod select_other_reply_tests;

#[path = "fit_tree_selection_tests.rs"]
mod fit_tree_selection_tests;

#[path = "popup_ownership_tests.rs"]
mod popup_ownership_tests;

fn tutorial_key(n: &mut Native, key: Key, modifiers: egui::Modifiers) {
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
fn tutorial_click(n: &mut Native, point: [f64; 3]) {
    let pos = n.editor.viewport.sketch_screen_point(point);
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

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_sketch_keyboard_and_native_rectangles() {
    let mut n = Native::new("tutorial-rectangle-keys");
    n.send("new", json!({"name":"TutorialRectangleFixture"}), true);
    // Fixture preparation is separate from the tutorial keyboard assertions.
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([{"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"}]));
    n.receive("sketch_begin", true);
    n.frame(vec![]);
    tutorial_key(&mut n, Key::G, egui::Modifiers::SHIFT);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Draw::Select,
        "Shift+G is not corner rectangle"
    );
    n.editor.busy = true;
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Draw::Select,
        "busy input belongs to the pending job"
    );
    n.editor.busy = false;
    egui::Popup::open_id(&n.ctx, egui::Id::new("tutorial-owner-popup"));
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(
        n.editor.sketch_ui.tool,
        Draw::Select,
        "popup owns the frame"
    );
    egui::Popup::close_all(&n.ctx);
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Draw::Rectangle);
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Draw::Select);
    tutorial_key(&mut n, Key::R, egui::Modifiers::NONE);
    assert_eq!(n.editor.sketch_ui.tool, Draw::CenterRectangle);
    tutorial_key(&mut n, Key::Q, egui::Modifiers::NONE);
    assert!(n.editor.sketch_ui.construction);
    tutorial_click(&mut n, [10., 10., 0.]);
    tutorial_click(&mut n, [15., 14., 0.]);
    n.receive("transaction", true);
    let sketch = object(&n.editor.state, "Sketch");
    assert_eq!(sketch["sketch"]["geometry_count"], 4);
    assert!(
        sketch["sketch"]["geometry"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["construction"] == true)
    );
    tutorial_key(&mut n, Key::Q, egui::Modifiers::NONE);
    tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
    tutorial_click(&mut n, [25., 10., 0.]);
    tutorial_click(&mut n, [30., 14., 0.]);
    n.receive("transaction", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry_count"],
        8
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_shift_extrude_finishes_native_session_with_exact_profile() {
    let mut n = Native::new("tutorial-extrude-handoff");
    n.send("new", json!({"name":"TutorialExtrudeFixture"}), true);
    // Fixture preparation: a native Body and sketch; tutorial actions start after Begin.
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.receive("sketch_begin", true);
    n.editor.selected_object = Some("Body".into());
    n.editor.body_ui.selected = vec!["Body".into()];
    n.frame(vec![]);
    tutorial_key(&mut n, Key::E, egui::Modifiers::SHIFT);
    assert!(n.editor.busy && n.editor.sketch_ui.session.is_some());
    n.receive("sketch_finish", true);
    assert!(n.editor.sketch_ui.session.is_none());
    let draft = n.editor.draft.as_ref().expect("native Extrude task");
    assert_eq!(draft.parameters["type"], "PartDesign::Pad");
    assert_eq!(draft.parameters["properties"]["Profile"]["link"], "Sketch");
    assert!(n.editor.busy, "native schema request must be admitted");
    n.receive("feature_schema", true);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_revolve_tab_axis_and_enter_commit() {
    let mut n = Native::new("tutorial-revolve-axis");
    n.send("new", json!({"name":"TutorialRevolveFixture"}), true);
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.receive("sketch_begin", true);
    // Fixture coordinates, not dimensions specified by the Tech Tip.
    n.transaction(json!([
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,1,0],"end":[10,1,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,1,0],"end":[10,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,5,0],"end":[5,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,5,0],"end":[5,1,0]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[0,2,1,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[1,2,2,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[2,2,3,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[3,2,0,1]}
    ]));
    n.frame(vec![]);
    tutorial_key(&mut n, Key::W, egui::Modifiers::SHIFT);
    n.receive("sketch_finish", true);
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["properties"]["Profile"]["link"],
        "Sketch"
    );
    n.receive("feature_schema", true);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    if n.editor.busy {
        n.receive("preview", true);
    }
    tutorial_key(&mut n, Key::Tab, egui::Modifiers::NONE);
    assert!(n.editor.property_ui.axis_pick);
    // Fixture coordinates are not source dimensions. Select the same native
    // H axis at a visible point outside the sketch's endpoint hit regions.
    let axis_candidates = [[-20., 0., 0.], [-40., 0., 0.], [20., 0., 0.], [40., 0., 0.]];
    let point = axis_candidates
        .into_iter()
        .find(|point| {
            let screen = n.editor.viewport.sketch_screen_point(*point);
            n.editor.viewport.response.as_ref().is_some_and(|r| {
                r.rect.shrink(6.).contains(screen)
                    && n.ctx.layer_id_at(screen) == Some(r.layer_id)
                    && n.editor.viewport.sketch_hit(screen)
                        == Some(SketchElement {
                            index: -1,
                            point: 0,
                        })
            })
        })
        .expect("visible unobstructed native horizontal axis point");
    let screen = n.editor.viewport.sketch_screen_point(point);
    let hit = n.editor.viewport.sketch_hit(screen);
    fs::write(n.folder.join("axis-before-click.json"), serde_json::to_vec_pretty(&json!({
        "point":point, "candidates":axis_candidates, "screen":[screen.x,screen.y], "hit":hit,
        "axis_pick":n.editor.property_ui.axis_pick,
        "response_rect":n.editor.viewport.response.as_ref().map(|r| json!([r.rect.min.x,r.rect.min.y,r.rect.max.x,r.rect.max.y]))
    })).unwrap()).unwrap();
    assert_eq!(
        hit,
        Some(SketchElement {
            index: -1,
            point: 0
        }),
        "point must identify native horizontal axis"
    );
    tutorial_click(&mut n, point);
    assert!(
        !n.editor.property_ui.axis_pick,
        "axis click must be accepted"
    );
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["properties"]["ReferenceAxis"],
        json!({"link":"Sketch","subelements":["H_Axis"]})
    );
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    n.receive("preview_commit", true);
    assert!(n.editor.draft.is_none());
    let solid = objects(&n.editor.state)
        .iter()
        .find(|o| o["type"] == "PartDesign::Revolution")
        .unwrap();
    assert_eq!(solid["shape"]["valid"], true);
    assert_eq!(solid["shape"]["solids"], 1);
    let committed_name = solid["name"].as_str().unwrap().to_owned();
    let committed_axis = solid["properties"]["ReferenceAxis"].clone();
    assert_eq!(
        committed_axis,
        json!({"link":"Sketch","subelements":["H_Axis"]})
    );
    n.send("undo", json!({}), true);
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == committed_name)
    );
    n.send("redo", json!({}), true);
    let replayed = object(&n.editor.state, &committed_name);
    assert_eq!(replayed["properties"]["ReferenceAxis"], committed_axis);
    assert_eq!(replayed["shape"]["solids"], 1);
    let path = n.save("tutorial-revolve.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    let reopened = object(&n.editor.state, &committed_name);
    assert_eq!(reopened["properties"]["ReferenceAxis"], committed_axis);
    assert_eq!(reopened["shape"]["solids"], 1);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_failed_finish_retains_sketch_until_cancel_and_redo() {
    let mut n = Native::new("tutorial-failed-finish");
    n.send("new", json!({"name":"TutorialFailedFinishFixture"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,1,0],"end":[10,1,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,1,0],"end":[10,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,5,0],"end":[5,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,5,0],"end":[5,1,0]},
        {"op":"add_object","type":"PartDesign::Pad","name":"Pad","body":"Body"},
        {"op":"set","object":"Pad","property":"Profile","value":{"link":"Sketch"}},
        {"op":"set","object":"Pad","property":"Length","value":5}
    ]));
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n.transaction(json!([{"op":"delete_geometry","object":"Sketch","index":0}]));
    n.frame(vec![]);
    tutorial_key(&mut n, Key::E, egui::Modifiers::SHIFT);
    n.receive("sketch_finish", false);
    assert!(n.editor.sketch_ui.session.is_some());
    assert!(n.editor.draft.is_none());
    assert!(n.editor.sketch_ui.after_finish.is_none());
    tutorial_key(&mut n, Key::Z, egui::Modifiers::COMMAND);
    n.receive("undo", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry_count"],
        4
    );
    tutorial_key(
        &mut n,
        Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    n.receive("redo", true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry_count"],
        3
    );
    n.click("Cancel");
    n.receive("sketch_cancel", true);
    assert!(n.editor.sketch_ui.session.is_none());
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry_count"],
        4
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_shift_sketch_selected_native_plane_and_normal() {
    let mut n = Native::new("tutorial-plane-entry");
    n.send("new", json!({"name":"TutorialPlaneFixture"}), true);
    // Fixture prepares a Body; subsequent selection and keys are actual Editor actions.
    n.transaction(json!([{"op":"add_object","type":"PartDesign::Body","name":"Body"}]));
    n.frame(vec![]);
    tutorial_key(&mut n, Key::M, egui::Modifiers::COMMAND);
    assert!(
        !n.editor.measurement_is_open(),
        "Ctrl+M is reserved for the missing Mate connector"
    );
    n.click("References");
    n.click("XY-plane");
    assert_eq!(n.editor.selected_object.as_deref(), Some("XY_Plane"));
    tutorial_key(&mut n, Key::S, egui::Modifiers::SHIFT);
    assert!(
        n.editor.busy,
        "selected plane should start native sketch creation without a chooser"
    );
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    assert!(n.editor.sketch_ui.session.is_some());
    assert!(n.editor.sketch_creation.is_none());
    n.editor.viewport.iso(); // Fixture prepares an oblique view before the tutorial N action.
    tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
    let origin = n.editor.viewport.sketch_screen_point([0., 0., 0.]);
    let right = n.editor.viewport.sketch_screen_point([10., 0., 0.]);
    let up = n.editor.viewport.sketch_screen_point([0., 10., 0.]);
    assert_eq!(origin.y, right.y);
    assert_eq!(origin.x, up.x);
    assert!(n.editor.sketch_ui.session.is_some());
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_revolve_axis_mode_cancel_clears_overlay() {
    let mut n = Native::new("tutorial-axis-cancel");
    n.send("new", json!({"name":"TutorialAxisCancelFixture"}), true);
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.receive("sketch_begin", true);
    // Fixture coordinates, not dimensions specified by the Tech Tip.
    n.transaction(json!([
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,1,0],"end":[10,1,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,1,0],"end":[10,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,5,0],"end":[5,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,5,0],"end":[5,1,0]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[0,2,1,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[1,2,2,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[2,2,3,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[3,2,0,1]}
    ]));
    n.frame(vec![]);
    tutorial_key(&mut n, Key::W, egui::Modifiers::SHIFT);
    n.receive("sketch_finish", true);
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["properties"]["Profile"]["link"],
        "Sketch"
    );
    n.receive("feature_schema", true);
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    if n.editor.busy {
        n.receive("preview", true);
    }
    tutorial_key(&mut n, Key::Tab, egui::Modifiers::NONE);
    assert!(n.editor.property_ui.axis_pick);
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    n.frame(vec![]);
    assert!(
        egui::Popup::is_any_open(&n.ctx),
        "axis overlay refresh must preserve the popup"
    );
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(
        n.editor.property_ui.axis_pick,
        "popup Escape must preserve axis selection"
    );
    n.click("Cancel");
    n.receive("preview_cancel", true);
    n.frame(vec![]);
    assert!(n.editor.draft.is_none());
    assert!(!n.editor.property_ui.axis_pick);
    assert!(n.editor.property_ui.axis_profile.is_none());
    let point = n.editor.viewport.sketch_screen_point([7., 0., 0.]);
    assert!(n.editor.viewport.sketch_hit(point).is_none());
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["type"] == "PartDesign::Revolution")
    );
    // Retry through the actual sketch tree row and the same shortcut.
    let output = n.frame(vec![]);
    let area = n
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("features")))
        .unwrap();
    let hits: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Sketch" => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                area.contains(rect.center()).then_some(rect.center())
            }
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 1, "one sketch tree row");
    for _ in 0..2 {
        for pressed in [true, false] {
            n.frame(vec![
                egui::Event::PointerMoved(hits[0]),
                egui::Event::PointerButton {
                    pos: hits[0],
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
    }
    n.receive("sketch_begin", true);
    tutorial_key(&mut n, Key::W, egui::Modifiers::SHIFT);
    n.receive("sketch_finish", true);
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["properties"]["Profile"]["link"],
        "Sketch"
    );
    assert!(!n.editor.property_ui.axis_pick);
    n.receive("feature_schema", true);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn tutorial_axis_preview_reply_popup_escape_preserves_owner() {
    let mut n = Native::new("tutorial-axis-reply");
    n.send("new", json!({"name":"TutorialAxisReplyFixture"}), true);
    n.editor.prepare_new_sketch();
    n.editor.sketch = Some("Sketch".into());
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.receive("sketch_begin", true);
    // Fixture coordinates, not dimensions specified by the Tech Tip.
    n.transaction(json!([
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,1,0],"end":[10,1,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,1,0],"end":[10,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[10,5,0],"end":[5,5,0]},
        {"op":"geometry","object":"Sketch","kind":"line","start":[5,5,0],"end":[5,1,0]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[0,2,1,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[1,2,2,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[2,2,3,1]},
        {"op":"constraint","object":"Sketch","kind":"Coincident","args":[3,2,0,1]}
    ]));
    n.frame(vec![]);
    tutorial_key(&mut n, Key::W, egui::Modifiers::SHIFT);
    n.receive("sketch_finish", true);
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["properties"]["Profile"]["link"],
        "Sketch"
    );
    n.receive("feature_schema", true);
    std::thread::sleep(Duration::from_millis(200));
    // Tab enables the axis selector; the same update dispatches the pending preview.
    tutorial_key(&mut n, Key::Tab, egui::Modifiers::NONE);
    assert!(n.editor.property_ui.axis_pick && n.editor.busy);
    n.click("View");
    assert!(egui::Popup::is_any_open(&n.ctx));
    let reply = n
        .raw
        .recv_timeout(Duration::from_secs(25))
        .expect("native preview reply");
    assert_eq!(reply.op, "preview");
    assert!(reply.accepted, "native preview: {:?}", reply.error);
    n.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,
        "state":reply.state,"scenes":reply.scenes,"observation":"queued actual native preview before popup Escape"}));
    n.record();
    n.forward.send(reply).unwrap();
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&n.ctx));
    assert!(n.editor.property_ui.axis_pick);
    assert!(n.editor.draft.is_some() && !n.editor.busy);
    tutorial_key(&mut n, Key::Enter, egui::Modifiers::NONE);
    assert!(
        n.editor.property_ui.axis_pick && n.editor.draft.is_some() && !n.editor.busy,
        "Enter while selecting an axis must not commit"
    );
    n.click("Cancel");
    n.receive("preview_cancel", true);
}

fn default_guide_click(n: &mut Native, plane: crate::viewport::StandardPlane) {
    n.frame(vec![]);
    let pos = n
        .editor
        .viewport
        .standard_guide_probe(plane)
        .expect("visible guide hit target");
    fs::write(
        n.folder.join("guide-target.json"),
        serde_json::to_vec_pretty(&json!({
            "plane":format!("{plane:?}"), "point":[pos.x,pos.y]
        }))
        .unwrap(),
    )
    .unwrap();
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

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn default_guide_preselect_and_command_first_create_native_support() {
    use crate::viewport::StandardPlane;
    for (index, plane) in [
        StandardPlane::Top,
        StandardPlane::Front,
        StandardPlane::Right,
    ]
    .into_iter()
    .enumerate()
    {
        for first_command in [false, true] {
            let mut n = Native::new(&format!("default-guide-{index}-{first_command}"));
            n.send(
                "new",
                json!({"name":format!("DefaultGuide{index}{first_command}")}),
                true,
            );
            assert!(objects(&n.editor.state).is_empty());
            let before = n.editor.state.clone();
            if first_command {
                n.click("Sketch");
                assert!(n.editor.awaiting_standard_plane());
                assert_eq!(n.editor.state, before);
            }
            default_guide_click(&mut n, plane);
            if !first_command {
                assert_eq!(n.editor.viewport.selected_standard_plane(), Some(plane));
                assert_eq!(n.editor.state, before);
                assert!(!n.editor.dirty && n.raw.try_recv().is_err());
                tutorial_key(&mut n, Key::S, egui::Modifiers::SHIFT);
            }
            assert!(
                n.editor.busy,
                "native creation must begin on selected guide"
            );
            n.receive("transaction", true);
            n.receive("sketch_begin", true);
            assert!(n.editor.sketch_creation.is_none());
            let name = n.editor.sketch.clone().expect("native sketch");
            let sketch = object(&n.editor.state, &name);
            assert_eq!(sketch["properties"]["MapMode"], "FlatFace");
            let support = sketch["properties"]["AttachmentSupport"][0]["link"]
                .as_str()
                .unwrap()
                .to_owned();
            let native_plane = object(&n.editor.state, &support);
            assert_eq!(native_plane["properties"]["Role"], plane.role());
            assert_eq!(native_plane["type"], "App::Plane");
            let expected_rotation = match plane {
                StandardPlane::Top => json!([0., 0., 0., 1.]),
                StandardPlane::Front => json!([
                    f64::from_bits(0x3fe6a09e667f3bcc),
                    0.,
                    0.,
                    f64::from_bits(0x3fe6a09e667f3bcd)
                ]),
                StandardPlane::Right => json!([0.5, 0.5, 0.5, 0.5]),
            };
            assert_eq!(sketch["global_placement"]["position"], json!([0., 0., 0.]));
            assert_eq!(sketch["global_placement"]["rotation"], expected_rotation);
            assert!(n.editor.viewport.selected_standard_plane().is_none());
            tutorial_key(&mut n, Key::N, egui::Modifiers::NONE);
            if plane == StandardPlane::Top && !first_command {
                tutorial_key(&mut n, Key::G, egui::Modifiers::NONE);
                tutorial_click(&mut n, [0., 0., 0.]);
                tutorial_click(&mut n, [10., 5., 0.]);
                n.receive("transaction", true);
                let sk = &object(&n.editor.state, &name)["sketch"];
                assert_eq!(sk["geometry_count"], 4);
                assert!(
                    sk["constraints"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|c| c["type"] == "Coincident")
                );
                n.click("Finish");
                n.receive("sketch_finish", true);
                let path = n.save("guide.FCStd");
                n.send("close", json!({}), true);
                n.send("open", json!({"path":path}), true);
                let reopened = object(&n.editor.state, &name);
                assert_eq!(reopened["properties"]["MapMode"], "FlatFace");
                assert_eq!(
                    reopened["properties"]["AttachmentSupport"][0]["link"],
                    support
                );
                assert_eq!(reopened["sketch"]["geometry_count"], 4);
            } else {
                n.click("Cancel");
                n.receive("sketch_cancel", true);
                n.receive("undo", true);
            }
        }
    }
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn default_guide_cancel_before_choice_preserves_redo_and_state() {
    let mut n = Native::new("default-guide-cancel");
    n.send("new", json!({"name":"DefaultGuideCancel"}), true);
    n.transaction(json!([{"op":"add_object","type":"Part::Box","name":"Box"}]));
    n.send("undo", json!({}), true);
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    assert!(objects(&before).is_empty() && before["redo_count"].as_u64().unwrap() > 0);
    n.click("Sketch");
    assert!(n.editor.awaiting_standard_plane());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(n.editor.sketch_creation.is_none());
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    assert!(n.raw.try_recv().is_err());
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn default_guides_busy_help_and_new_reply_cannot_reuse_selection() {
    use crate::viewport::StandardPlane;
    let mut n = Native::new("default-guide-ownership");
    n.send("new", json!({"name":"GuideOwnershipA"}), true);
    n.frame(vec![]);
    n.editor.busy = true;
    default_guide_click(&mut n, StandardPlane::Top);
    assert!(n.editor.viewport.selected_standard_plane().is_none());
    n.editor.busy = false;
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    default_guide_click(&mut n, StandardPlane::Top);
    assert!(n.editor.viewport.selected_standard_plane().is_none());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    default_guide_click(&mut n, StandardPlane::Top);
    assert_eq!(
        n.editor.viewport.selected_standard_plane(),
        Some(StandardPlane::Top)
    );
    n.editor.send("new", json!({"name":"GuideOwnershipB"}));
    let reply = n.raw.recv_timeout(Duration::from_secs(25)).unwrap();
    assert_eq!(reply.op, "new");
    assert!(reply.accepted);
    n.forward.send(reply).unwrap();
    n.frame(vec![egui::Event::Key {
        key: Key::S,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }]);
    assert!(n.editor.viewport.selected_standard_plane().is_none());
    assert!(n.editor.sketch_creation.is_none() && n.editor.sketch_ui.session.is_none());
    assert!(!n.editor.busy && objects(&n.editor.state).is_empty());
}

#[test]
#[ignore = "requires lifecycle runtime with genuinely missing checkpoint adapter"]
fn sketch_creation_missing_checkpoint_preserves_redo_before_native_creation() {
    let mut n = Native::new("missing-checkpoint-creation");
    n.send("new", json!({"name":"MissingCheckpointCreation"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"PriorBox"},
        {"op":"set","object":"PriorBox","property":"Length","value":23.75},
        {"op":"set","object":"PriorBox","property":"Width","value":2.5},
        {"op":"set","object":"PriorBox","property":"Height","value":3.25}
    ]));
    let prior_id = object(&n.editor.state, "PriorBox")["observed_native_id"].clone();
    assert!(
        prior_id.as_u64().is_some(),
        "observer must expose the actual native ID"
    );
    n.send("undo", json!({}), true);
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    assert!(objects(&before).is_empty());
    assert_eq!(before["redo_count"], 1);
    n.click("Sketch");
    assert!(n.editor.awaiting_standard_plane());
    default_guide_click(&mut n, crate::viewport::StandardPlane::Top);
    n.receive("transaction", false);
    assert!(n.editor.error.as_deref().is_some_and(|message| {
        message.contains("Native sketch editing is unavailable")
            && message.contains("No sketch was created")
    }));
    assert_eq!(
        n.editor.state, before,
        "rejected creation must preserve full native public state"
    );
    assert_eq!(n.editor.dirty, dirty);
    assert!(n.editor.sketch.is_none() && n.editor.sketch_ui.session.is_none());
    assert!(!n.editor.busy && !n.editor.sketch_ui.pending_begin);
    assert!(
        n.raw.try_recv().is_err(),
        "no Begin or cleanup Undo may be queued"
    );
    n.click("Cancel");
    assert!(n.editor.sketch_creation.is_none());
    n.send("redo", json!({}), true);
    let restored = object(&n.editor.state, "PriorBox");
    assert_eq!(restored["observed_native_id"], prior_id);
    assert_eq!(restored["properties"]["Length"], 23.75);
    assert_eq!(restored["properties"]["Width"], 2.5);
    assert_eq!(restored["properties"]["Height"], 3.25);
    assert_eq!(objects(&n.editor.state).len(), 1);
}

#[path = "sketch_name_tests.rs"]
mod sketch_name_tests;

#[path = "parallel_distance_tests.rs"]
mod parallel_distance_tests;

#[path = "trimetric_tests.rs"]
mod trimetric_tests;

#[path = "view_cube_tests.rs"]
mod view_cube_tests;

#[path = "block_tutorial_tests.rs"]
mod block_tutorial_tests;

#[path = "loft_tutorial_tests.rs"]
mod loft_tutorial_tests;

#[path = "receiver_input_tests.rs"]
mod receiver_input_tests;

#[path = "receiver_shortcut_tests.rs"]
mod receiver_shortcut_tests;

#[path = "receiver_master_profile_attempt_tests.rs"]
mod receiver_master_profile_attempt_tests;

#[path = "receiver_passive_escape_source_tests.rs"]
mod receiver_passive_escape_source_tests;
