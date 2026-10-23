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

// Current native captures shared by ordinary-dimension and angle lifecycle tests.
fn fresh_native() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/dimension_dialog_states.json"
    ))
    .unwrap()
}
fn fresh_rows(state: &Value) -> &Value {
    &state["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Sketch")
        .unwrap()["sketch"]["constraints"]
}
fn fresh_begin(h: &mut Harness, selection: Vec<SketchElement>) {
    h.editor.begin_dimension("Angle", selection);
}
#[test]
fn fresh_rapid_failed_apply_cancel_and_retry_use_current_revision_and_new_owner() {
    let n = fresh_native();
    let mut h = Harness::new(&n["base"]);
    h.select(0);
    h.begin();
    h.request("preview");
    let first = h.owner.clone();
    h.reply("preview", &n["seed"], true);
    h.select(4);
    h.apply("Missing.Length", true);
    let j = h.request("preview");
    assert_eq!(j.parameters["edits"][0]["expression"], "Missing.Length");
    h.cancel();
    assert!(h.input.try_recv().is_err());
    assert!(h.editor.busy);
    h.reply("preview", &n["failed"], false);
    assert!(!h.editor.busy);
    h.cancel();
    h.request("preview_cancel");
    h.reply("preview_cancel", &n["cancel"], true);
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(0));
    assert_eq!(h.editor.sketch_ui.constraint_name, "PriorCurve");
    h.owner = None;
    h.begin();
    h.request("preview");
    assert_ne!(h.owner, first);
    h.reply("preview", &n["retry_seed"], true);
    h.select(4);
    h.apply("60 deg", false);
    assert_eq!(
        h.request("preview").parameters["edits"][0]["value"],
        "60 deg"
    );
    h.reply("preview", &n["replacement"], true);
    h.request("preview_commit");
    h.reply("preview_commit", &n["commit"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert!(h.input.try_recv().is_err());
}

#[test]
fn fresh_deferred_failed_expression_cancel_preserves_redo_and_prior_selection() {
    let n = fresh_native();
    let mut h = Harness::new(&n["undo"]);
    h.select(0);
    h.begin();
    assert!(h.input.try_recv().is_err());
    h.apply("Missing.Length", true);
    assert_eq!(
        h.request("transaction").parameters["edits"][0]["expression"],
        "Missing.Length"
    );
    h.reply("transaction", &n["deferred_failed"], false);
    assert!(h.editor.sketch_ui.dimension.is_some());
    assert_eq!(h.editor.state["redo_count"], 1);
    let before = h.editor.state.clone();
    h.cancel();
    assert!(h.input.try_recv().is_err());
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.state, before);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(0));
    h.editor.send("redo", json!({}));
    h.request("redo");
    h.reply("redo", &n["redo"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    assert_eq!(fresh_rows(&h.editor.state)[4]["type"], "Angle");
}

#[test]
fn fresh_undo_radius_reuse_redo_and_session_cancel_do_not_revive_angle_row() {
    let n = fresh_native();
    let mut h = Harness::new(&n["circle_commit"]);
    h.select(4);
    h.editor.send("undo", json!({}));
    h.request("undo");
    h.reply("undo", &n["circle_undo"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    h.reply("transaction", &n["radius_reuse"], true);
    assert_eq!(fresh_rows(&h.editor.state)[4]["type"], "Radius");
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    h.select(4);
    h.editor.send("undo", json!({}));
    h.request("undo");
    h.reply("undo", &n["radius_undo"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    h.editor.send("redo", json!({}));
    h.request("redo");
    h.reply("redo", &n["radius_redo"], true);
    assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    h.reply("sketch_cancel", &n["circle_cancel"], true);
    assert!(h.editor.sketch_ui.session.is_none());
    assert!(h.editor.sketch.is_none());
    assert!(h.editor.viewport.sketch_selection.is_empty());
}

#[test]
fn fresh_both_axes_reference_with_redo_has_no_preview_or_dialog_in_all_orders() {
    let n = fresh_native();
    let a = SketchElement {
        index: -1,
        point: 0,
    };
    let b = SketchElement {
        index: -2,
        point: 0,
    };
    let p = SketchElement { index: 0, point: 1 };
    for (i, order) in [
        [a, b, p],
        [a, p, b],
        [b, a, p],
        [b, p, a],
        [p, a, b],
        [p, b, a],
    ]
    .into_iter()
    .enumerate()
    {
        let mut h = Harness::new(&n["axes_deferred"]);
        fresh_begin(&mut h, order.to_vec());
        assert!(h.editor.sketch_ui.dimension.is_none());
        let job = h.request("transaction");
        assert_eq!(job.parameters["edits"][0]["driving"], false);
        let curves: Vec<i32> = order
            .iter()
            .filter(|e| e.point == 0)
            .map(|e| e.index)
            .collect();
        assert_eq!(job.parameters["edits"][0]["args"][0], curves[0]);
        assert_eq!(job.parameters["edits"][0]["args"][1], curves[1]);
        h.reply("transaction", &n[format!("axes_order_{i}")], true);
        assert_eq!(h.editor.state["redo_count"], 0);
        assert!(h.editor.state["preview"].is_null());
        assert_eq!(h.editor.sketch_ui.selected_constraint, None);
        assert!(h.input.try_recv().is_err());
    }
}

#[test]
fn fresh_existing_angle_failed_apply_retains_edit_for_correction_regression() {
    let n = fresh_native();
    let mut h = Harness::new(&n["expression"]);
    h.editor.dimension_at(4);
    h.apply("Missing.Angle", true);
    let job = h.request("transaction");
    assert_eq!(job.parameters["edits"][0]["op"], "expression");
    assert_eq!(job.parameters["edits"][0]["expression"], "Missing.Angle");
    println!(
        "Dialog immediately after submission: {}",
        h.editor.sketch_ui.dimension.is_some()
    );
    h.reply("transaction", &n["existing_failed"], false);
    assert!(h.editor.error.is_some());
    assert_eq!(
        fresh_rows(&h.editor.state)[4]["value"],
        fresh_rows(&n["expression"])[4]["value"]
    );
    assert!(
        h.editor.sketch_ui.dimension.is_some(),
        "Rejected existing-angle expression closes the dimension editor and loses the submitted expression"
    );
}

#[test]
fn ordinary_expression_failure_refocuses_then_corrected_apply_closes_on_success() {
    let native = fresh_native();
    let mut h = Harness::new(&native["ordinary_base"]);
    h.editor.dimension_at(4);
    h.apply("Missing.Angle", true);
    let request = h.request("transaction");
    assert_eq!(
        request.parameters["edits"][0]["expression"],
        "Missing.Angle"
    );
    let edit = h.editor.sketch_ui.dimension.as_ref().unwrap();
    assert!(edit.submitting);
    assert_eq!(edit.value, "Missing.Angle");
    assert!(edit.expression);
    assert!(!h.editor.dirty);
    assert_eq!(h.editor.state, native["ordinary_base"]);
    h.cancel();
    h.dialog_frame(&egui::Context::default(), Some(egui::Key::Enter));
    assert!(h.input.try_recv().is_err());
    assert!(h.editor.sketch_ui.dimension.is_some());

    // A read-only refresh must not complete the pending dimension transaction.
    h.reply("inspect", &native["ordinary_base"], true);
    h.dialog_frame(&egui::Context::default(), Some(egui::Key::Enter));
    h.cancel();
    assert!(h.input.try_recv().is_err());
    assert!(h.editor.sketch_ui.dimension.as_ref().unwrap().submitting);
    h.reply("transaction", &native["ordinary_expression_failed"], false);
    let edit = h.editor.sketch_ui.dimension.as_ref().unwrap();
    assert!(!edit.submitting);
    assert!(edit.focus);
    assert_eq!(edit.value, "Missing.Angle");
    assert!(edit.expression);
    assert!(!h.editor.dirty);
    assert_eq!(
        h.editor.state["undo_count"],
        native["ordinary_base"]["undo_count"]
    );

    // Use the refocused widget to submit the correction, rather than setting
    // the test helper's focus flag again.
    let context = egui::Context::default();
    h.dialog_frame(&context, None);
    assert!(!h.editor.sketch_ui.dimension.as_ref().unwrap().focus);
    assert!(context.memory(|memory| memory.focused()).is_some());
    h.editor.sketch_ui.dimension.as_mut().unwrap().value = "75 deg".into();
    h.dialog_frame(&context, Some(egui::Key::Enter));
    let request = h.request("transaction");
    assert_eq!(request.parameters["edits"][0]["expression"], "75 deg");
    assert!(!h.editor.dirty);
    assert!(h.editor.sketch_ui.dimension.as_ref().unwrap().submitting);
    h.reply(
        "transaction",
        &native["ordinary_expression_corrected"],
        true,
    );
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert!(h.editor.dirty);
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    assert_eq!(h.editor.state, native["ordinary_expression_corrected"]);
    assert!(h.input.try_recv().is_err());
}

#[test]
fn ordinary_expression_failure_cancel_is_local_and_preserves_native_state_and_selection() {
    let native = fresh_native();
    let mut h = Harness::new(&native["ordinary_base"]);
    h.editor.dimension_at(4);
    h.apply("Missing.Angle", true);
    h.request("transaction");
    h.reply("transaction", &native["ordinary_expression_failed"], false);
    let before = h.editor.state.clone();
    let selected = h.editor.sketch_ui.selected_constraint;
    let name = h.editor.sketch_ui.constraint_name.clone();
    h.cancel();
    assert!(h.input.try_recv().is_err());
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.state, before);
    assert_eq!(h.editor.sketch_ui.selected_constraint, selected);
    assert_eq!(h.editor.sketch_ui.constraint_name, name);
    assert!(!h.editor.dirty);
}

#[test]
fn ordinary_literal_and_unit_failures_allow_retry_or_cancel_for_angle_and_distance() {
    let native = fresh_native();
    for (base, failed, corrected, index, bad, good) in [
        (
            "ordinary_expression_corrected",
            "ordinary_literal_failed",
            "ordinary_literal_corrected",
            4,
            "10 deg +",
            "65 deg",
        ),
        (
            "ordinary_literal_corrected",
            "distance_literal_failed",
            "distance_literal_corrected",
            2,
            "10 deg",
            "8 mm",
        ),
    ] {
        for cancel in [false, true] {
            let mut h = Harness::new(&native[base]);
            h.editor.dimension_at(index);
            h.apply(bad, false);
            let request = h.request("transaction");
            assert_eq!(
                request.parameters["edits"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap()["value"],
                bad
            );
            h.reply("transaction", &native[failed], false);
            let edit = h.editor.sketch_ui.dimension.as_ref().unwrap();
            assert_eq!(edit.value, bad);
            assert!(!edit.expression);
            assert!(!edit.submitting);
            assert!(edit.focus);
            assert!(!h.editor.dirty);
            if cancel {
                let before = h.editor.state.clone();
                h.cancel();
                assert!(h.input.try_recv().is_err());
                assert_eq!(h.editor.state, before);
                assert!(!h.editor.dirty);
            } else {
                h.apply(good, false);
                let request = h.request("transaction");
                assert_eq!(
                    request.parameters["edits"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap()["value"],
                    good
                );
                assert!(h.editor.sketch_ui.dimension.is_some());
                h.reply("transaction", &native[corrected], true);
                assert!(h.editor.dirty);
                assert_eq!(h.editor.state, native[corrected]);
            }
            assert!(h.editor.sketch_ui.dimension.is_none());
            assert_eq!(h.editor.sketch_ui.selected_constraint, Some(index));
        }
    }
}

#[test]
fn ordinary_new_dimension_failure_retains_literal_for_retry_or_local_cancel() {
    let native = fresh_native();
    for cancel in [false, true] {
        let mut h = Harness::new(&native["creation_base"]);
        h.editor
            .begin_dimension("Angle", vec![SketchElement { index: 0, point: 0 }]);
        assert!(h.input.try_recv().is_err());
        h.apply("10 mm", false);
        let request = h.request("transaction");
        assert_eq!(request.parameters["edits"][0]["kind"], "Angle");
        assert_eq!(request.parameters["edits"][1]["value"], "10 mm");
        assert!(!h.editor.dirty);
        h.dialog_frame(&egui::Context::default(), Some(egui::Key::Enter));
        assert!(h.input.try_recv().is_err());
        h.reply("transaction", &native["creation_failed"], false);
        let edit = h.editor.sketch_ui.dimension.as_ref().unwrap();
        assert_eq!(edit.index, None);
        assert_eq!(edit.value, "10 mm");
        assert!(!edit.expression);
        assert!(edit.focus);
        assert!(!edit.submitting);
        assert_eq!(fresh_rows(&h.editor.state).as_array().unwrap().len(), 0);
        if cancel {
            let before = h.editor.state.clone();
            h.cancel();
            assert_eq!(h.editor.state, before);
            assert!(h.input.try_recv().is_err());
            assert!(!h.editor.dirty);
        } else {
            h.apply("60 deg", false);
            h.request("transaction");
            assert!(h.editor.sketch_ui.dimension.is_some());
            h.reply("transaction", &native["creation_corrected"], true);
            assert!(h.editor.dirty);
            assert_eq!(fresh_rows(&h.editor.state).as_array().unwrap().len(), 1);
        }
        assert!(h.editor.sketch_ui.dimension.is_none());
        assert_eq!(h.editor.sketch_ui.selected_constraint, None);
    }
}

#[test]
fn ordinary_failed_retry_refuses_unrelated_constraint_at_reused_native_index() {
    let native = fresh_native();
    for cancel_without_retry in [false, true] {
        let mut h = Harness::new(&native["ordinary_base"]);
        h.editor.dimension_at(4);
        h.apply("Missing.Angle", true);
        h.request("transaction");
        h.reply("transaction", &native["ordinary_expression_failed"], false);
        // This is a captured native external refresh injection. The dialog did
        // not issue the delete/replacement operations itself.
        h.reply("inspect", &native["ordinary_reused_row"], true);
        assert_eq!(fresh_rows(&h.editor.state)[4]["type"], "Radius");
        let before = h.editor.state.clone();
        if cancel_without_retry {
            h.cancel();
            assert!(h.editor.sketch_ui.dimension.is_none());
            assert_eq!(h.editor.sketch_ui.selected_constraint, None);
            assert!(h.editor.sketch_ui.constraint_name.is_empty());
            assert_eq!(h.editor.state, before);
            assert!(h.input.try_recv().is_err());
            continue;
        }
        h.apply("65 deg", false);
        assert!(h.input.try_recv().is_err());
        assert_eq!(h.editor.sketch_ui.selected_constraint, None);
        assert!(h.editor.sketch_ui.constraint_name.is_empty());
        assert_eq!(
            h.editor.error.as_deref(),
            Some("This dimension changed. Cancel and select it again")
        );
        assert_eq!(
            h.editor.sketch_ui.dimension.as_ref().unwrap().value,
            "65 deg"
        );
        assert!(!h.editor.dirty);
        h.cancel();
        assert!(h.editor.sketch_ui.dimension.is_none());
        assert_eq!(h.editor.state, before);
        assert!(h.input.try_recv().is_err());
    }
}

#[test]
fn ordinary_unrelated_replies_and_send_failure_keep_dialog_cancelable() {
    let native = fresh_native();
    let mut h = Harness::new(&native["ordinary_base"]);
    h.editor.dimension_at(4);
    h.reply("inspect", &native["ordinary_base"], true);
    h.editor.dimension_reply("transaction", true);
    assert!(h.editor.sketch_ui.dimension.is_some());
    assert!(!h.editor.sketch_ui.dimension.as_ref().unwrap().submitting);
    let (_, replacement) = mpsc::sync_channel(1);
    drop(std::mem::replace(&mut h.input, replacement));
    h.apply("75 deg", true);
    assert!(!h.editor.busy);
    assert_eq!(h.editor.error.as_deref(), Some("Model thread stopped"));
    let edit = h.editor.sketch_ui.dimension.as_ref().unwrap();
    assert!(!edit.submitting);
    assert!(edit.focus);
    assert_eq!(edit.value, "75 deg");
    h.cancel();
    assert!(h.editor.sketch_ui.dimension.is_none());
    assert_eq!(h.editor.sketch_ui.selected_constraint, Some(4));
    assert_eq!(h.editor.state, native["ordinary_base"]);
    assert!(!h.editor.dirty);
}
