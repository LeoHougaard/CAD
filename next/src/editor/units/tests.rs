use crate::editor::measurements::MeasurementUi;
use crate::editor::*;
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
    fn case(case: &str) -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(std::env::var_os("CAD_UNITS_RUNTIME").expect("CAD_UNITS_RUNTIME")).unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            PathBuf::from(runtime["script"].as_str().unwrap()),
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
        let output = PathBuf::from(std::env::var_os("CAD_UNITS_OUTPUT").expect("CAD_UNITS_OUTPUT"))
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
        // A native reply can schedule the next owned step, such as a package
        // preview after parsing or sketch_begin after attached-sketch creation.
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

impl Native {
    fn units(&mut self, index: usize) {
        let label = self.editor.state["units"]["choices"][index]["label"]
            .as_str()
            .unwrap()
            .to_owned();
        self.click("Units");
        self.click("Document units...");
        self.click(&label);
        self.click("Apply units");
        self.receive("document_settings", true);
        assert_eq!(self.editor.state["units"]["index"], index);
    }
    fn text(&mut self, seed: &str, value: &str) {
        self.click(seed);
        self.key(Key::A, egui::Modifiers::COMMAND);
        self.frame(vec![egui::Event::Text(value.into())]);
    }
    fn fixture(&mut self, name: &str) {
        self.send("new", json!({"name":name}));
        self.send("transaction",json!({"label":"Fixture","edits":[
            {"op":"add_object","type":"Part::Box","name":"Box"},
            {"op":"set","object":"Box","property":"Length","value":25.4},
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
            {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[25.4,0,0]},
            {"op":"constraint","object":"Sketch","kind":"Distance","args":[0,25.4]},
            {"op":"geometry","object":"Sketch","kind":"circle","center":[40,40,0],"radius":12.7},
            {"op":"constraint","object":"Sketch","kind":"Radius","args":[1,12.7]},
            {"op":"geometry","object":"Sketch","kind":"circle","center":[70,0,0],"radius":6.35},
            {"op":"constraint","object":"Sketch","kind":"Diameter","args":[2,12.7]},
            {"op":"geometry","object":"Sketch","kind":"line","start":[70,70,0],"end":[80,80,0]},
            {"op":"constraint","object":"Sketch","kind":"Angle","args":[3,std::f64::consts::FRAC_PI_4]}
        ]}));
    }
    fn object(&self, name: &str) -> &Value {
        objects(&self.editor.state)
            .iter()
            .find(|o| o["name"] == name)
            .unwrap()
    }
    fn save(&mut self, name: &str) -> PathBuf {
        let path = self.output.join(name);
        self.editor.file_input = path.to_string_lossy().to_string();
        self.send("save", json!({"path":path}));
        assert!(!self.editor.dirty);
        path
    }
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_menu_history_noop_save_reopen_and_switch() {
    let mut n = Native::case("settings");
    n.fixture("UnitsSettings");
    n.send(
        "transaction",
        json!({"edits":[{"op":"set","object":"Box","property":"Length","value":50.8}]}),
    );
    n.send("undo", json!({}));
    let original = n.object("Box")["shape"].clone();
    let history = (
        n.editor.state["undo_count"].clone(),
        n.editor.state["redo_count"].clone(),
    );
    n.save("before.FCStd");
    let revision = n.editor.state["revision"].clone();
    n.click("Units");
    n.click("Document units...");
    n.click("Cancel units");
    assert_eq!(revision, n.editor.state["revision"]);
    assert!(!n.editor.dirty);
    n.units(3);
    assert!(n.editor.dirty);
    assert_eq!(n.object("Box")["shape"], original);
    assert_eq!(
        (
            n.editor.state["undo_count"].clone(),
            n.editor.state["redo_count"].clone()
        ),
        history
    );
    assert_eq!(
        n.object("Box")["property_metadata"]["Length"]["quantity"]["display"],
        "1.00 in"
    );
    n.send("redo", json!({}));
    assert_eq!(n.editor.state["units"]["index"], 3);
    assert_eq!(n.object("Box")["properties"]["Length"], 50.8);
    n.send("undo", json!({}));
    assert_eq!(n.editor.state["units"]["index"], 3);
    let saved = n.save("imperial.FCStd");
    let revision = n.editor.state["revision"].clone();
    n.units(3);
    assert_eq!(n.editor.state["revision"], revision);
    assert!(!n.editor.dirty);
    n.send("new", json!({"name":"OtherUnits"}));
    assert_eq!(n.editor.state["units"]["index"], 0);
    n.send("open", json!({"path":saved}));
    assert_eq!(n.editor.state["units"]["index"], 3);
    assert!(!n.editor.dirty);
    assert_eq!(n.object("Box")["shape"], original);
    for index in 0..10 {
        n.units(index);
        assert_eq!(n.object("Box")["shape"], original);
    }
    // MKS is auto preferred. A 25.4 mm value remains displayed in mm.
    n.units(1);
    assert_eq!(
        n.object("Box")["property_metadata"]["Length"]["quantity"]["default_unit"],
        "mm"
    );
    n.save("mks.FCStd");
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_property_input_invalid_recovery_and_deferred_cancel() {
    let mut n = Native::case("properties");
    n.fixture("UnitsProperties");
    n.units(3);
    n.editor.edit_native_properties("Box");
    n.text("1 in", "2");
    n.editor.preview();
    n.receive("preview", true);
    assert_eq!(n.object("Box")["properties"]["Length"], 50.8);
    n.text("2 in", "1 s");
    n.editor.preview();
    n.receive("preview", false);
    assert!(n.editor.draft.is_some());
    assert_eq!(n.object("Box")["properties"]["Length"], 50.8);
    n.text("1 s", "3");
    n.editor.preview();
    n.receive("preview", true);
    assert!((n.object("Box")["properties"]["Length"].as_f64().unwrap() - 76.2).abs() < 1e-12);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(n.editor.draft.is_none());
    n.save("edited.FCStd");
    n.send("undo", json!({}));
    let baseline = n.editor.state.clone();
    n.editor.edit_native_properties("Box");
    assert!(n.editor.draft.as_ref().unwrap().deferred);
    n.text("1 in", "4");
    n.editor.preview();
    assert!(!n.editor.busy);
    n.click("Cancel");
    assert_eq!(n.editor.state, baseline);
    assert!(n.editor.draft.is_none());
    n.send("redo", json!({}));
    assert!((n.object("Box")["properties"]["Length"].as_f64().unwrap() - 76.2).abs() < 1e-12);
    n.editor.start_native_feature("Part::Box");
    n.receive("feature_schema", true);
    // Creation defaults come from the cached native schema in this document.
    let schema = &n.events.last().unwrap()["data"];
    assert_eq!(
        schema["property_metadata"]["Length"]["quantity"]["default_unit"],
        "in"
    );
    // Height is the first visible scalar in this native alphabetical form.
    let seed = schema["property_metadata"]["Height"]["quantity"]["input"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = n.editor.draft.as_ref().unwrap().parameters["name"]
        .as_str()
        .unwrap()
        .to_owned();
    n.text(&seed, "2");
    n.editor.preview();
    n.receive("preview", true);
    assert_eq!(n.object(&created)["properties"]["Height"], 50.8);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(n.editor.draft.is_none());
    n.save("created.FCStd");
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_sketch_dimension_labels_default_input_and_gesture_guard() {
    let mut n = Native::case("dimensions");
    n.fixture("UnitsDimensions");
    n.units(3);
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let constraints = &n.object("Sketch")["sketch"]["constraints"];
    assert_eq!(constraints[0]["quantity"]["display"], "1.00 in");
    assert_eq!(constraints[1]["quantity"]["display"], "0.50 in");
    let output = n.frame(vec![]);
    assert!(
        output.shapes.iter().any(
            |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("1.00 in"))
        )
    );
    n.editor.dimension_at(0);
    n.text("1 in", "2");
    n.click("Apply");
    n.receive("transaction", true);
    assert_eq!(
        n.object("Sketch")["sketch"]["constraints"][0]["value"],
        50.8
    );
    n.editor.dimension_at(1);
    n.text("0.5 in", "1 s");
    n.click("Apply");
    n.receive("transaction", false);
    assert!(n.editor.sketch_ui.dimension.is_some());
    n.text("1 s", "0.75");
    n.click("Apply");
    n.receive("transaction", true);
    assert!(
        (n.object("Sketch")["sketch"]["constraints"][1]["value"]
            .as_f64()
            .unwrap()
            - 19.05)
            .abs()
            < 1e-12
    );
    n.editor.dimension_at(2);
    n.text("0.5 in", "1");
    n.click("Apply");
    n.receive("transaction", true);
    assert_eq!(
        n.object("Sketch")["sketch"]["constraints"][2]["value"],
        25.4
    );
    n.editor.dimension_at(3);
    n.text("45 °", "1 rad");
    n.click("Apply");
    n.receive("transaction", true);
    assert!(
        (n.object("Sketch")["sketch"]["constraints"][3]["value"]
            .as_f64()
            .unwrap()
            - 1.)
            .abs()
            < 1e-12
    );
    let before = n.editor.state.clone();
    n.editor.send("document_settings", json!({"unit_system":0}));
    n.receive("document_settings", false);
    assert_eq!(n.editor.state, before);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}));
    n.save("sketch.FCStd");
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_stale_dialog_and_physical_package_input() {
    let mut n = Native::case("stale-package");
    n.fixture("UnitsStale");
    n.units(3);
    n.click("Units");
    n.click("Document units...");
    // A second caller changes the same native document through the actor.
    n.editor.jobs.try_send(Job{op:"transaction".into(),parameters:json!({"document":n.editor.state["document"],"expected_revision":n.editor.state["revision"],"edits":[{"op":"set","object":"Box","property":"Length","value":30}]})}).unwrap();
    n.receive("transaction", true);
    let revision = n.editor.state["revision"].clone();
    n.click("Apply units");
    assert!(!n.editor.busy);
    assert_eq!(n.editor.state["revision"], revision);
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("Document changed")
    );
    n.click("Cancel units");
    n.editor.sketch = Some("Sketch".into());
    n.editor.start_package("sketch.circle");
    n.receive("quantities", true);
    let seed = n.editor.units_ui.fields["radius"]["input"]
        .as_str()
        .unwrap()
        .to_owned();
    n.text(&seed, "1 s");
    n.editor.preview();
    n.receive("quantities", false);
    assert!(n.editor.draft.is_some());
    n.text("1 s", "2");
    n.editor.preview();
    n.receive("quantities", true);
    n.receive("preview", true);
    assert_eq!(n.object("Sketch")["sketch"]["geometry"][4]["radius"], 50.8);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(n.editor.draft.is_none());
    n.save("package.FCStd");
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_attachment_offset_and_sketch_task_quantity_fields() {
    let mut n = Native::case("creation-tasks");
    n.fixture("UnitsCreation");
    n.units(3);
    n.editor.new_sketch_with_options();
    n.frame(vec![]);
    n.text("0 in", "2");
    n.text("0 °", "90");
    n.click("Create sketch");
    n.receive("transaction", true);
    n.receive("sketch_begin", true);
    let sketch = n.editor.sketch.clone().unwrap();
    assert_eq!(
        n.object(&sketch)["properties"]["AttachmentOffset"]["position"][0],
        50.8
    );
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}));
    n.save("attachment.FCStd");
    n.send("transaction",json!({"edits":[
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Corner"},
        {"op":"geometry","object":"Corner","kind":"line","start":[0,0,0],"end":[25.4,0,0]},
        {"op":"geometry","object":"Corner","kind":"line","start":[25.4,0,0],"end":[25.4,25.4,0]},
        {"op":"constraint","object":"Corner","kind":"Coincident","args":[0,2,1,1]}
    ]}));
    n.editor.request_sketch("Corner");
    n.receive("sketch_begin", true);
    n.editor.begin_sketch_task(
        "fillet",
        Some(vec![crate::sketch_commands::SketchElement {
            index: 0,
            point: 2,
        }]),
    );
    n.receive("quantities", true);
    let seed = n.editor.units_ui.fields["/radius"]["input"]
        .as_str()
        .unwrap()
        .to_owned();
    n.text(&seed, "0.25");
    n.receive("quantities", true);
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    n.receive("preview", true);
    n.click("Apply");
    n.receive("preview_commit", true);
    let arc = n.object("Corner")["sketch"]["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == "arc")
        .unwrap();
    assert!((arc["radius"].as_f64().unwrap() - 6.35).abs() < 1e-12);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}));
    n.save("fillet.FCStd");
}

