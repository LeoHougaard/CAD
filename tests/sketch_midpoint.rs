use cad::{
    dimensions::Selection,
    model::{Plane, Sketch, SketchConstraint, SketchPointReference},
    plugins::Numbers,
    sketch,
};
use std::collections::BTreeMap;

fn solve(sketch: &Sketch) -> sketch::ResolvedSketch {
    sketch::resolve(
        sketch,
        Plane::named("Top", 0.),
        &Numbers::for_format(&BTreeMap::new(), 2).unwrap(),
    )
    .unwrap()
}

fn assert_point(actual: [f64; 2], expected: [f64; 2]) {
    assert!(
        (actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn midpoint_persists_and_origin_centers_and_endpoints_follow_dimension_edits() {
    let mut s = Sketch::default();
    s.add_line([-10., 0.], [10., 0.]);
    s.add_circle([0., 0.], 2.);
    s.add_line([0., 0.], [0., 20.]);
    s.constraints.extend([
        SketchConstraint {
            kind: "Midpoint".into(),
            a: 0,
            b: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "Horizontal".into(),
            a: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "Length".into(),
            a: 1,
            value: "40".into(),
            ..Default::default()
        },
        SketchConstraint {
            kind: "Midpoint".into(),
            a: 2,
            b: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "Midpoint".into(),
            a: 3,
            b: 1,
            ..Default::default()
        },
    ]);
    let solved = solve(&s);
    assert_point(solved.entities[0].control_points()[0], [-20., 0.]);
    assert_point(solved.entities[0].control_points()[1], [20., 0.]);
    assert_point(solved.entities[1].control_points()[0], [0., 0.]);
    assert_point(solved.entities[2].control_points()[0], [0., 0.]);
    let json = serde_json::to_string(&s).unwrap();
    assert!(!json.contains("\"midpoint\""));
    s = serde_json::from_str(&json).unwrap();
    s.constraints[2].value = "12".into();
    let solved = solve(&s);
    assert_point(solved.entities[0].control_points()[0], [-6., 0.]);
    assert_point(solved.entities[0].control_points()[1], [6., 0.]);
}

#[test]
fn midpoint_arcs_keep_their_directed_branch_after_reopen_and_resize() {
    for clockwise in [false, true] {
        let mut s = Sketch::default();
        s.add_arc([0., 0.], [10., 0.], [0., 10.], clockwise);
        s.add_circle([1., 1.], 1.);
        s.constraints.extend([
            SketchConstraint {
                kind: "Origin".into(),
                a: 1,
                ..Default::default()
            },
            SketchConstraint {
                kind: "StartY".into(),
                a: 1,
                value: "0".into(),
                ..Default::default()
            },
            SketchConstraint {
                kind: "EndX".into(),
                a: 1,
                value: "0".into(),
                ..Default::default()
            },
            SketchConstraint {
                kind: "Radius".into(),
                a: 1,
                value: "20".into(),
                ..Default::default()
            },
            SketchConstraint {
                kind: "Midpoint".into(),
                a: 2,
                b: 1,
                ..Default::default()
            },
        ]);
        s = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        for radius in [20., 30., 7.] {
            s.constraints[3].value = radius.to_string();
            let solved = solve(&s);
            let sign = if clockwise { -1. } else { 1. };
            assert_point(
                solved.entities[1].control_points()[0],
                [sign * radius / 2f64.sqrt(), sign * radius / 2f64.sqrt()],
            );
        }
    }
}

#[test]
fn ordered_three_point_midpoint_survives_rectangle_curve_replacement() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 20.]);
    s.add_circle([20., 0.], 2.);
    s.constraints.push(SketchConstraint {
        kind: "Midpoint".into(),
        a: 1,
        b: 1,
        points: [0, 1],
        midpoint: Some(SketchPointReference {
            entity: 2,
            point: 0,
        }),
        ..Default::default()
    });
    let numbers = Numbers::for_format(&BTreeMap::new(), 2).unwrap();
    let solved = solve(&s);
    let (candidate, solved, _) =
        sketch::set_construction(&s, &solved, &[Selection::edge(1, 0)], Some(true), &numbers)
            .unwrap();
    assert_eq!(
        candidate
            .constraints
            .iter()
            .filter(|c| c.kind == "Midpoint")
            .count(),
        1
    );
    let c = candidate
        .constraints
        .iter()
        .find(|c| c.kind == "Midpoint")
        .unwrap();
    let point = |id, index| {
        solved
            .entities
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .control_points()[index]
    };
    let (a, b) = (point(c.a, c.points[0]), point(c.b, c.points[1]));
    let midpoint = c.midpoint.unwrap();
    assert_point(
        point(midpoint.entity, midpoint.point),
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
    );
    let reopened: Sketch =
        serde_json::from_str(&serde_json::to_string(&candidate).unwrap()).unwrap();
    assert_eq!(reopened, candidate);
    solve(&reopened);
}

