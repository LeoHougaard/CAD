use cad::{
    dimensions::Selection,
    engine::{Runtime, Scene},
    model::{Document, Plane, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
    sketch,
};
use serde_json::json;
use std::collections::BTreeSet;
fn fixture() -> (Document, String) {
    let registry = Registry::builtin().unwrap();
    let mut doc = Document::default();
    doc.variables.insert("width".into(), "40".into());
    let mut sk = Sketch::default();
    sk.add_rect([0., 0.], [40., 20.]);
    sk.entities[0].values[2] = "width".into();
    for (kind, value) in [("X", "0"), ("Y", "0"), ("Height", "20")] {
        sk.constraints.push(SketchConstraint {
            kind: kind.into(),
            a: 1,
            value: value.into(),
            ..Default::default()
        });
    }
    let mut f = registry.create("sketch", &mut doc).unwrap();
    f.set_sketch(&sk);
    doc.features.push(f);
    let solved = sketch::resolve(
        &sk,
        Plane::named("Top", 0.),
        &Numbers::for_document(&doc).unwrap(),
    )
    .unwrap();
    let region = sketch::regions(&solved).unwrap()[0].key.clone();
    let mut ex = registry.create("extrude", &mut doc).unwrap();
    ex.params["sketch"] = json!("f1");
    ex.params["depth"] = json!("10");
    ex.params["regions"] = json!([region]);
    doc.features.push(ex);
    let scene = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&doc)
        .unwrap()
        .scene;
    let support = scene.meshes[0]
        .faces
        .iter()
        .find(|f| f.plane.as_ref().is_some_and(|p| p.normal[0] > 0.99))
        .unwrap()
        .key
        .clone();
    let mut attached = registry.create("sketch", &mut doc).unwrap();
    attached.params["support"] = json!(support.clone());
    let mut profile = Sketch::default();
    profile.add_circle([5., 5.], 1.);
    attached.set_sketch(&profile);
    doc.features.push(attached);
    let edge = scene.meshes[0]
        .edges
        .iter()
        .find(|e| e.axis.is_some_and(|(_, d)| d[2].abs() > 0.99))
        .unwrap()
        .key
        .clone();
    let mut fillet = registry.create("fillet", &mut doc).unwrap();
    fillet.params["target"] = json!("f2:0");
    fillet.params["edges"] = json!([edge]);
    fillet.params["radius"] = json!("1");
    doc.features.push(fillet);
    (doc, support)
}
fn evaluate(doc: &Document) -> std::sync::Arc<Scene> {
    let scene = Runtime::new(Registry::builtin().unwrap())
        .evaluate(doc)
        .unwrap()
        .scene;
    assert!(scene.error.is_none(), "{:?}", scene.error);
    scene
}
fn keys(scene: &Scene) -> (BTreeSet<String>, BTreeSet<String>) {
    (
        scene
            .meshes
            .iter()
            .flat_map(|m| m.faces.iter().map(|f| f.key.clone()))
            .collect(),
        scene
            .meshes
            .iter()
            .flat_map(|m| m.edges.iter().map(|e| e.key.clone()))
            .collect(),
    )
}
fn same_frame(a: &Plane, b: &Plane) {
    for (a, b) in a
        .origin
        .iter()
        .chain(&a.u)
        .chain(&a.v)
        .chain(&a.normal)
        .zip(b.origin.iter().chain(&b.u).chain(&b.v).chain(&b.normal))
    {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }
}
#[test]
fn representation_changes_preserve_region_solid_and_support_identities_through_edits_and_reopen() {
    let (original, support) = fixture();
    let before = evaluate(&original);
    let mut changed = original.clone();
    let sketch = original.features[0].sketch().unwrap();
    let numbers = Numbers::for_document(&original).unwrap();
    let resolved = sketch::resolve(&sketch, Plane::named("Top", 0.), &numbers).unwrap();
    let region = sketch::regions(&resolved).unwrap()[0].key.clone();
    let (once, resolved, selected) =
        sketch::set_construction(&sketch, &resolved, &[Selection::edge(1, 1)], None, &numbers)
            .unwrap();
    let (twice, resolved, _) =
        sketch::set_construction(&once, &resolved, &selected, None, &numbers).unwrap();
    assert_eq!(sketch::regions(&resolved).unwrap()[0].key, region);
    assert!(twice.entities.iter().all(|e| e.curve_identity.is_some()));
    changed.features[0].set_sketch(&twice);
    let after = evaluate(&changed);
    assert_eq!(keys(&before), keys(&after));
    same_frame(&before.sketches["f3"].plane, &after.sketches["f3"].plane);
    assert_eq!(changed.features[2].params["support"], support);
    let mut expected = original.clone();
    expected.variables.insert("width".into(), "60".into());
    changed.variables = expected.variables.clone();
    let expected = evaluate(&expected);
    let actual = evaluate(&changed);
    assert_eq!(keys(&expected), keys(&actual));
    same_frame(&expected.sketches["f3"].plane, &actual.sketches["f3"].plane);
    assert!((actual.meshes[0].volume - expected.meshes[0].volume).abs() < 1e-6);
    let path = std::path::PathBuf::from("target/test-output")
        .join(format!("curve-provenance-{}.cadpart", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    changed.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    let actual = evaluate(&reopened);
    assert_eq!(keys(&expected), keys(&actual));
    same_frame(&expected.sketches["f3"].plane, &actual.sketches["f3"].plane);
    // The retained curve can still be trimmed via the shared logical lookup.
    let mut trim_sketch = twice.clone();
    trim_sketch.add_line([20., -5.], [20., 25.]);
    let solved = sketch::resolve(&trim_sketch, Plane::named("Top", 0.), &numbers).unwrap();
    let pieces = sketch::trim_entity(&solved, 1, 0, [30., 0.]).unwrap();
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].values, vec![0., 0., 20., 0.]);
}
#[test]
fn malformed_duplicate_and_wrong_family_curve_identities_fail_in_document_and_core() {
    let (doc, _) = fixture();
    let original = doc.features[0].sketch().unwrap();
    let numbers = Numbers::for_document(&doc).unwrap();
    let resolved = sketch::resolve(&original, Plane::named("Top", 0.), &numbers).unwrap();
    let (sk, _, _) = sketch::set_construction(
        &original,
        &resolved,
        &[Selection::edge(1, 1)],
        Some(false),
        &numbers,
    )
    .unwrap();
    for bad in [
        "r1.4",
        "r01.0",
        "r0.0",
        "r999.0",
        "c1",
        "r1.0garbage",
        "l999999999999999999999999999999999999999",
    ] {
        let mut candidate = sk.clone();
        candidate.entities[0].curve_identity = Some(bad.into());
        let mut doc = doc.clone();
        doc.features[0].set_sketch(&candidate);
        assert!(doc.validate().is_err(), "{bad}");
        assert!(
            sketch::resolve(&candidate, Plane::named("Top", 0.), &numbers).is_err(),
            "{bad}"
        );
    }
    let mut duplicate = sk.clone();
    duplicate.entities[1].curve_identity = duplicate.entities[0].curve_identity.clone();
    assert!(duplicate.validate_curve_identities().is_err());
    assert!(sketch::resolve(&duplicate, Plane::named("Top", 0.), &numbers).is_err());
    let value = serde_json::to_value(&original).unwrap();
    assert!(
        value["entities"][0].get("curve_identity").is_none(),
        "Legacy schema must retain its bytes when provenance is absent"
    );
}
