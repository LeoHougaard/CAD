use cad::{
    engine::Runtime,
    model::{Document, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
};
use serde_json::json;
use std::collections::BTreeMap;

fn part(w: f64, h: f64, depth: f64) -> Document {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut f = r.create("sketch", &mut d).unwrap();
    let mut s = Sketch::default();
    s.add_rect([-w / 2., -h / 2.], [w / 2., h / 2.]);
    f.set_sketch(&s);
    d.features.push(f);
    let mut f = r.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!(depth.to_string());
    d.features.push(f);
    d
}

#[test]
fn varied_extrusions_preserve_volume_and_closed_topology() {
    let r = Registry::builtin().unwrap();
    let mut rt = Runtime::new(r);
    for n in 1..=40 {
        let w = 3. + n as f64 * 1.13;
        let h = 2. + n as f64 * 0.67;
        let depth = 0.5 + n as f64 * 0.31;
        let mut d = part(w, h, depth);
        d.features[1].params["symmetric"] = json!(n % 2 == 0);
        d.features[1].params["reverse"] = json!(n % 3 == 0);
        let x = rt.evaluate(&d).unwrap();
        assert!(x.scene.error.is_none(), "n={n}: {:?}", x.scene.statuses);
        assert!((x.scene.meshes[0].volume - w * h * depth).abs() < 1e-6);
        assert_eq!(x.scene.meshes[0].faces.len(), 6);
    }
}

#[test]
fn varied_circular_holes_keep_exact_volumes() {
    let r = Registry::builtin().unwrap();
    let mut rt = Runtime::new(Registry::builtin().unwrap());
    for n in 1..=20 {
        let radius = 0.7 + n as f64 * 0.12;
        let mut d = part(50., 40., 10.);
        let mut s = Sketch::default();
        s.add_circle([0., 0.], radius);
        let mut f = r.create("sketch", &mut d).unwrap();
        f.params["support"] = json!("f2:end");
        f.set_sketch(&s);
        let id = f.id.clone();
        d.features.push(f);
        let mut f = r.create("extrude", &mut d).unwrap();
        f.params["sketch"] = json!(id);
        f.params["mode"] = json!("Remove");
    f.params["targets"] = json!(["f2:0"]);
        f.params["reverse"] = json!(true);
        f.params["depth"] = json!("10");
        d.features.push(f);
        let x = rt.evaluate(&d).unwrap();
        assert!(x.scene.error.is_none(), "{n}: {:?}", x.scene.statuses);
        let expected = (2000. - std::f64::consts::PI * radius * radius) * 10.;
        assert!((x.scene.meshes[0].volume - expected).abs() < 1e-5);
    }
}

#[test]
fn failed_multi_operation_plugin_is_transactional() {
    let mut r = Registry::builtin().unwrap();
    r.add(r#"{"id":"broken","name":"Broken","version":"1","category":"feature","script":"broken.rhai"}"#,"fn build(p) { [#{op: \"box\", mode: \"New\", width: 5.0, depth: 5.0, height: 5.0}, #{op: \"not-an-operation\"}] }").unwrap();
    let mut d = part(20., 10., 5.);
    let f = r.create("broken", &mut d).unwrap();
    d.features.push(f);
    let mut rt = Runtime::new(r);
    let x = rt.evaluate(&d).unwrap();
    assert!(x.scene.error.is_some());
    assert_eq!(x.scene.meshes.len(), 1);
    assert!((x.scene.meshes[0].volume - 1000.).abs() < 1e-6);
    assert!(!x.geometry.outputs.contains_key("f3"));
    d.features[2].suppressed = true;
    assert!(rt.evaluate(&d).unwrap().scene.error.is_none());
}

#[test]
fn script_execution_has_a_finite_budget() {
    let mut r = Registry::builtin().unwrap();
    r.add(
        r#"{"id":"loop","name":"Loop","version":"1","category":"feature","script":"loop.rhai"}"#,
        "fn build(p) { loop {} }",
    )
    .unwrap();
    let mut d = Document::default();
    let f = r.create("loop", &mut d).unwrap();
    d.features.push(f);
    let x = Runtime::new(r).evaluate(&d).unwrap();
    assert!(x.scene.error.as_ref().unwrap().contains("operations"));
}

#[test]
fn invalid_profiles_and_bad_dimensions_fail_without_a_solid() {
    for case in 0..6 {
        let mut d = part(30., 20., 5.);
        match case {
            0 => d.features[1].params["depth"] = json!("-2"),
            1 => {
                let mut s = Sketch::default();
                s.add_line([0., 0.], [10., 0.]);
                d.features[0].set_sketch(&s);
            }
            2 => {
                let mut s = Sketch::default();
                s.add_circle([0., 0.], 5.);
                s.add_circle([0., 0.], 5.);
                d.features[0].set_sketch(&s);
            }
            3 => d.features[1].params["depth"] = json!("1/0"),
            4 => {
                let mut s = d.features[0].sketch().unwrap();
                s.entities[0].values[2] = "0".into();
                d.features[0].set_sketch(&s);
            }
            _ => d.features[1].params["sketch"] = json!("deleted"),
        }
        let x = Runtime::new(Registry::builtin().unwrap())
            .evaluate(&d)
            .unwrap();
        assert!(x.scene.error.is_some(), "case {case}");
        assert!(x.scene.meshes.is_empty());
    }
}

#[test]
fn conflicting_constraints_report_failure() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [10., 0.]);
    for kind in ["Horizontal", "Vertical", "Length"] {
        s.constraints.push(SketchConstraint {
            kind: kind.into(),
            a: 1,
            b: 0,
            value: "10".into(),
            ..Default::default()
        });
    }
    assert!(
        cad::sketch::resolve(
            &s,
            cad::model::Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap()
        )
        .is_err()
    );
}

