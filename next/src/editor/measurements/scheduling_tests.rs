//! Deterministic native replies through the actual Editor/model thread. Selection
//! changes are direct viewport state updates; button/key routes use headless egui.
use super::*;

fn fixture(n: &mut Native, name: &str) {
    n.send("new", json!({"name":name}));
    n.send(
        "transaction",
        json!({"label":"Scheduling fixture","edits":[
            {"op":"add_object","type":"Part::Box","name":"Box"},
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch"},
            {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[17,0,0]}
        ]}),
    );
}

fn select(n: &mut Native, element: &str) {
    n.editor.viewport.selections = vec![reference("Box", element)];
    n.editor.body_ui.selected.clear();
}

fn quiet(n: &mut Native) {
    for _ in 0..5 {
        n.editor.measurement_pump();
        assert!(
            !n.editor.busy,
            "Unchanged completed/error key must not retry each frame"
        );
    }
    assert!(n.editor.measurement_ui.pending.is_none());
}

#[test]
#[ignore = "pinned native Editor scheduling regression; no desktop"]
fn native_measure_revisits_discarded_selection_and_coalesces_rapid_toggles() {
    let mut n = Native::case("discarded-selection");
    fixture(&mut n, "DiscardedSelection");
    select(&mut n, "Edge1");
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    assert!(n.editor.busy);
    let baseline = n.editor.state.clone();
    select(&mut n, "Edge2");
    n.editor.measurement_pump();
    n.receive("measure", true);
    assert!(n.editor.measurement_ui.result.is_none());
    assert!(n.editor.measurement_ui.attempted.is_none());
    select(&mut n, "Edge1");
    n.editor.measurement_pump();
    assert!(
        n.editor.busy,
        "A -> B -> discard A -> A must automatically queue A"
    );
    n.receive("measure", true);
    assert_eq!(
        n.editor.measurement_ui.result.as_ref().unwrap().selections[0]
            .reference
            .subelement,
        "Edge1"
    );
    quiet(&mut n);
    assert_eq!(n.editor.state, baseline);
    // A still-current in-flight reply after many toggles is accepted once.
    n.click("Recompute");
    assert!(n.editor.busy);
    for element in ["Edge2", "Edge1", "Vertex1", "Face1", "Edge1"] {
        select(&mut n, element);
        n.editor.measurement_pump();
        assert!(n.editor.busy);
    }
    n.receive("measure", true);
    assert!(n.editor.measurement_ui.result.is_some());
    quiet(&mut n);
    // End at B, discard A, then return to A without queueing B.
    n.click("Recompute");
    for element in ["Edge2", "Face1", "Edge1", "Edge2"] {
        select(&mut n, element);
        n.editor.measurement_pump();
    }
    n.receive("measure", true);
    select(&mut n, "Edge1");
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.receive("measure", true);
    quiet(&mut n);
    assert_eq!(n.events.iter().filter(|e| e["op"] == "measure").count(), 5);
    assert_eq!(n.editor.state, baseline);
}

