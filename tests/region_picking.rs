use cad::{
    engine::Runtime,
    model::{Document, Plane, Sketch},
    plugins::{Numbers, Registry},
};
use serde_json::json;
use std::collections::BTreeMap;

fn check(sketch: &Sketch, expected_cells: usize, selections: &[([f64; 2], f64)]) {
    let resolved = cad::sketch::resolve(
        sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let regions = cad::sketch::regions(&resolved).unwrap();
    assert_eq!(regions.len(), expected_cells, "{regions:?}");
    for &(point, expected_area) in selections {
        let hits: Vec<_> = regions.iter().filter(|r| r.contains(point)).collect();
        assert_eq!(hits.len(), 1, "point{point:?}: {regions:?}");
        let registry = Registry::builtin().unwrap();
        let mut doc = Document::default();
        let mut sf = registry.create("sketch", &mut doc).unwrap();
        sf.set_sketch(sketch);
        let id = sf.id.clone();
        doc.features.push(sf);
        let mut ex = registry.create("extrude", &mut doc).unwrap();
        ex.params["sketch"] = json!(id);
        ex.params["depth"] = json!("3");
        ex.params["regions"] = json!([hits[0].key]);
        doc.features.push(ex);
        let result = Runtime::new(registry).evaluate(&doc).unwrap();
        assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
        let volume: f64 = result.scene.meshes.iter().map(|m| m.volume).sum();
        assert!(
            (volume - expected_area * 3.).abs() < expected_area * 3e-7,
            "point{point:?}: expected{},got{volume}",
            expected_area * 3.
        );
    }
}

#[test]
fn nested_circles_with_nondividing_radial_bridge() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 10.);
    sketch.add_circle([0., 0.], 4.);
    sketch.add_line([4., 0.], [10., 0.]);
    check(
        &sketch,
        2,
        &[
            ([7., 1.], 84. * std::f64::consts::PI),
            ([0., 0.], 16. * std::f64::consts::PI),
        ],
    );
}

#[test]
fn nested_rectangles_with_nondividing_bridge() {
    let mut sketch = Sketch::default();
    sketch.add_rect([-10., -10.], [10., 10.]);
    sketch.add_rect([-4., -4.], [4., 4.]);
    sketch.add_line([4., 0.], [10., 0.]);
    check(&sketch, 2, &[([7., 1.], 336.), ([0., 0.], 64.)]);
}

#[test]
fn four_cells_meet_at_vertex() {
    let mut sketch = Sketch::default();
    sketch.add_rect([-10., -10.], [10., 10.]);
    sketch.add_line([-10., 0.], [10., 0.]);
    sketch.add_line([0., -10.], [0., 10.]);
    check(
        &sketch,
        4,
        &[
            ([-5., 5.], 100.),
            ([5., 5.], 100.),
            ([-5., -5.], 100.),
            ([5., -5.], 100.),
        ],
    );
}

#[test]
fn circle_cell_picking_stays_consistent_on_horizontal_rays_through_vertices() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 5.);
    sketch.add_circle([6., 0.], 5.);
    let resolved = cad::sketch::resolve(
        &sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let regions = cad::sketch::regions(&resolved).unwrap();
    for point in [[-2., 4.], [-2., -4.], [8., 4.], [8., -4.], [3., 0.]] {
        let expected: Vec<_> = regions
            .iter()
            .filter(|r| r.contains([point[0], point[1] + 1e-6]))
            .map(|r| r.key.clone())
            .collect();
        let actual: Vec<_> = regions
            .iter()
            .filter(|r| r.contains(point))
            .map(|r| r.key.clone())
            .collect();
        assert_eq!(actual, expected, "point{point:?}");
        assert_eq!(actual.len(), 1, "point{point:?}");
    }
}

#[test]
fn circle_cells_cover_the_union_once_away_from_boundaries() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 5.);
    sketch.add_circle([6., 0.], 5.);
    let resolved = cad::sketch::resolve(
        &sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let regions = cad::sketch::regions(&resolved).unwrap();
    for xi in -24..=48 {
        for yi in -24..=24 {
            let point = [f64::from(xi) * 0.25, f64::from(yi) * 0.25];
            let distances = [point[0].hypot(point[1]), (point[0] - 6.).hypot(point[1])];
            if distances.iter().any(|r| (*r - 5.).abs() < 1e-6) {
                continue;
            }
            let expected = usize::from(distances.iter().any(|r| *r < 5.));
            let hits = regions.iter().filter(|r| r.contains(point)).count();
            assert_eq!(hits, expected, "point{point:?}");
        }
    }
}

#[test]
fn mixed_line_circle_cells_cover_the_union_once_away_from_boundaries() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 5.);
    sketch.add_rect([0., -4.], [8., 4.]);
    let resolved = cad::sketch::resolve(
        &sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let regions = cad::sketch::regions(&resolved).unwrap();
    for xi in -24..=36 {
        for yi in -24..=24 {
            let point = [f64::from(xi) * 0.25, f64::from(yi) * 0.25];
            let radius = point[0].hypot(point[1]);
            let rectangle_boundary = ((point[0].abs() < 1e-6 || (point[0] - 8.).abs() < 1e-6)
                && (-4. ..=4.).contains(&point[1]))
                || ((point[1].abs() - 4.).abs() < 1e-6 && (0. ..=8.).contains(&point[0]));
            if (radius - 5.).abs() < 1e-6 || rectangle_boundary {
                continue;
            }
            let inside_rectangle = (0. ..8.).contains(&point[0]) && (-4. ..4.).contains(&point[1]);
            let expected = usize::from(radius < 5. || inside_rectangle);
            let hits = regions.iter().filter(|r| r.contains(point)).count();
            assert_eq!(hits, expected, "point {point:?}");
        }
    }
}
