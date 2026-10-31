//! Actual pointer actions over the pierced-slot native feasibility fixture.
use super::*;

fn start(n: &mut Native) {
    n.editor.request_sketch("ReceiverSlot");
    n.receive("sketch_begin", true);
    n.frame(vec![]);
    n.click("Extrude");
    n.receive("sketch_finish", true);
    assert_eq!(n.editor.draft.as_ref().unwrap().id, "native.slot_extrude");
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    let draft = n.editor.draft.as_ref().unwrap();
    assert_eq!(draft.parameters["mode"], "Blind");
    assert!(draft.valid);
    let pad = draft.parameters["name"].as_str().unwrap();
    let body = draft.parameters["new_body"].as_str().unwrap();
    assert_eq!(object(&n.editor.state, pad)["properties"]["Type"], "Length");
    assert_eq!(object(&n.editor.state, pad)["shape"]["solids"], 1);
    assert_eq!(object(&n.editor.state, body)["body_state"]["tip"], pad);
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    n.frame(vec![]);
}

fn select_vertex_mode(n: &mut Native) {
    if n.editor.draft.as_ref().unwrap().parameters["mode"] == "Blind" {
        n.click("Blind");
        n.click("Up to vertex");
    }
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["mode"],
        "Up to vertex"
    );
    assert!(!n.editor.draft.as_ref().unwrap().valid);
}

fn vertex_position(n: &Native) -> egui::Pos2 {
    let master = object(&n.editor.state, "Sketch1");
    let line = master["sketch"]["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["persistent_id"] == 6)
        .unwrap();
    let p = line["start"].as_array().unwrap();
    let world = [p[0].as_f64().unwrap(), 0., p[1].as_f64().unwrap()];
    n.editor.viewport.test_world_screen_point(world)
}

fn choose_vertex(n: &mut Native) {
    select_vertex_mode(n);
    let pos = vertex_position(n);
    let hits = n.editor.viewport.scene_vertices_at(pos);
    assert!(
        hits.iter()
            .any(|h| h.object == "Sketch1" && h.mapped_name.as_deref() == Some("g5v2;SKT"))
    );
    assert_eq!(
        n.ctx.layer_id_at(pos),
        n.editor.viewport.response.as_ref().map(|r| r.layer_id)
    );
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["vertex_map"],
        "g5v2;SKT"
    );
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
}

