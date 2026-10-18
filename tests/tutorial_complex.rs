//! Alejandro Reyes, Beginner's Guide to SolidWorks 2006, The Housing.
//! Primary publisher sample: https://static.sdcpublications.com/pdfsample/1-58503-262-x-2.pdf
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::path::Path;
const SOURCE: &str = "https://static.sdcpublications.com/pdfsample/1-58503-262-x-2.pdf";
const INCH: f64 = 25.4;

fn constraint(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}
fn rectangle(x: &str, y: &str, w: &str, h: &str) -> Sketch {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 10.]);
    for (kind, value) in [("X", x), ("Y", y), ("Width", w), ("Height", h)] {
        s.constraints.push(constraint(kind, 1, value));
    }
    s
}
fn add_sketch(
    doc: &mut Document,
    name: &str,
    support: &str,
    plane: &str,
    offset: &str,
    sketch: Sketch,
) -> String {
    let mut f = Registry::builtin().unwrap().create("sketch", doc).unwrap();
    f.name = name.into();
    f.params["support"] = json!(support);
    f.params["plane"] = json!(plane);
    f.params["offset"] = json!(offset);
    f.set_sketch(&sketch);
    let id = f.id.clone();
    doc.features.push(f);
    id
}
fn extrude(
    doc: &mut Document,
    name: &str,
    sketch: &str,
    depth: &str,
    mode: &str,
    reverse: bool,
) -> String {
    let mut f = Registry::builtin().unwrap().create("extrude", doc).unwrap();
    f.name = name.into();
    f.params["sketch"] = json!(sketch);
    f.params["depth"] = json!(depth);
    f.params["mode"] = json!(mode);
    f.params["target"] = json!("f2:0");
    f.params["reverse"] = json!(reverse);
    let id = f.id.clone();
    doc.features.push(f);
    id
}
fn housing_base() -> Document {
    let mut d = Document {
        name: "Reyes housing".into(),
        ..Default::default()
    };
    for (name, value) in [
        ("base_width", "6*inch"),
        ("depth", "2.625*inch"),
        ("base_thickness", "0.25*inch"),
        ("main_bore", "2.25*inch"),
        ("body_width", "4*inch"),
        ("height", "3.625*inch"),
    ] {
        d.variables.insert(name.into(), value.into());
    }
    let s = add_sketch(
        &mut d,
        "Base profile",
        "",
        "Top",
        "0",
        rectangle("-#base_width/2", "-#depth/2", "#base_width", "#depth"),
    );
    extrude(&mut d, "Base", &s, "#base_thickness", "New", false);
    let s = add_sketch(
        &mut d,
        "Body profile",
        "f2:end",
        "Top",
        "0",
        rectangle("-#body_width/2", "-#depth/2", "#body_width", "#depth"),
    );
    extrude(&mut d, "Body", &s, "#height", "Add", false);
    d
}

fn fillet(doc: &mut Document, name: &str, edges: Vec<String>, radius: &str) {
    let mut f = Registry::builtin().unwrap().create("fillet", doc).unwrap();
    f.name = name.into();
    f.params["target"] = json!("f2:0");
    f.params["edges"] = json!(edges);
    f.params["radius"] = json!(radius);
    doc.features.push(f);
}
fn circles(values: &[([f64; 2], f64)], dimensions: &[(&str, &str, &str)]) -> Sketch {
    let mut sketch = Sketch::default();
    for ((point, radius), (x, y, diameter)) in values.iter().zip(dimensions) {
        sketch.add_circle(*point, *radius);
        let entity = sketch.next_id;
        for (kind, value) in [("X", *x), ("Y", *y), ("Diameter", *diameter)] {
            sketch.constraints.push(constraint(kind, entity, value));
        }
    }
    sketch
}
fn circle(x: &str, y: &str, diameter: &str) -> Sketch {
    circles(&[([0., 20.], 5.)], &[(x, y, diameter)])
}
fn checked(rt: &mut Runtime, doc: &Document) -> Evaluation {
    let result = rt.evaluate(doc).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    result
}
fn add_round_boss(
    doc: &mut Document,
    name: &str,
    support: &str,
    x: &str,
    y: &str,
    diameter: &str,
    depth: &str,
) {
    let s = add_sketch(
        doc,
        &format!("{name} profile"),
        support,
        "Top",
        "0",
        circle(x, y, diameter),
    );
    extrude(doc, name, &s, depth, "Add", false);
}

