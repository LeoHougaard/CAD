//! Bounded Pierce source-order click route against actual Editor and native journal.
use super::*;

fn click_at(n: &mut Native, pos: egui::Pos2) {
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
fn settle(n: &mut Native) {
    for _ in 0..3 {
        n.frame(vec![]);
    }
}
fn point(n: &Native, index: usize) -> [f64; 3] {
    let p = &object(&n.editor.state, "Slot")["sketch"]["geometry"][index]["position"];
    [
        p[0].as_f64().unwrap(),
        p[1].as_f64().unwrap(),
        p[2].as_f64().unwrap(),
    ]
}
fn select_edge_and_point(n: &mut Native, x: f64, point_index: usize, edge_name: &str) {
    settle(n);
    let pos = n.editor.viewport.test_world_screen_point([x, 0., 20.]);
    assert!(n.editor.viewport.sketch_hit(pos).is_none());
    let hits = n.editor.viewport.scene_edges_at(pos);
    assert_eq!(hits.len(), 1, "scene edge must be unambiguous: {hits:?}");
    assert_eq!(hits[0].selection.object, "Master");
    assert_eq!(hits[0].selection.subelement, edge_name);
    click_at(n, pos);
    assert_eq!(
        n.editor
            .viewport
            .pierce_edge
            .as_ref()
            .unwrap()
            .selection
            .subelement,
        edge_name
    );
    let pos = n.editor.viewport.sketch_screen_point(point(n, point_index));
    assert_eq!(
        n.editor.viewport.sketch_hit(pos),
        Some(SketchElement {
            index: point_index as i32,
            point: 1
        })
    );
    click_at(n, pos);
    assert_eq!(
        n.editor.viewport.sketch_selection,
        vec![SketchElement {
            index: point_index as i32,
            point: 1
        }]
    );
    assert_eq!(
        n.editor
            .viewport
            .pierce_edge
            .as_ref()
            .unwrap()
            .selection
            .subelement,
        edge_name
    );
}
fn click_menu_pierce(n: &mut Native) {
    n.click("Constrain");
    let frame = n.frame(vec![]);
    let matches: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Pierce" => {
                Some(t.galley.rect.translate(t.pos.to_vec2()).center())
            }
            _ => None,
        })
        .collect();
    assert!(
        matches.len() >= 2,
        "toolbar and opened menu Pierce must both be visible"
    );
    click_at(n, *matches.last().unwrap());
}
fn external<'a>(state: &'a Value, edge: &str) -> &'a Value {
    let found: Vec<_> = object(state, "Slot")["sketch"]["externals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["source"]["link"] == "Master" && e["source"]["subelements"] == json!([edge]))
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["intersection"], true);
    assert_eq!(found[0]["projection"], false);
    assert_eq!(found[0]["defining"], false);
    assert_eq!(found[0]["missing"], false);
    assert_eq!(found[0]["frozen"], false);
    assert_eq!(found[0]["geometry"]["kind"], "point");
    found[0]
}
fn value(n: &Native, edge: &str) -> f64 {
    external(&n.editor.state, edge)["geometry"]["position"][0]
        .as_f64()
        .unwrap()
}

