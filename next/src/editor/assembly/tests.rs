//! Real Editor methods and egui Apply/Cancel, with the pinned headless worker.
#[path = "external_tests.rs"]
mod external_tests;
#[path = "partial_tests.rs"]
mod partial_tests;
#[path = "r1_tests.rs"]
mod r1_tests;
use super::super::*;
use super::*;
use std::{fs, thread::JoinHandle};

struct Native {
    editor: Editor,
    ctx: egui::Context,
    actor: Option<JoinHandle<()>>,
    output: PathBuf,
    events: Vec<Value>,
    time: f64,
}

impl Native {
    fn new() -> Self {
        let runtime: Value = serde_json::from_slice(
            &fs::read(std::env::var_os("CAD_ASSEMBLY_RUNTIME").unwrap()).unwrap(),
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
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, replies) = mpsc::channel();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
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
            measurement_ui: measurements::MeasurementUi::default(),
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
        let output = PathBuf::from(std::env::var_os("CAD_ASSEMBLY_OUTPUT").unwrap());
        fs::create_dir(&output).unwrap();
        Self {
            editor,
            ctx,
            actor: Some(actor),
            output,
            events: vec![],
            time: 0.,
        }
    }

    fn receive(&mut self, accepted: bool) {
        let reply = self
            .editor
            .replies
            .recv_timeout(Duration::from_secs(25))
            .unwrap();
        self.events.push(json!({"op":reply.op,"accepted":reply.accepted,"error":reply.error,"state":reply.state,"data":reply.data,
            "display_objects":reply.scenes.iter().map(|(m,_)|m["object"].clone()).collect::<Vec<_>>()}));
        fs::write(
            self.output.join("events.json"),
            serde_json::to_vec_pretty(&self.events).unwrap(),
        )
        .unwrap();
        assert_eq!(reply.accepted, accepted, "{:?}", reply.error);
        let (forward, replies) = mpsc::channel();
        let old = std::mem::replace(&mut self.editor.replies, replies);
        forward.send(reply).unwrap();
        self.editor.poll();
        self.editor.replies = old;
    }

