use cad::{
    engine::Runtime,
    model::{Document, Plane, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
    sketch,
};
use serde_json::json;
use std::collections::BTreeMap;

fn c(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}
fn numbers() -> Numbers {
    Numbers::new(&BTreeMap::new()).unwrap()
}
fn solve(s: &Sketch) -> sketch::ResolvedSketch {
    sketch::resolve(s, Plane::named("Top", 0.), &numbers()).unwrap()
}

#[test]
fn split_line_preserves_surviving_endpoint_dimensions_and_removes_old_length() {
    let mut s = Sketch::default();
    s.add_line([-10., 0.], [10., 0.]);
    for x in [-5., 5.] {
        s.add_line([x, -5.], [x, 5.]);
        s.entities.last_mut().unwrap().construction = true;
    }
    s.constraints.extend([
        c("Horizontal", 1, ""),
        c("Length", 1, "20"),
        c("X", 1, "-10"),
        c("Y", 1, "0"),
        c("EndX", 1, "10"),
    ]);
    let before = s.clone();
    let trimmed = sketch::apply_trim(&s, &solve(&s), 1, 0, [0., 0.], &numbers()).unwrap();
    assert_eq!(s, before);
    assert_eq!(trimmed.removed_constraints, 1);
    assert!(
        !trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Length")
    );
    let parts = trimmed
        .resolved
        .entities
        .iter()
        .filter(|e| !e.construction)
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].values, vec![-10., 0., -5., 0.]);
    assert_eq!(parts[1].values, vec![5., 0., 10., 0.]);
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Parallel")
    );
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "PointOnLine")
    );
}

#[test]
fn trimmed_circle_keeps_expression_radius_and_center_then_extrudes_exactly() {
    let mut vars = BTreeMap::from([("radius".into(), "5".into())]);
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.entities[0].values[2] = "#radius".into();
    s.add_line([-8., 0.], [8., 0.]);
    s.constraints.push(c("Origin", 1, ""));
    let n = Numbers::new(&vars).unwrap();
    let resolved = sketch::resolve(&s, Plane::named("Top", 0.), &n).unwrap();
    let trimmed = sketch::apply_trim(&s, &resolved, 1, 0, [0., 5.], &n).unwrap();
    assert_eq!(trimmed.sketch.entities[0].kind, "arc");
    assert_eq!(trimmed.removed_constraints, 0);
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Radius" && c.value == "#radius")
    );
    let registry = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut feature = registry.create("sketch", &mut d).unwrap();
    feature.set_sketch(&trimmed.sketch);
    d.features.push(feature);
    let mut feature = registry.create("extrude", &mut d).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!("3");
    d.features.push(feature);
    let mut runtime = Runtime::new(registry);
    for radius in [5., 6.] {
        vars.insert("radius".into(), radius.to_string());
        d.variables = vars.clone();
        let evaluated = runtime.evaluate(&d).unwrap();
        assert!(
            evaluated.scene.error.is_none(),
            "{:?}",
            evaluated.scene.statuses
        );
        assert!(
            (evaluated.scene.meshes[0].volume - 1.5 * std::f64::consts::PI * radius * radius).abs()
                < 1e-5
        );
    }
    let saved: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert!(runtime.evaluate(&saved).unwrap().scene.error.is_none());
    #[cfg(feature = "step")]
    {
        let folder = std::path::Path::new("target/test-output/geometry-controls");
        std::fs::create_dir_all(folder).unwrap();
        saved
            .save(&folder.join("trimmed-semicircle.cadpart"))
            .unwrap();
        runtime
            .evaluate(&saved)
            .unwrap()
            .geometry
            .export("step", &folder.join("trimmed-semicircle.step"))
            .unwrap();
        std::fs::write(folder.join("trimmed-semicircle.json"),serde_json::to_vec_pretty(&json!({
            "case":"trimmed-semicircle","step_exported":true,
            "expected_solids":[{"volume_mm3":1.5*std::f64::consts::PI*36.,"bounds_mm":[[-6.,-6.,0.],[6.,0.,3.]]}]
        })).unwrap()).unwrap();
    }
}

