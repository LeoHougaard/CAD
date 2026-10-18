use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Plane, Sketch},
    plugins::{Numbers, Registry},
    sketch::{ResolvedRegion, ResolvedSketch},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn resolved(sketch: &Sketch) -> ResolvedSketch {
    cad::sketch::resolve(
        sketch,
        Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap()
}
fn regions(sketch: &Sketch) -> Vec<ResolvedRegion> {
    cad::sketch::regions(&resolved(sketch)).unwrap()
}
fn at(regions: &[ResolvedRegion], point: [f64; 2]) -> String {
    let hits: Vec<_> = regions.iter().filter(|r| r.contains(point)).collect();
    assert_eq!(hits.len(), 1, "point {point:?}: {regions:?}");
    hits[0].key.clone()
}
fn document(sketch: &Sketch, selection: Value) -> Document {
    let registry = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut f = registry.create("sketch", &mut d).unwrap();
    f.set_sketch(sketch);
    let id = f.id.clone();
    d.features.push(f);
    let mut f = registry.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!(id);
    f.params["depth"] = json!("3");
    f.params["regions"] = selection;
    d.features.push(f);
    d
}
fn evaluate(doc: &Document) -> Evaluation {
    Runtime::new(Registry::builtin().unwrap())
        .evaluate(doc)
        .unwrap()
}
fn volume(doc: &Document, expected: f64, parts: usize) {
    let result = evaluate(doc);
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert_eq!(result.geometry.parts.len(), parts);
    let actual: f64 = result.scene.meshes.iter().map(|m| m.volume).sum();
    assert!(
        (actual - expected).abs() < expected.abs().max(1.) * 1e-7,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn disjoint_regions_select_subset_and_survive_entity_reordering_and_reopen() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 8.]);
    s.add_circle([25., 0.], 4.);
    s.add_circle([40., 0.], 2.);
    let r = regions(&s);
    assert_eq!(r.len(), 3);
    let circle = at(&r, [25., 0.]);
    let rectangle = at(&r, [3., 3.]);
    volume(
        &document(&s, json!([circle])),
        std::f64::consts::PI * 16. * 3.,
        1,
    );
    let mut d = document(&s, json!([rectangle.clone(), circle.clone()]));
    volume(&d, (80. + std::f64::consts::PI * 16.) * 3., 2);
    s.entities.reverse();
    s.entities.iter_mut().find(|e| e.id == 2).unwrap().values[2] = "5".into();
    assert!(regions(&s).iter().any(|r| r.key == circle));
    d.features[0].set_sketch(&s);
    let path = std::path::Path::new("target/test-output/selected-regions.cadpart");
    d.save(path).unwrap();
    volume(
        &Document::load(path).unwrap(),
        (80. + std::f64::consts::PI * 25.) * 3.,
        2,
    );
}

#[test]
fn whole_sketch_overlapping_rectangles_extrude_their_exact_union() {
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [10., 10.]);
    sketch.add_rect([5., 5.], [15., 15.]);
    let mut doc = document(&sketch, Value::Null);
    doc.features[1].params["depth"] = json!("5");
    volume(&doc, 875., 1);
    doc.features[1]
        .params
        .as_object_mut()
        .unwrap()
        .remove("regions");
    volume(&doc, 875., 1);
}

#[test]
fn nested_discs_and_annuli_are_individual_cells_and_adjacent_selection_unites() {
    let mut s = Sketch::default();
    for r in [10., 6., 2.] {
        s.add_circle([0., 0.], r);
    }
    let cells = regions(&s);
    assert_eq!(cells.len(), 3);
    assert_eq!(cells.iter().filter(|r| r.legacy_selected).count(), 2);
    let annulus = at(&cells, [8., 0.]);
    let middle = at(&cells, [4., 0.]);
    let disc = at(&cells, [0., 0.]);
    let pi = std::f64::consts::PI;
    volume(&document(&s, json!([middle])), pi * 32. * 3., 1);
    volume(&document(&s, json!([disc])), pi * 4. * 3., 1);
    volume(
        &document(&s, json!([annulus.clone(), at(&cells, [4., 0.])])),
        pi * 96. * 3.,
        1,
    );
    volume(
        &document(
            &s,
            json!(cells.iter().map(|r| r.key.clone()).collect::<Vec<_>>()),
        ),
        pi * 100. * 3.,
        1,
    );
    volume(&document(&s, Value::Null), pi * 68. * 3., 2);
}

