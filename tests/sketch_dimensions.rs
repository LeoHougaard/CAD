use cad::{
    dimensions::{self, Selection},
    model::{Plane, Sketch, SketchConstraint},
    plugins::Numbers,
    sketch,
};
use std::collections::BTreeMap;
fn solve(s: &Sketch) -> cad::model::Result<sketch::ResolvedSketch> {
    sketch::resolve(
        s,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
}
fn dimension(kind: &str, a: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a,
        value: value.into(),
        ..Default::default()
    }
}

#[test]
fn rectangle_starts_free_then_origin_width_height_fully_define_it() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [24., 18.]);
    assert_eq!(solve(&s).unwrap().dof, 4);
    s.constraints.push(SketchConstraint {
        kind: "Coincident".into(),
        a: 1,
        b: 0,
        ..Default::default()
    });
    assert_eq!(solve(&s).unwrap().dof, 2);
    s.constraints.push(dimension("Width", 1, "40"));
    assert_eq!(solve(&s).unwrap().dof, 1);
    s.constraints.push(dimension("Height", 1, "30"));
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 0);
    assert!(x.entities[0].fixed);
    assert!((x.entities[0].values[2] - 40.).abs() < 1e-6);
    assert!((x.entities[0].values[3] - 30.).abs() < 1e-6);
    s.constraints[1].value = "55".into();
    let x = solve(&s).unwrap();
    assert!((x.entities[0].values[0]).abs() < 1e-6);
    assert!((x.entities[0].values[2] - 55.).abs() < 1e-6);
}

#[test]
fn circle_center_and_diameter_are_independent_degrees_of_freedom() {
    let mut s = Sketch::default();
    s.add_circle([12., 7.], 5.);
    assert_eq!(solve(&s).unwrap().dof, 3);
    s.constraints.push(dimension("X", 1, "20"));
    s.constraints.push(dimension("Y", 1, "10"));
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 1);
    assert!(!x.entities[0].fixed);
    s.constraints.push(dimension("Diameter", 1, "14"));
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 0);
    assert!(x.entities[0].fixed);
    assert!((x.entities[0].values[2] - 7.).abs() < 1e-7);
}

#[test]
fn equality_and_concentricity_propagate_dimension_edits() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 6.);
    s.add_circle([20., 8.], 4.);
    s.constraints.push(SketchConstraint {
        kind: "Origin".into(),
        a: 1,
        ..Default::default()
    });
    s.constraints.push(dimension("Radius", 1, "8"));
    s.constraints.push(SketchConstraint {
        kind: "Equal".into(),
        a: 1,
        b: 2,
        ..Default::default()
    });
    let x = solve(&s).unwrap();
    assert!((x.entities[1].values[2] - 8.).abs() < 1e-7);
    assert_eq!(x.dof, 2);
    assert!(x.entities[0].fixed);
    assert!(!x.entities[1].fixed);
    s.constraints[2] = SketchConstraint {
        kind: "Concentric".into(),
        a: 1,
        b: 2,
        ..Default::default()
    };
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 1);
    assert!(x.entities[1].values[0].abs() < 1e-7);
    assert!(x.entities[1].values[1].abs() < 1e-7);
}

#[test]
fn tangent_circle_keeps_its_driving_radius() {
    let mut s = Sketch::default();
    s.add_line([-30., 0.], [30., 0.]);
    s.add_circle([4., 8.], 3.);
    s.constraints.push(SketchConstraint {
        kind: "Fix".into(),
        a: 1,
        ..Default::default()
    });
    s.constraints.push(dimension("Radius", 2, "5"));
    s.constraints.push(SketchConstraint {
        kind: "Tangent".into(),
        a: 1,
        b: 2,
        ..Default::default()
    });
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 1);
    assert!((x.entities[1].values[1] - 5.).abs() < 1e-6);
    assert!((x.entities[1].values[2] - 5.).abs() < 1e-7);
}