#[test]
#[ignore = "pinned native Editor scheduling revision regression; no desktop"]
fn native_measure_discarded_revision_can_automatically_measure_current_model() {
    let mut n = Native::case("discarded-revision");
    fixture(&mut n, "DiscardedRevision");
    n.editor.body_ui.selected = vec!["Box".into()];
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    let old_revision = n.editor.state["revision"].clone();
    let held = n.raw.recv_timeout(Duration::from_secs(40)).unwrap();
    assert!(held.accepted && held.op == "measure");
    // Test-only delivery reordering: advance the actual native worker and publish
    // that new snapshot before delivering the held read. Production send remains
    // serialized; this verifies the revision guard without inventing native data.
    n.editor.jobs.try_send(Job {op:"transaction".into(),parameters:json!({
        "document":n.editor.state["document"],"expected_revision":old_revision,
        "label":"Native revision advance","edits":[{"op":"set","object":"Box","property":"Length","value":23}]
    })}).unwrap();
    n.receive("transaction", true);
    let current = n.editor.state.clone();
    let dirty = n.editor.dirty;
    assert_ne!(current["revision"], old_revision);
    n.accept(held, "measure", true);
    assert!(n.editor.measurement_ui.result.is_none());
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.receive("measure", true);
    let result = n.editor.measurement_ui.result.as_ref().unwrap();
    assert_eq!(json!(result.revision), current["revision"]);
    assert!(
        (result.selections[0]
            .metrics
            .iter()
            .find(|m| m.key == "volume")
            .unwrap()
            .value
            - 2300.)
            .abs()
            < 1e-7
    );
    quiet(&mut n);
    assert_eq!(n.editor.state, current);
    assert_eq!(n.editor.dirty, dirty);
    // The ordinary Editor mutation route also clears the old result immediately.
    n.editor.send("transaction",json!({"label":"Ordinary edit","edits":[{"op":"set","object":"Box","property":"Width","value":12}]}));
    assert!(n.editor.measurement_ui.result.is_none());
    n.receive("transaction", true);
    n.editor.measurement_pump();
    n.receive("measure", true);
    quiet(&mut n);
}

#[test]
#[ignore = "pinned native Editor scheduling error/clear/close regression; no desktop"]
fn native_measure_failed_current_read_does_not_spin_and_clear_close_release_reads() {
    let mut n = Native::case("error-clear-close");
    fixture(&mut n, "ErrorClearClose");
    n.editor.edit_native_properties("Box");
    n.editor.draft.as_mut().unwrap().parameters["properties"]["Length"] = json!(21);
    n.editor.preview();
    n.receive("preview", true);
    let owner = n.editor.state.clone();
    let dirty = n.editor.dirty;
    select(&mut n, "Edge999");
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.receive("measure", false);
    let error = n.editor.measurement_ui.error.clone();
    assert!(error.is_some());
    quiet(&mut n);
    assert_eq!(n.editor.measurement_ui.error, error);
    n.click("Recompute");
    assert!(n.editor.busy);
    n.receive("measure", false);
    quiet(&mut n);
    assert_eq!(n.editor.state, owner);
    assert_eq!(n.editor.dirty, dirty);
    assert!(n.editor.draft.is_some());
    // A failed stale response also releases its attempt. Returning to that same
    // invalid reference queues once, then caches the current native error.
    n.click("Recompute");
    select(&mut n, "Edge1");
    n.editor.measurement_pump();
    n.receive("measure", false);
    select(&mut n, "Edge999");
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.receive("measure", false);
    quiet(&mut n);
    select(&mut n, "Edge1");
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.click("Clear selection");
    assert!(n.editor.measurement_references().is_empty());
    n.receive("measure", true);
    n.editor.measurement_pump();
    assert!(n.editor.measurement_ui.result.is_none());
    quiet(&mut n);
    assert_eq!(n.editor.state, owner);
    assert!(n.editor.draft.is_some());
    select(&mut n, "Edge1");
    n.editor.measurement_pump();
    assert!(n.editor.busy);
    n.key(Key::Escape, egui::Modifiers::NONE);
    n.receive("measure", true);
    assert!(!n.editor.measurement_ui.open && n.editor.measurement_ui.result.is_none());
    quiet(&mut n);
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.receive("measure", true);
    quiet(&mut n);
    assert_eq!(n.editor.state, owner);
    assert_eq!(n.editor.dirty, dirty);
    assert!(n.editor.draft.is_some());
    n.click("Recompute");
    assert!(n.editor.busy);
    n.close_measure_window();
    n.receive("measure", true);
    quiet(&mut n);
    assert_eq!(n.editor.state, owner);
    assert!(n.editor.draft.is_some());
    n.editor.finish_preview(false);
    n.receive("preview_cancel", true);
}