#[test]
#[ignore = "requires pinned native observer and explicit synthetic Pierce input"]
fn actual_pierce_clicks_master_edit_undo_and_reopen() {
    let mut n = Native::new("pierce-click-lifecycle");
    let path = std::env::var("CAD_PIERCE_INPUT").unwrap();
    n.send("open", json!({"path":path}), true);
    // This stock feasibility fixture includes an upper Pad, which the source
    // has not created at this point. Remove only that fixture artifact.
    let master_ids =
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["persistent_ids"].clone();
    let master_map =
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["element_map"].clone();
    let plane_support =
        object(&n.editor.state, "TopPlane")["properties"]["AttachmentSupport"].clone();
    let slot_support = object(&n.editor.state, "Slot")["properties"]["AttachmentSupport"].clone();
    assert_eq!(object(&n.editor.state, "Result")["type"], "PartDesign::Pad");
    n.transaction(json!([
        {"op":"remove_object","object":"Result"},
        {"op":"delete_geometry","object":"Slot","index":6},
        {"op":"delete_geometry","object":"Slot","index":5}
    ]));
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == "Result")
    );
    assert_eq!(
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["persistent_ids"],
        master_ids
    );
    assert_eq!(
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["element_map"],
        master_map
    );
    assert_eq!(
        object(&n.editor.state, "TopPlane")["properties"]["AttachmentSupport"],
        plane_support
    );
    assert_eq!(
        object(&n.editor.state, "Slot")["properties"]["AttachmentSupport"],
        slot_support
    );
    assert_eq!(
        object(&n.editor.state, "Master")["body"],
        object(&n.editor.state, "Slot")["body"]
    );
    assert_eq!(
        object(&n.editor.state, "Master")["effective_visibility"],
        true
    );
    n.editor.request_sketch("Slot");
    n.receive("sketch_begin", true);
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    settle(&mut n);
    n.click("Point");
    for arc in [1usize, 2] {
        let middle = &object(&n.editor.state, "Slot")["sketch"]["geometry"][arc]["mid"];
        let local = [
            middle[0].as_f64().unwrap(),
            middle[1].as_f64().unwrap(),
            middle[2].as_f64().unwrap(),
        ];
        let pos = n.editor.viewport.sketch_screen_point(local);
        assert_eq!(
            n.editor.viewport.sketch_hit(pos),
            Some(SketchElement {
                index: arc as i32,
                point: 0
            })
        );
        click_at(&mut n, pos);
        n.receive("transaction", true);
        settle(&mut n);
    }
    tutorial_key(&mut n, egui::Key::Escape, egui::Modifiers::NONE);
    let baseline = object(&n.editor.state, "Slot")["sketch"].clone();
    for (index, arc) in [(5, 1), (6, 2)] {
        assert!(
            baseline["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["type"] == "PointOnObject"
                    && c["first"] == index
                    && c["first_pos"] == 1
                    && c["second"] == arc)
        );
    }
    select_edge_and_point(&mut n, -100., 5, "Edge1");
    click_menu_pierce(&mut n);
    n.receive("transaction", true);
    settle(&mut n);
    let reference1 = external(&n.editor.state, "Edge1")["persistent_reference"].clone();
    select_edge_and_point(&mut n, 100., 6, "Edge2");
    n.click("Pierce");
    n.receive("transaction", true);
    settle(&mut n);
    let reference2 = external(&n.editor.state, "Edge2")["persistent_reference"].clone();
    assert_eq!(
        object(&n.editor.state, "Slot")["sketch"]["degrees_of_freedom"],
        0
    );
    n.send("undo", json!({}), true);
    assert_eq!(
        object(&n.editor.state, "Slot")["sketch"]["externals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    n.send("undo", json!({}), true);
    assert_eq!(object(&n.editor.state, "Slot")["sketch"], baseline);
    n.send("redo", json!({}), true);
    n.send("redo", json!({}), true);
    assert_eq!(
        external(&n.editor.state, "Edge1")["persistent_reference"],
        reference1
    );
    assert_eq!(
        external(&n.editor.state, "Edge2")["persistent_reference"],
        reference2
    );
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    n.transaction(json!([{"op":"datum","object":"Master","index":3,"value":"-110 mm"}]));
    assert!((value(&n, "Edge1") + 110.).abs() < 1e-6);
    n.send("undo", json!({}), true);
    assert!((value(&n, "Edge1") + 100.).abs() < 1e-6);
    n.send("redo", json!({}), true);
    assert!((value(&n, "Edge1") + 110.).abs() < 1e-6);
    let saved = n.save("pierce-result.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":saved}), true);
    assert!((value(&n, "Edge1") + 110.).abs() < 1e-6);
    assert_eq!(
        external(&n.editor.state, "Edge1")["persistent_reference"],
        reference1
    );
    assert_eq!(
        external(&n.editor.state, "Edge2")["persistent_reference"],
        reference2
    );
}