fn build_housing() -> (Document, String) {
    let dir = Path::new("output/tutorials/complex");
    let mut doc = housing_base();
    let mut rt = Runtime::new(Registry::builtin().unwrap());
    let base = checked(&mut rt, &doc);
    let vertical: Vec<_> = base.scene.meshes[0]
        .edges
        .iter()
        .filter(|e| {
            e.points.len() == 2
                && (e.points[0][0] - e.points[1][0]).abs() < 1e-5
                && (e.points[0][1] - e.points[1][1]).abs() < 1e-5
                && (e.points[0][2] - e.points[1][2]).abs() > 1.
        })
        .map(|e| e.key.clone())
        .collect();
    assert_eq!(vertical.len(), 8);
    fillet(
        &mut doc,
        "Quarter inch corner fillets",
        vertical,
        "0.25*inch",
    );
    let rounded = checked(&mut rt, &doc);
    let expected = (6. * 2.625 * 0.25 + 4. * 2.625 * 3.625
        - (4. - std::f64::consts::PI) * 0.25_f64.powi(2) * 3.875)
        * INCH.powi(3);
    assert!((rounded.scene.meshes[0].volume - expected).abs() < 0.01);
    let horizontal: Vec<_> = rounded.scene.meshes[0]
        .edges
        .iter()
        .filter(|e| e.points.iter().all(|p| (p[2] - 6.35).abs() < 1e-4))
        .map(|e| e.key.clone())
        .collect();
    fillet(
        &mut doc,
        "Base finishing fillets, blocked",
        horizontal,
        "0.125*inch",
    );
    let attempted = rt.evaluate(&doc).unwrap();
    let error = attempted
        .scene
        .error
        .clone()
        .expect("Base fillet edge identity needs repair before enabling this source stage");
    doc.save(&dir.join("base-fillet-reproduction.cadpart"))
        .unwrap();
    doc.features.last_mut().unwrap().suppressed = true;
    doc.variables
        .insert("front_wall".into(), "0.375*inch".into());
    doc.variables
        .insert("side_wall".into(), "0.375*inch".into());
    let s = add_sketch(
        &mut doc,
        "Interior profile",
        "f4:end",
        "Top",
        "0",
        rectangle(
            "-#body_width/2+#side_wall",
            "-#depth/2+#front_wall",
            "#body_width-2*#side_wall",
            "#depth-2*#front_wall",
        ),
    );
    extrude(&mut doc, "Interior pocket", &s, "3.5*inch", "Remove", true);
    let pocket = checked(&mut rt, &doc);
    let inner_edges: Vec<_> = pocket.scene.meshes[0]
        .edges
        .iter()
        .filter(|e| {
            e.points.len() == 2
                && (e.points[0][0] - e.points[1][0]).abs() < 1e-5
                && (e.points[0][1] - e.points[1][1]).abs() < 1e-5
                && (e.points[0][2] - e.points[1][2]).abs() > 80.
                && (e.points[0][0].abs() as f64 - 1.625 * INCH).abs() < 0.001
        })
        .map(|e| e.key.clone())
        .collect();
    assert_eq!(inner_edges.len(), 4);
    fillet(
        &mut doc,
        "Interior corner fillets",
        inner_edges,
        "0.15*inch",
    );
    checked(&mut rt, &doc);
    // Local support axes follow each original side edge. These coordinates are
    // measured from the registered edge origin rather than from a screen view.
    add_round_boss(
        &mut doc,
        "Front flange",
        "f2:side:1:0",
        "#base_width/2",
        "#base_thickness+#height-1.875*inch",
        "3.25*inch",
        "0.25*inch",
    );
    checked(&mut rt, &doc);
    add_round_boss(
        &mut doc,
        "Back flange",
        "f2:side:1:2",
        "#base_width/2",
        "#base_thickness+#height-1.875*inch",
        "3.25*inch",
        "0.25*inch",
    );
    checked(&mut rt, &doc);
    add_round_boss(
        &mut doc,
        "Right shaft boss",
        "f4:side:1:1",
        "#depth/2",
        "#height-0.75*inch",
        "1*inch",
        "0.5*inch",
    );
    checked(&mut rt, &doc);
    add_round_boss(
        &mut doc,
        "Left shaft boss",
        "f4:side:1:3",
        "#depth/2",
        "#height-0.75*inch",
        "1*inch",
        "0.5*inch",
    );
    checked(&mut rt, &doc);
    let s = add_sketch(
        &mut doc,
        "Main bore profile",
        "f11:end",
        "Top",
        "0",
        circle(
            "#base_width/2",
            "#base_thickness+#height-1.875*inch",
            "#main_bore",
        ),
    );
    extrude(
        &mut doc,
        "Main bore through both flanges",
        &s,
        "#depth+0.5*inch",
        "Remove",
        true,
    );
    checked(&mut rt, &doc);
    let s = add_sketch(
        &mut doc,
        "Shaft bore profile",
        "f15:end",
        "Top",
        "0",
        circle("#depth/2", "#height-0.75*inch", "0.625*inch"),
    );
    extrude(
        &mut doc,
        "Shaft bore through both bosses",
        &s,
        "#body_width+1*inch",
        "Remove",
        true,
    );
    checked(&mut rt, &doc);
    // The book changes front/back wall thickness after creating the first tap.
    // This exercises that same upstream sketch edit before making the patterns.
    doc.variables
        .insert("front_wall".into(), "0.25*inch".into());
    checked(&mut rt, &doc);
    for (side, support) in [("Front", "f11:end"), ("Back", "f13:end")] {
        let mut sketch = Sketch::default();
        for i in 0..6 {
            let angle = std::f64::consts::TAU * i as f64 / 6.;
            sketch.add_circle(
                [
                    (3. + 1.375 * angle.cos()) * INCH,
                    (2. + 1.375 * angle.sin()) * INCH,
                ],
                0.201 * INCH / 2.,
            );
            let entity = sketch.next_id;
            for (kind, value) in [
                ("X", format!("#base_width/2+({})*inch", 1.375 * angle.cos())),
                (
                    "Y",
                    format!(
                        "#base_thickness+#height-1.875*inch+({})*inch",
                        1.375 * angle.sin()
                    ),
                ),
                ("Diameter", "0.201*inch".into()),
            ] {
                sketch.constraints.push(constraint(kind, entity, &value));
            }
        }
        let s = add_sketch(
            &mut doc,
            &format!("{side} flange drill profile"),
            support,
            "Top",
            "0",
            sketch,
        );
        extrude(
            &mut doc,
            &format!("{side} flange six tap drills"),
            &s,
            "#front_wall+0.25*inch",
            "Remove",
            true,
        );
        checked(&mut rt, &doc);
    }
    let mut sketch = Sketch::default();
    for x in [-1., 1.] {
        for y in [-1., 1.] {
            sketch.add_circle([x * 1.75 * INCH, y * 1.0625 * INCH], 0.1065 * INCH / 2.);
            let entity = sketch.next_id;
            for (kind, value) in [
                ("X", format!("{x}*(#body_width/2-0.25*inch)")),
                ("Y", format!("{y}*(#depth/2-0.25*inch)")),
                ("Diameter", "0.1065*inch".into()),
            ] {
                sketch.constraints.push(constraint(kind, entity, &value));
            }
        }
    }
    let s = add_sketch(&mut doc, "Lid drill profile", "f4:end", "Top", "0", sketch);
    extrude(
        &mut doc,
        "Four lid tap drills",
        &s,
        "0.75*inch",
        "Remove",
        true,
    );
    checked(&mut rt, &doc);
    let mut sketch = Sketch::default();
    for x in [-1., 1.] {
        for i in 0..3 {
            sketch.add_circle(
                [x * 2.5 * INCH, (-0.8125 + i as f64 * 0.75) * INCH],
                0.125 * INCH,
            );
            let entity = sketch.next_id;
            for (kind, value) in [
                ("X", format!("{x}*(#base_width/2-0.5*inch)")),
                ("Y", format!("-#depth/2+(0.5+{})*inch", i as f64 * 0.75)),
                ("Diameter", "0.25*inch".into()),
            ] {
                sketch.constraints.push(constraint(kind, entity, &value));
            }
        }
    }
    let s = add_sketch(
        &mut doc,
        "Base mounting drill profile",
        "f2:end",
        "Top",
        "0",
        sketch,
    );
    extrude(
        &mut doc,
        "Six base mounting holes",
        &s,
        "#base_thickness",
        "Remove",
        true,
    );
    checked(&mut rt, &doc);
    (doc, error)
}

