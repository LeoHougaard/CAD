use cad::{
    dimensions::{self, Selection},
    engine::Runtime,
    model::{Document, Plane, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
    sketch,
};
use serde_json::json;
use std::collections::BTreeMap;

fn constraint(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}
fn solve(sketch: &Sketch) -> sketch::ResolvedSketch {
    sketch::resolve(
        sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap()
}

#[test]
fn free_rectangle_dimension_changes_preserve_center_orientation_and_other_entities() {
    let mut sketch = Sketch::default();
    sketch.add_rect([15., 25.], [55., 45.]);
    sketch.add_circle([100., 80.], 7.);
    sketch.constraints.push(constraint("Width", 1, "40"));
    sketch.constraints.push(constraint("Height", 1, "20"));
    for width in [70., 5., 120., 40.] {
        let prior = solve(&sketch);
        for (entity, solved) in sketch.entities.iter_mut().zip(&prior.entities) {
            for (value, &number) in entity.values.iter_mut().zip(&solved.values) {
                *value = number.to_string();
            }
        }
        sketch.constraints[0].value = width.to_string();
        let resolved = solve(&sketch);
        let v = &resolved.entities[0].values;
        assert!((v[0] + v[2] / 2. - 35.).abs() < 1e-7, "{v:?}");
        assert!(
            (v[1] - 25.).abs() < 1e-7 && (v[3] - 20.).abs() < 1e-7,
            "{v:?}"
        );
        assert!((v[2] - width).abs() < 1e-7);
        assert_eq!(resolved.entities[1].values, vec![100., 80., 7.]);
        assert_eq!(resolved.dof, 5);
    }
}

#[test]
fn arc_radius_dimension_and_tangent_endpoint_survive_upstream_edits() {
    let mut sketch = Sketch::default();
    sketch.add_arc([0., 0.], [10., 0.], [0., 10.], false);
    sketch.add_line([10., 0.], [10., -20.]);
    sketch.constraints.extend([
        constraint("Origin", 1, ""),
        constraint("Radius", 1, "10"),
        constraint("Vertical", 2, ""),
        SketchConstraint {
            kind: "Tangent".into(),
            a: 1,
            b: 2,
            points: [1, 0],
            ..Default::default()
        },
    ]);
    let first = solve(&sketch);
    assert_eq!(
        dimensions::infer(&first, &[Selection::edge(1, 0)], [15., 15.])
            .unwrap()
            .kind,
        "Radius"
    );
    sketch.constraints[1].value = "25".into();
    let second = solve(&sketch);
    let arc = second.entities[0].arc().unwrap();
    let line = &second.entities[1].values;
    assert!((arc.radius - 25.).abs() < 1e-7);
    assert!((second.entities[0].values[2] - line[0]).abs() < 1e-7);
    assert!((second.entities[0].values[3] - line[1]).abs() < 1e-7);
    assert!((line[0] - line[2]).abs() < 1e-7);
    assert_eq!(first.dof, second.dof);
}

#[test]
fn exact_arc_profile_extrudes_and_dimension_edit_preserves_region_identity() {
    let registry = Registry::builtin().unwrap();
    let mut document = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_arc([0., 0.], [10., 0.], [-10., 0.], false);
    sketch.add_line([-10., 0.], [10., 0.]);
    sketch.constraints.extend([
        constraint("Origin", 1, ""),
        constraint("Radius", 1, "10"),
        SketchConstraint {
            kind: "Y".into(),
            a: 1,
            points: [1, 0],
            value: "0".into(),
            ..Default::default()
        },
        constraint("Horizontal", 2, ""),
        SketchConstraint {
            kind: "Coincident".into(),
            a: 1,
            b: 2,
            points: [1, 1],
            ..Default::default()
        },
        SketchConstraint {
            kind: "Coincident".into(),
            a: 1,
            b: 2,
            points: [2, 0],
            ..Default::default()
        },
    ]);
    let first = solve(&sketch);
    let regions = sketch::regions(&first).unwrap();
    assert_eq!(regions.len(), 1);
    let region = regions[0].key.clone();
    let mut feature = registry.create("sketch", &mut document).unwrap();
    feature.set_sketch(&sketch);
    document.features.push(feature);
    let mut feature = registry.create("extrude", &mut document).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!("4");
    feature.params["regions"] = json!([region]);
    document.features.push(feature);
    let mut runtime = Runtime::new(registry);
    for radius in [10., 15.] {
        sketch.constraints[1].value = radius.to_string();
        document.features[0].set_sketch(&sketch);
        let result = runtime.evaluate(&document).unwrap();
        assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
        let mesh = &result.scene.meshes[0];
        assert!(
            (mesh.volume - 2. * std::f64::consts::PI * radius * radius).abs() < 1e-5,
            "{}",
            mesh.volume
        );
        assert_eq!(
            mesh.faces.len(),
            4,
            "Arc extrusion must retain a single analytic cylindrical side"
        );
        assert_eq!(
            sketch::regions(&result.scene.sketches["f1"]).unwrap()[0].key,
            region
        );
    }
    let reopened: Document =
        serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
    assert!(runtime.evaluate(&reopened).unwrap().scene.error.is_none());
}

#[test]
fn major_arcs_keep_direction_and_construction_arcs_do_not_make_regions() {
    let mut sketch = Sketch::default();
    sketch.add_arc([0., 0.], [10., 0.], [0., 10.], true);
    let arc = solve(&sketch).entities[0].arc().unwrap();
    assert!((arc.sweep + 1.5 * std::f64::consts::PI).abs() < 1e-12);
    sketch.entities[0].construction = true;
    sketch.add_rect([20., 20.], [30., 30.]);
    assert_eq!(sketch::regions(&solve(&sketch)).unwrap().len(), 1);
}