#[test]
#[ignore = "requires pinned native observer and explicit synthetic Pierce input"]
fn stale_pierce_reference_rejects_without_partial_sketch_action() {
    let mut n = Native::new("pierce-stale");
    let path = std::env::var("CAD_PIERCE_INPUT").unwrap();
    n.send("open", json!({"path":path}), true);
    n.editor.request_sketch("Slot");
    n.receive("sketch_begin", true);
    let before = object(&n.editor.state, "Slot")["sketch"].clone();
    let id = before["geometry"][5]["persistent_id"].as_i64().unwrap();
    let session = n.editor.sketch_ui.session.clone().unwrap();
    let document = n.editor.state["document"].clone();
    n.send("transaction",json!({"label":"Pierce stale", "edits":[{"op":"pierce","object":"Slot","source":"Master","subelement":"Edge1","mapped_name":"stale","index":5,"point":1,"geometry_id":id,"document":document,"session":session}]}),false);
    assert_eq!(
        object(&n.editor.state, "Slot")["sketch"],
        before,
        "failed native action must restore exact sketch snapshot"
    );
    assert!(n.editor.sketch_ui.session.is_some());
}
#[test]
#[ignore = "requires pinned native observer and explicit synthetic Pierce input"]
fn invalid_pierce_requests_preserve_sketch_owner_and_exact_snapshot() {
    let mut n = Native::new("pierce-denials");
    let path = std::env::var("CAD_PIERCE_INPUT").unwrap();
    n.send("open", json!({"path":path}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"OtherBody"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Other","body":"OtherBody"},
        {"op":"geometry","object":"Other","kind":"line","start":[0,0,0],"end":[0,40,0]}
    ]));
    n.editor.request_sketch("Slot");
    n.receive("sketch_begin", true);
    let before = object(&n.editor.state, "Slot")["sketch"].clone();
    let id = before["geometry"][5]["persistent_id"].as_i64().unwrap();
    let session = n.editor.sketch_ui.session.clone().unwrap();
    let document = n.editor.state["document"].clone();
    let base = json!({"op":"pierce","object":"Slot","source":"Master","subelement":"Edge1","mapped_name":"g1;SKT","index":5,"point":1,"geometry_id":id,"document":document,"session":session});
    for (field, replacement) in [
        ("source", json!("Other")),
        ("source", json!("Slot")),
        ("mapped_name", json!("stale")),
        ("geometry_id", json!(id + 999)),
        ("index", json!(0)),
        ("point", json!(0)),
        ("document", json!("wrong")),
        ("session", json!("wrong")),
    ] {
        let mut bad = base.clone();
        bad[field] = replacement;
        n.send(
            "transaction",
            json!({"label":"Reject Pierce","edits":[bad]}),
            false,
        );
        assert_eq!(
            object(&n.editor.state, "Slot")["sketch"],
            before,
            "rejected action changed native sketch for {field}"
        );
        assert!(n.editor.sketch_ui.session.is_some());
    }
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    n.transaction(json!([{"op":"set","object":"Master","property":"Visibility","value":false}]));
    n.editor.request_sketch("Slot");
    n.receive("sketch_begin", true);
    let hidden_before = object(&n.editor.state, "Slot")["sketch"].clone();
    let mut hidden = base;
    hidden["session"] = json!(n.editor.sketch_ui.session.clone().unwrap());
    n.send(
        "transaction",
        json!({"label":"Reject hidden","edits":[hidden]}),
        false,
    );
    assert_eq!(object(&n.editor.state, "Slot")["sketch"], hidden_before);
}

// Keep these checks separate from the exact sketch-undo rollback gate above.
fn forward_association(n: &Native, persistent_ids: &Value, expected_left: f64) {
    let sketch = &object(&n.editor.state, "Slot")["sketch"];
    assert_eq!(&sketch["native_xml"]["persistent_ids"], persistent_ids);
    assert_eq!(sketch["degrees_of_freedom"], 0);
    for (edge, reference, index, expected) in [
        ("Edge1", "Master.;g1;SKT", 5, expected_left),
        ("Edge2", "Master.;g2;SKT", 6, 100.),
    ] {
        let ext = external(&n.editor.state, edge);
        assert_eq!(ext["persistent_reference"], reference);
        assert_eq!(ext["geometry"]["position"][0], expected);
        assert_eq!(point(n, index)[0], expected);
        assert!(sketch["constraints"].as_array().unwrap().iter().any(|c| {
            c["type"] == "Coincident"
                && c["first"] == index
                && c["first_pos"] == 1
                && c["second"] == ext["geometry_index"]
                && c["second_pos"] == 1
                && c["active"] == true
        }));
    }
}