#[test]
fn shared_rectangle_edges_and_dividing_lines_form_pickable_exact_cells() {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 10.]);
    s.add_rect([10., 0.], [20., 10.]);
    s.add_line([5., 0.], [5., 10.]);
    let cells = regions(&s);
    assert_eq!(cells.len(), 3);
    let left = at(&cells, [2., 5.]);
    let middle = at(&cells, [8., 5.]);
    let right = at(&cells, [15., 5.]);
    volume(&document(&s, json!([middle])), 150., 1);
    volume(&document(&s, json!([left, right])), 450., 2);
    volume(
        &document(
            &s,
            json!(cells.iter().map(|r| r.key.clone()).collect::<Vec<_>>()),
        ),
        600.,
        1,
    );
}

#[test]
fn intersecting_circles_keep_exact_arcs_and_select_lens_or_union() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.add_circle([6., 0.], 5.);
    let cells = regions(&s);
    assert_eq!(cells.len(), 3);
    let lens = at(&cells, [3., 0.]);
    let left = at(&cells, [-2., 0.]);
    let right = at(&cells, [8., 0.]);
    let area = 50. * (0.6f64).acos() - 24.;
    volume(&document(&s, json!([lens.clone()])), area * 3., 1);
    volume(
        &document(&s, json!([lens.clone(), left])),
        std::f64::consts::PI * 25. * 3.,
        1,
    );
    volume(
        &document(&s, json!([lens, right, at(&cells, [-2., 0.])])),
        (std::f64::consts::PI * 50. - area) * 3.,
        1,
    );
}

#[test]
fn crossing_rectangle_and_circle_keep_exact_partial_arcs() {
    let mut s = Sketch::default();
    s.add_rect([0., -8.], [8., 8.]);
    s.add_circle([0., 0.], 5.);
    let cells = regions(&s);
    assert_eq!(cells.len(), 3);
    let semicircle = at(&cells, [2., 0.]);
    volume(
        &document(&s, json!([semicircle])),
        std::f64::consts::PI * 12.5 * 3.,
        1,
    );
    volume(
        &document(&s, json!([at(&cells, [-2., 0.])])),
        std::f64::consts::PI * 12.5 * 3.,
        1,
    );
}

#[test]
fn inner_connected_arrangement_cuts_one_hole_in_outer_cell() {
    let mut s = Sketch::default();
    s.add_rect([-20., -20.], [20., 20.]);
    s.add_circle([-3., 0.], 5.);
    s.add_circle([3., 0.], 5.);
    let cells = regions(&s);
    assert_eq!(cells.len(), 4);
    let outer = at(&cells, [15., 0.]);
    let region = cells.iter().find(|r| r.key == outer).unwrap();
    assert_eq!(region.holes.len(), 1);
    let lens = 50. * (0.6f64).acos() - 24.;
    let union = std::f64::consts::PI * 50. - lens;
    volume(&document(&s, json!([outer])), (1600. - union) * 3., 1);
}

#[test]
fn stale_or_empty_selections_fail_without_changing_the_saved_selection() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 10.);
    s.add_circle([0., 0.], 4.);
    let key = at(&regions(&s), [8., 0.]);
    let mut d = document(&s, json!([key.clone()]));
    volume(&d, std::f64::consts::PI * 84. * 3., 1);
    s.entities.pop();
    d.features[0].set_sketch(&s);
    let x = evaluate(&d);
    assert!(x.scene.error.as_ref().unwrap().contains("no longer exists"));
    assert!(x.geometry.parts.is_empty());
    assert_eq!(d.features[1].params["regions"], json!([key]));
    for value in [json!([]), json!(["made up"]), json!([1]), json!(true)] {
        let d = document(&s, value);
        let x = evaluate(&d);
        assert!(x.scene.error.is_some());
        assert!(x.geometry.parts.is_empty());
    }
}

#[test]
fn unused_open_lines_do_not_create_slivers_or_change_the_closed_region() {
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.add_line([0., 0.], [0., 9.]);
    let r = regions(&s);
    assert_eq!(r.len(), 1);
    volume(
        &document(&s, json!([r[0].key.clone()])),
        std::f64::consts::PI * 25. * 3.,
        1,
    );
}

