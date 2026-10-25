//! Actual Editor forms and model thread against the pinned worker, no desktop.
use super::*;
use crate::sketch_commands::SketchElement;

fn mirror_start(n: &mut Native) {
    n.editor.viewport.sketch_selection = vec![SketchElement { index: 0, point: 0 }];
    n.click("Edit tools");
    n.click("Mirror");
    assert!(n.editor.sketch_ui.task.is_some());
}
fn preview(n: &mut Native, accepted: bool) {
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    n.receive("preview", accepted);
}
fn count(n: &Native) -> u64 {
    object(&n.editor.state, "Sketch")["sketch"]["geometry_count"]
        .as_u64()
        .unwrap()
}
fn native_xml(n: &Native) -> Value {
    object(&n.editor.state, "Sketch")["sketch"]["native_xml"].clone()
}
fn setup(case: &str, consumed: bool) -> Native {
    let mut n = Native::new(case);
    n.send("new", json!({"name":case.replace('-',"_")}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"},
        {"op":"geometry","object":"Sketch","kind":"circle","center":[5,5,0],"radius":2},
        {"op":"constraint","object":"Sketch","kind":"Radius","args":[0,2]},
        {"op":"constraint_state","object":"Sketch","index":0,"name":"SourceRadius"},
        {"op":"expression","object":"Sketch","property":"Constraints.SourceRadius","expression":"2 mm"},
        {"op":"geometry","object":"Sketch","kind":"line","start":[0,0,0],"end":[1,1,0],"construction":true}
    ]));
    if consumed {
        n.transaction(json!([
            {"op":"add_object","type":"PartDesign::Pad","name":"Pad","body":"Body"},
            {"op":"set","object":"Body","property":"AllowCompound","value":true},
            {"op":"set","object":"Pad","property":"Profile","value":{"link":"Sketch"}},
            {"op":"set","object":"Pad","property":"Length","value":5}
        ]));
    }
    if case == "mirror-persistent-identities" {
        n.transaction(json!([
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower"},
            {"op":"external_geometry","object":"Follower","source":"Sketch","subelement":"Edge1"}
        ]));
    }
    n.save("baseline.FCStd");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    n
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_apply_cancel_action_history_and_reopen() {
    let mut n = setup("mirror-editor", true);
    let baseline = object(&n.editor.state, "Sketch")["sketch"].clone();
    mirror_start(&mut n);
    preview(&mut n, true);
    assert_eq!(count(&n), 3);
    n.click("X axis");
    n.click("Y axis");
    preview(&mut n, true);
    assert_eq!(count(&n), 3, "replacement must use captured baseline");
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(object(&n.editor.state, "Sketch")["sketch"], baseline);
    mirror_start(&mut n);
    n.click("Create symmetry constraints");
    preview(&mut n, true);
    assert!(
        object(&n.editor.state, "Sketch")["sketch"]["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Symmetric")
    );
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(n.editor.sketch_ui.task.is_none());
    assert_eq!(count(&n), 3);
    n.send("undo", json!({}), true);
    assert_eq!(count(&n), 2);
    // Opening/canceling a deferred task preserves the accepted action's Redo.
    mirror_start(&mut n);
    n.click("Cancel");
    assert!(!n.editor.busy);
    n.send("redo", json!({}), true);
    assert_eq!(count(&n), 3);
    n.send("undo", json!({}), true);
    assert_eq!(count(&n), 2);
    mirror_start(&mut n);
    n.click("Create symmetry constraints");
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    assert!(!n.editor.busy, "redo defers native preview until Apply");
    n.click("Apply");
    n.receive("transaction", true);
    assert_eq!(count(&n), 3);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("mirrored.FCStd");
    n.send("undo", json!({}), true);
    assert_eq!(count(&n), 2);
    n.send("redo", json!({}), true);
    assert_eq!(count(&n), 3);
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(count(&n), 3);
    assert!(
        object(&n.editor.state, "Sketch")["expressions"]
            .to_string()
            .contains("2 mm")
    );
    assert_eq!(object(&n.editor.state, "Body")["body_state"]["tip"], "Pad");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let reopened = object(&n.editor.state, "Sketch").clone();
    mirror_start(&mut n);
    n.click("Create symmetry constraints");
    n.click("Delete original geometries");
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert_eq!(count(&n), 3);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(count(&n), 3);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"],
        reopened["sketch"]
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_invalid_selection_native_failure_and_session_cancel() {
    let mut n = setup("mirror-failure", false);
    n.editor
        .begin_sketch_task("mirror", Some(vec![SketchElement { index: 1, point: 1 }]));
    assert!(n.editor.sketch_ui.task.is_none());
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("X axis");
    n.click("Geometry 0 point 3");
    // Native center symmetry on the source circle duplicates its own locus.
    n.click("Create symmetry constraints");
    preview(&mut n, false);
    assert!(n.editor.sketch_ui.task.is_some());
    assert_eq!(count(&n), 3);
    n.click("Geometry 0 point 3");
    n.click("Line 1");
    n.click("Create symmetry constraints");
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert_eq!(count(&n), 3);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(count(&n), 2);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_consumed_finish_failure_stays_repairable() {
    let mut n = setup("mirror-consumed-failure", true);
    let baseline = object(&n.editor.state, "Sketch")["sketch"].clone();
    mirror_start(&mut n);
    n.click("X axis");
    n.click("Geometry 0 point 3");
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), false);
    assert!(n.editor.sketch_ui.session.is_some());
    assert_eq!(count(&n), 3);
    n.send("undo", json!({}), true);
    assert_eq!(count(&n), 2);
    n.send("redo", json!({}), true);
    assert_eq!(count(&n), 3);
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(object(&n.editor.state, "Sketch")["sketch"], baseline);
    n.save("restored.FCStd");
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_accepted_native_ids_survive_later_tasks_and_history() {
    let mut n = setup("mirror-persistent-identities", true);
    let source = native_xml(&n);
    let follower = object(&n.editor.state, "Follower")["sketch"]["externals"].clone();
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    let accepted = native_xml(&n);
    mirror_start(&mut n);
    preview(&mut n, true);
    let accepted_prefix = accepted["geometry"].as_array().unwrap();
    assert_eq!(
        &native_xml(&n)["geometry"].as_array().unwrap()[..accepted_prefix.len()],
        accepted_prefix
    );
    n.click("X axis");
    n.click("Y axis");
    preview(&mut n, true);
    let valid_preview = native_xml(&n);
    assert_eq!(
        &valid_preview["geometry"].as_array().unwrap()[..accepted_prefix.len()],
        accepted_prefix
    );
    n.click("Y axis");
    n.click("Geometry 0 point 3");
    n.click("Create symmetry constraints");
    preview(&mut n, false);
    assert_eq!(
        native_xml(&n),
        valid_preview,
        "failed replacement must restore the native candidate itself"
    );
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(
        native_xml(&n),
        accepted,
        "later Task Cancel must retain every accepted native ID/XML byte"
    );
    assert_eq!(
        object(&n.editor.state, "Follower")["sketch"]["externals"],
        follower
    );
    n.send("undo", json!({}), true);
    assert_eq!(native_xml(&n), source);
    mirror_start(&mut n);
    n.click("Cancel");
    assert!(!n.editor.busy);
    n.send("redo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("identity.FCStd");
    n.send("undo", json!({}), true);
    assert_eq!(native_xml(&n), source);
    n.send("redo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(native_xml(&n), accepted);
    assert_eq!(
        object(&n.editor.state, "Follower")["sketch"]["externals"],
        follower
    );
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
    assert_eq!(native_xml(&n), accepted);
}

#[test]
#[ignore = "requires runtime with missing adapter and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_unavailable_constraints_preserves_default_and_redo() {
    let mut n = setup("mirror-unavailable", false);
    let caps = object(&n.editor.state, "Sketch")["sketch"]["task_capabilities"]["mirror"].clone();
    assert_eq!(caps["default_copy"]["available"], true);
    assert_eq!(caps["create_symmetry_constraints"]["available"], false);
    assert!(
        caps["create_symmetry_constraints"]["reason"]
            .as_str()
            .unwrap()
            .contains("missing")
    );
    let before = n.editor.state.clone();
    n.send(
        "transaction",
        json!({"edits":[{"op":"mirror_geometry","object":"Sketch","indices":[0],
        "reference":{"index":-1,"point":0},"create_symmetry_constraints":true}]}),
        false,
    );
    assert!(
        n.editor
            .error
            .as_ref()
            .unwrap()
            .contains("sketch_mode_unavailable")
    );
    assert_eq!(
        n.editor.state, before,
        "unavailable mode rejects before revision/history/native changes"
    );
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("Create symmetry constraints");
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    assert!(
        !n.editor.busy,
        "disabled checkbox must not stage a replacement"
    );
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(
        !object(&n.editor.state, "Sketch")["sketch"]["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "Symmetric")
    );
    let accepted = native_xml(&n);
    n.send("undo", json!({}), true);
    mirror_start(&mut n);
    n.click("Create symmetry constraints");
    std::thread::sleep(Duration::from_millis(170));
    n.frame(vec![]);
    assert!(!n.editor.busy);
    n.click("Cancel");
    n.send("redo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_cancel", json!({"token":token}), true);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_plain_circle_action_retains_native_identity_after_task_cancel() {
    let mut n = setup("plain-circle-accepted-action", false);
    let source = native_xml(&n);
    n.transaction(json!([{"op":"geometry","object":"Sketch","kind":"circle",
        "center":[12,9,0],"radius":1,"construction":true}]));
    let accepted = native_xml(&n);
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(native_xml(&n), accepted);
    n.send("undo", json!({}), true);
    assert_eq!(native_xml(&n), source);
    n.send("redo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("plain-circle.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(native_xml(&n), accepted);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn mirror_editor_carbon_copy_keeps_native_identity_and_live_expressions() {
    let mut n = Native::new("carbon-copy-checkpoint");
    n.send("new", json!({"name":"CarbonCopyCheckpoint"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Source","body":"Body"},
        {"op":"geometry","object":"Source","kind":"circle","center":[5,5,0],"radius":2},
        {"op":"constraint","object":"Source","kind":"Radius","args":[0,2]},
        {"op":"constraint_state","object":"Source","index":0,"name":"SourceRadius"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Sketch","body":"Body"}
    ]));
    n.save("baseline.FCStd");
    n.editor.request_sketch("Sketch");
    n.receive("sketch_begin", true);
    let empty = native_xml(&n);
    n.click("Edit tools");
    n.click("Carbon Copy");
    assert!(n.editor.sketch_ui.task.is_some());
    preview(&mut n, true);
    n.click("Apply");
    n.receive("preview_commit", true);
    let accepted = native_xml(&n);
    assert!(!accepted["expressions"].as_array().unwrap().is_empty());
    mirror_start(&mut n);
    preview(&mut n, true);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(native_xml(&n), accepted);
    n.send("undo", json!({}), true);
    assert_eq!(native_xml(&n), empty);
    n.send("redo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    let token = n.editor.sketch_ui.session.clone().unwrap();
    n.send("sketch_finish", json!({"token":token}), true);
    let path = n.save("carbon.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    assert_eq!(native_xml(&n), accepted);
    n.transaction(json!([{"op":"datum","object":"Source","index":0,"value":"4 mm"}]));
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry"][0]["radius"],
        4.
    );
    n.send("undo", json!({}), true);
    assert_eq!(native_xml(&n), accepted);
    n.send("redo", json!({}), true);
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry"][0]["radius"],
        4.
    );
    let path = n.save("carbon-driven.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":path}), true);
    n.transaction(json!([{"op":"datum","object":"Source","index":0,"value":"5 mm"}]));
    assert_eq!(
        object(&n.editor.state, "Sketch")["sketch"]["geometry"][0]["radius"],
        5.
    );
}