#[test]
fn edge_midpoint_survives_construction_conversion_but_not_changed_trim_bounds() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 20.]);
    s.add_circle([20., 0.], 2.);
    s.constraints.push(SketchConstraint {
        kind: "Midpoint".into(),
        a: 2,
        b: 1,
        ..Default::default()
    });
    let numbers = Numbers::for_format(&BTreeMap::new(), 2).unwrap();
    let solved = solve(&s);
    let (candidate, _, _) =
        sketch::set_construction(&s, &solved, &[Selection::edge(1, 0)], Some(true), &numbers)
            .unwrap();
    let c = candidate
        .constraints
        .iter()
        .find(|c| c.kind == "Midpoint")
        .unwrap();
    assert!(
        candidate
            .entities
            .iter()
            .any(|e| e.id == c.b && e.kind == "line")
    );
    let mut s = Sketch::default();
    s.add_line([0., 0.], [40., 0.]);
    s.add_line([10., -10.], [10., 10.]);
    s.add_circle([20., 0.], 2.);
    s.constraints.push(SketchConstraint {
        kind: "Midpoint".into(),
        a: 3,
        b: 1,
        ..Default::default()
    });
    let result = sketch::apply_trim(&s, &solve(&s), 1, 0, [5., 0.], &numbers).unwrap();
    assert!(
        !result
            .sketch
            .constraints
            .iter()
            .any(|c| c.kind == "Midpoint")
    );
}

#[test]
fn standalone_points_are_parametric_constraint_inputs_and_never_profile_edges() {
    let mut s = Sketch::default();
    s.add_rect([-20., -10.], [20., 10.]);
    s.add_point([0., 10.]);
    s.constraints.extend([
        SketchConstraint {
            kind: "Width".into(),
            a: 1,
            value: "40".into(),
            ..Default::default()
        },
        SketchConstraint {
            kind: "Midpoint".into(),
            a: 2,
            b: 1,
            points: [0, 2],
            ..Default::default()
        },
    ]);
    let solved = solve(&s);
    assert_point(solved.entities[1].control_points()[0], [0., 10.]);
    let keys = sketch::regions(&solved)
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 1);
    s.constraints[0].value = "60".into();
    let solved = solve(&s);
    assert_point(solved.entities[1].control_points()[0], [0., 10.]);
    assert_eq!(
        sketch::regions(&solved)
            .unwrap()
            .into_iter()
            .map(|r| r.key)
            .collect::<Vec<_>>(),
        keys
    );
    let s: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_point(solve(&s).entities[1].control_points()[0], [0., 10.]);
}

#[test]
fn fixed_endpoint_anchors_only_that_point_and_survives_upstream_dimension_edits() {
    let mut s = Sketch::default();
    s.add_line([10., 20.], [30., 20.]);
    s.constraints.extend([
        SketchConstraint {
            kind: "FixPoint".into(),
            a: 1,
            points: [0, 0],
            position: Some([10., 20.]),
            ..Default::default()
        },
        SketchConstraint {
            kind: "Horizontal".into(),
            a: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "Length".into(),
            a: 1,
            value: "50".into(),
            ..Default::default()
        },
    ]);
    for length in [50., 10., 75.] {
        s.constraints[2].value = length.to_string();
        let solved = solve(&s);
        assert_point(solved.entities[0].control_points()[0], [10., 20.]);
        assert_point(solved.entities[0].control_points()[1], [10. + length, 20.]);
        assert_eq!(solved.dof, 0);
    }
}

#[test]
fn concentric_tracks_the_declared_endpoint_and_keeps_circle_radius_free() {
    let mut s = Sketch::default();
    s.add_line([10., 20.], [30., 20.]);
    s.add_circle([30., 20.], 3.);
    s.constraints.extend([
        SketchConstraint {
            kind: "FixPoint".into(),
            a: 1,
            position: Some([10., 20.]),
            ..Default::default()
        },
        SketchConstraint {
            kind: "Horizontal".into(),
            a: 1,
            ..Default::default()
        },
        SketchConstraint {
            kind: "Length".into(),
            a: 1,
            value: "40".into(),
            ..Default::default()
        },
        SketchConstraint {
            kind: "Concentric".into(),
            a: 1,
            b: 2,
            points: [1, 0],
            ..Default::default()
        },
    ]);
    let solved = solve(&s);
    assert_point(solved.entities[0].control_points()[0], [10., 20.]);
    assert_point(solved.entities[1].control_points()[0], [50., 20.]);
    assert!((solved.entities[1].values[2] - 3.).abs() < 1e-8);
    assert_eq!(solved.dof, 1);
    assert_eq!(
        serde_json::from_str::<Sketch>(&serde_json::to_string(&s).unwrap()).unwrap(),
        s
    );
}
