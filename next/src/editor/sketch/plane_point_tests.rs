use super::*;

fn click_screen(n: &mut Native, pos: egui::Pos2) {
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
}

fn select_plane_and_vertex(n: &mut Native) {
    n.frame(vec![]);
    let rect = n.editor.viewport.response.as_ref().unwrap().rect;
    let mut plane_pos = None;
    for x in (-100..=100).step_by(10) {
        for y in (-100..=100).step_by(10) {
            let pos = n
                .editor
                .viewport
                .test_world_screen_point([x as f64, y as f64, 0.]);
            if rect.shrink(8.).contains(pos)
                && n.ctx.layer_id_at(pos) == n.editor.viewport.response.as_ref().map(|r| r.layer_id)
                && n.editor
                    .viewport
                    .test_scene_pick(pos)
                    .is_some_and(|s| s.object == "XY_Plane")
            {
                plane_pos = Some(pos);
                break;
            }
        }
        if plane_pos.is_some() {
            break;
        }
    }
    click_screen(n, plane_pos.expect("visible Top plane pick"));
    assert_eq!(n.editor.viewport.selections.len(), 1);
    assert_eq!(n.editor.viewport.selections[0].object, "XY_Plane");
    let pos = n.editor.viewport.test_world_screen_point([-100., 0., 182.]);
    assert!(rect.shrink(8.).contains(pos));
    assert_eq!(
        n.ctx.layer_id_at(pos),
        n.editor.viewport.response.as_ref().map(|r| r.layer_id)
    );
    let hits = n.editor.viewport.scene_vertices_at(pos);
    assert!(
        hits.iter()
            .any(|v| v.object == "Sketch1" && v.mapped_name.as_deref() == Some("g6v2;SKT"))
    );
    click_screen(n, pos);
    assert_eq!(n.editor.viewport.selections.len(), 2);
    assert_eq!(n.editor.viewport.selections[1].object, "Sketch1");
}

fn start_plane(n: &mut Native) -> String {
    n.click("Plane");
    n.receive("feature_schema", true);
    let draft = n.editor.draft.as_ref().unwrap();
    assert_eq!(draft.parameters["properties"]["MapMode"], "ParallelPlane");
    assert_eq!(draft.parameters["plane_point"]["mapped_name"], "g6v2;SKT");
    let name = draft.parameters["name"].as_str().unwrap().to_owned();
    std::thread::sleep(Duration::from_millis(200));
    n.frame(vec![]);
    n.receive("preview", true);
    name
}

fn assert_plane(n: &Native, name: &str, z: f64) {
    let plane = object(&n.editor.state, name);
    assert_eq!(plane["type"], "PartDesign::Plane");
    assert!(
        plane["test_support_xml"]
            .as_str()
            .unwrap()
            .contains("g6v2;SKT")
    );
    assert_eq!(plane["properties"]["MapMode"], "ParallelPlane");
    assert_eq!(
        plane["properties"]["AttachmentSupport"],
        json!([
        {"link":"XY_Plane","subelements":[""]},
        {"link":"Sketch1","subelements":["Vertex7"]}])
    );
    assert!(
        (plane["properties"]["Placement"]["position"][2]
            .as_f64()
            .unwrap()
            - z)
            .abs()
            < 1e-6
    );
    assert_eq!(
        object(&n.editor.state, "Body2")["body_state"]["tip"],
        "Revolution6"
    );
    assert_eq!(object(&n.editor.state, "Revolution6")["shape"]["solids"], 1);
}

#[test]
#[ignore = "requires the pinned native observer and receiver lower replay input"]
fn receiver_plane_point_clicks_cancel_commit_move_undo_and_reopen() {
    let mut n = Native::new("receiver-plane-point");
    let path = std::env::var("CAD_PLANE_POINT_INPUT").unwrap();
    n.send("open", json!({"path":path}), true);
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    select_plane_and_vertex(&mut n);
    tutorial_key(&mut n, Key::Space, egui::Modifiers::NONE);
    assert!(n.editor.viewport.selections.is_empty());
    select_plane_and_vertex(&mut n);
    let cancelled = start_plane(&mut n);
    assert_plane(&n, &cancelled, 182.);
    n.click("Cancel");
    n.receive("preview_cancel", true);
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == cancelled)
    );
    n.editor.viewport.clear_selection();
    select_plane_and_vertex(&mut n);
    let name = start_plane(&mut n);
    assert_plane(&n, &name, 182.);
    n.click("Apply");
    n.receive("preview_commit", true);
    let id = object(&n.editor.state, &name)["test_native_id"].clone();
    n.transaction(json!([{"op":"datum","object":"Sketch1","index":32,"value":"190 mm"}]));
    assert_plane(&n, &name, 190.);
    n.send("undo", json!({}), true);
    assert_plane(&n, &name, 182.);
    n.send("redo", json!({}), true);
    assert_plane(&n, &name, 190.);
    let saved = n.save("receiver-plane.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":saved}), true);
    assert_plane(&n, &name, 190.);
    assert_eq!(object(&n.editor.state, &name)["test_native_id"], id);
    assert_eq!(
        object(&n.editor.state, "Sketch1")["sketch"]["native_xml"]["element_map"]["Vertex7"],
        "g6v2;SKT"
    );
}

