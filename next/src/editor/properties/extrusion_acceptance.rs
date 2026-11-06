use super::super::*;
use super::*;
use std::{fs, path::PathBuf, thread::JoinHandle};

#[path = "hole_acceptance.rs"]
mod hole_acceptance;

fn acceptance_dir() -> PathBuf {
    PathBuf::from(
        std::env::var_os("CAD_EDITOR_ACCEPTANCE_OUTPUT").expect("set CAD_EDITOR_ACCEPTANCE_OUTPUT"),
    )
}
fn input_file(variable: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| panic!("set {variable}")))
}
fn write_json(name: &str, data: &Value) {
    fs::write(
        acceptance_dir().join(name),
        serde_json::to_vec_pretty(data).unwrap(),
    )
    .unwrap();
}
fn object<'a>(state: &'a Value, name: &str) -> &'a Value {
    objects(state).iter().find(|o| o["name"] == name).unwrap()
}

struct NativeEditor {
    editor: Editor,
    raw: Receiver<Reply>,
    forward: Sender<Reply>,
    actor: Option<JoinHandle<()>>,
    captures: Vec<Value>,
    case: String,
}
impl NativeEditor {
    fn new(case: &str) -> Self {
        let runtime: Value = serde_json::from_str(
            &fs::read_to_string(input_file("CAD_EDITOR_ACCEPTANCE_RUNTIME")).unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            std::env::var_os("CAD_EDITOR_ACCEPTANCE_WORKER")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend/freecad_worker.py")
                }),
        );
        config.args = runtime["args"]
            .as_array()
            .expect("runtime args")
            .iter()
            .map(|arg| arg.as_str().expect("runtime argument").into())
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
            property_ui: PropertyUi::default(),
            transform_ui: super::super::transforms::TransformUi::default(),
            exchange_ui: super::super::exchange::ExchangeUi::default(),
            draft: None,
            measurement_ui: super::super::measurements::MeasurementUi::default(),
            units_ui: Default::default(),
            sequence: 0,
            error: None,
            status: String::new(),
            path: String::new(),
            file_input: String::new(),
            file_dialog: None,
            file_browser: super::super::file_browser::FileBrowser::default(),
            overwrite: false,
            dirty: false,
            pending_file: None,
            allow_close: false,
            fit_next: false,
        };
        Self {
            editor,
            raw,
            forward,
            actor: Some(actor),
            captures: vec![],
            case: case.into(),
        }
    }
    fn receive(&mut self, op: &str, accepted: bool) {
        let reply = self
            .raw
            .recv_timeout(Duration::from_secs(60))
            .expect("native actor reply timeout");
        self.captures.push(
            json!({"op":reply.op,"accepted":reply.accepted,"state":reply.state,
            "error":reply.error,"data":reply.data,"scenes":reply.scenes}),
        );
        write_json(
            &format!("{}-native-responses.json", self.case),
            &json!(self.captures),
        );
        assert_eq!(reply.op, op);
        assert_eq!(
            reply.accepted, accepted,
            "unexpected native result: {:?}",
            reply.error
        );
        self.forward.send(reply).unwrap();
        self.editor.poll();
        assert!(!self.editor.busy);
    }
    fn send(&mut self, op: &str, params: Value) {
        self.editor.send(op, params);
        self.receive(op, true);
        assert!(self.editor.error.is_none(), "{:?}", self.editor.error);
    }
    fn preview(&mut self, accepted: bool) {
        self.editor.preview();
        assert!(
            self.editor.busy,
            "preview did not queue native work: {:?}",
            self.editor.error
        );
        self.receive("preview", accepted);
    }
    fn properties(&mut self, properties: Value, expressions: Value) {
        let d = self.editor.draft.as_mut().unwrap();
        d.parameters["properties"] = properties;
        d.parameters["expressions"] = expressions;
        d.changed = Some(Instant::now());
        d.valid = false;
    }
    fn params(&self) -> &Value {
        &self.editor.draft.as_ref().unwrap().parameters
    }
}
impl Drop for NativeEditor {
    fn drop(&mut self) {
        let (sender, _) = mpsc::sync_channel(1);
        self.editor.jobs = sender;
        if let Some(actor) = self.actor.take() {
            actor.join().unwrap();
        }
    }
}