#[test]
fn dimension_division_is_real_and_nonfinite_values_are_rejected() {
    let n = Numbers::new(&BTreeMap::new()).unwrap();
    assert!((n.eval("1/2*inch").unwrap() - 12.7).abs() < 1e-9);
    assert!(n.eval("1/0").is_err());
    assert!(n.eval("1e100").is_err());
    assert!(n.value(&json!(1e100)).is_err());
}

#[test]
fn malformed_documents_and_stale_identity_counters_are_rejected() {
    let mut d = part(10., 10., 2.);
    d.next_id = 1;
    assert!(d.validate().is_err());
    d.next_id = 3;
    d.features[0].params = json!(null);
    assert!(d.validate().is_err());
    d = part(10., 10., 2.);
    let mut s = d.features[0].sketch().unwrap();
    s.next_id = 0;
    d.features[0].set_sketch(&s);
    assert!(d.validate().is_err());
}

#[test]
fn repeated_save_replaces_atomically_and_reopens_latest_document() {
    let path = std::path::Path::new("target/test-output/repeated-save.cadpart");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut d = part(10., 10., 2.);
    d.save(path).unwrap();
    d.name = "Saved twice".into();
    d.save(path).unwrap();
    assert_eq!(Document::load(path).unwrap(), d);
}

#[test]
fn suppression_and_reordering_never_silently_retarget_a_reference() {
    let mut d = cad::model::demo();
    d.features.swap(0, 1);
    let x = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&d)
        .unwrap();
    assert!(x.scene.error.is_some());
    assert!(x.scene.meshes.is_empty());
    d = cad::model::demo();
    d.features.remove(1);
    let x = Runtime::new(Registry::builtin().unwrap())
        .evaluate(&d)
        .unwrap();
    assert!(x.scene.error.is_some());
    assert!(x.scene.meshes.is_empty());
}

#[test]
fn coincident_line_endpoints_stay_joined_after_coordinate_edits() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [20., 0.]);
    s.add_line([20., 0.], [20., 10.]);
    s.add_line([20., 10.], [0., 10.]);
    s.add_line([0., 10.], [0., 0.]);
    s.entities[0].values[2] = "25".into();
    let resolved = cad::sketch::resolve(
        &s,
        cad::model::Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    assert_eq!(cad::sketch::loops(&resolved).unwrap().len(), 1);
    assert!((resolved.entities[0].values[2] - resolved.entities[1].values[0]).abs() < 1e-6);
}

#[test]
fn duplicate_and_tangent_region_boundaries_are_rejected() {
    for case in 0..4 {
        let mut s = Sketch::default();
        match case {
            0 => {
                s.add_circle([0., 0.], 5.);
                s.add_circle([0., 0.], 5.);
            }
            1 => {
                s.add_circle([0., 0.], 5.);
                s.add_circle([10., 0.], 5.);
            }
            2 => {
                s.add_rect([0., 0.], [10., 10.]);
                s.add_circle([5., 5.], 5.);
            }
            _ => {
                s.add_rect([0., 0.], [10., 10.]);
                s.add_rect([10., 0.], [20., 10.]);
            }
        }
        let x = cad::sketch::resolve(
            &s,
            cad::model::Plane::named("Top", 0.),
            &Numbers::new(&BTreeMap::new()).unwrap(),
        )
        .unwrap();
        assert!(cad::sketch::loops(&x).is_err(), "case {case}");
    }
}

#[test]
fn failed_file_write_preserves_the_previous_document() {
    use std::io::Write;
    let path = std::path::Path::new("target/test-output/interrupted.cadpart");
    let d = part(10., 10., 2.);
    d.save(path).unwrap();
    let result = cad::model::atomic_write(path, |f| {
        f.write_all(b"partial").unwrap();
        Err("Simulated write failure".into())
    });
    assert!(result.is_err());
    assert_eq!(Document::load(path).unwrap(), d);
}

#[test]
fn disabled_plugins_report_a_missing_dependency_without_destroying_history() {
    let dir = std::path::Path::new("target/test-output/disabled-packages");
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("disabled.txt"), "extrude\n").unwrap();
    let r = Registry::load(dir);
    let d = part(10., 10., 2.);
    let x = Runtime::new(r).evaluate(&d).unwrap();
    assert!(x.scene.error.as_ref().unwrap().contains("Missing plugin"));
    assert_eq!(d.features.len(), 2);
    assert!(x.scene.meshes.is_empty());
}