#[test]
#[ignore = "requires the pinned native observer and receiver lower replay input"]
fn receiver_plane_point_rejects_stale_vertex_without_changes() {
    let mut n = Native::new("receiver-plane-stale");
    n.send(
        "open",
        json!({"path":std::env::var("CAD_PLANE_POINT_INPUT").unwrap()}),
        true,
    );
    let before = n.editor.state["objects"].clone();
    n.send(
        "preview",
        json!({"token":"stale-point","label":"stale point","edits":[
            {"op":"validate_plane_point","object":"Sketch1","body":"Body2","plane":"XY_Plane",
             "subelement":"Vertex7","mapped_name":"stale"},
            {"op":"add_object","type":"PartDesign::Plane","name":"Rejected","body":"Body2"}
        ]}),
        false,
    );
    assert_eq!(n.editor.state["objects"], before);
    assert!(n.editor.state["preview"].is_null());
    n.transaction(json!([{"op":"add_object","type":"PartDesign::Body","name":"OtherBody"}]));
    let before = n.editor.state["objects"].clone();
    n.send(
        "preview",
        json!({"token":"wrong-owner","label":"wrong owner","edits":[
            {"op":"validate_plane_point","object":"Sketch1","body":"OtherBody","plane":"XY_Plane",
             "subelement":"Vertex7","mapped_name":"g6v2;SKT"},
            {"op":"add_object","type":"PartDesign::Plane","name":"Rejected","body":"OtherBody"}
        ]}),
        false,
    );
    assert_eq!(n.editor.state["objects"], before);
    assert!(n.editor.state["preview"].is_null());
    n.click("Document");
    n.click("Workspace units...");
    n.click("Plane");
    assert!(n.editor.draft.is_none() && !n.editor.busy);
    n.click("Cancel workspace units");
}

fn record_denial_phase(n: &Native, ledger: &mut Vec<Value>, phase: &str) {
    ledger.push(json!({"phase":phase,"state":n.editor.state,"error":n.editor.error}));
    fs::write(
        n.folder.join("phase-ledger.json"),
        serde_json::to_vec_pretty(ledger).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires the pinned native observer and receiver lower replay input"]
fn receiver_plane_point_denials_after_explicit_display_baseline() {
    let mut n = Native::new("receiver-plane-post-display-denials");
    let mut ledger = Vec::new();
    n.send(
        "open",
        json!({"path":std::env::var("CAD_PLANE_POINT_INPUT").unwrap()}),
        true,
    );
    record_denial_phase(&n, &mut ledger, "cold-open");
    n.send("inspect", json!({}), true);
    record_denial_phase(&n, &mut ledger, "stale-before-after-display-inspect");
    let before_stale = n.editor.state["objects"].clone();
    n.send(
        "preview",
        json!({"token":"stale-after-inspect","label":"stale after inspect","edits":[
            {"op":"validate_plane_point","object":"Sketch1","body":"Body2","plane":"XY_Plane",
             "subelement":"Vertex7","mapped_name":"stale"},
            {"op":"add_object","type":"PartDesign::Plane","name":"Rejected","body":"Body2"}
        ]}),
        false,
    );
    record_denial_phase(&n, &mut ledger, "stale-after-denial");
    assert!(
        n.events.last().unwrap()["error"]
            .as_str()
            .unwrap_or_default()
            .contains("stale_reference")
    );
    assert!(
        n.editor.state["objects"] == before_stale,
        "full native objects changed; see phase ledger"
    );
    assert!(n.editor.state["preview"].is_null());
    n.transaction(json!([{"op":"add_object","type":"PartDesign::Body","name":"OtherBody"}]));
    record_denial_phase(&n, &mut ledger, "wrong-owner-fixture");
    n.send("inspect", json!({}), true);
    record_denial_phase(&n, &mut ledger, "wrong-owner-before-after-display-inspect");
    let before_owner = n.editor.state["objects"].clone();
    n.send(
        "preview",
        json!({"token":"wrong-owner-after-inspect","label":"wrong owner after inspect","edits":[
            {"op":"validate_plane_point","object":"Sketch1","body":"OtherBody","plane":"XY_Plane",
             "subelement":"Vertex7","mapped_name":"g6v2;SKT"},
            {"op":"add_object","type":"PartDesign::Plane","name":"Rejected","body":"OtherBody"}
        ]}),
        false,
    );
    record_denial_phase(&n, &mut ledger, "wrong-owner-after-denial");
    assert!(
        n.events.last().unwrap()["error"]
            .as_str()
            .unwrap_or_default()
            .contains("invalid_geometry")
    );
    assert!(
        n.editor.state["objects"] == before_owner,
        "full native objects changed; see phase ledger"
    );
    assert!(n.editor.state["preview"].is_null());
    n.click("Document");
    n.click("Workspace units...");
    record_denial_phase(&n, &mut ledger, "units-owner-open");
    n.click("Plane");
    record_denial_phase(&n, &mut ledger, "plane-click-during-units-owner");
    assert!(n.editor.draft.is_none() && !n.editor.busy);
    n.click("Cancel workspace units");
    record_denial_phase(&n, &mut ledger, "units-owner-cancelled");
}
