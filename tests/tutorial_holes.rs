//! Jourden Part 3, supported hole and whole-circle/history steps.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::{f64::consts::PI, path::Path};
const INCH: f64 = 25.4;
const SOURCE: &str = "https://resources.finalsite.net/images/v1722986241/brightonk12com/nocj9hngnnoxr8c3h2fl/tutorialextrudepartholesandarcs.pdf";
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
        name: "Jourden holes and circle history".into(),
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
    f.params["targets"] = json!(["f2:0"]);
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
    f.params["targets"] = json!(["f2:0"]);
    f.params["depth"] = json!("1*inch");
    doc.features.push(f);
}

fn circle_feature(
    doc: &mut Document,
    support: &str,
    center: [f64; 2],
    diameter: f64,
    mode: &str,
    depth: &str,
    name: &str,
) {
    let registry = Registry::builtin().unwrap();
    let mut sketch = Sketch::default();
    sketch.add_circle(center.map(|x| x * INCH), diameter / 2. * INCH);
    sketch.constraints.extend([
        constraint("X", 1, &format!("{}*inch", center[0])),
        constraint("Y", 1, &format!("{}*inch", center[1])),
        constraint("Diameter", 1, &format!("{diameter}*inch")),
    ]);
    let mut f = registry.create("sketch", doc).unwrap();
    let sketch_id = f.id.clone();
    f.name = format!("{name} sketch");
    f.params["support"] = json!(support);
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", doc).unwrap();
    f.name = name.into();
    f.params["sketch"] = json!(sketch_id);
    f.params["mode"] = json!(mode);
    f.params["target"] = json!("f2:0");
    f.params["targets"] = json!(["f2:0"]);
    f.params["depth"] = json!(depth);
    f.params["reverse"] = json!(true);
    doc.features.push(f);
}

fn verify(result: &Evaluation, expected_inches3: f64) {
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert_eq!(result.scene.meshes.len(), 1);
    assert!(result.scene.sketches.values().all(|s| s.dof == 0));
    let actual = result.scene.meshes[0].volume / INCH.powi(3);
    assert!(
        (actual - expected_inches3).abs() < 1e-5,
        "volume {actual} in^3, expected {expected_inches3}"
    );
    let report = brepkit_operations::validate::validate_solid(
        &result.geometry.topo,
        result.geometry.parts[0].solid,
    )
    .unwrap();
    assert!(report.is_valid(), "{:?}", report.issues);
}

fn artifact(doc: &Document, result: &Evaluation, name: &str) {
    let dir = Path::new("output/tutorials/holes");
    let path = dir.join(format!("{name}.cadpart"));
    doc.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded, *doc);
    let reopened = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&loaded)
        .unwrap();
    assert!(
        reopened.scene.error.is_none(),
        "Reopening {name}: {:?}",
        reopened.scene.statuses
    );
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
        let volume = brepkit_operations::measure::solid_volume(&topo, solids[0], 0.04).unwrap();
        assert!((volume - result.scene.meshes[0].volume).abs() < 1e-3);
    }
}

// Independent quadrature of the side-hole material filled by the later disc.
// Integrate in x. The disc's y chord multiplies the overlap of the hole's z
// chord with 0 <= z <= 1. All lengths here are in source inches.
fn filled_hole_volume() -> f64 {
    let n = 100_000;
    let dx = 0.75 / n as f64;
    let f = |x: f64| {
        let half_height = (0.375_f64.powi(2) - (x - 0.75).powi(2)).max(0.).sqrt();
        2. * (4. - x * x).sqrt() * (half_height + half_height.min(0.25))
    };
    (0..=n)
        .map(|i| {
            let weight = if i == 0 || i == n {
                1.
            } else if i % 2 == 0 {
                2.
            } else {
                4.
            };
            weight * f(0.375 + i as f64 * dx)
        })
        .sum::<f64>()
        * dx
        / 3.
}

