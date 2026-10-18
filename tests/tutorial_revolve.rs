use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::json;
use std::{f64::consts::PI, path::Path};

const SOURCE: &str = "https://resources.finalsite.net/images/v1722986533/brightonk12com/ywtdpiqjzrokemitsmv4/tutorialonshaperevolvesolidmodel_1.pdf";
const INCH: f64 = 25.4;

fn dimension(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}

// PDF page 4, dimensions in inches. Sketch coordinates are radius and height.
fn tutorial(registry: &Registry) -> Document {
    let mut doc = Document {
        name: "Jourden revolve profile".into(),
        ..Default::default()
    };
    doc.variables
        .insert("center_diameter".into(), "1.75*inch".into());
    let vertices = [
        [0., 0.5],
        [2., 0.5],
        [2., 0.],
        [2.5, 0.],
        [2.5, 1.],
        [0.875, 1.],
        [0.875, 2.],
        [0.25, 2.],
        [0.25, 1.25],
        [0., 1.25],
    ];
    let mut sketch = Sketch::default();
    for i in 0..vertices.len() {
        let a = vertices[i].map(|x| x * INCH);
        let b = vertices[(i + 1) % vertices.len()].map(|x| x * INCH);
        sketch.add_line(a, b);
        sketch.constraints.push(dimension(
            if a[1] == b[1] {
                "Horizontal"
            } else {
                "Vertical"
            },
            (i + 1) as u64,
            "",
        ));
    }
    // Ten independent coordinates fully constrain the orthogonal closed profile.
    // Onshape's doubled axis dimensions are represented by radius expressions.
    for (kind, entity, value) in [
        ("X", 1, "0"),
        ("X", 2, "4*inch/2"),
        ("X", 4, "5*inch/2"),
        ("X", 7, "center_diameter/2"),
        ("X", 9, "0.5*inch/2"),
        ("Y", 3, "0"),
        ("Y", 1, "0.5*inch"),
        ("Y", 6, "1*inch"),
        ("Y", 8, "2*inch"),
        ("Y", 10, "(2-0.75)*inch"),
    ] {
        sketch.constraints.push(dimension(kind, entity, value));
    }
    sketch.add_line([0., -0.5 * INCH], [0., 2.5 * INCH]);
    sketch.entities.last_mut().unwrap().construction = true;
    sketch.constraints.push(dimension("Fix", 11, ""));
    let mut feature = registry.create("sketch", &mut doc).unwrap();
    feature.name = "Revolve Profile".into();
    feature.params["plane"] = json!("Front");
    feature.set_sketch(&sketch);
    doc.features.push(feature);
    let mut feature = registry.create("revolve", &mut doc).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["axis"] = json!("V");
    doc.features.push(feature);
    doc
}

fn analytic_volume(center_diameter_inches: f64, angle: f64) -> f64 {
    let r2 = (center_diameter_inches / 2.).powi(2);
    PI * ((2.5_f64.powi(2) - 2_f64.powi(2)) * 0.5
        + 2.5_f64.powi(2) * 0.5
        + r2 * 0.25
        + (r2 - 0.25_f64.powi(2)) * 0.75)
        * INCH.powi(3)
        * angle
        / 360.
}

fn add_bolt_hole(doc: &mut Document, registry: &Registry) {
    doc.variables.insert("bolt_circle".into(), "3*inch".into());
    doc.variables
        .insert("hole_diameter".into(), "0.5*inch".into());
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 1.5 * INCH);
    sketch.entities.last_mut().unwrap().construction = true;
    sketch.add_line([0., 0.], [0., 2. * INCH]);
    sketch.entities.last_mut().unwrap().construction = true;
    sketch.add_circle([0., 1.5 * INCH], 0.25 * INCH);
    sketch.constraints.extend([
        dimension("Origin", 1, ""),
        dimension("Diameter", 1, "bolt_circle"),
        dimension("Fix", 2, ""),
        SketchConstraint {
            kind: "PointOnCircle".into(),
            a: 3,
            b: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "PointOnLine".into(),
            a: 3,
            b: 2,
            ..Default::default()
        },
        dimension("Diameter", 3, "hole_diameter"),
    ]);
    let mut feature = registry.create("sketch", doc).unwrap();
    feature.name = "Bolt Holes".into();
    // Revolve has no face anchors yet. Keep this offset parametric instead.
    feature.params["plane"] = json!("Top");
    feature.params["offset"] = json!("1*inch");
    feature.set_sketch(&sketch);
    doc.features.push(feature);
    let mut feature = registry.create("extrude", doc).unwrap();
    feature.name = "Bolt Hole Cut".into();
    feature.params["sketch"] = json!("f3");
    feature.params["mode"] = json!("Remove");
    feature.params["depth"] = json!("0.5*inch");
    feature.params["reverse"] = json!(true);
    doc.features.push(feature);
}

fn check(result: &Evaluation, expected: f64) {
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert_eq!(result.geometry.parts.len(), 1);
    assert_eq!(result.scene.sketches["f1"].dof, 0);
    let mesh = &result.scene.meshes[0];
    assert!(
        (mesh.volume - expected).abs() < expected * 1e-6,
        "volume {} expected {expected}",
        mesh.volume
    );
    assert!(!mesh.indices.is_empty());
    assert!(mesh.positions.iter().flatten().all(|x| x.is_finite()));
    let validation = brepkit_operations::validate::validate_solid(
        &result.geometry.topo,
        result.geometry.parts[0].solid,
    )
    .unwrap();
    assert!(validation.is_valid(), "{:?}", validation.issues);
}

