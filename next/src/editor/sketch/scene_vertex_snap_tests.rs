use super::*;

fn fixture(n: &mut Native) {
    n.send("new", json!({"name":"SceneVertexFixture"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Master"},
        {"op":"geometry","object":"Master","kind":"line","start":[0,0,0],"end":[-40,0,0]},
        {"op":"geometry","object":"Master","kind":"line","start":[-40,0,0],"end":[-40,62,0]},
        {"op":"constraint","object":"Master","kind":"Coincident","args":[0,1,-1,1]},
        {"op":"constraint","object":"Master","kind":"Horizontal","args":[0]},
        {"op":"constraint","object":"Master","kind":"Distance","args":[0,40]},
        {"op":"constraint","object":"Master","kind":"Coincident","args":[0,2,1,1]},
        {"op":"constraint","object":"Master","kind":"Vertical","args":[1]},
        {"op":"constraint","object":"Master","kind":"Distance","args":[1,62]},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower"}
    ]));
    n.editor.request_sketch("Follower");
    n.receive("sketch_begin", true);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.editor.viewport.fit_sketch();
    for _ in 0..3 {
        n.frame(vec![]);
    }
    tutorial_key(n, Key::L, egui::Modifiers::NONE);
}

fn projected(state: &Value) -> &Value {
    let entries = object(state, "Follower")["sketch"]["externals"]
        .as_array()
        .unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry["source"]["link"], "Master");
    assert_eq!(entry["geometry"]["kind"], "point");
    assert_eq!(entry["missing"], false);
    assert_eq!(entry["frozen"], false);
    assert!(
        entry["persistent_reference"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    entry
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn scene_vertex_line_is_atomic_and_follows_native_owner() {
    let mut n = Native::new("scene-vertex-line");
    fixture(&mut n);
    let baseline = object(&n.editor.state, "Follower")["sketch"].clone();
    // A first vertex pick and Escape do not project anything.
    tutorial_click(&mut n, [-40., 0., 0.]);
    assert_eq!(n.editor.sketch_ui.scene_snaps.iter().flatten().count(), 1);
    assert!(!n.editor.busy);
    tutorial_key(&mut n, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_click(&mut n, [0., 0., 0.]);
    tutorial_click(&mut n, [-40., 0., 0.]);
    n.receive("transaction", true);
    let entry = projected(&n.editor.state);
    let id = entry["geometry_index"].as_i64().unwrap();
    let reference = entry["persistent_reference"].clone();
    let constraints = object(&n.editor.state, "Follower")["sketch"]["constraints"]
        .as_array()
        .unwrap();
    assert!(constraints.iter().any(|c| c["type"] == "Coincident"
        && c["first"] == 0
        && c["first_pos"] == 2
        && c["second"] == id
        && c["second_pos"] == 1));
    n.send("undo", json!({}), true);
    assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
    n.send("redo", json!({}), true);
    assert_eq!(
        projected(&n.editor.state)["persistent_reference"],
        reference
    );
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    let source = n.save("before-master-change.FCStd");
    let source_bytes = fs::read(&source).unwrap();
    n.transaction(json!([{"op":"datum","object":"Master","index":2,"value":"45 mm"}]));
    let follower = &object(&n.editor.state, "Follower")["sketch"];
    let end = &follower["geometry"][0]["end"];
    assert!((end[0].as_f64().unwrap() + 45.).abs() < 1e-6, "{end}");
    assert_eq!(
        projected(&n.editor.state)["persistent_reference"],
        reference
    );
    n.send("undo", json!({}), true);
    assert!(
        (object(&n.editor.state, "Follower")["sketch"]["geometry"][0]["end"][0]
            .as_f64()
            .unwrap()
            + 40.)
            .abs()
            < 1e-6
    );
    n.send("redo", json!({}), true);
    let result = n.save("after-master-change.FCStd");
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    n.send("close", json!({}), true);
    n.send("open", json!({"path":result}), true);
    assert_eq!(
        projected(&n.editor.state)["persistent_reference"],
        reference
    );
    assert!(
        (object(&n.editor.state, "Follower")["sketch"]["geometry"][0]["end"][0]
            .as_f64()
            .unwrap()
            + 45.)
            .abs()
            < 1e-6
    );
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn body_front_sketch_scene_vertex_follows_master() {
    let mut n = Native::new("scene-vertex-body-front");
    n.send("new", json!({"name":"BodyFrontFixture"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"PartDesign::Body","name":"Body"},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Master","body":"Body","origin_plane":"XZ_Plane"},
        {"op":"geometry","object":"Master","kind":"line","start":[0,0,0],"end":[-40,0,0]},
        {"op":"geometry","object":"Master","kind":"line","start":[-40,0,0],"end":[-40,62,0]},
        {"op":"constraint","object":"Master","kind":"Coincident","args":[0,1,-1,1]},
        {"op":"constraint","object":"Master","kind":"Horizontal","args":[0]},
        {"op":"constraint","object":"Master","kind":"Distance","args":[0,40]},
        {"op":"constraint","object":"Master","kind":"Coincident","args":[0,2,1,1]},
        {"op":"constraint","object":"Master","kind":"Vertical","args":[1]},
        {"op":"constraint","object":"Master","kind":"Distance","args":[1,62]},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower","body":"Body","origin_plane":"XZ_Plane"}
    ]));
    let master = object(&n.editor.state, "Master");
    let follower = object(&n.editor.state, "Follower");
    assert_eq!(master["body"], "Body");
    assert_eq!(follower["body"], "Body");
    assert_eq!(master["properties"]["MapMode"], "FlatFace");
    assert_eq!(follower["properties"]["MapMode"], "FlatFace");
    assert_eq!(
        master["properties"]["AttachmentSupport"],
        follower["properties"]["AttachmentSupport"]
    );
    let plane = master["properties"]["AttachmentSupport"][0]["link"]
        .as_str()
        .unwrap();
    assert_eq!(
        object(&n.editor.state, plane)["properties"]["Role"],
        "XZ_Plane"
    );
    assert_eq!(master["effective_visibility"], true);
    n.editor.request_sketch("Follower");
    n.receive("sketch_begin", true);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.editor.viewport.fit_sketch();
    for _ in 0..3 {
        n.frame(vec![]);
    }
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    tutorial_click(&mut n, [0., 0., 0.]);
    tutorial_click(&mut n, [-40., 0., 0.]);
    n.receive("transaction", true);
    let entry = projected(&n.editor.state);
    let id = entry["geometry_index"].as_i64().unwrap();
    let reference = entry["persistent_reference"].clone();
    let constraints = object(&n.editor.state, "Follower")["sketch"]["constraints"]
        .as_array()
        .unwrap();
    assert!(constraints.iter().any(|c| c["type"] == "Coincident"
        && c["first"] == 0
        && c["first_pos"] == 2
        && c["second"] == id
        && c["second_pos"] == 1));
    n.editor.finish_sketch(true);
    n.receive("sketch_finish", true);
    n.transaction(json!([{"op":"datum","object":"Master","index":2,"value":"45 mm"}]));
    let end = &object(&n.editor.state, "Follower")["sketch"]["geometry"][0]["end"];
    assert!((end[0].as_f64().unwrap() + 45.).abs() < 1e-6, "{end}");
    assert_eq!(
        projected(&n.editor.state)["persistent_reference"],
        reference
    );
    let result = n.save("body-front-result.FCStd");
    n.send("close", json!({}), true);
    n.send("open", json!({"path":result}), true);
    assert_eq!(object(&n.editor.state, "Master")["body"], "Body");
    assert_eq!(object(&n.editor.state, "Follower")["body"], "Body");
    assert_eq!(
        projected(&n.editor.state)["persistent_reference"],
        reference
    );
    assert!(
        (object(&n.editor.state, "Follower")["sketch"]["geometry"][0]["end"][0]
            .as_f64()
            .unwrap()
            + 45.)
            .abs()
            < 1e-6
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn cancelling_sketch_restores_accepted_line_and_projection() {
    let mut n = Native::new("scene-vertex-cancel");
    fixture(&mut n);
    let baseline = object(&n.editor.state, "Follower")["sketch"].clone();
    tutorial_click(&mut n, [0., 0., 0.]);
    tutorial_click(&mut n, [-40., 0., 0.]);
    n.receive("transaction", true);
    projected(&n.editor.state);
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn pending_scene_pick_and_bad_native_source_fail_closed() {
    let mut n = Native::new("scene-vertex-negative");
    fixture(&mut n);
    let baseline = object(&n.editor.state, "Follower")["sketch"].clone();
    tutorial_click(&mut n, [-40., 0., 0.]);
    assert_eq!(n.editor.sketch_ui.scene_snaps.iter().flatten().count(), 1);
    n.editor.sketch_ui.scene_snaps[0].as_mut().unwrap().session = "wrong-session".into();
    tutorial_click(&mut n, [0., 0., 0.]);
    assert!(!n.editor.busy);
    assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
    assert!(n.editor.sketch_ui.points.is_empty());
    for (source, subelement, mapped) in [
        ("Master", "Vertex2", "wrong-owner"),
        ("Follower", "Vertex1", "invalid"),
        ("Absent", "Vertex1", "invalid"),
        ("Master", "Edge1", "invalid"),
    ] {
        n.send(
            "transaction",
            json!({"edits":[
                {"op":"geometry","object":"Follower","kind":"line","start":[0,0,0],"end":[-40,0,0]},
                {"op":"auto_external_coincident","object":"Follower","index":0,"point":2,
                 "source":source,"subelement":subelement,"mapped_name":mapped}
            ]}),
            false,
        );
        assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
    }
    n.editor.finish_sketch(false);
    n.receive("sketch_cancel", true);
    assert_eq!(object(&n.editor.state, "Follower")["sketch"], baseline);
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn overlapping_scene_owners_are_not_resolved_by_coordinates() {
    let mut n = Native::new("scene-vertex-ambiguous");
    n.send("new", json!({"name":"AmbiguousScene"}), true);
    n.transaction(json!([
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Master"},
        {"op":"geometry","object":"Master","kind":"line","start":[-40,0,0],"end":[-40,20,0]},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Other"},
        {"op":"geometry","object":"Other","kind":"line","start":[-40,0,0],"end":[-30,0,0]},
        {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower"}
    ]));
    n.editor.request_sketch("Follower");
    n.receive("sketch_begin", true);
    for _ in 0..3 {
        n.frame(vec![]);
    }
    n.editor.viewport.fit_sketch();
    for _ in 0..3 {
        n.frame(vec![]);
    }
    tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
    let before = n.editor.state.clone();
    tutorial_click(&mut n, [-40., 0., 0.]);
    assert!(!n.editor.busy && n.editor.sketch_ui.points.is_empty());
    assert_eq!(n.editor.state, before);
    assert!(
        n.editor
            .error
            .as_deref()
            .is_some_and(|e| e.contains("More than one"))
    );
}

#[test]
#[ignore = "requires CAD_LIFECYCLE_RUNTIME and CAD_LIFECYCLE_OUTPUT"]
fn solid_vertices_do_not_block_lines_or_eligible_sketch_vertices() {
    for master_visible in [false, true] {
        let mut n = Native::new(if master_visible {
            "scene-vertex-solid-and-sketch"
        } else {
            "scene-vertex-solid-only"
        });
        n.send("new", json!({"name":"SolidVertexFixture"}), true);
        n.transaction(json!([
            {"op":"add_object","type":"Part::Box","name":"Box"},
            {"op":"set","object":"Box","property":"Placement","value":{"position":[8,4,-10],"rotation":[0,0,0,1]}},
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Master"},
            {"op":"geometry","object":"Master","kind":"line","start":[8,4,0],"end":[8,14,0]},
            {"op":"set","object":"Master","property":"Visibility","value":master_visible},
            {"op":"add_object","type":"Sketcher::SketchObject","name":"Follower"}
        ]));
        n.editor.request_sketch("Follower");
        n.receive("sketch_begin", true);
        for _ in 0..3 {
            n.frame(vec![]);
        }
        tutorial_key(&mut n, Key::L, egui::Modifiers::NONE);
        let position = n.editor.viewport.sketch_screen_point([8., 4., 0.]);
        let hits = n.editor.viewport.scene_vertices_at(position);
        assert!(hits.iter().any(|h| h.object == "Box"));
        assert_eq!(hits.iter().any(|h| h.object == "Master"), master_visible);
        tutorial_click(&mut n, [8., 4., 0.]);
        assert_eq!(n.editor.sketch_ui.points.len(), 1);
        assert_eq!(
            n.editor.sketch_ui.scene_snaps.iter().flatten().count(),
            usize::from(master_visible)
        );
        assert!(n.editor.error.is_none(), "{:?}", n.editor.error);
        tutorial_click(&mut n, [-8., 4., 0.]);
        n.receive("transaction", true);
        let sketch = &object(&n.editor.state, "Follower")["sketch"];
        assert_eq!(sketch["geometry_count"], 1);
        assert_eq!(
            sketch["externals"].as_array().unwrap().len(),
            usize::from(master_visible)
        );
        if master_visible {
            projected(&n.editor.state);
        }
        n.editor.finish_sketch(false);
        n.receive("sketch_cancel", true);
        assert_eq!(
            object(&n.editor.state, "Follower")["sketch"]["geometry_count"],
            0
        );
    }
}