#[test]
fn divided_region_side_support_follows_parameter_edits() {
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [20., 10.]);
    sketch.add_line([5., 0.], [5., 10.]);
    let key = at(&regions(&sketch), [2., 5.]);
    let mut doc = document(&sketch, json!([key]));
    let result = evaluate(&doc);
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    let support = result
        .geometry
        .anchors
        .iter()
        .find(|(key, plane, _)| {
            key.contains(":region-side:")
                && (plane.origin[0] - 5.).abs() < 1e-7
                && plane.normal[0] > 0.9
        })
        .unwrap()
        .0
        .clone();
    let registry = Registry::builtin().unwrap();
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.params["support"] = json!(support);
    let mut boss = Sketch::default();
    boss.add_rect([1., 1.], [2., 2.]);
    f.set_sketch(&boss);
    let id = f.id.clone();
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.params["sketch"] = json!(id);
    f.params["depth"] = json!("2");
    f.params["mode"] = json!("Add");
    f.params["targets"] = json!(["f2:0"]);
    doc.features.push(f);
    volume(&doc, 152., 1);
    sketch.entities[1].values[0] = "6".into();
    sketch.entities[1].values[2] = "6".into();
    doc.features[0].set_sketch(&sketch);
    volume(&doc, 182., 1);
}

#[test]
fn display_fill_preserves_annular_holes() {
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 10.);
    sketch.add_circle([0., 0.], 6.);
    let cells = regions(&sketch);
    let key = at(&cells, [8., 0.]);
    let annulus = cells.iter().find(|r| r.key == key).unwrap();
    let mut area = 0.;
    for t in &annulus.triangles {
        area += ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1])
            - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]))
            .abs()
            / 2.;
    }
    assert!(
        (area - 64. * std::f64::consts::PI).abs() < 0.1,
        "display fill area {area}"
    );
    assert!(!annulus.contains([0., 0.]));
}

#[cfg(feature = "step")]
#[test]
fn selected_region_step_roundtrips_remain_analytic() {
    let mut circles = Sketch::default();
    circles.add_circle([0., 0.], 5.);
    circles.add_circle([6., 0.], 5.);
    let cells = regions(&circles);
    let lens = at(&cells, [3., 0.]);
    let lens_area = 50. * (0.6f64).acos() - 24.;
    let mut divided = Sketch::default();
    divided.add_rect([0., 0.], [20., 10.]);
    divided.add_line([5., 0.], [5., 10.]);
    let divided_cells = regions(&divided);
    let mut nested = Sketch::default();
    nested.add_circle([0., 0.], 10.);
    nested.add_circle([0., 0.], 6.);
    let nested_cells = regions(&nested);
    let fixtures = [
        (
            "lens",
            document(&circles, json!([lens.clone()])),
            lens_area * 3.,
        ),
        (
            "circle-union",
            document(
                &circles,
                json!(cells.iter().map(|r| r.key.clone()).collect::<Vec<_>>()),
            ),
            (50. * std::f64::consts::PI - lens_area) * 3.,
        ),
        (
            "divided-rectangle",
            document(&divided, json!([at(&divided_cells, [2., 5.])])),
            150.,
        ),
        (
            "annulus",
            document(&nested, json!([at(&nested_cells, [8., 0.])])),
            64. * std::f64::consts::PI * 3.,
        ),
    ];
    let directory = std::path::Path::new("target/test-output/regions");
    std::fs::create_dir_all(directory).unwrap();
    for (name, doc, expected) in fixtures {
        volume(&doc, expected, 1);
        let result = evaluate(&doc);
        let path = directory.join(format!("{name}.step"));
        result.geometry.export("step", &path).unwrap();
        let mut topology = brepkit_topology::Topology::new();
        let solids =
            brepkit_io::step::read_step(&std::fs::read_to_string(&path).unwrap(), &mut topology)
                .unwrap();
        assert_eq!(solids.len(), 1);
        let validity = brepkit_operations::validate::validate_solid(&topology, solids[0]).unwrap();
        assert!(validity.is_valid(), "{name}: {:?}", validity.issues);
        let actual = cad::engine::solid_volume(&topology, solids[0]).unwrap();
        assert!(
            (actual - expected).abs() < expected * 1e-7,
            "{name}: {actual} vs {expected}"
        );
        doc.save(&directory.join(format!("{name}.cadpart")))
            .unwrap();
        std::fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&json!({"case":name,"expected_volume_mm3":expected,"actual_volume_mm3":actual,"step_roundtrip_verified":true})).unwrap()).unwrap();
    }
}
