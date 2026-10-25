//! Actual Editor/update/model_thread with a native worker. Run ignored tests with
//! CAD_LIFECYCLE_RUNTIME pointing to a runtime manifest and CAD_LIFECYCLE_OUTPUT
//! pointing to a fresh writable directory. No desktop application is launched.
use super::super::*;
use super::*;
use std::{fs, thread::JoinHandle};

#[path = "mirror_tests.rs"]
mod mirror_tests;

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
