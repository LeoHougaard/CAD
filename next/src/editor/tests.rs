//! Actor/transport failure checks only. Native workflow tests verify geometry.
use super::*;
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    thread::JoinHandle,
};

#[test]
fn cube_corner_pointer_route_preserves_model_and_history() {
    let state: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/view_cube_corner_state.json"
    ))
    .unwrap();
    let (jobs, input) = mpsc::sync_channel(1);
    let (_output, replies) = mpsc::channel();
    // Real Editor frames, with no worker thread/process: any emitted model job
    // fails the test. The state is a retained completed native sketch snapshot.
    let mut editor = Editor {
        jobs,
        replies,
        busy: false,
        registry: Registry::builtin().unwrap(),
        state: state.clone(),
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
        sequence: 17,
        error: None,
        status: String::new(),
        path: "retained-prefix.FCStd".into(),
        file_input: String::new(),
        file_dialog: None,
        file_browser: file_browser::FileBrowser::default(),
        overwrite: false,
        dirty: true,
        pending_file: None,
        allow_close: false,
        fit_next: false,
    };
    editor.viewport.set_view(crate::viewport::StandardView::Top);
    let ctx = egui::Context::default();
    let frame = |editor: &mut Editor, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 850.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(editor, ctx, &mut eframe::Frame::_new_kittest());
            },
        )
    };
    frame(&mut editor, vec![]);
    let output = frame(&mut editor, vec![]);
    let cube = editor.viewport.view_cube_rect().unwrap();
    let polygons: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Path(path)
                if path.closed
                    && path.points.len() == 4
                    && path.points.iter().all(|p| cube.contains(*p)) =>
            {
                Some(&path.points)
            }
            _ => None,
        })
        .collect();
    assert_eq!(polygons.len(), 1, "actual rendered Top cube face");
    let pos = *polygons[0]
        .iter()
        .max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)))
        .unwrap();
    assert!(pos.x > cube.center().x && pos.y > cube.center().y);
    let before = (editor.path.clone(), editor.dirty, editor.sequence);
    for pressed in [true, false] {
        frame(
            &mut editor,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            editor.state, state,
            "corner input must preserve all native/history fields"
        );
        assert!(
            input.try_recv().is_err(),
            "corner input emitted a model job"
        );
    }
    let origin = editor.viewport.sketch_screen_point([0., 0., 0.]);
    let axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
        .map(|p| editor.viewport.sketch_screen_point(p) - origin);
    assert!(axes[0].x > 0. && axes[0].y > 0.);
    assert!(axes[1].x > 0. && axes[1].y < 0.);
    assert!(axes[2].y < 0.);
    // Fixed screen-space f32 subtraction bound, frozen before execution.
    assert!(
        axes.iter()
            .all(|axis| (axis.length() - axes[0].length()).abs() < 1e-3)
    );
    assert_eq!((editor.path.clone(), editor.dirty, editor.sequence), before);
    assert!(!editor.busy && editor.error.is_none());
    assert!(editor.viewport.view_cube_owns_pointer());
}

struct Actor {
    jobs: Option<SyncSender<Job>>,
    replies: Receiver<Reply>,
    stopped: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Actor {
    fn new() -> Self {
        let mut config = WorkerConfig::new(std::env::current_exe().unwrap(), "--quiet");
        config.args = [
            "--ignored",
            "--exact",
            "editor::tests::actor_fixture",
            "--nocapture",
        ]
        .map(Into::into)
        .to_vec();
        config.response_timeout = Duration::from_secs(3);
        let (jobs, input) = mpsc::sync_channel(1);
        let (output, replies) = mpsc::channel();
        let (done, stopped) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            model_thread(config, input, output, egui::Context::default());
            let _ = done.send(());
        });
        Self {
            jobs: Some(jobs),
            replies,
            stopped,
            thread: Some(thread),
        }
    }

    fn call(&self, op: &str, parameters: Value) -> Reply {
        self.jobs
            .as_ref()
            .unwrap()
            .try_send(Job {
                op: op.into(),
                parameters,
            })
            .unwrap();
        self.replies
            .recv_timeout(Duration::from_secs(5))
            .expect("actor reply timed out")
    }

    fn edit(&self, state: &Value, display_failure: &str) -> Reply {
        self.call(
            "transaction",
            json!({"document":state["document"],
            "expected_revision":state["revision"], "display_failure":display_failure}),
        )
    }
}