fn modifier_probe(
    doc: &Document,
    original: &Evaluation,
    kind: &str,
    keys: Vec<String>,
    expected: f64,
) -> serde_json::Value {
    assert!(!keys.is_empty(), "Source {kind} edge was not found");
    let registry = Registry::builtin().unwrap();
    let mut probe = doc.clone();
    let mut f = registry.create(kind, &mut probe).unwrap();
    f.params["target"] = json!("f2:0");
    f.params["edges"] = json!(keys);
    f.params[if kind == "fillet" {
        "radius"
    } else {
        "distance"
    }] = json!("0.5*inch");
    probe.features.push(f);
    let evaluated = Runtime::new(registry).evaluate(&probe).unwrap();
    let name = format!("source-{kind}");
    if let Some(error) = &evaluated.scene.error {
        probe
            .save(&Path::new("output/tutorials/holes").join(format!("{name}-failed.cadpart")))
            .unwrap();
        assert!((evaluated.scene.meshes[0].volume - original.scene.meshes[0].volume).abs() < 1e-6);
        json!({"result":"blocked", "error":error, "selected_edges":keys, "previous_solid_preserved":true})
    } else {
        verify(&evaluated, expected);
        artifact(&probe, &evaluated, &name);
        json!({"result":"passed", "selected_edges":keys, "volume_mm3":evaluated.scene.meshes[0].volume})
    }
}

