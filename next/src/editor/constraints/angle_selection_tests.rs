//! Actual editor methods with captured public-host native snapshots. Only the
//! preview wire token is rebound to this test process's emitted owner token.
use super::super::*;
use super::*;

struct Harness {
    editor: Editor,
    input: mpsc::Receiver<Job>,
    output: mpsc::Sender<Reply>,
    owner: Option<String>,
}

fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/angle_selection_states.json"
    ))
    .unwrap()
}

impl Harness {
    fn new(state: &Value) -> Self {
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, replies) = mpsc::channel();
        let mut editor = Editor {
            jobs,
            replies,
            busy: false,
            registry: Registry::builtin().unwrap(),
            state: state.clone(),
            viewport: Viewport::default(),
            sketch: Some("Sketch".into()),
            selected_object: None,
            sketch_ui: SketchUi::default(),
            sketch_creation: None,
            body_ui: BodyUi::default(),
            property_ui: properties::PropertyUi::default(),
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
        editor.sketch_ui.session = state["sketch_edit"]["token"].as_str().map(str::to_owned);
        editor.sync_sketch();
        Self {
            editor,
            input,
            output,
            owner: None,
        }
    }

    fn request(&mut self, op: &str) -> Job {
        let job = self.input.try_recv().unwrap();
        assert_eq!(job.op, op);
        assert_eq!(
            job.parameters["expected_revision"],
            self.editor.state["revision"]
        );
        assert_eq!(job.parameters["document"], self.editor.state["document"]);
        assert_eq!(
            job.parameters["sketch_token"],
            json!(self.editor.sketch_ui.session)
        );
        if op == "preview" && self.owner.is_none() {
            self.owner = job.parameters["token"].as_str().map(str::to_owned);
        }
        if matches!(op, "preview" | "preview_cancel" | "preview_commit") {
            assert_eq!(job.parameters["token"].as_str(), self.owner.as_deref());
        }
        job
    }

    fn reply(&mut self, op: &str, state: &Value, accepted: bool) {
        let mut state = state.clone();
        if !state["preview"].is_null() {
            state["preview"]["token"] = json!(self.owner);
        }
        self.output
            .send(Reply {
                op: op.into(),
                accepted,
                state,
                scenes: vec![],
                error: (!accepted).then(|| "Native edit rejected".into()),
                elapsed: Duration::ZERO,
                data: Value::Null,
            })
            .unwrap();
        self.editor.poll();
    }

    fn select(&mut self, index: usize) {
        let snapshot = self.editor.sketch_snapshot().unwrap();
        self.editor.highlight_constraint(&snapshot, index);
    }

    fn begin(&mut self) {
        self.editor.begin_dimension(
            "Angle",
            vec![
                SketchElement { index: 0, point: 0 },
                SketchElement { index: 1, point: 0 },
                SketchElement { index: 2, point: 1 },
            ],
        );
    }

    fn dialog_frame(&mut self, context: &egui::Context, key: Option<egui::Key>) {
        let snapshot = self.editor.sketch_snapshot().unwrap();
        let events = key
            .into_iter()
            .map(|key| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect();
        let _ = context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| self.editor.dimension_dialog(ctx, &snapshot),
        );
    }

    fn cancel(&mut self) {
        self.dialog_frame(&egui::Context::default(), Some(egui::Key::Escape));
    }

    fn apply(&mut self, value: &str, expression: bool) {
        let edit = self.editor.sketch_ui.dimension.as_mut().unwrap();
        edit.value = value.into();
        edit.expression = expression;
        edit.focus = true;
        let context = egui::Context::default();
        self.dialog_frame(&context, None);
        self.dialog_frame(&context, Some(egui::Key::Enter));
    }
}

#[test]
fn cancelled_angle_row_never_selects_later_radius_at_reused_index() {
    let all = data();
    let native = &all["review"];
    let index = native["index"].as_u64().unwrap() as usize;
    let mut h = Harness::new(&native["baseline"]);
    h.begin();
    h.request("preview");
    h.reply("preview", &native["seed"], true);
    h.select(index);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(index));
    h.cancel();
    h.request("preview_cancel");
    h.reply("preview_cancel", &native["cancelled"], true);
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    assert!(h.editor.sketch_ui.constraint_name.is_empty());
    h.reply("transaction", &native["later"], true);
    assert_eq!(
        native_constraint(&h.editor.sketch_snapshot().unwrap(), index).unwrap()["type"],
        "Radius"
    );
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    assert!(h.editor.sketch_ui.constraint_name.is_empty());
}

#[test]
fn cancel_restores_prior_native_row_and_name_and_keeps_a_valid_later_choice() {
    let native = data();
    for current in [4, 2] {
        let mut h = Harness::new(&native["baseline"]);
        h.select(0);
        h.begin();
        h.request("preview");
        h.reply("preview", &native["seed"], true);
        h.select(current);
        h.cancel();
        h.request("preview_cancel");
        h.reply("preview_cancel", &native["cancelled"], true);
        let expected = if current == 4 { 0 } else { current };
        assert_eq!(h.editor.sketch_ui.selected_constraint, Some(expected));
        if expected == 0 {
            assert_eq!(h.editor.sketch_ui.constraint_name, "PriorCurve");
        }
        h.reply("transaction", &native["later"], true);
        assert_eq!(h.editor.sketch_ui.selected_constraint, Some(expected));
    }
}