#[test]
#[ignore = "pinned native runtime; actual Editor and headless egui"]
fn native_units_assembly_placement_text_keeps_quaternion_unitless() {
    let mut n = Native::case("assembly");
    n.fixture("UnitsAssembly");
    n.units(3);
    n.send(
        "transaction",
        json!({"edits":[{"op":"assembly_create","name":"Assembly","joints":"Joints"}]}),
    );
    n.editor.start_assembly_task(json!({"op":"assembly_insert","assembly":"Assembly","source":"Box","name":"Instance","placement":{"position":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}}),"Insert component");
    n.text("0 in", "2");
    n.editor.preview();
    assert!(!n.editor.busy);
    n.click("Apply");
    n.receive("transaction", true);
    assert_eq!(
        n.object("Instance")["properties"]["Placement"]["position"],
        json!([50.8, 0., 0.])
    );
    assert_eq!(
        n.object("Instance")["properties"]["Placement"]["rotation"],
        json!([0., 0., 0., 1.])
    );
    n.save("assembly.FCStd");
}

#[path = "followup_r1.rs"]
mod followup_r1;

#[path = "review_r2.rs"]
mod review_r2;
#[path = "settings_guard_tests.rs"]
mod settings_guard_tests;

#[path = "open_cleanup_tests.rs"]
mod open_cleanup_tests;

#[path = "workspace_tests.rs"]
mod workspace_tests;

#[path = "workspace_review.rs"]
mod workspace_review;