    fn send(&mut self, op: &str, parameters: Value) {
        self.editor.send(op, parameters);
        assert!(self.editor.busy);
        self.receive(true);
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.1;
        let mut frame = eframe::Frame::_new_kittest();
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500., 1000.),
                )),
                time: Some(self.time),
                modifiers: events
                    .iter()
                    .rev()
                    .find_map(|event| {
                        if let egui::Event::Key { modifiers, .. } = event {
                            Some(*modifiers)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut self.editor, ctx, &mut frame),
        )
    }

    fn click(&mut self, text: &str) {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        let area = self
            .ctx
            .memory(|m| m.area_rect(egui::Id::new("Feature")))
            .unwrap();
        let texts: Vec<_> = frame
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(t) => Some((
                    t.galley.text().to_owned(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        let matches: Vec<_> = texts
            .iter()
            .filter(|(t, r)| t == text && area.contains(r.center()))
            .collect();
        assert_eq!(matches.len(), 1, "one task button {text}: {texts:?}");
        let pos = matches[0].1.center();
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

    fn task(&mut self, request: Value, accepted: bool) {
        self.editor.start_assembly_task(request, "Assembly task");
        self.editor.preview();
        assert!(self.editor.draft.as_ref().unwrap().valid);
        self.click("Apply");
        assert!(self.editor.busy);
        self.receive(accepted);
        assert_eq!(self.editor.draft.is_none(), accepted);
    }
    fn obj(&self, name: &str) -> &Value {
        objects(&self.editor.state)
            .iter()
            .find(|o| o["name"] == name)
            .unwrap()
    }
}

impl Native {
    fn click_pos(&mut self, pos: egui::Pos2) {
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

    fn text_positions(&mut self, text: &str) -> Vec<egui::Pos2> {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        let area = self
            .ctx
            .memory(|memory| memory.area_rect(egui::Id::new("Feature")))
            .unwrap();
        frame
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    let center = t.galley.rect.translate(t.pos.to_vec2()).center();
                    (shape.clip_rect.contains(center) && area.contains(center)).then_some(center)
                }
                _ => None,
            })
            .collect()
    }

    fn reveal(&mut self, text: &str) {
        if !self.text_positions(text).is_empty() {
            return;
        }
        for delta in [180., -180.] {
            for _ in 0..12 {
                let area = self
                    .ctx
                    .memory(|memory| memory.area_rect(egui::Id::new("Feature")))
                    .unwrap();
                self.frame(vec![
                    egui::Event::PointerMoved(area.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., delta),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                if !self.text_positions(text).is_empty() {
                    return;
                }
            }
        }
        panic!("Missing visible assembly field {text}");
    }

    fn click_text(&mut self, text: &str) {
        self.reveal(text);
        let pos = self.text_positions(text)[0];
        self.click_pos(pos);
    }

    fn replace_at(&mut self, pos: egui::Pos2, value: &str) {
        self.click_pos(pos);
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.frame(if value.is_empty() {
            vec![egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        } else {
            vec![egui::Event::Text(value.into())]
        });
    }

    fn expression_input(&mut self, value: &str) {
        self.reveal("Expression");
        let pos = self.text_positions("Expression")[0] + egui::vec2(100., 0.);
        self.replace_at(pos, value);
    }

    fn literal_input(&mut self, property: &str, previous: f64, value: &str) {
        let metadata = self.obj("Joint")["property_metadata"][property]["quantity"].clone();
        assert_eq!(metadata["value"], previous);
        assert_eq!(self.request()["properties"][property], previous);
        let seed = metadata["input"].as_str().unwrap();
        self.reveal(seed);
        let labels = self.text_positions(property);
        assert_eq!(labels.len(), 1, "one visible property {property}");
        let positions: Vec<_> = self
            .text_positions(seed)
            .into_iter()
            .filter(|pos| pos.y > labels[0].y && pos.y < labels[0].y + 50.)
            .collect();
        assert_eq!(
            positions.len(),
            1,
            "one quantity input for {property}: {seed}"
        );
        self.replace_at(positions[0], value);
        assert_eq!(
            self.request()["properties"][property],
            crate::document_units::input(value, &metadata)
        );
    }

    fn edit_joint_values(&mut self) {
        assert!(self.editor.edit_assembly_object("Joint"));
        self.frame(vec![]);
        if self.text_positions("Distance").is_empty() {
            self.click_text("Joint values, offsets and limits");
        }
        self.reveal("Distance");
    }

    fn apply_joint(&mut self, accepted: bool) {
        let request = self.request();
        let offsets: Vec<_> = ["Offset1", "Offset2"]
            .into_iter()
            .filter(|property| {
                request["properties"].get(*property).is_none()
                    && !request["expressions"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .any(|path| path.trim_start_matches('.').starts_with(property))
            })
            .map(|property| (property, self.obj("Joint")["properties"][property].clone()))
            .collect();
        self.editor.preview();
        assert!(self.editor.draft.as_ref().unwrap().valid);
        self.click("Apply");
        assert!(self.editor.busy);
        self.receive(accepted);
        assert_eq!(self.editor.draft.is_none(), accepted);
        for (property, before) in offsets {
            assert_eq!(self.obj("Joint")["properties"][property], before);
        }
    }

    fn request(&self) -> Value {
        self.editor.draft.as_ref().unwrap().parameters["request"].clone()
    }

    fn assert_distance(&self, value: f64, expression: Option<&str>) {
        assert!(
            (self.obj("Joint")["properties"]["Distance"]
                .as_f64()
                .unwrap()
                - value)
                .abs()
                < 1e-8
        );
        assert_eq!(
            self.obj("Joint")["expressions"]["Distance"],
            json!(expression)
        );
        assert_eq!(self.obj("Joint")["properties"]["JointType"], "Gears");
    }

    fn record_case(&self, name: &str, request: Value) {
        fs::write(self.output.join(format!("{name}.json")),serde_json::to_vec_pretty(&json!({
            "request":request,"native_joint":self.obj("Joint"),"revision":self.editor.state["revision"],
            "undo_count":self.editor.state["undo_count"],"redo_count":self.editor.state["redo_count"],
        })).unwrap()).unwrap();
    }

    fn cancel_without_write(&mut self) {
        let baseline = self.editor.state.clone();
        let events = self.events.len();
        self.click("Cancel");
        assert!(self.editor.draft.is_none());
        assert!(!self.editor.busy);
        assert_eq!(self.editor.state, baseline);
        assert_eq!(self.events.len(), events);
    }
}

#[test]
#[ignore = "requires CAD_ASSEMBLY_RUNTIME and fresh CAD_ASSEMBLY_OUTPUT; actual Editor/native override regression"]
fn native_joint_override_opt_out() {
    let mut n = Native::new();
    n.send("new", json!({"name":"OverrideEditor"}));
    n.send(
        "transaction",
        json!({"label":"Native source","edits":[
            {"op":"add_object","type":"Part::Box","name":"Source"},
            {"op":"set","object":"Source","property":"Length","value":15},
            {"op":"set","object":"Source","property":"Width","value":15},
            {"op":"set","object":"Source","property":"Height","value":15}
        ]}),
    );
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    for (name, x) in [("A", 0), ("B", 80)] {
        n.task(
            json!({"op":"assembly_insert","assembly":"Assembly","name":name,"source":"Source",
            "placement":{"position":[x,0,0],"rotation":[0,0,0,1]}}),
            true,
        );
    }
    n.task(json!({"op":"assembly_joint","assembly":"Assembly","name":"Joint","create":true,"joint_type":"Gears",
        "reference1":{"link":"A","subelements":["Face2","Face2"]},"reference2":{"link":"B","subelements":["Face3","Face3"]},
        "properties":{"Distance":"8 mm","Distance2":"12 mm",
        "Offset2":{"position":[1.25,-2.5,3.75],"rotation":[0,0,0.6,0.8]}},"expressions":{}}),true);
    n.assert_distance(8., None);

    // Repainting and applying untouched unit fields retain native values exactly.
    let properties = n.obj("Joint")["properties"].clone();
    let expressions = n.obj("Joint")["expressions"].clone();
    n.edit_joint_values();
    assert_eq!(n.request()["properties"], json!({}));
    assert_eq!(n.request()["expressions"], json!({}));
    let no_op = n.request();
    n.apply_joint(true);
    assert_eq!(n.obj("Joint")["properties"], properties);
    assert_eq!(n.obj("Joint")["expressions"], expressions);
    n.record_case("00-no-op-native-preservation", no_op);

    // Exact reviewer failure: type an expression, uncheck its property and Apply.
    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.Length");
    assert_eq!(n.request()["expressions"]["Distance"], "Source.Length");
    n.click_text("Distance");
    let unchecked = n.request();
    assert!(unchecked["properties"].get("Distance").is_none());
    n.apply_joint(true);
    n.record_case("01-unchecked-expression", unchecked.clone());
    n.assert_distance(8., None);
    assert!(unchecked["expressions"].get("Distance").is_none());

    // Enabled expressions still reach the native engine, including history.
    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.Length");
    n.apply_joint(true);
    n.assert_distance(15., Some("Source.Length"));
    n.send("undo", json!({}));
    n.assert_distance(8., None);
    n.send("redo", json!({}));
    n.assert_distance(15., Some("Source.Length"));

    // Opting out of a replacement formula retains the accepted formula. An
    // unrelated radius edit makes the Apply/Undo boundary observable natively.
    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.Width + 3 mm");
    n.click_text("Distance");
    n.click_text("Distance2");
    n.literal_input("Distance2", 12., "14 mm");
    let replaced_off = n.request();
    assert!(replaced_off["properties"].get("Distance").is_none());
    assert!(replaced_off["expressions"].get("Distance").is_none());
    n.apply_joint(true);
    n.record_case("02-unchecked-formula-update", replaced_off);
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 14.);
    n.send("undo", json!({}));
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 12.);
    n.send("redo", json!({}));
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 14.);
    n.send("undo", json!({}));

    // A pending expression clear and literal must both disappear on opt-out.
    n.edit_joint_values();
    n.click_text("Distance");
    n.literal_input("Distance", 15., "7.5 mm");
    n.expression_input("");
    assert_eq!(
        n.request()["properties"]["Distance"],
        json!({"quantity_input":"7.5 mm","default_unit":n.obj("Joint")["property_metadata"]["Distance"]["quantity"]["default_unit"]})
    );
    assert!(n.request()["expressions"]["Distance"].is_null());
    n.click_text("Distance");
    let unchecked = n.request();
    assert!(unchecked["properties"].get("Distance").is_none());
    assert!(unchecked["expressions"].get("Distance").is_none());
    n.click_text("Distance");
    assert_eq!(n.request()["properties"]["Distance"], 15.);
    assert!(n.request()["expressions"].get("Distance").is_none());
    assert!(!n.text_positions("Source.Length").is_empty());
    n.click_text("Distance");
    n.click_text("Distance2");
    n.literal_input("Distance2", 12., "13 mm");
    let clear_off = n.request();
    n.apply_joint(true);
    n.record_case("03-unchecked-clear-recheck", clear_off);
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 13.);
    n.send("undo", json!({}));
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 12.);
    n.send("redo", json!({}));
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(n.obj("Joint")["properties"]["Distance2"], 13.);
    n.send("undo", json!({}));

    // Explicit clear while checked switches the expression to a native quantity.
    n.edit_joint_values();
    n.click_text("Distance");
    n.literal_input("Distance", 15., "7.5 mm");
    n.expression_input("");
    let clear = n.request();
    n.apply_joint(true);
    n.record_case("03-enabled-clear-literal", clear);
    n.assert_distance(7.5, None);
    n.send("undo", json!({}));
    n.assert_distance(15., Some("Source.Length"));
    n.send("redo", json!({}));
    n.assert_distance(7.5, None);

    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.Width");
    n.apply_joint(true);
    n.assert_distance(15., Some("Source.Width"));
    n.edit_joint_values();
    n.click_text("Distance");
    n.literal_input("Distance", 15., "8 mm");
    n.expression_input("");
    n.apply_joint(true);
    n.assert_distance(8., None);

    // A literal-only override also disappears without changing the native value.
    n.edit_joint_values();
    n.click_text("Distance");
    n.literal_input("Distance", 8., "9.25 mm");
    n.click_text("Distance");
    let literal_off = n.request();
    n.apply_joint(true);
    n.assert_distance(8., None);
    n.record_case("04-unchecked-literal", literal_off);

    // Native failure retains the entered request; correction and retry remain usable.
    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.MissingDimension");
    n.apply_joint(false);
    n.assert_distance(8., None);
    assert_eq!(
        n.request()["expressions"]["Distance"],
        "Source.MissingDimension"
    );
    n.expression_input("Source.Length");
    let retried = n.request();
    n.apply_joint(true);
    n.assert_distance(15., Some("Source.Length"));
    n.record_case("05-failed-apply-retry", retried);
    n.send("undo", json!({}));
    n.assert_distance(8., None);
    n.send("redo", json!({}));
    n.assert_distance(15., Some("Source.Length"));

    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.MissingAgain");
    n.apply_joint(false);
    n.click_text("Distance");
    assert!(n.request()["expressions"].get("Distance").is_none());
    n.cancel_without_write();
    n.assert_distance(15., Some("Source.Length"));
    n.send("undo", json!({}));
    let redo_before = n.editor.state["redo_count"].clone();
    n.edit_joint_values();
    n.click_text("Distance");
    n.expression_input("Source.Width");
    n.click_text("Distance");
    n.cancel_without_write();
    assert_eq!(n.editor.state["redo_count"], redo_before);
    n.send("redo", json!({}));
    n.assert_distance(15., Some("Source.Length"));

    // Offset component expressions are currently native/API fields. Inject the
    // pending paths into the task, then exercise its actual checkbox and Apply.
    let mut request = json!({"op":"assembly_joint","assembly":"Assembly","name":"Joint","create":false,"joint_type":"Gears",
        "reference1":n.obj("Joint")["properties"]["Reference1"],"reference2":n.obj("Joint")["properties"]["Reference2"],
        "properties":{},"expressions":{"Offset1.Base.x":"Source.Height / 3"}});
    let edits = assembly_commands::build(&n.editor.state, &request).unwrap();
    n.send(
        "transaction",
        json!({"label":"Native offset expression only","edits":edits}),
    );
    assert_eq!(
        n.obj("Joint")["expressions"][".Offset1.Base.x"],
        "Source.Height / 3"
    );
    assert!(
        (n.obj("Joint")["properties"]["Offset1"]["position"][0]
            .as_f64()
            .unwrap()
            - 5.)
            .abs()
            < 1e-8
    );
    n.edit_joint_values();
    n.click_text("Offset1");
    {
        let pending = &mut n.editor.draft.as_mut().unwrap().parameters["request"];
        pending["properties"]["Offset1"]["position"][0] = json!(25.);
        pending["expressions"]["Offset1.Base.x"] = json!("Source.Width");
        pending["expressions"]["Offset1.Base.y"] = Value::Null;
        pending["expressions"][".Offset1.Base.z"] = Value::Null;
        pending["expressions"]["Offset1.Rotation.Angle"] = json!("10 deg");
        pending["expressions"]["Distance2"] = json!("Source.Width");
    }
    n.click_text("Offset1");
    let offset_off = n.request();
    assert!(offset_off["properties"].get("Offset1").is_none());
    assert_eq!(
        offset_off["expressions"],
        json!({"Distance2":"Source.Width"})
    );
    n.apply_joint(true);
    n.record_case("06-unchecked-offset-paths", offset_off);
    assert_eq!(
        n.obj("Joint")["expressions"][".Offset1.Base.x"],
        "Source.Height / 3"
    );
    assert_eq!(n.obj("Joint")["expressions"]["Distance2"], "Source.Width");
    n.send("undo", json!({}));
    assert_eq!(
        n.obj("Joint")["expressions"][".Offset1.Base.x"],
        "Source.Height / 3"
    );
    assert!(n.obj("Joint")["expressions"]["Distance2"].is_null());
    n.send("redo", json!({}));

    n.edit_joint_values();
    n.click_text("Offset1");
    assert!(
        (n.request()["properties"]["Offset1"]["position"][0]
            .as_f64()
            .unwrap()
            - 5.)
            .abs()
            < 1e-8
    );
    n.cancel_without_write();
    // The public compiler intentionally keeps expression-only updates and clears.
    request["expressions"] = json!({"Offset1.Base.x":null});
    let edits = assembly_commands::build(&n.editor.state, &request).unwrap();
    n.send(
        "transaction",
        json!({"label":"Explicit offset expression clear","edits":edits}),
    );
    assert!(n.obj("Joint")["expressions"][".Offset1.Base.x"].is_null());
    n.send("undo", json!({}));
    assert_eq!(
        n.obj("Joint")["expressions"][".Offset1.Base.x"],
        "Source.Height / 3"
    );
    n.send(
        "transaction",
        json!({"label":"Native expression remains live","edits":[
        {"op":"set","object":"Source","property":"Height","value":18}]}),
    );
    assert!(
        (n.obj("Joint")["properties"]["Offset1"]["position"][0]
            .as_f64()
            .unwrap()
            - 6.)
            .abs()
            < 1e-8
    );
    let saved = n.output.join("JointOverride.FCStd");
    n.editor.file_input = saved.to_string_lossy().into_owned();
    n.send("save", json!({"path":saved}));
    n.send("close", json!({}));
    n.send("open", json!({"path":saved}));
    n.assert_distance(15., Some("Source.Length"));
    assert_eq!(
        n.obj("Joint")["expressions"][".Offset1.Base.x"],
        "Source.Height / 3"
    );
    fs::write(n.output.join("override-result.json"),serde_json::to_vec_pretty(&json!({
        "passed":true,"actual_editor":true,"native_quantities":true,"desktop_gui":false,
        "nonzero_native_solver_rollback_verified":false,
        "joint_type":"Gears","checked_cases":["expression opt-out","literal opt-out","pending clear opt-out",
        "literal/expression switching","recheck","failed Apply/retry","failed Apply/Cancel","Cancel with Redo",
        "offset component expression/clear opt-out","expression-only public API","native parent update","save/reopen"]
    })).unwrap()).unwrap();
}

