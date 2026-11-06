//! Integration gate. Requires the reviewed assembly Link display/frame changes.
//! This gate intentionally fails on the standalone public display baseline.
use super::*;

fn near(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < tolerance),
        "{actual:?} != {expected:?}"
    );
}
fn point(v: &Value) -> [f64; 3] {
    [
        v[0].as_f64().unwrap(),
        v[1].as_f64().unwrap(),
        v[2].as_f64().unwrap(),
    ]
}

#[test]
#[ignore = "P1 integration gate: requires merged reviewed assembly display/frame patch and generated scene fixture"]
fn native_measure_link_scene_selection_and_witness_frames() {
    let runtime: Value = serde_json::from_slice(
        &fs::read(std::env::var_os("CAD_MEASUREMENT_RUNTIME").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fs::canonicalize(runtime["script"].as_str().unwrap()).unwrap(),
        fs::canonicalize(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend/freecad_worker.py")
        )
        .unwrap(),
        "Gate binary must be compiled from the same merged source as runtime.script"
    );
    let controls: Value = serde_json::from_slice(
        &fs::read(std::env::var_os("CAD_MEASUREMENT_SCENE_CONTROLS").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = PathBuf::from(std::env::var_os("CAD_MEASUREMENT_SCENE_FIXTURE").unwrap());
    let mut n = Native::case("link-scene");
    n.send("open", json!({"path":fixture}));
    let before = n.editor.state.clone();
    let dirty = n.editor.dirty;
    let objects = controls["vertices"].as_array().unwrap();
    for control in objects {
        let name = control["object"].as_str().unwrap();
        assert!(
            crate::body_commands::display_objects(&n.editor.state)
                .iter()
                .any(|o| o == name),
            "P1: {name} is absent from actual display_objects"
        );
        assert!(
            n.editor
                .viewport
                .topology_references(name)
                .iter()
                .any(|r| r == "Vertex1"),
            "P1: {name} has no vertex identity in the actual model-thread/viewport scene"
        );
        let object = super::super::objects(&n.editor.state)
            .iter()
            .find(|o| o["name"] == name)
            .unwrap();
        near(
            point(&object["global_placement"]["position"]),
            point(&control["global_placement"]["position"]),
            2e-7,
        );
        let actual = object["global_placement"]["rotation"].as_array().unwrap();
        let expected = control["global_placement"]["rotation"].as_array().unwrap();
        let dot: f64 = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| a.as_f64().unwrap() * b.as_f64().unwrap())
            .sum();
        assert!(
            (dot.abs() - 1.).abs() < 1e-8,
            "P1: {name} global rotation differs"
        );
    }
    n.editor.viewport.selection_mode = crate::viewport::SelectionMode::Vertices;
    n.editor.viewport.display_mode = crate::viewport::DisplayMode::Wireframe;
    n.frame(vec![]);
    n.editor.viewport.fit();
    n.frame(vec![]);
    let mut observations = vec![];
    for control in objects {
        let name = control["object"].as_str().unwrap();
        let world = point(&control["point"]);
        // With no sketch, the public projection helper uses the identity sketch
        // frame. Click the independent native world point in the real viewport.
        assert!(n.editor.sketch.is_none());
        let pos = n.editor.viewport.sketch_screen_point(world);
        n.click_at(pos, egui::Modifiers::NONE);
        assert_eq!(
            n.editor.viewport.selected.as_ref().unwrap().object,
            name,
            "P1: pick frame mismatch"
        );
        assert_eq!(
            n.editor.viewport.selected.as_ref().unwrap().subelement,
            "Vertex1"
        );
        n.key(Key::OpenBracket, egui::Modifiers::NONE);
        n.receive("measure", true);
        let selected = &n.editor.measurement_ui.result.as_ref().unwrap().selections[0];
        near(selected.center.value, world, 2e-7);
        observations.push(json!({"object":name,"subelement":selected.reference.subelement,"world":world,"screen":[pos.x,pos.y],"measured":selected.center.value}));
        n.key(Key::Escape, egui::Modifiers::NONE);
    }
    let first = &objects[1];
    let second = &objects[2];
    n.click_at(
        n.editor
            .viewport
            .sketch_screen_point(point(&first["point"])),
        egui::Modifiers::NONE,
    );
    n.click_at(
        n.editor
            .viewport
            .sketch_screen_point(point(&second["point"])),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(n.editor.viewport.selections.len(), 2);
    n.key(Key::OpenBracket, egui::Modifiers::NONE);
    n.receive("measure", true);
    let result = n.editor.measurement_ui.result.as_ref().unwrap();
    let distance = result.distance.as_ref().unwrap();
    let a = point(&first["point"]);
    let b = point(&second["point"]);
    let expected = a
        .iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!((distance.metric.value - expected).abs() < 2e-7);
    assert!(!distance.witnesses.is_empty());
    for witnesses in &distance.witnesses {
        near(witnesses[0].value, a, 2e-7);
        near(witnesses[1].value, b, 2e-7);
    }
    assert_eq!(n.editor.state, before);
    assert_eq!(n.editor.dirty, dirty);
    fs::write(n.output.join("combined-scene-observations.json"),serde_json::to_vec_pretty(&json!({"passed":true,"vertices":observations,"distance":distance.metric.value,"witnesses":distance.witnesses.iter().map(|w|[w[0].value,w[1].value]).collect::<Vec<_>>(),"state_unchanged":true})).unwrap()).unwrap();
}