#[test]
fn spline_cleanup_cancel_restores_the_verified_baseline_index() {
    let all = data();
    let native = &all["cleanup"];
    let mut h = Harness::new(&native["baseline"]);
    h.select(5);
    h.begin();
    h.request("preview");
    h.reply("preview", &native["seed"], true);
    h.select(4);
    assert_eq!(h.editor.sketch_ui.constraint_name, "PriorRadius");
    h.reply("inspect", &native["seed"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    h.cancel();
    h.request("preview_cancel");
    h.reply("preview_cancel", &native["cancelled"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(5));
    assert_eq!(h.editor.sketch_ui.constraint_name, "PriorRadius");
    assert_eq!(
        native_constraint(&h.editor.sketch_snapshot().unwrap(), 5).unwrap()["type"],
        "Radius"
    );
}

#[test]
fn failed_apply_refresh_and_successful_commit_keep_the_selected_angle() {
    let all = data();
    let native = &all["accepted"];
    let mut h = Harness::new(&native["baseline"]);
    h.select(0);
    h.begin();
    h.request("preview");
    h.reply("preview", &native["seed"], true);
    h.select(4);
    h.apply("Missing.Length", true);
    h.request("preview");
    h.reply("preview", &native["failed"], false);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    assert!(
        !h.editor
            .sketch_ui
            .dimension
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .applying
    );
    assert!(h.input.try_recv().is_err());
    h.reply("inspect", &native["failed"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    h.apply("60 deg", false);
    h.request("preview");
    h.reply("preview", &native["replacement"], true);
    h.request("preview_commit");
    h.reply("preview_commit", &native["committed"], true);
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    h.reply("inspect", &native["committed"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
}

#[test]
fn failed_cancel_retains_temporary_selection_until_native_owner_closes() {
    let native = data();
    let mut h = Harness::new(&native["baseline"]);
    h.select(0);
    h.begin();
    h.request("preview");
    h.reply("preview", &native["seed"], true);
    h.select(4);
    h.cancel();
    h.request("preview_cancel");
    h.reply("preview_cancel", &native["retained"], false);
    assert!(h.editor.sketch_ui.dimension.is_some());
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    assert!(!h.editor.state["preview"].is_null());
    h.cancel();
    h.request("preview_cancel");
    h.reply("preview_cancel", &native["cancelled"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(0));
}

#[test]
fn deferred_cancel_retains_redo_and_restores_only_a_valid_native_selection() {
    let native = data();
    for choose in [None, Some(2)] {
        let mut h = Harness::new(&native["deferred"]);
        h.select(0);
        h.begin();
        assert!(h.input.try_recv().is_err());
        assert!(
            h.editor
                .sketch_ui
                .dimension
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .deferred
        );
        if let Some(index) = choose {
            h.select(index);
        }
        let before = h.editor.state.clone();
        h.cancel();
        assert!(h.input.try_recv().is_err());
        assert_eq!(h.editor.state, before);
        assert_eq!(
            h.editor.sketch_ui.selected_constraint,
            Some(choose.unwrap_or(0))
        );
        assert!(h.editor.sketch_ui.dimension.is_none());
        assert_eq!(h.editor.state["redo_count"], 1);
    }
}

#[test]
fn deferred_apply_and_immediate_reference_do_not_reuse_old_row_selection() {
    let native = data();
    let mut h = Harness::new(&native["deferred"]);
    h.select(0);
    h.begin();
    h.select(0);
    h.apply("75 deg", false);
    h.request("transaction");
    h.reply("transaction", &native["deferred_result"], true);
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(0));
    let reference = &native["reference"];
    let mut h = Harness::new(&reference["baseline"]);
    h.select(4);
    h.begin();
    assert!(h.editor.sketch_ui.dimension.is_none());
    h.request("transaction");
    h.reply("transaction", &reference["result"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    assert!(h.editor.sketch_ui.constraint_name.is_empty());
    h.select(5);
    h.reply("inspect", &reference["result"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(5));
}

#[test]
fn prior_row_is_not_restored_when_its_native_slot_has_changed() {
    let native = data();
    let original = &native["review"]["seed"]["objects"][0];
    let previous = native_constraint(original, 4).unwrap().clone();
    let mut h = Harness::new(&native["review"]["later"]);
    assert_eq!(
        native_constraint(&h.editor.sketch_snapshot().unwrap(), 4).unwrap()["type"],
        "Radius"
    );
    h.editor.restore_dimension_selection(&DimensionPreview {
        baseline: original.clone(),
        previous_constraint: Some(previous),
        selected_constraint: None,
        token: "unused".into(),
        seeding: false,
        applying: false,
        deferred: false,
        reference: false,
    });
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    assert!(h.editor.sketch_ui.constraint_name.is_empty());
}