fn save_case(doc: &Document, result: &Evaluation, dir: &Path, name: &str) {
    let path = dir.join(format!("{name}.cadpart"));
    doc.save(&path).unwrap();
    assert_eq!(Document::load(&path).unwrap(), *doc);
    #[cfg(feature = "step")]
    {
        let path = dir.join(format!("{name}.step"));
        result.geometry.export("step", &path).unwrap();
        let mut topo = brepkit_topology::Topology::new();
        let solids =
            brepkit_io::step::read_step(&std::fs::read_to_string(path).unwrap(), &mut topo)
                .unwrap();
        assert_eq!(solids.len(), 1);
        let volume = brepkit_operations::measure::solid_volume(&topo, solids[0], 0.03).unwrap();
        assert!((volume - result.scene.meshes[0].volume).abs() < volume * 1e-6);
    }
    #[cfg(not(feature = "step"))]
    let _ = result;
}

#[test]
fn jourden_dimensioned_revolve_parametric_edit_partial_angle_and_roundtrip() {
    let registry = Registry::builtin().unwrap();
    let mut doc = tutorial(&registry);
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let dir = Path::new("output/tutorials/revolve");
    std::fs::create_dir_all(dir).unwrap();
    let full = runtime.evaluate(&doc).unwrap();
    check(&full, analytic_volume(1.75, 360.));
    save_case(&doc, &full, dir, "revolve-full");
    doc.features[1].params["angle"] = json!("180");
    let half = runtime.evaluate(&doc).unwrap();
    check(&half, analytic_volume(1.75, 180.));
    assert_eq!(half.scene.reused_features, 1);
    save_case(&doc, &half, dir, "revolve-half");
    doc.features[1].params["angle"] = json!("360");
    doc.variables
        .insert("center_diameter".into(), "1.50*inch".into());
    let edited = runtime.evaluate(&doc).unwrap();
    check(&edited, analytic_volume(1.50, 360.));
    assert_eq!(edited.scene.reused_features, 0);
    save_case(&doc, &edited, dir, "revolve-edited");
    doc.variables
        .insert("center_diameter".into(), "1.75*inch".into());
    let restored = runtime.evaluate(&doc).unwrap();
    check(&restored, analytic_volume(1.75, 360.));
    assert!((restored.scene.meshes[0].volume - full.scene.meshes[0].volume).abs() < 1e-7);
    let loaded = Document::load(&dir.join("revolve-edited.cadpart")).unwrap();
    check(
        &runtime.evaluate(&loaded).unwrap(),
        analytic_volume(1.50, 360.),
    );
    add_bolt_hole(&mut doc, &registry);
    let hole = runtime.evaluate(&doc).unwrap();
    check(
        &hole,
        analytic_volume(1.75, 360.) - PI * (0.25 * INCH).powi(2) * 0.5 * INCH,
    );
    assert_eq!(hole.scene.sketches["f3"].dof, 0);
    save_case(&doc, &hole, dir, "revolve-one-hole");
    doc.variables
        .insert("hole_diameter".into(), "0.375*inch".into());
    doc.variables
        .insert("bolt_circle".into(), "2.75*inch".into());
    let hole_edited = runtime.evaluate(&doc).unwrap();
    check(
        &hole_edited,
        analytic_volume(1.75, 360.) - PI * (0.375 / 2. * INCH).powi(2) * 0.5 * INCH,
    );
    let hole_entity = &hole_edited.scene.sketches["f3"].entities[2];
    assert!(hole_entity.values[0].abs() < 1e-7);
    assert!((hole_entity.values[1] - 2.75 / 2. * INCH).abs() < 1e-6);
    assert!((hole_entity.values[2] - 0.375 / 2. * INCH).abs() < 1e-6);
    save_case(&doc, &hole_edited, dir, "revolve-one-hole-edited");
    let report = json!({
        "source": SOURCE,
        "title": "Tutorial OnShape Revolve Solid Model",
        "author": "Matthew Jourden, Brighton High School",
        "scope": "Dimensioned profile, revolve and first bolt hole, PDF pages 2-7. Full tutorial remains incomplete.",
        "method": "Automated document, sketch-solver and geometry APIs. Native GUI not exercised by this test.",
        "units": "Source inches converted through inch = 25.4 mm expressions",
        "passed": ["closed orthogonal profile, 0 DOF", "full 360 degree revolve", "180 degree revolve", "analytic shell-integral volume", "valid solid topology", "upstream diameter edit and restore", "history prefix reuse for angle edit", "CAD document roundtrip", "STEP export and same-kernel import volume", "construction circle and line locate first bolt hole, 0 DOF", "first hole cut", "hole diameter and bolt-circle edits propagate"],
        "adaptations": ["Sketch V axis replaces selecting arbitrary construction line", "Radius expressions replace doubled diameter dimensions to an axis", "Source initial and final center cylinder diameters both read 1.75 inches. Parametric edit tested 1.75 -> 1.50 -> 1.75 inches.", "Bolt-hole sketch uses offset Top plane because revolve does not register stable planar face anchors", "Cut uses explicit 0.5 inch depth rather than through-all termination"],
        "blocked": ["Circular feature pattern of eight bolt holes is unavailable", "Tutorial downstream rounds and fillets not tested on the incomplete patterned part"],
        "volumes_mm3": {"full": full.scene.meshes[0].volume, "half": half.scene.meshes[0].volume, "edited": edited.scene.meshes[0].volume, "one_hole": hole.scene.meshes[0].volume, "one_hole_edited": hole_edited.scene.meshes[0].volume},
        "regeneration_ms": {"full_debug": full.scene.millis, "half_debug": half.scene.millis, "edited_debug": edited.scene.millis},
    });
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