#[test]
fn jourden_holes_and_whole_circle_reorder() {
    std::fs::create_dir_all("output/tutorials/holes").unwrap();
    let mut doc = base();
    add_cut(&mut doc);
    doc.variables.insert("margin".into(), "0.5*inch".into());
    add_boss(&mut doc);
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    // PDF page 3, raised base terrace, center 0.75 in from both outer edges.
    circle_feature(
        &mut doc,
        "f2:side:1:2",
        [0.75, 0.75],
        1.,
        "Remove",
        "0.5*inch",
        "Simple Hole Extrude",
    );
    let top = runtime.evaluate(&doc).unwrap();
    let top_volume = 31.75 - PI * 0.5_f64.powi(2) * 0.5;
    verify(&top, top_volume);
    assert_eq!(top.scene.sketches["f7"].plane.normal, [0., 0., 1.]);
    artifact(&doc, &top, "top-hole");
    // PDF page 7. Explicit 4-inch depth substitutes for through-all.
    circle_feature(
        &mut doc,
        "f2:end",
        [0.75, 0.75],
        0.75,
        "Remove",
        "4*inch",
        "Side Hole Extrude",
    );
    let side = runtime.evaluate(&doc).unwrap();
    let side_volume = top_volume - PI * 0.375_f64.powi(2) * 4.;
    verify(&side, side_volume);
    artifact(&doc, &side, "side-hole");
    // PDF page 10 Option 2: full circle at the bottom edge midpoint.
    circle_feature(
        &mut doc,
        "f2:side:1:0",
        [0., 2.],
        4.,
        "Add",
        "1*inch",
        "Left Arc Extrude",
    );
    let filled = runtime.evaluate(&doc).unwrap();
    assert!(
        filled
            .scene
            .statuses
            .iter()
            .find(|s| s.id == "f11")
            .unwrap()
            .error
            .is_none()
    );
    let bottom = &filled.scene.sketches["f11"].plane;
    assert_eq!(
        side.geometry
            .face_keys(side.geometry.parts[0].solid)
            .unwrap()
            .iter()
            .filter(|(_, key)| key == "f2:side:1:0")
            .count(),
        2
    );
    assert_eq!(bottom.origin, [0., 0., 0.]);
    assert_eq!(bottom.u, [1., 0., 0.]);
    assert_eq!(bottom.v, [0., -1., 0.]);
    assert_eq!(bottom.normal, [0., 0., -1.]);
    let reordered_volume = side_volume + 2. * PI;
    let filled_status = if let Some(error) = &filled.scene.error {
        assert!(error.contains("mesh approximation"), "{error}");
        verify(&side, side_volume);
        assert!((filled.scene.meshes[0].volume - side.scene.meshes[0].volume).abs() < 1e-6);
        doc.save(Path::new(
            "output/tutorials/holes/circle-after-side-hole-failed.cadpart",
        ))
        .unwrap();
        json!({"result":"blocked", "error":error, "previous_solid_preserved":true})
    } else {
        verify(&filled, reordered_volume + filled_hole_volume());
        artifact(&doc, &filled, "circle-fills-hole");
        json!({"result":"passed"})
    };
    // Preserve IDs while moving the circle sketch and add before the side cut.
    let added: Vec<_> = doc.features.drain(10..12).collect();
    doc.features.splice(8..8, added);
    let reordered = runtime.evaluate(&doc).unwrap();
    verify(&reordered, reordered_volume);
    artifact(&doc, &reordered, "circle-reordered");
    if filled.scene.error.is_none() {
        assert!(filled.scene.meshes[0].volume > reordered.scene.meshes[0].volume + INCH.powi(3));
    }
    let edges = &reordered.scene.meshes[0].edges;
    let close = |a: f32, b: f64| (f64::from(a) / INCH - b).abs() < 1e-5;
    // PDF page 12, two upper outside edges and the long inside riser edge.
    let fillet_keys = edges
        .iter()
        .filter(|e| {
            let a = e.points.first().unwrap();
            let b = e.points.last().unwrap();
            let top_end = close(a[2], 3.)
                && close(b[2], 3.)
                && ((close(a[1], 0.) && close(b[1], 0.)) || (close(a[1], -4.) && close(b[1], -4.)));
            let inside =
                close(a[0], 1.5) && close(b[0], 1.5) && close(a[2], 1.5) && close(b[2], 1.5);
            top_end || inside
        })
        .map(|e| e.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(fillet_keys.len(), 3);
    // Two convex 1.5-inch edges remove the corner-minus-quarter-circle area;
    // the concave four-inch edge adds that same area.
    let fillet_status = modifier_probe(
        &doc,
        &reordered,
        "fillet",
        fillet_keys,
        reordered_volume + 0.5_f64.powi(2) * (1. - PI / 4.),
    );
    // PDF page 13, long top outside edge on the added one-inch cube.
    let chamfer_keys = edges
        .iter()
        .filter(|e| e.points.iter().all(|p| close(p[0], 5.) && close(p[2], 1.)))
        .map(|e| e.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(chamfer_keys.len(), 1);
    let chamfer_status = modifier_probe(
        &doc,
        &reordered,
        "chamfer",
        chamfer_keys,
        reordered_volume - 0.5 * 0.5 * 0.5,
    );
    let report = json!({
        "source": SOURCE,
        "title": "OnShape: Extrude Part 3 Holes and Arcs",
        "method": "Rust document/solver/kernel integration test. GUI not exercised.",
        "tested": ["page 3 dimensioned blind top cut", "page 7 dimensioned side cut", "page 10 whole-circle add before side cut", "history reorder", "all sketches 0 DOF", "analytic volumes", "valid topology", "save/load and fresh regeneration", "split coplanar support retains original frame"],
        "adaptations": ["Part 1 connected profile replaces trim", "Hole tool step omitted", "Side cut uses explicit 4-inch depth instead of through-all", "Circle midpoint located with dimensions rather than edge midpoint relation"],
        "unavailable_or_untested": ["Hole tool and mate connectors, including conical drill tip", "Arc/trim option", "Source fillet probe covers three identifiable edges rather than every indicated round"],
        "operation_probes": {"circle_after_side_cut":filled_status, "three_source_fillets":fillet_status, "source_chamfer":chamfer_status},
        "step_roundtrip": cfg!(feature="step"),
        "volumes_in3": {"top_hole": top_volume, "side_hole": side_volume, "circle_after_side_hole": if filled.scene.error.is_none() {json!(filled.scene.meshes[0].volume/INCH.powi(3))} else {serde_json::Value::Null}, "circle_before_side_hole": reordered_volume},
        "result": "passed supported subset; full tutorial incomplete"
    });
    std::fs::write(
        "output/tutorials/holes/report.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn attachment_uses_its_named_part_and_rejects_missing_faces_and_anchors() {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    for offset in [0., -10.] {
        let mut sketch = Sketch::default();
        sketch.add_rect([0., 0.], [20., 20.]);
        let mut f = registry.create("sketch", &mut doc).unwrap();
        let id = f.id.clone();
        f.params["offset"] = json!(offset);
        f.set_sketch(&sketch);
        doc.features.push(f);
        let mut f = registry.create("extrude", &mut doc).unwrap();
        f.params["sketch"] = json!(id);
        doc.features.push(f);
    }
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.params["support"] = json!("f2:start");
    f.set_sketch(&Sketch::default());
    doc.features.push(f);
    let mut runtime = Runtime::new(registry);
    let opposed = runtime.evaluate(&doc).unwrap();
    assert!(opposed.scene.error.is_none(), "{:?}", opposed.scene.error);
    let source_plane = opposed.scene.meshes.iter().find(|m| m.part == "f2:0").unwrap()
        .faces.iter().find(|f| f.key == "f2:start").unwrap().plane.as_ref().unwrap();
    assert_eq!(opposed.scene.sketches["f5"].plane.normal, source_plane.normal,
        "An unrelated opposing coplanar part must not replace the named support");
    assert_eq!(opposed.scene.sketches["f5"].plane.origin, [0., 0., 0.]);
    doc.features[3].params["depth"] = json!("5");
    let single = runtime.evaluate(&doc).unwrap();
    assert!(single.scene.error.is_none(), "{:?}", single.scene.statuses);
    assert_eq!(single.scene.sketches["f5"].plane.origin, [0., 0., 0.]);
    doc.features[1].suppressed = true;
    let missing_face = runtime.evaluate(&doc).unwrap();
    assert!(
        missing_face
            .scene
            .error
            .as_ref()
            .unwrap()
            .contains("missing")
    );
    doc.features[1].suppressed = false;
    doc.features[4].params["support"] = json!("does-not-exist");
    let missing_anchor = runtime.evaluate(&doc).unwrap();
    assert!(
        missing_anchor
            .scene
            .error
            .as_ref()
            .unwrap()
            .contains("missing")
    );
}

fn minimal_disc_fuse_document() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document {
        name: "Perpendicular cylinder fuse reproduction".into(),
        ..Default::default()
    };
    let mut s = Sketch::default();
    s.add_rect([0., -4. * INCH], [4. * INCH, 0.]);
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.set_sketch(&s);
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("1.5*inch");
    doc.features.push(f);
    circle_feature(
        &mut doc,
        "f2:side:1:0",
        [0.75, 0.75],
        0.75,
        "Remove",
        "4*inch",
        "Side hole",
    );
    circle_feature(
        &mut doc,
        "f2:start",
        [0., 2.],
        4.,
        "Add",
        "1*inch",
        "Disc add",
    );
    doc
}

#[test]
fn parallel_runtimes_do_not_attribute_another_models_mesh_fallback() {
    let failing = minimal_disc_fuse_document();
    let mut valid = failing.clone();
    valid.features.truncate(4);
    let expected = (24. - PI * 0.375_f64.powi(2) * 4.) * INCH.powi(3);
    let start = std::sync::Barrier::new(3);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            start.wait();
            for _ in 0..4 {
                let result = Runtime::new(Registry::builtin().unwrap())
                    .evaluate(&failing)
                    .unwrap();
                let error = result.scene.error.as_ref().unwrap();
                assert!(error.contains("mesh approximation"), "{error}");
                // Only the real failing Fuse gets rejected.
                assert_eq!(
                    result
                        .scene
                        .statuses
                        .iter()
                        .filter(|s| s.error.is_some())
                        .count(),
                    1
                );
                assert_eq!(result.scene.statuses.last().unwrap().id, "f6");
                assert!((result.scene.meshes[0].volume - expected).abs() < 1e-6);
            }
        });
        for _ in 0..2 {
            let start = &start;
            let valid = &valid;
            scope.spawn(move || {
                start.wait();
                // Each rebuild has a fresh Runtime, as after save/reopen.
                for _ in 0..20 {
                    let result = Runtime::new(Registry::builtin().unwrap())
                        .evaluate(valid)
                        .unwrap();
                    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
                    assert!((result.scene.meshes[0].volume - expected).abs() < 1e-6);
                }
            });
        }
    });
}

