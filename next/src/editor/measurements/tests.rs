use super::*;
use std::{fs, thread::JoinHandle};

struct Native {
    editor: Editor,
    raw: Receiver<Reply>,
    forward: Sender<Reply>,
    actor: Option<JoinHandle<()>>,
    ctx: egui::Context,
    time: f64,
    events: Vec<Value>,
    output: PathBuf,
}
impl Native {
    fn new() -> Self {
        Self::case("workflow")
    }
    fn case(case: &str) -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(
                std::env::var_os("CAD_MEASUREMENT_RUNTIME").expect("CAD_MEASUREMENT_RUNTIME"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend/freecad_worker.py"),
        );
        config.args = runtime["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().into())
            .collect();
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
            measurement_ui: MeasurementUi::default(),
            units_ui: Default::default(),
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
        let output = PathBuf::from(
            std::env::var_os("CAD_MEASUREMENT_OUTPUT").expect("CAD_MEASUREMENT_OUTPUT"),
        )
        .join(case);
        fs::create_dir_all(&output).unwrap();
        Self {
            editor,
            raw,
            forward,
            actor: Some(actor),
            ctx: egui::Context::default(),
            time: 0.,
            events: vec![],
            output,
        }
    }
    fn receive(&mut self, op: &str, accepted: bool) {
        let r = self.raw.recv_timeout(Duration::from_secs(40)).unwrap();
        self.accept(r, op, accepted);
    }
    fn accept(&mut self, r: Reply, op: &str, accepted: bool) {
        assert_eq!(r.op, op);
        assert_eq!(r.accepted, accepted, "{:?}", r.error);
        self.events.push(json!({"op":r.op,"accepted":r.accepted,"state":r.state,"data":r.data,"error":r.error,"scene_count":r.scenes.len()}));
        if op == "measure" {
            assert!(r.scenes.is_empty());
        }
        fs::write(
            self.output.join("editor-events.json"),
            serde_json::to_vec_pretty(&self.events).unwrap(),
        )
        .unwrap();
        self.forward.send(r).unwrap();
        self.editor.poll();
        assert!(!self.editor.busy);
    }
    fn send(&mut self, op: &str, p: Value) {
        self.editor.send(op, p);
        assert!(self.editor.busy);
        self.receive(op, true);
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let mut frame = eframe::Frame::_new_kittest();
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 1000.),
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
        let output = self.frame(vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::epaint::Shape::Text(t) = &s.shape {
                    (t.galley.text() == text)
                        .then(|| t.galley.rect.translate(t.pos.to_vec2()).center())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Visible button missing: {text}"));
        self.click_at(pos, egui::Modifiers::NONE);
    }
    fn click_at(&mut self, pos: egui::Pos2, modifiers: egui::Modifiers) {
        self.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            },
        ]);
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        }]);
    }
    fn close_measure_window(&mut self) {
        self.frame(vec![]);
        let output = self.frame(vec![]);
        let title = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == "Measure / Inspect" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .expect("Measure window title is visible");
        // Locate the actual rendered X strokes in this window's title row.
        let pos = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::LineSegment { points, .. } => {
                    let delta = points[1] - points[0];
                    let center = points[0] + delta * 0.5;
                    ((delta.x.abs() - delta.y.abs()).abs() < 0.1
                        && delta.x.abs() > 4.
                        && center.x > title.right()
                        && (center.y - title.center().y).abs() < title.height())
                    .then_some(center)
                }
                _ => None,
            })
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .expect("Rendered window close icon");
        self.click_at(pos, egui::Modifiers::NONE);
        assert!(
            !self.editor.measurement_ui.open,
            "Actual window close button must dismiss Measure"
        );
    }
    fn key(&mut self, key: Key, modifiers: egui::Modifiers) {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }]);
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let (tx, _) = mpsc::sync_channel(1);
        self.editor.jobs = tx;
        if let Some(actor) = self.actor.take() {
            actor.join().unwrap();
        }
    }
}
fn reference(name: &str, element: &str) -> crate::viewport::Selection {
    crate::viewport::Selection {
        object: name.into(),
        subelement: element.into(),
    }
}

