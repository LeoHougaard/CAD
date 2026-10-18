use cad::{
    engine::Runtime,
    model::{Document, Plane, Sketch},
    plugins::Registry,
};
use serde_json::json;

fn part() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut document = Document::default();
    let mut sketch = registry.create("sketch", &mut document).unwrap();
    let mut geometry = Sketch::default();
    geometry.add_rect([0., 0.], [12., 8.]);
    sketch.set_sketch(&geometry);
    document.features.push(sketch);
    let mut extrude = registry.create("extrude", &mut document).unwrap();
    extrude.params["sketch"] = json!("f1");
    document.features.push(extrude);
    document
}

#[test]
fn frozen_context_planes_drive_sketch_offsets_and_exact_extrude_extents() {
    let mut document = part();
    document.features[0].params["context_plane"] = json!(Plane::named("Top", 5.));
    document.features[0].params["offset"] = json!("2");
    document.features[1].params["context_up_to"] = json!(Plane::named("Top", 23.));
    let mut runtime = Runtime::new(Registry::builtin().unwrap());
    let result = runtime.evaluate(&document).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.error);
    assert_eq!(result.scene.sketches["f1"].plane.origin[2], 7.);
    assert!((result.scene.meshes[0].volume - 12. * 8. * 16.).abs() < 1e-6);
    assert_eq!(result.scene.meshes[0].bounds[1][2], 23.);
    document.features[1].params["context_up_to"] = json!(Plane::named("Top", -3.));
    document.features[1].params["reverse"] = json!(true);
    let result = runtime.evaluate(&document).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.error);
    assert!((result.scene.meshes[0].volume - 960.).abs() < 1e-6);
    assert_eq!(result.scene.meshes[0].bounds[0][2], -3.);
}

#[test]
fn invalid_or_unsupported_context_frames_fail_instead_of_changing_geometry() {
    let original = part();
    for plane in [
        Plane {
            u: [2., 0., 0.],
            ..Plane::named("Top", 0.)
        },
        Plane {
            v: [1., 0., 0.],
            ..Plane::named("Top", 0.)
        },
    ] {
        let mut document = original.clone();
        document.features[0].params["context_plane"] = json!(plane);
        let result = Runtime::new(Registry::builtin().unwrap())
            .evaluate(&document)
            .unwrap();
        assert!(result.scene.error.is_some());
        assert!(result.scene.meshes.is_empty());
    }
    for (plane, symmetric) in [
        (Plane::named("Front", 20.), false),
        (Plane::named("Top", 20.), true),
        (Plane::named("Top", -20.), false),
    ] {
        let mut document = original.clone();
        document.features[1].params["context_up_to"] = json!(plane);
        document.features[1].params["symmetric"] = json!(symmetric);
        let result = Runtime::new(Registry::builtin().unwrap())
            .evaluate(&document)
            .unwrap();
        assert!(result.scene.error.is_some());
        assert!(result.scene.meshes.is_empty());
    }
    assert_eq!(original, part());
}