impl Drop for Native {
    fn drop(&mut self) {
        let (empty, _) = mpsc::sync_channel(1);
        let old = std::mem::replace(&mut self.editor.jobs, empty);
        drop(old);
        if let Some(actor) = self.actor.take() {
            let _ = actor.join();
        }
    }
}

#[test]
#[ignore = "requires CAD_ASSEMBLY_RUNTIME and a fresh CAD_ASSEMBLY_OUTPUT; no desktop window"]
fn native_assembly_authoring() {
    let mut n = Native::new();
    n.send("new", json!({"name":"EditorAssembly"}));
    n.send("transaction",json!({"label":"Plate and pin","edits":[
        {"op":"add_object","type":"Part::Box","name":"Plate"},{"op":"set","object":"Plate","property":"Length","value":40},
        {"op":"add_object","type":"Part::Cylinder","name":"Pin"},{"op":"set","object":"Pin","property":"Radius","value":2}
    ]}));
    n.task(
        json!({"op":"assembly_create","name":"Assembly","joints":"Joints"}),
        true,
    );
    for (name, source, x) in [
        ("PlateInstance", "Plate", 0),
        ("PinInstance", "Pin", 30),
        ("RepeatedPin", "Pin", 60),
    ] {
        n.task(
            json!({"op":"assembly_insert","assembly":"Assembly","name":name,"source":source,
            "placement":{"position":[x,0,0],"rotation":[0,0,0,1]}}),
            true,
        );
    }
    assert!(
        n.events.last().unwrap()["display_objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o == "PinInstance")
    );
    n.task(json!({"op":"assembly_ground","assembly":"Assembly","object":"PlateInstance","name":"Ground","grounded":true}),true);
    let joint = json!({"op":"assembly_joint","assembly":"Assembly","name":"Joint","create":true,"joint_type":"Fixed",
        "reference1":{"link":"PlateInstance","subelements":["Face6","Face6"]},
        "reference2":{"link":"PinInstance","subelements":["Face3","Face3"]},"properties":{},"expressions":{}});
    n.task(joint.clone(), true);
    let properties = n.obj("Joint")["properties"].clone();
    assert!(n.editor.edit_assembly_object("Joint"));
    n.editor.preview();
    n.click("Cancel");
    assert!(n.editor.draft.is_none());
    assert_eq!(n.obj("Joint")["properties"], properties);
    assert!(!n.editor.busy);
    n.send("undo", json!({}));
    let before = n.editor.state.clone();
    n.editor
        .start_assembly_task(joint.clone(), "Canceled assembly task");
    n.editor.preview();
    n.click("Cancel");
    assert!(n.editor.draft.is_none());
    assert_eq!(n.editor.state, before);
    assert!(!n.editor.busy);
    n.send("redo", json!({}));
    let mut invalid = joint.clone();
    invalid["create"] = json!(false);
    invalid["reference1"]["subelements"] = json!(["Face999", "Face999"]);
    n.task(invalid, false);
    assert_eq!(n.obj("Joint")["properties"], properties);
    n.editor.draft.as_mut().unwrap().parameters["request"] = json!({"op":"assembly_joint","assembly":"Assembly","name":"Joint","create":false,
        "joint_type":"Revolute","reference1":properties["Reference1"],"reference2":properties["Reference2"],"properties":{},"expressions":{}});
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    assert!(n.editor.draft.is_none());
    assert_eq!(n.obj("Joint")["properties"]["JointType"], "Revolute");
    assert!(n.editor.edit_assembly_object("RepeatedPin"));
    n.editor.draft.as_mut().unwrap().parameters["request"]["placement"]["position"] =
        json!([60, 20, 0]);
    n.editor.preview();
    n.click("Apply");
    n.receive(true);
    assert!(n.editor.draft.is_none());
    let refs = n.obj("Joint")["properties"]["Reference1"].clone();
    n.send("transaction",json!({"label":"Update native source","edits":[{"op":"set","object":"Plate","property":"Height","value":20}]}));
    assert_eq!(n.obj("Joint")["properties"]["Reference1"], refs);
    let saved = n.output.join("EditorAssembly.FCStd");
    n.editor.file_input = saved.to_string_lossy().into_owned();
    n.send("save", json!({"path":saved}));
    n.send("close", json!({}));
    n.send("open", json!({"path":saved}));
    assert!(n.editor.edit_assembly_object("Joint"));
    n.editor.preview();
    n.click("Cancel");
    assert!(n.editor.draft.is_none());
    n.task(
        json!({"op":"assembly_remove","assembly":"Assembly","object":"Joint"}),
        true,
    );
    n.task(
        json!({"op":"assembly_remove","assembly":"Assembly","object":"PinInstance"}),
        true,
    );
    n.send("undo", json!({}));
    assert_eq!(n.obj("PinInstance")["type"], "App::Link");
    n.send("redo", json!({}));
    fs::write(
        n.output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"actual_editor":true,"headless_egui":true,"desktop_gui":false}),
        )
        .unwrap(),
    )
    .unwrap();
}
