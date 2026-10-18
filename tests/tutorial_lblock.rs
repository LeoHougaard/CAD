//! Jourden, "OnShape: Extrude Tutorial Part 1", pages 4-11.
//! The source draws two rectangles and trims them. Until trim exists, this
//! exercises its final six-line profile, not that original editing workflow.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

const INCH: f64 = 25.4;
const SOURCE: &str = "https://drive.google.com/file/d/1PUrUjtnXYu7C9-piphr1rg4BUoxOE2WZ/view";

fn constraint(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}

fn lblock() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document {
        name: "Jourden L-block".into(),
        ..Default::default()
    };
    let vertices = [
        [0., 0.],
        [5., 0.],
        [5., 1.5],
        [2.5, 1.5],
        [2.5, 3.],
        [0., 3.],
    ];
    let mut sketch = Sketch::default();
    for i in 0..vertices.len() {
        sketch.add_line(
            vertices[i].map(|v| v * INCH),
            vertices[(i + 1) % vertices.len()].map(|v| v * INCH),
        );
        sketch.constraints.push(constraint(
            if i % 2 == 0 { "Horizontal" } else { "Vertical" },
            i as u64 + 1,
            "",
        ));
    }
    sketch.constraints.push(constraint("Origin", 1, ""));
    for (entity, dimension) in [
        (1, "5*inch"),
        (3, "2.5*inch"),
        (4, "1.5*inch"),
        (6, "3*inch"),
    ] {
        sketch
            .constraints
            .push(constraint("Length", entity, dimension));
    }
    let mut feature = registry.create("sketch", &mut doc).unwrap();
    feature.name = "Main Profile".into();
    feature.params["plane"] = json!("Front");
    feature.set_sketch(&sketch);
    doc.features.push(feature);
    let mut feature = registry.create("extrude", &mut doc).unwrap();
    feature.name = "Main Extrude".into();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!("4*inch");
    doc.features.push(feature);
    doc
}

fn verify(evaluation: &Evaluation, width: f64) {
    assert!(
        evaluation.scene.error.is_none(),
        "{:?}",
        evaluation.scene.statuses
    );
    assert_eq!(evaluation.scene.meshes.len(), 1);
    let sketch = &evaluation.scene.sketches["f1"];
    assert_eq!(sketch.dof, 0, "The profile must be dimensioned, not frozen");
    assert!(sketch.entities.iter().all(|e| e.fixed));
    assert_eq!(sketch.entities.len(), 6);
    let expected = [
        [0., 0.],
        [width, 0.],
        [width, 1.5],
        [width - 2.5, 1.5],
        [width - 2.5, 3.],
        [0., 3.],
    ];
    for (i, entity) in sketch.entities.iter().enumerate() {
        let actual = &entity.values;
        let start = expected[i].map(|x| x * INCH);
        let end = expected[(i + 1) % expected.len()].map(|x| x * INCH);
        for (actual, expected) in actual.iter().zip([start[0], start[1], end[0], end[1]]) {
            assert!(
                (actual - expected).abs() < 1e-5,
                "line {i}: {actual} != {expected}"
            );
        }
    }
    let expected_volume = (width * 3. - 2.5 * 1.5) * 4. * INCH.powi(3);
    let mesh = &evaluation.scene.meshes[0];
    assert!((mesh.volume - expected_volume).abs() < 1e-3);
    assert_eq!(mesh.faces.len(), 8, "Two caps and six side faces");
    let part = &evaluation.geometry.parts[0];
    let topology =
        brepkit_operations::validate::validate_solid(&evaluation.geometry.topo, part.solid)
            .unwrap();
    assert!(topology.is_valid(), "{topology:?}");
    // Every tessellated edge must have exactly two oppositely directed uses.
    let mut edges = BTreeMap::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0_i32));
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(
        edges
            .values()
            .all(|&(uses, winding)| uses == 2 && winding == 0)
    );
}

#[cfg(feature = "step")]
fn verify_step(evaluation: &Evaluation, path: &Path) {
    evaluation.geometry.export("step", path).unwrap();
    let mut topology = brepkit_topology::Topology::new();
    let solids =
        brepkit_io::step::read_step(&std::fs::read_to_string(path).unwrap(), &mut topology)
            .unwrap();
    assert_eq!(solids.len(), 1);
    assert!(
        brepkit_operations::validate::validate_solid(&topology, solids[0])
            .unwrap()
            .is_valid()
    );
    let volume = brepkit_operations::measure::solid_volume(&topology, solids[0], 0.04).unwrap();
    assert!((volume - evaluation.scene.meshes[0].volume).abs() < 1e-3);
}

#[test]
fn jourden_lblock_dimensions_regenerate_after_width_edit_and_file_roundtrip() {
    let directory = Path::new("output/tutorials/lblock");
    std::fs::create_dir_all(directory).unwrap();
    let mut doc = lblock();
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let initial = runtime.evaluate(&doc).unwrap();
    verify(&initial, 5.);
    doc.save(&directory.join("lblock-5in.cadpart")).unwrap();
    #[cfg(feature = "step")]
    verify_step(&initial, &directory.join("lblock-5in.step"));

    let mut sketch = doc.features[0].sketch().unwrap();
    assert!(!sketch.constraints.iter().any(|c| c.kind == "Fix"));
    sketch
        .constraints
        .iter_mut()
        .find(|c| c.kind == "Length" && c.a == 1)
        .unwrap()
        .value = "4*inch".into();
    doc.features[0].set_sketch(&sketch);
    let edited = runtime.evaluate(&doc).unwrap();
    verify(&edited, 4.);
    let path = directory.join("lblock-4in.cadpart");
    doc.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded, doc);
    let reloaded = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&loaded)
        .unwrap();
    verify(&reloaded, 4.);
    #[cfg(feature = "step")]
    verify_step(&reloaded, &directory.join("lblock-4in.step"));

    let report = json!({
        "tutorial": "OnShape: Extrude Tutorial Part 1, Single Profile Extrude",
        "author": "Matthew Jourden, Brighton High School",
        "source": SOURCE,
        "pages": "4-11",
        "test": "jourden_lblock_dimensions_regenerate_after_width_edit_and_file_roundtrip",
        "verification": "Rust engine integration test, not native UI interaction",
        "adaptations": ["The source draws overlapping rectangles and trims two edges. Trim is unavailable. This test draws the same final six-line outline directly."],
        "not_tested": ["Rectangle trim interaction", "Cloud sharing/comments", "Native UI entry and mouse workflow"],
        "plane": "Front",
        "source_units": "inch",
        "kernel_units": "millimeter",
        "driving_dimensions_in": {"initial_width":5, "edited_width":4, "height":3, "notch_width":2.5, "notch_height":1.5, "depth":4},
        "checks": ["zero sketch degrees of freedom without Fix", "every solved endpoint before and after edit", "analytic volume", "eight analytic faces", "valid B-rep", "closed consistently wound display mesh", "save/load equality and fresh regeneration"],
        "step_roundtrip_verified": cfg!(feature = "step"),
        "initial_volume_mm3": initial.scene.meshes[0].volume,
        "edited_volume_mm3": edited.scene.meshes[0].volume,
        "initial_rebuild_ms": initial.scene.millis,
        "edited_rebuild_ms": edited.scene.millis,
        "result": "passed"
    });
    std::fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
