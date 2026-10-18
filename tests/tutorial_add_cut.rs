//! Jourden, "OnShape: Extrude Part 2 Adding Material and Cutting Material".
//! Dimensions and face placement follow pages 3-6. The Part 1 base profile
//! substitutes connected lines for the unavailable rectangle trimming tool.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

const INCH: f64 = 25.4;
const SOURCE: &str = "https://resources.finalsite.net/images/v1722986235/brightonk12com/bg0jjfkr34hujkzkp4oq/tutorialextrudepartadd_cutmaterial.pdf";

fn constraint(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}

fn base() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document {
        name: "Jourden add and cut".into(),
        ..Default::default()
    };
    let points = [
        [0., 0.],
        [4., 0.],
        [4., 1.5],
        [1.5, 1.5],
        [1.5, 3.],
        [0., 3.],
    ];
    let mut sketch = Sketch::default();
    for i in 0..6 {
        sketch.add_line(
            points[i].map(|x| x * INCH),
            points[(i + 1) % 6].map(|x| x * INCH),
        );
        sketch.constraints.push(constraint(
            if i % 2 == 0 { "Horizontal" } else { "Vertical" },
            i as u64 + 1,
            "",
        ));
    }
    sketch.constraints.push(constraint("Origin", 1, ""));
    for (entity, value) in [
        (1, "4*inch"),
        (3, "2.5*inch"),
        (4, "1.5*inch"),
        (6, "3*inch"),
    ] {
        sketch.constraints.push(constraint("Length", entity, value));
    }
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.name = "Main Profile".into();
    f.params["plane"] = json!("Front");
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.name = "Main Extrude".into();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("4*inch");
    doc.features.push(f);
    doc
}

fn add_cut(doc: &mut Document) {
    let registry = Registry::builtin().unwrap();
    doc.variables.insert("margin".into(), "1*inch".into());
    // The riser's local U is world +Z, V is world -Y, normal is +X.
    let mut sketch = Sketch::default();
    sketch.add_rect([0.75 * INCH, INCH], [1.5 * INCH, 3. * INCH]);
    for (kind, value) in [
        ("X", "0.75*inch"),
        ("Y", "#margin"),
        ("Width", "0.75*inch"),
        ("Height", "4*inch - 2*#margin"),
    ] {
        sketch.constraints.push(constraint(kind, 1, value));
    }
    let mut f = registry.create("sketch", doc).unwrap();
    f.name = "Top Cut".into();
    f.params["support"] = json!("f2:side:1:3");
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", doc).unwrap();
    f.name = "Top Cut Extrude".into();
    f.params["sketch"] = json!("f3");
    f.params["mode"] = json!("Remove");
    f.params["target"] = json!("f2:0");
    f.params["depth"] = json!("1*inch");
    f.params["reverse"] = json!(true);
    doc.features.push(f);
}

fn add_boss(doc: &mut Document) {
    let registry = Registry::builtin().unwrap();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [INCH, INCH]);
    sketch.constraints.push(constraint("Origin", 1, ""));
    sketch.constraints.push(constraint("Width", 1, "1*inch"));
    sketch.constraints.push(constraint("Height", 1, "1*inch"));
    let mut f = registry.create("sketch", doc).unwrap();
    f.name = "Cube Add".into();
    f.params["support"] = json!("f2:side:1:1");
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", doc).unwrap();
    f.name = "Cube Extrude".into();
    f.params["sketch"] = json!("f5");
    f.params["mode"] = json!("Add");
    f.params["target"] = json!("f2:0");
    f.params["depth"] = json!("1*inch");
    doc.features.push(f);
}

fn verify(result: &Evaluation, width: f64, margin: f64, boss: bool) -> f64 {
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert_eq!(result.scene.meshes.len(), 1);
    assert!(result.scene.sketches.values().all(|s| s.dof == 0));
    let cut = &result.scene.sketches["f3"];
    assert_eq!(cut.plane.normal, [1., 0., 0.]);
    for (actual, expected) in
        cut.plane
            .origin
            .into_iter()
            .zip([(width - 2.5) * INCH, 0., 1.5 * INCH])
    {
        assert!((actual - expected).abs() < 1e-5);
    }
    for (actual, expected) in cut.entities[0].values.iter().zip([
        0.75 * INCH,
        margin * INCH,
        0.75 * INCH,
        (4. - 2. * margin) * INCH,
    ]) {
        assert!((actual - expected).abs() < 1e-5);
    }
    if boss {
        let add = &result.scene.sketches["f5"];
        for (actual, expected) in add.plane.origin.into_iter().zip([width * INCH, 0., 0.]) {
            assert!((actual - expected).abs() < 1e-5);
        }
        assert_eq!(add.plane.normal, [1., 0., 0.]);
    }
    let volume = ((width * 3. - 2.5 * 1.5) * 4. - (4. - 2. * margin) * 0.75
        + if boss { 1. } else { 0. })
        * INCH.powi(3);
    let mesh = &result.scene.meshes[0];
    assert!(
        (mesh.volume - volume).abs() < 1e-3,
        "{} != {volume}",
        mesh.volume
    );
    let part = &result.geometry.parts[0];
    assert!(
        brepkit_operations::validate::validate_solid(&result.geometry.topo, part.solid)
            .unwrap()
            .is_valid()
    );
    let mut edges = BTreeMap::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            let edge = edges.entry((a.min(b), a.max(b))).or_insert((0, 0_i32));
            edge.0 += 1;
            edge.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(
        edges
            .values()
            .all(|&(count, winding)| count == 2 && winding == 0)
    );
    volume
}

