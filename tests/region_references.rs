use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::json;
use std::collections::BTreeSet;

fn document(lens: bool) -> Document {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_circle([0., 0.], 5.);
    if lens {
        sketch.add_circle([6., 0.], 5.);
    }
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.set_sketch(&sketch);
    doc.features.push(f);
    let mut f = registry.create("extrude", &mut doc).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("6");
    doc.features.push(f);
    if lens {
        let resolved = cad::sketch::resolve(
            &doc.features[0].sketch().unwrap(),
            cad::model::Plane::named("Top", 0.),
            &cad::plugins::Numbers::new(&Default::default()).unwrap(),
        )
        .unwrap();
        let region = cad::sketch::regions(&resolved)
            .unwrap()
            .into_iter()
            .find(|r| r.contains([3., 0.]))
            .unwrap();
        doc.features[1].params["regions"] = json!([region.key]);
    }
    doc
}
fn evaluate(doc: &Document) -> Evaluation {
    Runtime::new(Registry::builtin().unwrap())
        .evaluate(doc)
        .unwrap()
}
fn edge_keys(result: &Evaluation) -> BTreeSet<String> {
    let edges: Vec<_> = result
        .scene
        .meshes
        .iter()
        .flat_map(|m| m.edges.iter())
        .filter(|e| !e.seam)
        .collect();
    assert!(
        edges.iter().all(|e| !e.key.contains("untracked:")),
        "{:?}",
        edges.iter().map(|e| &e.key).collect::<Vec<_>>()
    );
    let keys: BTreeSet<_> = edges.iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys.len(), edges.len());
    keys
}

#[test]
fn lens_curved_sides_and_both_vertical_edges_have_distinct_stable_references() {
    let mut doc = document(true);
    let baseline = evaluate(&doc);
    assert!(baseline.scene.error.is_none());
    let keys = edge_keys(&baseline);
    assert_eq!(keys.len(), 6);
    let vertices: Vec<_> = keys
        .iter()
        .filter(|k| k.contains(":region-vertex:"))
        .collect();
    assert_eq!(vertices.len(), 2);
    let sides: BTreeSet<_> = baseline.scene.meshes[0]
        .faces
        .iter()
        .filter(|f| f.plane.is_none())
        .map(|f| f.key.clone())
        .collect();
    assert_eq!(sides.len(), 2);
    assert!(
        sides
            .iter()
            .all(|k| k.contains(":region-side:") && !k.contains("untracked:"))
    );
    let mut sketch = doc.features[0].sketch().unwrap();
    sketch.entities[1].values[0] = "6.5".into();
    sketch.entities[1].values[2] = "5.5".into();
    doc.features[0].set_sketch(&sketch);
    let edited = evaluate(&doc);
    assert!(edited.scene.error.is_none(), "{:?}", edited.scene.statuses);
    assert_eq!(keys, edge_keys(&edited));
}

#[test]
fn whole_circle_rims_remain_tracked_after_radius_and_depth_changes() {
    let mut doc = document(false);
    let baseline = evaluate(&doc);
    let keys = edge_keys(&baseline);
    assert_eq!(keys.len(), 2);
    let mut sketch = doc.features[0].sketch().unwrap();
    sketch.entities[0].values[2] = "7".into();
    doc.features[0].set_sketch(&sketch);
    doc.features[1].params["depth"] = json!("8");
    let edited = evaluate(&doc);
    assert!(edited.scene.error.is_none());
    assert_eq!(keys, edge_keys(&edited));
}

#[test]
fn selecting_a_curved_rim_reaches_fillet_geometry_after_the_defensive_copy() {
    for lens in [false, true] {
        let mut doc = document(lens);
        let baseline = evaluate(&doc);
        let keys = edge_keys(&baseline);
        let key = keys.into_iter().find(|k| k.contains("f2:end")).unwrap();
        let mut f = Registry::builtin()
            .unwrap()
            .create("fillet", &mut doc)
            .unwrap();
        f.params["edges"] = json!([key]);
        f.params["radius"] = json!("0.5");
        doc.features.push(f);
        let result = evaluate(&doc);
        assert!(
            result.scene.error.is_none(),
            "lens={lens}: {:?}",
            result.scene.statuses
        );
        assert!(result.scene.meshes[0].volume < baseline.scene.meshes[0].volume);
    }
}

#[test]
fn failed_features_roll_back_curved_provenance_alongside_their_solids() {
    let mut doc = document(false);
    let baseline = evaluate(&doc);
    let expected = edge_keys(&baseline);
    let mut registry = Registry::builtin().unwrap();
    registry.add(r#"{"id":"failed-extrusion","name":"Failed extrusion","version":"1","category":"feature","script":"failed.rhai","fields":[{"key":"depth","label":"Depth","kind":"number","default":"6"}]}"#,
        r#"fn build(p) { p.op = "extrude"; p.sketch = "f1"; [p, #{op:"missing-operation"}] }"#).unwrap();
    let f = registry.create("failed-extrusion", &mut doc).unwrap();
    doc.features.push(f);
    let failed = Runtime::new(registry).evaluate(&doc).unwrap();
    assert!(failed.scene.error.is_some());
    assert_eq!(failed.geometry.parts.len(), 1);
    assert_eq!(expected, edge_keys(&failed));
}

#[test]
fn untracked_references_fail_explicitly_without_discarding_the_prior_solid() {
    let mut registry = Registry::builtin().unwrap();
    registry.add(r#"{"id":"primitive","name":"Primitive","version":"1","category":"feature","script":"primitive.rhai"}"#,r#"fn build(p) { #{op:"cylinder",radius:5.0,height:6.0,mode:"New"} }"#).unwrap();
    let mut base = Document::default();
    let f = registry.create("primitive", &mut base).unwrap();
    base.features.push(f);
    let result = Runtime::shared(std::sync::Arc::new(registry))
        .evaluate(&base)
        .unwrap();
    let edge = result.scene.meshes[0]
        .edges
        .iter()
        .find(|e| !e.seam)
        .unwrap()
        .key
        .clone();
    let face = result.scene.meshes[0].faces[0].key.clone();
    assert!(edge.contains("untracked:") && face.contains("untracked:"));
    for plugin in ["fillet", "chamfer", "shell"] {
        let mut registry = Registry::builtin().unwrap();
        registry.add(r#"{"id":"primitive","name":"Primitive","version":"1","category":"feature","script":"primitive.rhai"}"#,r#"fn build(p) { #{op:"cylinder",radius:5.0,height:6.0,mode:"New"} }"#).unwrap();
        let mut doc = base.clone();
        let mut f = registry.create(plugin, &mut doc).unwrap();
        f.params[if plugin == "shell" { "faces" } else { "edges" }] =
            json!([if plugin == "shell" { &face } else { &edge }]);
        doc.features.push(f);
        let rejected = Runtime::new(registry).evaluate(&doc).unwrap();
        let error = rejected.scene.error.as_ref().unwrap();
        assert!(
            error.contains("unsupported") && error.contains("persistent reference"),
            "{error}"
        );
        assert_eq!(rejected.geometry.parts.len(), 1);
    }
}