#[test]
#[ignore = "requires pinned native runtime; actual Editor methods and headless egui, no desktop"]
fn native_measure_button_shortcut_clear_cancel_recompute_and_gesture_reads() {
    let mut n = Native::new();
    n.send("new", json!({"name":"MeasureEditor"}));
    n.send(
        "transaction",
        json!({"label":"Measurement fixture","edits":[
            {"op":"add_object","type":"Part::Box","name":"Box"},
            {"op":"set","object":"Box","property":"Length","value":25.4},
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
            {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[12,0,0]}
        ]}),
    );
    n.editor.body_ui.selected = vec!["Box".into()];
    let baseline = n.editor.state.clone();
    let dirty = n.editor.dirty;
    n.editor.error = Some("Existing model message".into());
    n.click("Measure");
    assert!(n.editor.measurement_ui.open && n.editor.busy);
    n.receive("measure", true);
    assert_eq!(n.editor.state, baseline);
    assert_eq!(n.editor.dirty, dirty);
    assert_eq!(n.editor.error.as_deref(), Some("Existing model message"));
    let result = n.editor.measurement_ui.result.as_ref().unwrap();
    assert!(
        (result.selections[0]
            .metrics
            .iter()
            .find(|m| m.key == "volume")
            .unwrap()
            .value
            - 2540.)
            .abs()
            < 1e-6
    );
    n.click("Recompute");
    n.receive("measure", true);
    // Whole-object tree selection can coexist with a native viewport element.
    n.editor.viewport.selections = vec![reference("Box", "Vertex1")];
    n.editor.measurement_pump();
    n.receive("measure", true);
    assert_eq!(
        n.editor
            .measurement_ui
            .result
            .as_ref()
            .unwrap()
            .selections
            .len(),
        2
    );
    assert_eq!(
        n.editor
            .measurement_ui
            .result
            .as_ref()
            .unwrap()
            .distance
            .as_ref()
            .unwrap()
            .metric
            .value,
        0.
    );
    n.click("Clear selection");
    assert!(n.editor.measurement_ui.result.is_none());
    assert!(n.editor.measurement_references().is_empty());
    assert!(!n.editor.busy);
    n.editor.viewport.selections = vec![reference("Box", "Face1"), reference("Box", "Edge2")];
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.receive("measure", true);
    assert!(
        n.editor
            .measurement_ui
            .result
            .as_ref()
            .unwrap()
            .distance
            .is_some()
    );
    n.editor.viewport.selections = vec![reference("Box", "Vertex1")];
    n.editor.measurement_pump();
    assert!(n.editor.measurement_ui.result.is_none());
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.measurement_ui.open);
    n.receive("measure", true);
    assert!(n.editor.measurement_ui.result.is_none());
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    assert!(n.editor.measurement_ui.open);
    n.receive("measure", true);
    // Native preview remains owned after both successful and failed reads.
    n.editor.edit_native_properties("Box");
    let token = n.editor.draft.as_ref().unwrap().token.clone();
    n.editor.draft.as_mut().unwrap().parameters["properties"]["Length"] = json!(30);
    n.editor.preview();
    assert!(n.editor.measurement_ui.result.is_none());
    n.receive("preview", true);
    n.editor.viewport.selections = vec![reference("Box", "Edge1")];
    n.editor.measurement_pump();
    n.receive("measure", true);
    let preview = n.editor.state.clone();
    n.editor.viewport.selections = vec![reference("Box", "Edge999")];
    n.editor.measurement_pump();
    n.receive("measure", false);
    assert_eq!(n.editor.state, preview);
    assert_eq!(n.editor.state["preview"]["token"], token);
    assert!(n.editor.measurement_ui.result.is_none());
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.state, preview);
    assert!(n.editor.draft.is_some());
    n.editor.finish_preview(false);
    n.receive("preview_cancel", true);
    assert!(n.editor.draft.is_none());
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let sketch_token = n.editor.sketch_ui.session.clone().unwrap();
    n.editor.viewport.sketch_selection =
        vec![crate::sketch_commands::SketchElement { index: 0, point: 0 }];
    n.editor.sketch_ui.external = true;
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    assert!(!n.editor.measurement_ui.open && !n.editor.busy);
    n.editor.sketch_ui.external = false;
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.receive("measure", true);
    let session = n.editor.state.clone();
    assert_eq!(
        n.editor.measurement_ui.result.as_ref().unwrap().selections[0].metrics[0].value,
        12.
    );
    n.editor.viewport.clear_sketch_selection();
    n.editor.viewport.selections = vec![reference("Box", "Face1")];
    n.editor.measurement_pump();
    n.receive("measure", false);
    assert_eq!(n.editor.state, session);
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert_eq!(n.editor.state, session);
    assert!(n.editor.sketch_ui.session.is_some());
    n.send("sketch_cancel", json!({"token":sketch_token}));
    // A changed selection while a read is in flight cannot publish its old result.
    n.editor.viewport.selections = vec![reference("Box", "Edge1")];
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    assert!(n.editor.busy);
    n.editor.viewport.selections = vec![reference("Box", "Vertex2")];
    n.receive("measure", true);
    assert!(n.editor.measurement_ui.result.is_none());
    n.editor.measurement_pump();
    n.receive("measure", true);
    assert_eq!(
        n.editor.measurement_ui.result.as_ref().unwrap().selections[0]
            .reference
            .subelement,
        "Vertex2"
    );
}