impl Drop for Actor {
    fn drop(&mut self) {
        self.jobs.take();
        if self.stopped.recv_timeout(Duration::from_secs(5)).is_ok() {
            let result = self.thread.take().unwrap().join();
            if !std::thread::panicking() {
                result.expect("actor thread panicked");
            }
        } else if !std::thread::panicking() {
            panic!("actor did not stop after input channel closed");
        }
    }
}

struct FixtureDocument {
    revision: u64,
    path: Option<String>,
    edits: u64,
    display_failure: String,
}

fn state(name: &str, doc: &FixtureDocument, events: &[String]) -> Value {
    if name == "Multiple" {
        return json!({"document":name,"revision":doc.revision,"fixture_edits":doc.edits,
        "fixture_events":events,"objects":[
            {"name":"BodyA","type":"PartDesign::Body","body_state":{"display_object":"ShapeA"}},
            {"name":"BodyB","type":"PartDesign::Body","body_state":{"display_object":"ShapeB"}}
        ]});
    }
    json!({"document":name,"revision":doc.revision,"fixture_edits":doc.edits,
        "fixture_events":events,"objects":[{"name":"Shape","type":"Part::Box","shape":{"solids":1}}]})
}

// Invoked only in a child process. This fixture models protocol envelopes and
// document ownership, not native geometry, recomputation or FreeCAD behavior.
#[test]
#[ignore]
fn actor_fixture() {
    let mut documents = BTreeMap::<String, FixtureDocument>::new();
    let mut events = Vec::<String>::new();
    let mut revision = 0;
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let op = request["op"].as_str().unwrap();
        let document = request["document"].as_str().unwrap_or("");
        let result: Result<Value, (&str, &str)> = (|| {
            if op == "capabilities" {
                return Ok(json!({"fixture":"actor lifecycle"}));
            }
            if matches!(op, "new" | "open") {
                let source = request[if op == "new" { "name" } else { "path" }]
                    .as_str()
                    .unwrap();
                events.push(format!("{op}:{source}"));
                if source == "missing.FCStd" {
                    return Err(("not_found", "replacement failed validation"));
                }
                if op == "open"
                    && documents
                        .values()
                        .any(|doc| doc.path.as_deref() == Some(source))
                {
                    return Err(("already_open", "native document is already open"));
                }
                let name = source.trim_end_matches(".FCStd").to_owned();
                if documents.contains_key(&name) {
                    return Err(("name_conflict", "name already open"));
                }
                revision += 1;
                let doc = FixtureDocument {
                    revision,
                    path: (op == "open").then(|| source.into()),
                    edits: 0,
                    display_failure: String::new(),
                };
                let snapshot = state(&name, &doc, &events);
                documents.insert(name, doc);
                return Ok(snapshot);
            }
            let doc = documents
                .get_mut(document)
                .ok_or(("not_found", "document was closed"))?;
            if matches!(op, "transaction" | "close")
                && request["expected_revision"].as_u64() != Some(doc.revision)
            {
                return Err(("stale_revision", "revision changed"));
            }
            match op {
                "inspect" => Ok(state(document, doc, &events)),
                "transaction" => {
                    revision += 1;
                    doc.revision = revision;
                    doc.edits += 1;
                    doc.display_failure = request["display_failure"].as_str().unwrap_or("").into();
                    Ok(json!({"state":state(document, doc, &events)}))
                }
                "close" => {
                    events.push(format!("close:{document}"));
                    documents.remove(document);
                    Ok(json!({"document":document,"closed":true}))
                }
                "mesh" | "topology" => {
                    let second = request["object"] == "ShapeB";
                    if doc.display_failure == op
                        || second && doc.display_failure == "second_mesh" && op == "mesh"
                    {
                        return Err(("display_error", "display failed after native commit"));
                    }
                    let revision =
                        doc.revision + u64::from(second && doc.display_failure == "stale_second");
                    let object = if second && doc.display_failure == "wrong_object" {
                        json!("ShapeA")
                    } else {
                        request["object"].clone()
                    };
                    let owner = if second && doc.display_failure == "wrong_document" {
                        "Foreign"
                    } else {
                        document
                    };
                    Ok(json!({"document":owner,"object":object,"revision":revision,
                        "fixture_events":events,"open_documents":documents.keys().collect::<Vec<_>>()}))
                }
                _ => Err(("unsupported_operation", "fixture operation unsupported")),
            }
        })();
        let response = match result {
            Ok(result) => json!({"version":1,"id":request["id"],"ok":true,"result":result}),
            Err((code, message)) => json!({"version":1,"id":request["id"],"ok":false,
                "error":{"code":code,"message":message}}),
        };
        println!("{response}");
        io::stdout().flush().unwrap();
    }
}