#[test]
fn line_equality_and_angle_drive_a_connected_profile() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [20., 0.]);
    s.add_line([20., 0.], [23., 14.]);
    s.constraints.push(SketchConstraint {
        kind: "Origin".into(),
        a: 1,
        ..Default::default()
    });
    s.constraints.push(SketchConstraint {
        kind: "Horizontal".into(),
        a: 1,
        ..Default::default()
    });
    s.constraints.push(dimension("Length", 1, "25"));
    s.constraints.push(SketchConstraint {
        kind: "Equal".into(),
        a: 1,
        b: 2,
        ..Default::default()
    });
    s.constraints.push(SketchConstraint {
        kind: "Angle".into(),
        a: 1,
        b: 2,
        value: "90".into(),
        ..Default::default()
    });
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 0);
    assert!((x.entities[1].values[3] - 25.).abs() < 1e-6);
}

#[test]
fn reference_dimensions_measure_without_changing_geometry() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.constraints.push(SketchConstraint {
        kind: "Diameter".into(),
        a: 1,
        value: "200".into(),
        reference: true,
        ..Default::default()
    });
    let x = solve(&s).unwrap();
    assert_eq!(x.dof, 3);
    assert!((dimensions::measure(&x, &s.constraints[0]).unwrap() - 10.).abs() < 1e-8);
}

#[test]
fn dimension_tool_infers_edges_circles_points_and_parallel_lines() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 30.]);
    s.add_circle([60., 15.], 5.);
    let x = solve(&s).unwrap();
    assert_eq!(
        dimensions::infer(&x, &[Selection::edge(1, 0)], [20., -10.])
            .unwrap()
            .kind,
        "Width"
    );
    assert_eq!(
        dimensions::infer(&x, &[Selection::edge(1, 1)], [50., 15.])
            .unwrap()
            .kind,
        "Height"
    );
    assert_eq!(
        dimensions::infer(&x, &[Selection::edge(2, 0)], [70., 25.])
            .unwrap()
            .kind,
        "Diameter"
    );
    let points = [Selection::point(1, 0), Selection::point(2, 0)];
    assert_eq!(
        dimensions::infer(&x, &points, [30., 70.]).unwrap().kind,
        "DistanceX"
    );
    assert_eq!(
        dimensions::infer(&x, &points, [100., 8.]).unwrap().kind,
        "DistanceY"
    );
    assert_eq!(
        dimensions::infer(
            &x,
            &[Selection::edge(1, 0), Selection::edge(1, 2)],
            [50., 15.]
        )
        .unwrap()
        .kind,
        "LineDistance"
    );
}

#[test]
fn conflicting_driving_dimensions_fail_and_reference_recovers() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.constraints.push(dimension("Radius", 1, "5"));
    s.constraints.push(dimension("Diameter", 1, "12"));
    assert!(solve(&s).is_err());
    s.constraints[1].reference = true;
    assert!(solve(&s).is_ok());
}

#[test]
fn named_dimensions_accept_onshape_style_variable_expressions() {
    let n = Numbers::new(&BTreeMap::from([("width".into(), "80".into())])).unwrap();
    assert_eq!(n.eval("=#width / 2").unwrap(), 40.);
}

#[test]
fn placed_dimension_labels_follow_their_geometry_after_an_edit() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 30.]);
    s.constraints.push(dimension("X", 1, "0"));
    s.constraints.push(dimension("Y", 1, "0"));
    s.constraints.push(dimension("Width", 1, "40"));
    s.constraints.push(dimension("Height", 1, "30"));
    let first = solve(&s).unwrap();
    let mut c = dimensions::infer(&first, &[Selection::edge(1, 1)], [50., 15.]).unwrap();
    c.reference = false;
    let original = dimensions::label_position(&first, &c);
    assert_eq!(original, [50., 15.]);
    s.constraints[2].value = "60".into();
    let next = solve(&s).unwrap();
    let p = dimensions::label_position(&next, &c);
    assert!((p[0] - 70.).abs() < 1e-6);
    assert!((p[1] - 15.).abs() < 1e-6);
}