fn check_native(n: &Native, pad: &str, body: &str, binder: &str, plane: &str) {
    let feature = object(&n.editor.state, pad);
    for name in [pad, binder, plane] {
        let tags = &object(&n.editor.state, name)["test_owner_tags"];
        let uid = tags["document_uid"].as_str().unwrap();
        assert_eq!(tags["tags"]["VertexExtrusionOwner"]["value"], pad);
        assert_eq!(
            tags["tags"]["VertexExtrusionOwnerId"]["value"],
            feature["native_id"]
        );
        assert_eq!(tags["tags"]["VertexExtrusionDocumentUid"]["value"], uid);
        for key in [
            "VertexExtrusionOwner",
            "VertexExtrusionOwnerId",
            "VertexExtrusionDocumentUid",
        ] {
            let status = tags["tags"][key]["status"].as_array().unwrap();
            assert!(status.contains(&json!("ReadOnly")) && status.contains(&json!("Hidden")));
            assert!(
                tags["tags"][key]["xml"]
                    .as_str()
                    .unwrap()
                    .contains("Content")
            );
        }
        assert!(
            tags["tags"]["VertexExtrusionDocumentUid"]["xml"]
                .as_str()
                .unwrap()
                .contains(uid)
        );
    }
    let lower = object(&n.editor.state, "Sketch1")["sketch"]["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["persistent_id"] == 6)
        .unwrap()["start"][1]
        .as_f64()
        .unwrap();
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    assert_eq!(object(&n.editor.state, body)["body_state"]["tip"], pad);
    assert_eq!(feature["shape"]["solids"], 1);
    assert_eq!(feature["shape"]["valid"], true);
    assert!((feature["shape"]["bounds"]["min"][2].as_f64().unwrap() - lower).abs() < 1e-6);
    assert!(
        object(&n.editor.state, plane)["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g5v2;SKT")
    );
    let xml = object(&n.editor.state, binder)["test_support_xml"]
        .as_str()
        .unwrap();
    for mapping in ["g2;SKT", "g5;SKT", "g3;SKT", "g4;SKT"] {
        assert!(xml.contains(mapping));
    }
    assert_eq!(feature["test_vertex_csg"]["common"], 0.0);
    assert_eq!(
        feature["test_vertex_csg"]["cut"],
        feature["test_vertex_csg"]["pad"]
    );
}

fn edit_right_vertex(n: &mut Native, pad: &str) {
    n.editor.edit_native_properties(pad);
    let draft = n.editor.draft.as_ref().unwrap();
    assert_eq!(draft.id, "native.vertex_edit");
    assert_eq!(draft.parameters["vertex_map"], "g5v2;SKT");
    let master = object(&n.editor.state, "Sketch1");
    let line = master["sketch"]["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["persistent_id"] == 14)
        .unwrap();
    let p = line["start"].as_array().unwrap();
    let pos = n.editor.viewport.test_world_screen_point([
        p[0].as_f64().unwrap(),
        0.,
        p[1].as_f64().unwrap(),
    ]);
    n.frame(vec![]);
    assert!(
        n.editor
            .viewport
            .scene_vertices_at(pos)
            .iter()
            .any(|v| v.object == "Sketch1" && v.mapped_name.as_deref() == Some("g13v2;SKT"))
    );
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters["vertex_map"],
        "g13v2;SKT"
    );
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn vertex_new_editor_clicks_history_and_reopen() {
    let mut n = Native::new("vertex-new-editor");
    n.send(
        "open",
        json!({"path":std::env::var("CAD_VERTEX_INPUT").unwrap()}),
        true,
    );
    // The stock feasibility file contains the proven result. Remove just that
    // result and its helpers to leave its original pierced source sketch.
    n.transaction(json!([
        {"op":"remove_object","object":"ReceiverExtrude"},
        {"op":"remove_object","object":"ReceiverVertexPlane"},
        {"op":"remove_object","object":"ReceiverProfileBinder"},
        {"op":"remove_object","object":"ReceiverNewBody"}
    ]));
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    start(&mut n);
    choose_vertex(&mut n);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["internal_vertex_helper"].is_string())
    );

    start(&mut n);
    choose_vertex(&mut n);
    let p = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pad = p["name"].as_str().unwrap().to_owned();
    let body = p["new_body"].as_str().unwrap().to_owned();
    let binder = p["binder"].as_str().unwrap().to_owned();
    let plane = p["plane"].as_str().unwrap().to_owned();
    assert_eq!(object(&n.editor.state, &pad)["shape"]["solids"], 1);
    assert_eq!(
        object(&n.editor.state, &binder)["internal_vertex_helper"],
        pad
    );
    assert_eq!(
        object(&n.editor.state, &plane)["internal_vertex_helper"],
        pad
    );
    assert_eq!(
        object(&n.editor.state, &binder)["properties"]["Visibility"],
        false
    );
    assert_eq!(
        object(&n.editor.state, &plane)["properties"]["Visibility"],
        false
    );
    let rendered = n.frame(vec![]);
    assert!(!rendered.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == binder || text.galley.text() == plane)));
    check_native(&n, &pad, &body, &binder, &plane);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    assert_eq!(object(&n.editor.state, &body)["body_state"]["tip"], pad);
    let baseline = object(&n.editor.state, &pad)["shape"]["volume"]
        .as_f64()
        .unwrap();
    let angle = object(&n.editor.state, "Sketch1")["sketch"]["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["type"] == "Angle" && (c["value"].as_f64().unwrap() - 32f64.to_radians()).abs() < 1e-8
        })
        .unwrap()["index"]
        .as_u64()
        .unwrap();
    n.transaction(json!([{"op":"datum","object":"Sketch1","index":angle,"value":"34 deg"}]));
    check_native(&n, &pad, &body, &binder, &plane);
    let moved = object(&n.editor.state, &pad)["shape"]["volume"]
        .as_f64()
        .unwrap();
    assert!((moved - baseline).abs() > 1e-4);
    n.send("undo", json!({}), true);
    check_native(&n, &pad, &body, &binder, &plane);
    assert!(
        (object(&n.editor.state, &pad)["shape"]["volume"]
            .as_f64()
            .unwrap()
            - baseline)
            .abs()
            < 1e-4
    );
    n.send("redo", json!({}), true);
    check_native(&n, &pad, &body, &binder, &plane);
    assert!(
        (object(&n.editor.state, &pad)["shape"]["volume"]
            .as_f64()
            .unwrap()
            - moved)
            .abs()
            < 1e-4
    );
    let saved = n.save("vertex-new.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":saved}), true);
    check_native(&n, &pad, &body, &binder, &plane);
    let pad_id = object(&n.editor.state, &pad)["native_id"]
        .as_i64()
        .unwrap()
        .to_string();
    for name in [&pad, &binder, &plane] {
        let entry = object(&n.editor.state, name);
        let saved_tags = &entry["test_saved_tag_xml"];
        let uid = entry["test_owner_tags"]["document_uid"].as_str().unwrap();
        assert!(
            saved_tags["VertexExtrusionOwner"]
                .as_str()
                .unwrap()
                .contains(&pad)
        );
        assert!(
            saved_tags["VertexExtrusionOwnerId"]
                .as_str()
                .unwrap()
                .contains(&pad_id)
        );
        assert!(
            saved_tags["VertexExtrusionDocumentUid"]
                .as_str()
                .unwrap()
                .contains(uid)
        );
    }
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    assert_eq!(object(&n.editor.state, &body)["body_state"]["tip"], pad);
    assert!(
        (object(&n.editor.state, &pad)["shape"]["volume"]
            .as_f64()
            .unwrap()
            - moved)
            .abs()
            < 1e-4
    );
    edit_right_vertex(&mut n, &pad);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert!(
        object(&n.editor.state, &plane)["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g5v2;SKT")
    );
    edit_right_vertex(&mut n, &pad);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert!(
        object(&n.editor.state, &plane)["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g13v2;SKT")
    );
    n.send("undo", json!({}), true);
    assert!(
        object(&n.editor.state, &plane)["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g5v2;SKT")
    );
    n.send("redo", json!({}), true);
    assert!(
        object(&n.editor.state, &plane)["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g13v2;SKT")
    );
    for (name, property, value) in [
        (&pad, "VertexExtrusionOwner", json!("Spoofed")),
        (&binder, "VertexExtrusionOwnerId", json!(999999)),
        (&plane, "VertexExtrusionDocumentUid", json!("Spoofed")),
    ] {
        let before = n.editor.state["objects"].clone();
        n.send(
            "transaction",
            json!({"label":"Reject ownership tag edit","edits":[
            {"op":"set","object":name,"property":property,"value":value}]}),
            false,
        );
        assert_eq!(
            n.editor.state["objects"], before,
            "raw state changed after protected-tag edit"
        );
    }
    n.transaction(json!([
        {"op":"add_object","type":"Part::Box","name":"Consumer"},
        {"op":"expression","object":"Consumer","property":"Length","expression":format!("{pad}.Length + 1 mm")}
    ]));
    let before = n.editor.state["objects"].clone();
    n.send(
        "transaction",
        json!({"label":"Reject dependent deletion","edits":[{"op":"remove_object","object":pad}]}),
        false,
    );
    assert_eq!(
        n.editor.state["objects"], before,
        "raw dependent-delete state changed"
    );
    n.transaction(json!([{"op":"remove_object","object":"Consumer"}]));
    n.transaction(json!([{"op":"remove_object","object":pad}]));
    assert!(!objects(&n.editor.state).iter().any(|o| o["name"] == pad
        || o["name"] == body
        || o["name"] == binder
        || o["name"] == plane));
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    n.send("undo", json!({}), true);
    assert_eq!(object(&n.editor.state, &body)["body_state"]["tip"], pad);
    assert_eq!(
        object(&n.editor.state, &binder)["internal_vertex_helper"],
        pad
    );
    assert_eq!(
        object(&n.editor.state, &plane)["internal_vertex_helper"],
        pad
    );
    n.send("redo", json!({}), true);
    assert!(!objects(&n.editor.state).iter().any(|o| o["name"] == body));
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn vertex_new_rejects_invalid_native_targets_without_objects() {
    let mut n = Native::new("vertex-new-invalid");
    n.send(
        "open",
        json!({"path":std::env::var("CAD_VERTEX_INPUT").unwrap()}),
        true,
    );
    n.transaction(json!([
        {"op":"remove_object","object":"ReceiverExtrude"},
        {"op":"remove_object","object":"ReceiverVertexPlane"},
        {"op":"remove_object","object":"ReceiverProfileBinder"},
        {"op":"remove_object","object":"ReceiverNewBody"}
    ]));
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    start(&mut n);
    select_vertex_mode(&mut n);
    n.click("Up to vertex");
    assert!(egui::Popup::is_any_open(&n.ctx));
    let before_popup = n.editor.state.clone();
    let parameters_popup = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pos = vertex_position(&n);
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    n.events.push(
        json!({"phase":"popup-owner-vertex-click", "position":[pos.x,pos.y],
        "state":n.editor.state, "draft":n.editor.draft.as_ref().map(|d| d.parameters.clone())}),
    );
    n.record();
    assert_eq!(n.editor.state, before_popup);
    assert_eq!(
        n.editor.draft.as_ref().unwrap().parameters,
        parameters_popup
    );
    assert!(n.raw.try_recv().is_err());
    if egui::Popup::is_any_open(&n.ctx) {
        tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    }
    tutorial_key(&mut n, Key::F1, egui::Modifiers::NONE);
    assert!(workflow_guide::active(&n.ctx));
    let before = n.editor.state.clone();
    let parameters = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pos = vertex_position(&n);
    for pressed in [true, false] {
        n.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    n.events.push(
        json!({"phase":"help-owner-vertex-click", "position":[pos.x,pos.y],
        "help_active":workflow_guide::active(&n.ctx), "state":n.editor.state,
        "draft":n.editor.draft.as_ref().map(|d| d.parameters.clone())}),
    );
    n.record();
    assert!(workflow_guide::active(&n.ctx));
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.draft.as_ref().unwrap().parameters, parameters);
    assert!(n.raw.try_recv().is_err());
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert!(!workflow_guide::active(&n.ctx));
    choose_vertex(&mut n);
    let valid = n.editor.draft.as_ref().unwrap().parameters.clone();
    let token = n.editor.draft.as_ref().unwrap().token.clone();
    let accepted = n.editor.state["objects"].clone();
    let mut bad_replacement = valid.clone();
    bad_replacement["vertex_map"] = json!("g999v1;SKT");
    n.send(
        "preview",
        json!({"label":"invalid replacement","token":token,"edits":[bad_replacement]}),
        false,
    );
    assert_eq!(
        n.editor.state["objects"], accepted,
        "raw accepted preview changed after rejection"
    );
    assert!(!n.editor.state["preview"].is_null());
    n.click("Cancel");
    n.receive("preview_cancel", true);
    for (label, field, value) in [
        ("stale-map", "vertex_map", json!("g999v1;SKT")),
        ("stale-id", "target_id", json!(999999999)),
        ("wrong-type", "vertex", json!("Edge6")),
        ("source-cycle", "target", valid["source"].clone()),
        ("foreign-body", "body_id", json!(999999999)),
    ] {
        let mut edit = valid.clone();
        edit[field] = value;
        let before = n.editor.state["objects"].clone();
        n.send(
            "preview",
            json!({"label":label,"token":label,"edits":[edit]}),
            false,
        );
        assert_eq!(
            n.editor.state["objects"], before,
            "raw state changed for {label}"
        );
        assert!(n.editor.state["preview"].is_null());
    }
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"ForeignBody"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"ForeignSketch","body":"ForeignBody"},
        {"op":"geometry","object":"ForeignSketch","kind":"line","start":[0,0,0],"end":[0,5,0]},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"CycleSketch","body":"Body2"},
        {"op":"geometry","object":"CycleSketch","kind":"line","start":[0,0,0],"end":[0,5,0]},
        {"op":"external_geometry","object":"CycleSketch","source":"ReceiverSlot","subelement":"Edge2"}
    ]));
    for (label, target) in [
        ("foreign-root", "ForeignSketch"),
        ("real-cycle", "CycleSketch"),
    ] {
        let candidate = object(&n.editor.state, target);
        let mut edit = valid.clone();
        edit["target"] = json!(target);
        edit["target_id"] = candidate["native_id"].clone();
        edit["vertex"] = json!("Vertex1");
        edit["vertex_map"] = candidate["sketch"]["native_xml"]["element_map"]["Vertex1"].clone();
        let before = n.editor.state["objects"].clone();
        n.send(
            "preview",
            json!({"label":label,"token":label,"edits":[edit]}),
            false,
        );
        assert_eq!(
            n.editor.state["objects"], before,
            "raw state changed for {label}"
        );
        assert!(n.editor.state["preview"].is_null());
    }
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn five_curve_slot_routes_to_same_new_task() {
    let mut n = Native::new("vertex-five-curve");
    n.send(
        "open",
        json!({"path":std::env::var("CAD_VERTEX_INPUT").unwrap()}),
        true,
    );
    n.transaction(json!([
        {"op":"remove_object","object":"ReceiverExtrude"},
        {"op":"remove_object","object":"ReceiverVertexPlane"},
        {"op":"remove_object","object":"ReceiverProfileBinder"},
        {"op":"remove_object","object":"ReceiverNewBody"},
        {"op":"delete_geometry","object":"ReceiverSlot","index":6},
        {"op":"delete_geometry","object":"ReceiverSlot","index":5}
    ]));
    assert_eq!(
        object(&n.editor.state, "ReceiverSlot")["sketch"]["geometry_count"],
        5
    );
    start(&mut n);
    choose_vertex(&mut n);
    let p = n.editor.draft.as_ref().unwrap().parameters.clone();
    check_native(
        &n,
        p["name"].as_str().unwrap(),
        p["new_body"].as_str().unwrap(),
        p["binder"].as_str().unwrap(),
        p["plane"].as_str().unwrap(),
    );
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn vertex_preview_snapshot_stays_exact_after_display_including_plane_references() {
    let mut n = Native::new("vertex-display-snapshot");
    n.send(
        "open",
        json!({"path":std::env::var("CAD_VERTEX_INPUT").unwrap()}),
        true,
    );
    n.transaction(json!([
        {"op":"remove_object","object":"ReceiverExtrude"},
        {"op":"remove_object","object":"ReceiverVertexPlane"},
        {"op":"remove_object","object":"ReceiverProfileBinder"},
        {"op":"remove_object","object":"ReceiverNewBody"}
    ]));
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    start(&mut n);
    choose_vertex(&mut n);
    let params = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pad = params["name"].as_str().unwrap();
    let binder = params["binder"].as_str().unwrap();
    let plane = params["plane"].as_str().unwrap();
    let before = n.editor.state.clone();
    assert!(before["preview"].is_object());
    for name in [pad, binder, plane] {
        assert!(object(&before, name)["native_id"].is_number());
    }
    let planes_before: Vec<_> = objects(&before).iter().filter(|o| o["type"] == "PartDesign::Plane")
        .map(|o| json!({"name":o["name"],"native_id":o["native_id"],
            "reference_display":o["reference_display"],"support":o["properties"]["AttachmentSupport"],
            "support_xml":o["test_support_xml"]})).collect();
    assert!(planes_before.iter().any(|o| o["name"] == plane));
    n.send("inspect", json!({}), true);
    let after = n.editor.state.clone();
    let planes_after: Vec<_> = objects(&after).iter().filter(|o| o["type"] == "PartDesign::Plane")
        .map(|o| json!({"name":o["name"],"native_id":o["native_id"],
            "reference_display":o["reference_display"],"support":o["properties"]["AttachmentSupport"],
            "support_xml":o["test_support_xml"]})).collect();
    fs::write(
        n.folder.join("before-display-snapshot.json"),
        serde_json::to_vec_pretty(&before).unwrap(),
    )
    .unwrap();
    fs::write(
        n.folder.join("after-display-snapshot.json"),
        serde_json::to_vec_pretty(&after).unwrap(),
    )
    .unwrap();
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(
        planes_before, planes_after,
        "native plane references changed after display"
    );
    assert_eq!(
        before["objects"], after["objects"],
        "raw object snapshots changed after display"
    );
}

fn face_support_fixture(n: &mut Native) {
    n.send(
        "open",
        json!({"path":std::env::var("CAD_VERTEX_INPUT").unwrap()}),
        true,
    );
    n.transaction(json!([
        {"op":"remove_object","object":"ReceiverExtrude"},
        {"op":"remove_object","object":"ReceiverVertexPlane"},
        {"op":"remove_object","object":"ReceiverProfileBinder"},
        {"op":"remove_object","object":"ReceiverNewBody"},
        {"op":"set","object":"ReceiverSlot","property":"AttachmentSupport",
         "value":[{"link":"ReceiverSlotPlane","subelements":["Face1"]}]}
    ]));
    assert_eq!(
        object(&n.editor.state, "ReceiverSlot")["properties"]["AttachmentSupport"],
        json!([{"link":"ReceiverSlotPlane","subelements":["Face1"]}])
    );
    assert_eq!(
        object(&n.editor.state, "ReceiverSlot")["properties"]["MapMode"],
        "FlatFace"
    );
    n.editor.viewport.iso();
    n.editor.viewport.fit();
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn vertex_face_support_creates_and_recognizes_native_graph() {
    let mut n = Native::new("vertex-face-support");
    face_support_fixture(&mut n);
    start(&mut n);
    choose_vertex(&mut n);
    let p = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pad = p["name"].as_str().unwrap().to_owned();
    let body = p["new_body"].as_str().unwrap().to_owned();
    let binder = p["binder"].as_str().unwrap().to_owned();
    let plane = p["plane"].as_str().unwrap().to_owned();
    check_native(&n, &pad, &body, &binder, &plane);
    n.click("Apply");
    n.receive("preview_commit", true);
    assert_eq!(
        object(&n.editor.state, "ReceiverSlot")["properties"]["AttachmentSupport"],
        json!([{"link":"ReceiverSlotPlane","subelements":["Face1"]}])
    );
    assert_eq!(
        object(&n.editor.state, &binder)["internal_vertex_helper"],
        pad
    );
    assert_eq!(
        object(&n.editor.state, &plane)["internal_vertex_helper"],
        pad
    );
    assert_eq!(
        object(&n.editor.state, &pad)["vertex_extrusion"]["source"],
        "ReceiverSlot"
    );
    check_native(&n, &pad, &body, &binder, &plane);
}

#[test]
#[ignore = "requires pinned stock paired-slot fixture and native Editor observer"]
fn vertex_invalid_plane_support_denies_create_and_owned_graph() {
    let mut n = Native::new("vertex-invalid-plane-support");
    face_support_fixture(&mut n);
    start(&mut n);
    choose_vertex(&mut n);
    let p = n.editor.draft.as_ref().unwrap().parameters.clone();
    let pad = p["name"].as_str().unwrap().to_owned();
    let binder = p["binder"].as_str().unwrap().to_owned();
    n.click("Apply");
    n.receive("preview_commit", true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"OtherSupportBody"},
        {"op":"add_object","type":"PartDesign::Plane","name":"OtherSupportPlane","body":"OtherSupportBody"}
    ]));
    for (label, link, subs) in [
        ("missing-face", "ReceiverSlotPlane", json!(["Face2"])),
        (
            "multiple-faces",
            "ReceiverSlotPlane",
            json!(["Face1", "Face2"]),
        ),
        ("wrong-type", "XY_Plane", json!([""])),
        ("foreign-body", "OtherSupportPlane", json!(["Face1"])),
    ] {
        n.transaction(
            json!([{"op":"set","object":"ReceiverSlot","property":"AttachmentSupport",
            "value":[{"link":link,"subelements":subs}]}]),
        );
        assert!(
            object(&n.editor.state, &pad)["vertex_extrusion"].is_null(),
            "{label}"
        );
        assert!(
            object(&n.editor.state, &binder)["internal_vertex_helper"].is_null(),
            "{label}"
        );
        let before = n.editor.state["objects"].clone();
        n.send(
            "preview",
            json!({"label":label,"token":label,"edits":[p.clone()]}),
            false,
        );
        assert_eq!(
            n.editor.state["objects"], before,
            "raw denial changed objects: {label}"
        );
        n.transaction(
            json!([{"op":"set","object":"ReceiverSlot","property":"AttachmentSupport",
            "value":[{"link":"ReceiverSlotPlane","subelements":["Face1"]}]}]),
        );
        assert_eq!(
            object(&n.editor.state, &binder)["internal_vertex_helper"],
            pad,
            "{label}"
        );
    }
}