#[test]
fn accepted_transaction_survives_display_failure() {
    for display in ["mesh", "topology"] {
        let actor = Actor::new();
        let initial = actor.call("new", json!({"name":"Original"}));
        assert!(initial.accepted && initial.error.is_none());
        let edited = actor.edit(&initial.state, display);
        assert!(
            edited.accepted,
            "native commit must remain accepted after {display} fails"
        );
        assert!(edited.error.as_deref().unwrap().contains("display_error"));
        assert_eq!(edited.state["fixture_edits"], 1);
        assert!(edited.state["revision"].as_u64() > initial.state["revision"].as_u64());
        let continued = actor.edit(&edited.state, "");
        assert!(continued.accepted && continued.error.is_none());
        assert_eq!(continued.state["fixture_edits"], 2);
    }
}

#[test]
fn multiple_body_display_requires_matching_identity_and_revision_for_every_object() {
    let actor = Actor::new();
    let initial = actor.call("new", json!({"name":"Multiple"}));
    assert!(initial.accepted && initial.error.is_none());
    assert_eq!(initial.scenes.len(), 2);
    assert_eq!(initial.scenes[0].0["object"], "ShapeA");
    assert_eq!(initial.scenes[1].0["object"], "ShapeB");
    let mut state = initial.state;
    for failure in [
        "second_mesh",
        "wrong_object",
        "wrong_document",
        "stale_second",
    ] {
        let edited = actor.edit(&state, failure);
        assert!(edited.accepted && edited.error.is_some());
        assert!(
            edited.scenes.is_empty(),
            "A failed display must not publish only one of the bodies"
        );
        let continued = actor.edit(&edited.state, "");
        assert!(continued.accepted && continued.error.is_none());
        assert_eq!(continued.scenes.len(), 2);
        state = continued.state;
    }
}

#[test]
fn failed_and_repeated_open_preserve_editable_document() {
    let actor = Actor::new();
    let first = actor.call("open", json!({"path":"Original.FCStd"}));
    assert!(first.accepted);
    let mut current = first.state;
    for (path, error) in [
        ("missing.FCStd", "not_found"),
        ("Original.FCStd", "already_open"),
    ] {
        let rejected = actor.call("open", json!({"path":path}));
        assert!(!rejected.accepted && rejected.error.as_deref().unwrap().contains(error));
        assert_eq!(rejected.state["document"], "Original");
        assert_eq!(rejected.state["revision"], current["revision"]);
        assert!(
            !rejected.state["fixture_events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event.as_str().unwrap().starts_with("close:"))
        );
        let edited = actor.edit(&rejected.state, "");
        assert!(edited.accepted && edited.error.is_none());
        current = edited.state;
    }
}

#[test]
fn valid_replacements_close_old_document_after_acceptance() {
    let actor = Actor::new();
    assert!(actor.call("new", json!({"name":"Original"})).accepted);
    let replacement = actor.call("new", json!({"name":"Replacement"}));
    assert!(replacement.accepted && replacement.error.is_none());
    assert_eq!(
        replacement.scenes[0].0["fixture_events"],
        json!(["new:Original", "new:Replacement", "close:Original"])
    );
    assert_eq!(
        replacement.scenes[0].0["open_documents"],
        json!(["Replacement"])
    );
    let opened = actor.call("open", json!({"path":"Saved.FCStd"}));
    assert!(opened.accepted && opened.error.is_none());
    assert_eq!(
        opened.scenes[0].0["fixture_events"],
        json!([
            "new:Original",
            "new:Replacement",
            "close:Original",
            "open:Saved.FCStd",
            "close:Replacement"
        ])
    );
    assert_eq!(opened.scenes[0].0["open_documents"], json!(["Saved"]));
    assert!(actor.edit(&opened.state, "").accepted);
}