#[path = "scheduling_tests.rs"]
mod scheduling_tests;

#[path = "scene_tests.rs"]
mod scene_tests;

// Review-only probes appended to private frozen measurement tests.
#[test]
#[ignore = "requires CAD_MEASUREMENT_RUNTIME and fresh CAD_MEASUREMENT_OUTPUT"]
fn review_help_escape_then_measure_preserves_native_preview() {
    let mut n = Native::case("review-help-measure-preview");
    n.send("new", json!({"name":"ReviewHelpMeasurePreview"}));
    n.send(
        "transaction",
        json!({"label":"Review Box","edits":[
            {"op":"add_object","type":"Part::Box","name":"Box"}
        ]}),
    );
    n.editor.body_ui.selected = vec!["Box".into()];
    n.editor.send(
        "preview",
        json!({"token":"review-help-preview","label":"Review preview","edits":[
            {"op":"set","object":"Box","property":"Length","value":17.}
        ]}),
    );
    n.receive("preview", true);
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.receive("measure", true);
    let measurement = format!("{:?}", n.editor.measurement_ui.result.as_ref().unwrap());
    n.key(Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    assert!(n.editor.measurement_ui.open);
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.key(Key::Z, egui::Modifiers::COMMAND);
    n.key(Key::S, egui::Modifiers::COMMAND);
    n.click("Parity checklist");
    n.click("Current instructions");
    assert!(n.raw.try_recv().is_err() && !n.editor.busy);
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx) && n.editor.measurement_ui.open);
    assert_eq!(
        format!("{:?}", n.editor.measurement_ui.result.as_ref().unwrap()),
        measurement
    );
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    n.key(Key::Escape, egui::Modifiers::NONE);
    assert!(!n.editor.measurement_ui.open);
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.busy && n.raw.try_recv().is_err());
    fs::write(
        n.output.join("help-measure-observations.json"),
        serde_json::to_vec_pretty(&json!({
            "passed":true,"state_unchanged":true,"dirty_unchanged":true,
            "help_escape_preserved_measurement":true,"measure_escape_preserved_native_preview":true,
            "preview":n.editor.state["preview"],"measurement":measurement
        }))
        .unwrap(),
    )
    .unwrap();
    n.send("preview_cancel", json!({"token":"review-help-preview"}));
}