#[test]
#[ignore = "requires pinned native observer and explicit synthetic Pierce input"]
fn source_forward_pierce_master_edit_history_and_reopen() {
    let mut n = Native::new("pierce-source-forward");
    let path = std::env::var("CAD_PIERCE_INPUT").unwrap();
    n.send("open", json!({"path":path}), true);
    // This stock feasibility fixture includes an upper Pad, which the source
    // has not created at this point. Remove only that fixture artifact.
    let master_ids =
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["persistent_ids"].clone();
    let master_map =
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["element_map"].clone();
    let plane_support =
        object(&n.editor.state, "TopPlane")["properties"]["AttachmentSupport"].clone();
    let slot_support = object(&n.editor.state, "Slot")["properties"]["AttachmentSupport"].clone();
    assert_eq!(object(&n.editor.state, "Result")["type"], "PartDesign::Pad");
    n.transaction(json!([
        {"op":"remove_object","object":"Result"},
        {"op":"delete_geometry","object":"Slot","index":6},
        {"op":"delete_geometry","object":"Slot","index":5}
    ]));
    assert!(
        !objects(&n.editor.state)
            .iter()
            .any(|o| o["name"] == "Result")
    );
    assert_eq!(
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["persistent_ids"],
        master_ids
    );
    assert_eq!(
        object(&n.editor.state, "Master")["sketch"]["native_xml"]["element_map"],
        master_map
    );
    assert_eq!(
        object(&n.editor.state, "TopPlane")["properties"]["AttachmentSupport"],
        plane_support
    );
    assert_eq!(
        object(&n.editor.state, "Slot")["properties"]["AttachmentSupport"],
        slot_support
    );
    assert_eq!(
        object(&n.editor.state, "Master")["body"],
        object(&n.editor.state, "Slot")["body"]
    );
    assert_eq!(
        object(&n.editor.state, "Master")["effective_visibility"],
        true
    );
    n.editor.request_sketch("Slot");
    n.receive("sketch_begin", true);
    n.editor.viewport.iso();
    n.editor.viewport.fit();
    settle(&mut n);
    n.click("Point");
    for arc in [1usize, 2] {
        let middle = &object(&n.editor.state, "Slot")["sketch"]["geometry"][arc]["mid"];
        let local = [
            middle[0].as_f64().unwrap(),
            middle[1].as_f64().unwrap(),
            middle[2].as_f64().unwrap(),
        ];
        let pos = n.editor.viewport.sketch_screen_point(local);
        assert_eq!(
            n.editor.viewport.sketch_hit(pos),
            Some(SketchElement {
                index: arc as i32,
                point: 0
            })
        );
        click_at(&mut n, pos);
        n.receive("transaction", true);
        settle(&mut n);
    }
    tutorial_key(&mut n, egui::Key::Escape, egui::Modifiers::NONE);
    let baseline = object(&n.editor.state, "Slot")["sketch"].clone();
    for (index, arc) in [(5, 1), (6, 2)] {
        assert!(
            baseline["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["type"] == "PointOnObject"
                    && c["first"] == index
                    && c["first_pos"] == 1
                    && c["second"] == arc)
        );
    }
    assert_eq!(baseline["degrees_of_freedom"], 4);
    let persistent_ids = baseline["native_xml"]["persistent_ids"].clone();
    select_edge_and_point(&mut n, -100., 5, "Edge1");
    click_menu_pierce(&mut n);
    n.receive("transaction", true);
    settle(&mut n);
    let reference1 = external(&n.editor.state, "Edge1")["persistent_reference"].clone();
    assert_eq!(reference1, "Master.;g1;SKT");
    assert_eq!(
        object(&n.editor.state, "Slot")["sketch"]["degrees_of_freedom"],
        2
    );
    select_edge_and_point(&mut n, 100., 6, "Edge2");
    n.click("Pierce");
    n.receive("transaction", true);
    settle(&mut n);
    let reference2 = external(&n.editor.state, "Edge2")["persistent_reference"].clone();
    assert_eq!(reference2, "Master.;g2;SKT");
    assert_eq!(
        object(&n.editor.state, "Slot")["sketch"]["degrees_of_freedom"],
        0
    );
    forward_association(&n, &persistent_ids, -100.);
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    n.transaction(json!([{"op":"datum","object":"Master","index":3,"value":"-110 mm"}]));
    forward_association(&n, &persistent_ids, -110.);
    n.send("undo", json!({}), true);
    forward_association(&n, &persistent_ids, -100.);
    n.send("redo", json!({}), true);
    forward_association(&n, &persistent_ids, -110.);
    let saved = n.save("pierce-forward.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":saved}), true);
    forward_association(&n, &persistent_ids, -110.);
}