fn expected_volume(doc: &Document) -> f64 {
    let numbers = cad::plugins::Numbers::new(&doc.variables).unwrap();
    let value = |name: &str| numbers.eval(name).unwrap() / INCH;
    let bw = value("base_width");
    let d = value("depth");
    let b = value("base_thickness");
    let w = value("body_width");
    let h = value("height");
    let fw = value("front_wall");
    let sw = value("side_wall");
    let bore = value("main_bore");
    let pi = std::f64::consts::PI;
    let rounded_rect = |w: f64, h: f64, r: f64| w * h - (4. - pi) * r * r;
    let stock = rounded_rect(bw, d, 0.25) * b + rounded_rect(w, d, 0.25) * h;
    let pocket = rounded_rect(w - 2. * sw, d - 2. * fw, 0.15) * 3.5;
    let bosses = 2. * pi * (3.25_f64 / 2.).powi(2) * 0.25 + 2. * pi * 0.5_f64.powi(2) * 0.5;
    let bores = pi * (bore / 2.).powi(2) * (2. * fw + 2. * 0.25)
        + pi * (0.625_f64 / 2.).powi(2) * (2. * sw + 2. * 0.5);
    let flange_drills = 12. * pi * (0.201_f64 / 2.).powi(2) * (fw + 0.25);
    let top_drills = 4. * pi * (0.1065_f64 / 2.).powi(2) * 0.75;
    let base_drills = 6. * pi * 0.125_f64.powi(2) * b;
    (stock - pocket + bosses - bores - flange_drills - top_drills - base_drills) * INCH.powi(3)
}
fn verify(result: &Evaluation, doc: &Document) {
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert_eq!(result.scene.meshes.len(), 1);
    assert_eq!(result.scene.sketches.len(), 13);
    assert!(result.scene.sketches.values().all(|s| s.dof == 0));
    assert!(
        doc.features
            .iter()
            .filter_map(|f| f.sketch())
            .all(|s| s.constraints.iter().all(|c| c.kind != "Fix"))
    );
    let expected = expected_volume(doc);
    assert!(
        (result.scene.meshes[0].volume - expected).abs() < 0.01,
        "{} != {expected}",
        result.scene.meshes[0].volume
    );
    let part = &result.geometry.parts[0];
    assert!(
        brepkit_operations::validate::validate_solid(&result.geometry.topo, part.solid)
            .unwrap()
            .is_valid()
    );
    let mut uses = std::collections::BTreeMap::new();
    for triangle in result.scene.meshes[0].indices.as_chunks::<3>().0 {
        for i in 0..3 {
            let a = triangle[i];
            let b = triangle[(i + 1) % 3];
            let edge = uses.entry((a.min(b), a.max(b))).or_insert((0, 0_i32));
            edge.0 += 1;
            edge.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(
        uses.values().all(|&(n, w)| n == 2 && w == 0),
        "Open or inconsistently oriented display mesh"
    );
    let numbers = cad::plugins::Numbers::new(&doc.variables).unwrap();
    for (id, expected_z) in [
        ("f7", numbers.eval("base_thickness+height").unwrap()),
        ("f26", numbers.eval("base_thickness+height").unwrap()),
        ("f28", numbers.eval("base_thickness").unwrap()),
    ] {
        assert!(
            (result.scene.sketches[id].plane.origin[2] - expected_z).abs() < 1e-5,
            "Attached plane {id} did not follow its parent"
        );
    }
}
fn save_checked(doc: &Document, result: &Evaluation, name: &str) {
    let dir = Path::new("output/tutorials/complex");
    let path = dir.join(format!("{name}.cadpart"));
    doc.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened, *doc);
    let result2 = checked(&mut Runtime::new(Registry::builtin().unwrap()), &reopened);
    verify(&result2, &reopened);
    #[cfg(feature = "step")]
    {
        let path = dir.join(format!("{name}.step"));
        result.geometry.export("step", &path).unwrap();
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
        assert!((volume - expected_volume(doc)).abs() < 0.01);
    }
    #[cfg(not(feature = "step"))]
    let _ = result;
}

#[test]
fn reyes_housing_complex_model() {
    let dir = Path::new("output/tutorials/complex");
    std::fs::create_dir_all(dir).unwrap();
    let (mut doc, error) = build_housing();
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = checked(&mut runtime, &doc);
    verify(&result, &doc);
    save_checked(&doc, &result, "housing");
    let baseline = doc.clone();
    let baseline_volume = result.scene.meshes[0].volume;
    let mut edits = vec![];
    for (name, value) in [
        ("depth", "3*inch"),
        ("body_width", "4.5*inch"),
        ("height", "4*inch"),
        ("base_thickness", "0.3*inch"),
        ("main_bore", "2*inch"),
        ("front_wall", "0.3125*inch"),
    ] {
        doc.variables.insert(name.into(), value.into());
        let rebuilt = checked(&mut runtime, &doc);
        verify(&rebuilt, &doc);
        edits.push(json!({"variable":name,"value":value,"volume_mm3":rebuilt.scene.meshes[0].volume,"analytic_mm3":expected_volume(&doc),"rebuild_ms":rebuilt.scene.millis}));
    }
    let resized = checked(&mut runtime, &doc);
    save_checked(&doc, &resized, "housing-resized");
    doc = baseline.clone();
    doc.rollback = Some(4);
    let rollback = checked(&mut runtime, &doc);
    let unrounded = (6. * 2.625 * 0.25 + 4. * 2.625 * 3.625) * INCH.powi(3);
    assert!((rollback.scene.meshes[0].volume - unrounded).abs() < 0.01);
    doc.rollback = None;
    verify(&checked(&mut runtime, &doc), &doc);
    // The lid drilling is an independent finishing operation, suppression must
    // restore exactly the four drilled cylinders without affecting other cuts.
    doc.features[26].suppressed = true;
    let suppressed = checked(&mut runtime, &doc);
    let lid_volume = 4. * std::f64::consts::PI * (0.1065_f64 * INCH / 2.).powi(2) * 0.75 * INCH;
    assert!((suppressed.scene.meshes[0].volume - baseline_volume - lid_volume).abs() < 0.01);
    doc.features[26].suppressed = false;
    // Reordering independent front/back drilling pairs preserves the model.
    doc.features[21..25].rotate_left(2);
    verify(&checked(&mut runtime, &doc), &doc);
    doc = baseline.clone();
    doc.features.swap(0, 1);
    let invalid = runtime.evaluate(&doc).unwrap();
    assert!(invalid.scene.error.as_ref().unwrap().contains("earlier"));
    assert!(invalid.scene.meshes.is_empty());
    doc = baseline.clone();
    doc.features[3].suppressed = true;
    let missing = runtime.evaluate(&doc).unwrap();
    assert!(missing.scene.error.is_some());
    assert_eq!(missing.scene.meshes.len(), 1);
    assert!((missing.scene.meshes[0].volume - 6. * 2.625 * 0.25 * INCH.powi(3)).abs() < 0.01);
    let report = json!({
        "tutorial":"The Housing, Beginner's Guide to SolidWorks 2006", "author":"Alejandro Reyes", "publisher":"SDC Publications", "source":SOURCE,"source_pages":"9-50",
        "verification":"Rust engine integration and file tests, not native UI entry",
        "source_completion":"incomplete: blocked base finishing fillet and omitted final edge finish; functional housing geometry verified separately",
        "functional_geometry":{"timeline_entries":baseline.features.len(),"sketches":result.scene.sketches.len(),"faces":result.scene.meshes[0].faces.len(),"volume_mm3":baseline_volume,"analytic_volume_mm3":expected_volume(&baseline)},
        "known_failure":{"feature":"Base finishing fillets","source_radius_in":0.125,"error":error,"reproduction":"base-fillet-reproduction.cadpart","baseline_handling":"feature preserved and explicitly suppressed"},
        "adaptations":["Interior sketch fillet is implemented as an equivalent 0.15in fillet on four pocket vertical edges", "Feature mirrors and circular/linear cut patterns are expressed as fully dimensioned symmetric sketches because existing pattern/mirror commands operate on whole bodies", "Through-all and up-to-next are expressed by parent dimensions so dependent lengths update", "Wall thickness changes before the drilling patterns here; the source changes it after the first Hole Wizard hole", "Tapped holes use conventional #7 0.201in and #36 0.1065in cylindrical tap drills; threads, cosmetic annotations and drill-point cones are omitted"],
        "tap_drill_reference":"https://cdn.automationdirect.com/static/specs/oldspec/drilltapcombos050823.pdf",
        "not_tested":["Native sketch-entry workflow", "Final source 0.125in finishing edge fillets", "Dimensional tolerance annotations"],
        "checks":["all 13 sketches zero DOF without Fix", "analytic volume of rounded rectangles, bosses, bores and drills", "valid B-rep", "closed oriented mesh", "six sequential parent-variable edits", "attached planes update", "rollback and restoration", "independent cut suppression", "valid independent cut reordering", "invalid dependency reorder fails", "suppressed parent fails while preserving prior solid", "save/reopen and regeneration"],
        "parent_edits":edits, "step_roundtrip_verified":cfg!(feature="step"),"result":"functional baseline passed with documented source exclusions"
    });
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