#[test]
fn minimal_perpendicular_cylinder_fuse_preserves_precise_solid_on_rejection() {
    let doc = minimal_disc_fuse_document();
    let mut prefix = doc.clone();
    prefix.features.truncate(4);
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let before = runtime.evaluate(&prefix).unwrap();
    assert!(before.scene.error.is_none());
    let after = runtime.evaluate(&doc).unwrap();
    std::fs::create_dir_all("output/tutorials/holes").unwrap();
    doc.save(Path::new(
        "output/tutorials/holes/minimal-disc-fuse-reproduction.cadpart",
    ))
    .unwrap();
    if let Some(error) = &after.scene.error {
        assert!(error.contains("mesh approximation"), "{error}");
        assert!((before.scene.meshes[0].volume - after.scene.meshes[0].volume).abs() < 1e-6);
    } else {
        let expected =
            (24. - PI * 0.375_f64.powi(2) * 4. + 2. * PI + filled_hole_volume()) * INCH.powi(3);
        assert!((after.scene.meshes[0].volume - expected).abs() < 1e-3);
    }
    std::fs::write(
        "output/tutorials/holes/minimal-disc-fuse-report.json",
        serde_json::to_vec_pretty(&json!({
            "scope":"Six-feature reduction of the source's circle-after-side-hole operation",
            "feature_count":doc.features.len(),
            "error":after.scene.error,
            "prior_volume_mm3":before.scene.meshes[0].volume,
            "result_volume_mm3":after.scene.meshes[0].volume,
        }))
        .unwrap(),
    )
    .unwrap();
}