fn artifact(doc: &Document, result: &Evaluation, name: &str) {
    let dir = Path::new("output/tutorials/add-cut");
    doc.save(&dir.join(format!("{name}.cadpart"))).unwrap();
    let loaded = Document::load(&dir.join(format!("{name}.cadpart"))).unwrap();
    assert_eq!(loaded, *doc);
    let reopened = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&loaded)
        .unwrap();
    assert!(reopened.scene.error.is_none());
    assert!((reopened.scene.meshes[0].volume - result.scene.meshes[0].volume).abs() < 1e-3);
    #[cfg(feature = "step")]
    {
        let path = dir.join(format!("{name}.step"));
        result.geometry.export("step", &path).unwrap();
        let mut topo = brepkit_topology::Topology::new();
        let solids =
            brepkit_io::step::read_step(&std::fs::read_to_string(path).unwrap(), &mut topo)
                .unwrap();
        assert_eq!(solids.len(), 1);
        assert!(
            brepkit_operations::validate::validate_solid(&topo, solids[0])
                .unwrap()
                .is_valid()
        );
        let volume = brepkit_operations::measure::solid_volume(&topo, solids[0], 0.04).unwrap();
        assert!((volume - result.scene.meshes[0].volume).abs() < 1e-3);
    }
}

#[test]
fn jourden_add_cut_preserves_attached_sketches_after_parent_edit() {
    let dir = Path::new("output/tutorials/add-cut");
    std::fs::create_dir_all(dir).unwrap();
    let mut doc = base();
    add_cut(&mut doc);
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let initial = runtime.evaluate(&doc).unwrap();
    let initial_volume = verify(&initial, 4., 1., false);
    artifact(&doc, &initial, "initial-cut");
    doc.variables.insert("margin".into(), "0.5*inch".into());
    let edited = runtime.evaluate(&doc).unwrap();
    let edited_volume = verify(&edited, 4., 0.5, false);
    artifact(&doc, &edited, "widened-cut");
    add_boss(&mut doc);
    let completed = runtime.evaluate(&doc).unwrap();
    let completed_volume = verify(&completed, 4., 0.5, true);
    artifact(&doc, &completed, "completed-add-cut");
    let mut main = doc.features[0].sketch().unwrap();
    main.constraints
        .iter_mut()
        .find(|c| c.kind == "Length" && c.a == 1)
        .unwrap()
        .value = "4.5*inch".into();
    doc.features[0].set_sketch(&main);
    let resized = runtime.evaluate(&doc).unwrap();
    let resized_volume = verify(&resized, 4.5, 0.5, true);
    artifact(&doc, &resized, "parent-width-edit");
    let report = json!({
        "tutorial":"OnShape: Extrude Part 2 Adding Material and Cutting Material",
        "author":"Matthew Jourden, Brighton High School", "source":SOURCE,"pages":"2-7",
        "verification":"Rust engine integration test; native UI workflow not tested here",
        "adaptations":["Part 1 base uses connected lines because rectangle trim is unavailable", "Both cut-end offsets share one driving margin variable instead of two separate edge dimensions"],
        "diagram_interpretation":"Page 6 boss bottom and end edges are coincident with supporting face edges. Its 1-by-1-inch profile projects outward by 1 inch.",
        "checks":["all sketches zero DOF", "tracked riser and outer-wall supports", "cut depth 1in", "cut width edit 2in to 3in", "1 cubic inch boss fused into one part", "parent width edit moves both supports", "analytic volumes", "valid B-rep and closed oriented mesh", "save/load and fresh rebuild"],
        "step_roundtrip_verified":cfg!(feature="step"),
        "volumes_mm3":{"initial_cut":initial_volume,"widened_cut":edited_volume,"completed":completed_volume,"parent_width_edit":resized_volume},
        "result":"passed"
    });
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