#[derive(Clone, Debug)]
struct Text {
    text: String,
    rect: egui::Rect,
    color: egui::Color32,
}
fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<Text> {
    fn collect(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<Text>) {
        match shape {
            egui::Shape::Text(t) => {
                let rect = egui::Rect::from_min_size(t.pos, t.galley.size());
                if clip.contains(rect.center()) {
                    out.push(Text {
                        text: t.galley.job.text.clone(),
                        rect,
                        color: t.fallback_color,
                    });
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect(s, clip, out);
                }
            }
            _ => (),
        }
    }
    let mut out = vec![];
    for s in shapes {
        collect(&s.shape, s.clip_rect, &mut out);
    }
    out
}
struct Form {
    ctx: egui::Context,
    panels: bool,
    last: Vec<Text>,
    frames: Vec<Value>,
}
impl Form {
    fn new(panels: bool) -> Self {
        Self {
            ctx: egui::Context::default(),
            panels,
            last: vec![],
            frames: vec![],
        }
    }
    fn frame(&mut self, e: &mut Editor, events: Vec<egui::Event>) {
        let output = self.ctx.run(
            egui::RawInput {
                time: Some(self.frames.len() as f64 / 30.),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 1200.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                if self.panels {
                    e.panels(ctx);
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| e.native_property_fields(ui));
                }
            },
        );
        self.last = texts(&output.shapes);
        self.frames.push(json!(self.last.iter().map(|t| json!({"text":t.text,
            "rect":[t.rect.min.x,t.rect.min.y,t.rect.max.x,t.rect.max.y],"color":t.color.to_array()})).collect::<Vec<_>>()));
    }
    fn find(&self, text: &str) -> egui::Pos2 {
        self.last
            .iter()
            .find(|t| t.text == text)
            .unwrap_or_else(|| panic!("missing {text}; visible {:?}", self.last))
            .rect
            .center()
    }
    fn click_at(&mut self, e: &mut Editor, pos: egui::Pos2) {
        self.frame(
            e,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        self.frame(
            e,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    fn click(&mut self, e: &mut Editor, text: &str) {
        let p = self.find(text);
        self.click_at(e, p);
    }
    fn seek(&mut self, e: &mut Editor, text: &str) {
        self.frame(e, vec![]);
        for _ in 0..12 {
            if self.last.iter().any(|t| t.text == text) {
                return;
            }
            self.frame(
                e,
                vec![
                    egui::Event::PointerMoved(egui::pos2(200., 250.)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -140.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..4 {
                self.frame(e, vec![]);
            }
        }
        panic!("unreachable rendered row {text}; {:?}", self.last);
    }
    fn save(&self, name: &str) {
        write_json(name, &json!(self.frames));
    }
}

fn task(kind: &str, properties: Value, expressions: Value) -> Value {
    json!({"type":format!("PartDesign::{kind}"),"name":"Result","body":"Body","create":true,
        "properties":properties,"expressions":expressions})
}

fn run_kind(kind: &str) {
    let fixtures: Value = serde_json::from_str(
        &fs::read_to_string(input_file("CAD_EDITOR_ACCEPTANCE_FIXTURES")).unwrap(),
    )
    .unwrap();
    let mut h = NativeEditor::new(kind);
    h.send("open", json!({"path":fixtures[kind]["path"]}));
    // Creation schema and selected profile pass through the real async reply path.
    let before_create = h.editor.state.clone();
    h.editor.body_ui.active = Some("Body".into());
    h.editor.selected_object = Some("Profile".into());
    h.editor
        .start_native_feature(&format!("PartDesign::{kind}"));
    assert!(h.editor.property_ui.loading);
    h.receive("feature_schema", true);
    assert!(!h.editor.property_ui.loading);
    assert_eq!(h.params()["properties"]["Profile"]["link"], "Profile");
    assert_eq!(
        h.editor.property_ui.schema["property_types"]["Type"],
        "App::PropertyEnumeration"
    );
    assert!(h.editor.draft.as_ref().unwrap().changed.is_some());
    h.editor.finish_preview(false);
    assert!(h.editor.draft.is_none());
    assert_eq!(h.editor.state, before_create);
    assert!(h.raw.try_recv().is_err());
    let seed = task(kind, json!({"Profile":{"link":"Profile"}}), json!({}));
    let edits = crate::property_commands::build(&seed).unwrap();
    h.editor.transaction("Initial Length", edits);
    h.receive("transaction", true);
    let baseline = h.editor.state.clone();
    let initial = object(&baseline, "Result").clone();
    let front = if kind == "Pad" {
        "Target"
    } else {
        "OtherTarget"
    };
    let back = if kind == "Pad" {
        "OtherTarget"
    } else {
        "Target"
    };
    let face = fixtures[kind]["targets"][front][if kind == "Pad" { "upper" } else { "lower" }]
        .as_str()
        .unwrap();
    let face2 = fixtures[kind]["targets"][back][if kind == "Pad" { "lower" } else { "upper" }]
        .as_str()
        .unwrap();
    let link = json!({"link":front,"subelements":[face]});
    let link2 = json!({"link":back,"subelements":[face2]});

    // Normal mode switch through the actual combo after an incomplete preview.
    h.editor.edit_native_properties("Result");
    h.properties(json!({"Type":"UpToFace"}), json!({}));
    h.preview(false);
    assert_eq!(
        object(&h.editor.state, "Result")["properties"]["Type"],
        "Length"
    );
    let mut form = Form::new(false);
    form.frame(&mut h.editor, vec![]);
    form.frame(&mut h.editor, vec![]);
    assert!(form.last.iter().any(|t| t.text == "UpToFace"));
    // UpToFace is the combo selected text and the row label. The first is the combo.
    form.click(&mut h.editor, "UpToFace");
    form.frame(&mut h.editor, vec![]);
    form.click(&mut h.editor, "Length");
    assert_eq!(h.params()["properties"]["Type"], "Length");
    h.preview(true);
    assert!(h.editor.draft.as_ref().unwrap().valid);
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    assert!(h.editor.draft.is_none());
    assert_eq!(h.editor.state["undo_count"], baseline["undo_count"]);
    form.save(&format!("{kind}-normal-mode-form.json"));

    // Original Type expression 3, a real rejected native response, then an actual
    // rendered selection button using the task's original native references.
    h.editor.edit_native_properties("Result");
    h.properties(json!({}), json!({"Type":"3"}));
    h.preview(false);
    assert!(h.editor.state["preview"].is_null());
    h.editor.viewport.selected = Some(crate::viewport::Selection {
        object: front.into(),
        subelement: face.into(),
    });
    let mut target = Form::new(false);
    target.seek(&mut h.editor, "UpToFace");
    target.seek(&mut h.editor, "Use native selection");
    target.click(&mut h.editor, "Use native selection");
    assert_eq!(h.params()["properties"]["UpToFace"], link);
    assert_eq!(h.params()["expressions"]["Type"], "3");
    assert!(!h.editor.draft.as_ref().unwrap().valid);
    h.preview(true);
    assert_eq!(
        h.editor.property_ui.schema,
        object(&h.editor.state, "Result").clone()
    );
    assert!(h.editor.draft.as_ref().unwrap().valid);
    let accepted = object(&h.editor.state, "Result").clone();
    assert!(accepted["valid"].as_bool().unwrap());
    // Failed replacement must retain the accepted native candidate and schema.
    h.properties(json!({"UpToFace":link}), json!({"Type":"3 +"}));
    h.preview(false);
    assert_eq!(
        object(&h.editor.state, "Result")["properties"],
        accepted["properties"]
    );
    assert_eq!(
        object(&h.editor.state, "Result")["expressions"],
        accepted["expressions"]
    );
    assert_eq!(h.editor.property_ui.schema, accepted);
    assert!(!h.editor.draft.as_ref().unwrap().valid);
    target = Form::new(false);
    target.seek(&mut h.editor, "UpToFace");
    h.properties(json!({"UpToFace":link}), json!({"Type":"3"}));
    h.preview(true);
    let mut buttons = Form::new(true);
    for _ in 0..12 {
        buttons.frame(&mut h.editor, vec![]);
    }
    buttons.click(&mut h.editor, "Apply");
    assert!(h.editor.busy);
    // A second direct finish attempt cannot enqueue another job while busy.
    h.editor.finish_preview(true);
    h.receive("preview_commit", true);
    assert!(h.raw.try_recv().is_err());
    assert!(h.editor.draft.is_none());
    assert_eq!(
        h.editor.state["undo_count"].as_u64().unwrap(),
        baseline["undo_count"].as_u64().unwrap() + 1
    );
    target.save(&format!("{kind}-expression-target-form.json"));
    buttons.save(&format!("{kind}-apply-form.json"));

    // Opposite and exact path clears through the actual Expression paths UI.
    h.editor.edit_native_properties("Result");
    h.editor.property_ui.filter = "__only_expression_paths__".into();
    h.editor
        .property_ui
        .buffers
        .insert("@path".into(), ".Type".into());
    let mut clear = Form::new(false);
    clear.frame(&mut h.editor, vec![]);
    clear.click(&mut h.editor, "Expression paths");
    for _ in 0..5 {
        clear.frame(&mut h.editor, vec![]);
    }
    clear.click(&mut h.editor, "Clear expression");
    assert_eq!(h.params()["expressions"], json!({".Type":null}));
    h.preview(true);
    assert_eq!(
        object(&h.editor.state, "Result")["expressions"]["Type"],
        "3"
    );
    h.editor
        .property_ui
        .buffers
        .insert("@path".into(), "Type".into());
    clear.frame(&mut h.editor, vec![]);
    clear.click(&mut h.editor, "Clear expression");
    assert_eq!(h.params()["expressions"], json!({".Type":null,"Type":null}));
    h.editor.draft.as_mut().unwrap().parameters["properties"]["Type"] = json!("Length");
    h.preview(true);
    assert!(
        object(&h.editor.state, "Result")["expressions"]
            .get("Type")
            .is_none()
    );
    assert_eq!(
        object(&h.editor.state, "Result")["properties"]["Type"],
        "Length"
    );
    h.editor.finish_preview(true);
    h.receive("preview_commit", true);
    clear.save(&format!("{kind}-clear-expression-form.json"));

    // Dormant second-side values and component expressions through actual sync.
    h.editor.edit_native_properties("Result");
    h.properties(
        json!({"SideType":"One side","Type2":"Length","Length2":"17 mm", "Offset2":"2 mm",
        "UpToShape2":[{"link":format!("{back}Surface")}]}),
        json!({".Type2":"3",".Length2":"Length + 4 mm","Offset2":"2 mm"}),
    );
    h.preview(true);
    assert_eq!(
        h.params()["properties"]["Length2"],
        object(&h.editor.state, "Result")["properties"]["Length2"]
    );
    for field in ["Length2", "Offset2", "Type2", "UpToShape2"] {
        assert!(
            h.params()["properties"].get(field).is_some(),
            "sync dropped dormant {field}"
        );
    }
    let dormant = h.params().clone();
    let native_dormant = object(&h.editor.state, "Result").clone();
    h.editor.finish_preview(true);
    h.receive("preview_commit", true);
    h.editor.edit_native_properties("Result");
    h.properties(json!({"SideType":"Two sides"}), json!({}));
    h.preview(false);
    let mut second = Form::new(false);
    second.seek(&mut h.editor, "Type2");
    second.seek(&mut h.editor, "UpToFace2");
    h.editor.viewport.selected = Some(crate::viewport::Selection {
        object: back.into(),
        subelement: face2.into(),
    });
    // Focus the existing property search to isolate the second target button.
    h.editor.property_ui.filter = "UpToFace2".into();
    let mut second_select = Form::new(false);
    second_select.frame(&mut h.editor, vec![]);
    second_select.click(&mut h.editor, "Use native selection");
    assert_eq!(h.params()["properties"]["UpToFace2"], link2);
    h.preview(true);
    for (path, expr) in dormant["expressions"].as_object().unwrap() {
        assert_eq!(
            &object(&h.editor.state, "Result")["expressions"][path],
            expr
        );
    }
    for field in ["Length2", "Offset2", "UpToShape2"] {
        assert_eq!(
            object(&h.editor.state, "Result")["properties"][field],
            native_dormant["properties"][field]
        );
    }
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    assert_eq!(
        object(&h.editor.state, "Result")["properties"],
        native_dormant["properties"]
    );
    second.save(&format!("{kind}-second-side-form.json"));

    // Native direction errors and accepted schema sync, then Cancel.
    h.editor.edit_native_properties("Result");
    h.properties(
        json!({"Direction":[1,0,0]}),
        json!({"UseCustomVector":"1",".Direction.x":"0.2"}),
    );
    h.preview(false);
    let mut direction = Form::new(false);
    direction.seek(&mut h.editor, "Direction");
    h.editor.draft.as_mut().unwrap().parameters["properties"]["Direction"] = json!([0.2, 0, 1]);
    h.preview(true);
    assert_eq!(h.params()["expressions"][".Direction.x"], "0.2");
    assert_eq!(
        h.editor.property_ui.schema["expressions"][".Direction.x"],
        "0.2"
    );
    h.editor.finish_preview(false);
    h.receive("preview_cancel", true);
    direction.save(&format!("{kind}-direction-form.json"));

    // Redo deferral compiles without native mutation. Cancel keeps history.
    h.send("undo", json!({}));
    let undone = h.editor.state.clone();
    assert!(undone["redo_count"].as_u64().unwrap() > 0);
    h.editor.edit_native_properties("Result");
    assert!(h.editor.draft.as_ref().unwrap().deferred);
    h.properties(json!({"Length":"9 mm"}), json!({}));
    h.editor.preview();
    assert!(!h.editor.busy);
    assert!(h.editor.draft.as_ref().unwrap().valid);
    assert!(h.raw.try_recv().is_err());
    let mut cancel = Form::new(true);
    for _ in 0..12 {
        cancel.frame(&mut h.editor, vec![]);
    }
    cancel.click(&mut h.editor, "Cancel");
    assert!(h.editor.draft.is_none());
    assert!(h.raw.try_recv().is_err());
    assert_eq!(h.editor.state, undone);
    h.send("redo", json!({}));
    h.send("undo", json!({}));
    let undo_count = h.editor.state["undo_count"].as_u64().unwrap();
    h.editor.edit_native_properties("Result");
    h.properties(json!({"Length":"9 mm"}), json!({}));
    h.editor.preview();
    let mut deferred_apply = Form::new(true);
    for _ in 0..12 {
        deferred_apply.frame(&mut h.editor, vec![]);
    }
    deferred_apply.click(&mut h.editor, "Apply");
    assert!(h.editor.busy);
    assert!(h.editor.draft.as_ref().unwrap().committing);
    h.receive("transaction", true);
    assert!(h.editor.draft.is_none());
    assert_eq!(h.editor.state["undo_count"], undo_count + 1);
    assert_eq!(h.editor.state["redo_count"], 0);
    h.send("undo", json!({}));
    assert_eq!(
        object(&h.editor.state, "Result")["properties"]["Length"],
        object(&undone, "Result")["properties"]["Length"]
    );
    h.send("redo", json!({}));
    cancel.save(&format!("{kind}-deferred-cancel-form.json"));
    deferred_apply.save(&format!("{kind}-deferred-apply-form.json"));
    write_json(
        &format!("{kind}-result.json"),
        &json!({"passed":true,"baseline":initial,"native_responses":h.captures.len(),
        "methods":["start_native_feature","native_schema_reply","edit_native_properties","native_property_fields","native_link_selection","preview","feature_edits","send","model_thread","poll","sync_native_schema","panels","finish_preview"],
        "cases":["creation-schema-reply-profile-default-cancel","normal-mode-switch-after-rejection","Type-expression-3-target-selection-after-rejection","failed-replacement-retains-candidate",
            "one-Apply-one-native-history-entry","exact-and-opposite-expression-path-clears","dormant-sync-retention","second-side-target-after-rejection",
            "custom-direction-rejection-correction-sync","deferred-Redo-Cancel","deferred-Redo-Apply-Undo-Redo"]}),
    );
    // Drop closes the actor and worker. The desktop never sends a standalone
    // close op through model_thread, which still expects a displayable state.
}

#[test]
#[ignore = "requires pinned native runtime and generated public extrusion fixtures"]
fn extrusion_editor_pad() {
    run_kind("Pad");
}
#[test]
#[ignore = "requires pinned native runtime and generated public extrusion fixtures"]
fn extrusion_editor_pocket() {
    run_kind("Pocket");
}

#[path = "reference_selection_native.rs"]
mod reference_selection;

#[path = "suppression_editor.rs"]
mod suppression;