#[test]
fn trim_rectangle_remaps_origin_and_surviving_height_then_closes_and_extrudes() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 20.]);
    s.add_line([10., -5.], [10., 25.]);
    s.entities[1].construction = true;
    s.constraints.extend([
        c("Origin", 1, ""),
        c("Width", 1, "40"),
        c("Height", 1, "20"),
    ]);
    let trimmed = sketch::apply_trim(&s, &solve(&s), 1, 0, [5., 0.], &numbers()).unwrap();
    assert_eq!(trimmed.removed_constraints, 1);
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Origin")
    );
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Length" && c.value == "20")
    );
    let mut closed = trimmed.sketch;
    closed.add_line([0., 0.], [10., 0.]);
    let resolved = solve(&closed);
    assert_eq!(sketch::regions(&resolved).unwrap().len(), 1);
    let registry = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut feature = registry.create("sketch", &mut d).unwrap();
    feature.set_sketch(&closed);
    d.features.push(feature);
    let mut feature = registry.create("extrude", &mut d).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!("2");
    d.features.push(feature);
    let mut runtime = Runtime::new(registry);
    let evaluated = runtime.evaluate(&d).unwrap();
    assert!(
        evaluated.scene.error.is_none(),
        "{:?}",
        evaluated.scene.statuses
    );
    assert!((evaluated.scene.meshes[0].volume - 1600.).abs() < 1e-6);
}

#[test]
fn failed_trim_does_not_modify_constraints_or_geometry() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.add_circle([0., 0.], 5.);
    s.constraints.push(c("Radius", 1, "5"));
    let before = s.clone();
    assert!(sketch::apply_trim(&s, &solve(&s), 1, 0, [0., 5.], &numbers()).is_err());
    assert_eq!(s, before);
}

#[test]
fn split_tangent_line_keeps_the_shared_arc_endpoint_on_its_actual_survivor() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [30., 0.]);
    s.add_arc([30., 10.], [30., 0.], [40., 10.], false);
    for x in [10., 20.] {
        s.add_line([x, -5.], [x, 5.]);
        s.entities.last_mut().unwrap().construction = true;
    }
    s.constraints.extend([
        c("Horizontal", 1, ""),
        c("Origin", 1, ""),
        c("EndX", 1, "30"),
        c("Radius", 2, "10"),
        SketchConstraint {
            kind: "Tangent".into(),
            a: 1,
            b: 2,
            points: [1, 1],
            ..Default::default()
        },
    ]);
    let trimmed = sketch::apply_trim(&s, &solve(&s), 1, 0, [15., 0.], &numbers()).unwrap();
    assert_eq!(trimmed.removed_constraints, 0);
    let tangent = trimmed
        .sketch
        .constraints
        .iter()
        .find(|c| c.kind == "Tangent")
        .unwrap();
    assert_ne!(
        tangent.a, 1,
        "The shared endpoint belongs to the second survivor"
    );
    let line = trimmed
        .resolved
        .entities
        .iter()
        .find(|e| e.id == tangent.a)
        .unwrap();
    assert_eq!(line.values, vec![20., 0., 30., 0.]);
    let arc = trimmed
        .resolved
        .entities
        .iter()
        .find(|e| e.id == 2)
        .unwrap();
    assert!((arc.values[2] - 30.).abs() < 1e-7 && arc.values[3].abs() < 1e-7);
}

#[test]
fn new_trim_endpoints_follow_their_construction_bounds_after_dimension_edits() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [30., 0.]);
    s.add_line([10., -10.], [10., 10.]);
    s.entities[1].construction = true;
    s.constraints.extend([
        c("Origin", 1, ""),
        c("Horizontal", 1, ""),
        c("X", 2, "10"),
        c("Vertical", 2, ""),
    ]);
    let trimmed = sketch::apply_trim(&s, &solve(&s), 1, 0, [20., 0.], &numbers()).unwrap();
    assert!(
        trimmed
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "PointOnLine" && c.a == 1 && c.b == 2)
    );
    let mut updated = trimmed.sketch;
    updated
        .constraints
        .iter_mut()
        .find(|c| c.kind == "X" && c.a == 2)
        .unwrap()
        .value = "15".into();
    let solved = solve(&updated);
    assert!((solved.entities[0].values[2] - 15.).abs() < 1e-7);
    assert!(solved.entities[0].values[3].abs() < 1e-7);
}
