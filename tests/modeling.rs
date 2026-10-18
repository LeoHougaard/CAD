use cad::{
    engine::Runtime,
    model::{self, Document, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
};
use serde_json::json;
use std::collections::BTreeMap;

fn runtime() -> Runtime {
    Runtime::new(Registry::builtin().unwrap())
}
fn rectangular_part() -> Document {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = r.create("sketch", &mut d).unwrap();
    let mut sk = Sketch::default();
    sk.add_rect([0., 0.], [40., 30.]);
    s.set_sketch(&sk);
    d.features.push(s);
    let mut e = r.create("extrude", &mut d).unwrap();
    e.params["sketch"] = json!("f1");
    e.params["depth"] = json!("20");
    d.features.push(e);
    d
}

#[test]
fn upstream_edit_updates_attached_holes_and_exact_volume() {
    let mut d = model::demo();
    let mut r = runtime();
    let a = r.evaluate(&d).unwrap();
    assert!(a.scene.error.is_none(), "{:?}", a.scene.statuses);
    let expected = (80. * 50. - 4. * std::f64::consts::PI * 9.) * 8.;
    assert!((a.scene.meshes[0].volume - expected).abs() < 1e-5);
    assert!(
        a.scene.meshes[0].faces.len() < 30,
        "Circular holes must remain analytic, not become many planar mesh faces"
    );
    d.variables.insert("width".into(), "120".into());
    d.variables.insert("thickness".into(), "12".into());
    let b = r.evaluate(&d).unwrap();
    assert!(b.scene.error.is_none(), "{:?}", b.scene.statuses);
    assert!(
        (b.scene.meshes[0].volume - (120. * 50. - 4. * std::f64::consts::PI * 9.) * 12.).abs()
            < 1e-5
    );
    assert_eq!(b.scene.sketches["f3"].plane.origin[2], 12.);
}

#[test]
fn exact_circular_profile_and_nested_holes() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut f = r.create("sketch", &mut d).unwrap();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 10.);
    s.add_circle([0., 0.], 4.);
    f.set_sketch(&s);
    d.features.push(f);
    let mut e = r.create("extrude", &mut d).unwrap();
    e.params["sketch"] = json!("f1");
    d.features.push(e);
    let result = runtime().evaluate(&d).unwrap();
    assert!(result.scene.error.is_none(), "{:?}", result.scene.statuses);
    assert!(
        (result.scene.meshes[0].volume - std::f64::consts::PI * (100. - 16.) * 10.).abs() < 1e-5
    );
    assert_eq!(result.scene.meshes[0].faces.len(), 4);
}

#[test]
fn suppression_rollback_and_missing_reference_fail_visibly() {
    let mut d = model::demo();
    let mut r = runtime();
    d.rollback = Some(2);
    let x = r.evaluate(&d).unwrap();
    assert!(x.scene.error.is_none());
    assert!((x.scene.meshes[0].volume - 32000.).abs() < 1e-5);
    d.rollback = None;
    d.features[1].suppressed = true;
    let x = r.evaluate(&d).unwrap();
    assert!(x.scene.error.is_some());
    assert!(x.scene.meshes.is_empty());
    assert!(
        x.scene.statuses[2]
            .error
            .as_ref()
            .unwrap()
            .contains("missing")
    );
}

#[test]
fn last_feature_edit_reuses_prefix_and_failure_preserves_prior_solid() {
    let mut d = rectangular_part();
    let mut r = runtime();
    r.evaluate(&d).unwrap();
    d.features[1].params["depth"] = json!("25");
    let x = r.evaluate(&d).unwrap();
    assert_eq!(x.scene.reused_features, 1);
    assert!((x.scene.meshes[0].volume - 30000.).abs() < 1e-5);
    let registry = Registry::builtin().unwrap();
    let mut bad = registry.create("fillet", &mut d).unwrap();
    bad.params["radius"] = json!("-1");
    d.features.push(bad);
    let y = r.evaluate(&d).unwrap();
    assert!(y.scene.error.is_some());
    assert!((y.scene.meshes[0].volume - 30000.).abs() < 1e-5);
}

#[test]
fn file_round_trip_preserves_expressions_and_plugin_versions() {
    let d = model::demo();
    let path = std::path::Path::new("target/test-output/roundtrip.cadpart");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    d.save(path).unwrap();
    assert_eq!(Document::load(path).unwrap(), d);
}

#[test]
fn plugin_package_can_add_a_feature_without_a_host_feature_variant() {
    let r = Registry::load(std::path::Path::new("plugins"));
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert!(r.plugins.contains_key("spacer"));
    let mut d = Document::default();
    let f = r.create("spacer", &mut d).unwrap();
    d.features.push(f);
    let mut rt = Runtime::new(r);
    let x = rt.evaluate(&d).unwrap();
    assert!(x.scene.error.is_none(), "{:?}", x.scene.statuses);
    assert!((x.scene.meshes[0].volume - std::f64::consts::PI * (100. - 16.) * 12.).abs() < 1e-5);
}

#[test]
fn changed_plugin_source_is_not_silently_used_by_an_old_document() {
    let mut d = rectangular_part();
    d.features[1].plugin_hash = "different".into();
    let x = runtime().evaluate(&d).unwrap();
    assert!(
        x.scene
            .error
            .as_ref()
            .unwrap()
            .contains("Plugin code changed")
    );
}

#[test]
fn real_constraint_solver_rebuilds_closed_lines() {
    let mut s = Sketch::default();
    s.add_line([0., 0.], [38., 1.]);
    s.constraints.push(SketchConstraint {
        kind: "Horizontal".into(),
        a: 1,
        b: 0,
        value: String::new(),
        ..Default::default()
    });
    s.constraints.push(SketchConstraint {
        kind: "Length".into(),
        a: 1,
        b: 0,
        value: "40".into(),
        ..Default::default()
    });
    let x = cad::sketch::resolve(
        &s,
        model::Plane::named("Top", 0.),
        &Numbers::new(&BTreeMap::new()).unwrap(),
    )
    .unwrap();
    let v = &x.entities[0].values;
    assert!((v[1] - v[3]).abs() < 1e-6);
    assert!(((v[2] - v[0]).hypot(v[3] - v[1]) - 40.).abs() < 1e-6);
}

#[test]
fn variable_dependencies_units_and_cycles() {
    let n = Numbers::new(&BTreeMap::from([
        ("a".into(), "b*2".into()),
        ("b".into(), "inch".into()),
    ]))
    .unwrap();
    assert!((n.eval("a+2*mm").unwrap() - 52.8).abs() < 1e-8);
    assert!(
        Numbers::new(&BTreeMap::from([
            ("a".into(), "b".into()),
            ("b".into(), "a".into())
        ]))
        .is_err()
    );
}

#[cfg(feature = "step")]
#[test]
fn exported_step_reimports_to_matching_volume_and_stl_is_well_formed() {
    let x = runtime().evaluate(&model::demo()).unwrap();
    std::fs::create_dir_all("target/test-output").unwrap();
    x.geometry
        .export(
            "step",
            std::path::Path::new("target/test-output/plate.step"),
        )
        .unwrap();
    let mut topo = brepkit_topology::Topology::new();
    let solids = brepkit_io::step::read_step(
        &std::fs::read_to_string("target/test-output/plate.step").unwrap(),
        &mut topo,
    )
    .unwrap();
    assert_eq!(solids.len(), 1);
    let v = brepkit_operations::measure::solid_volume(&topo, solids[0], 0.03).unwrap();
    assert!((v - x.scene.meshes[0].volume).abs() < 1e-4);
    x.geometry
        .export("stl", std::path::Path::new("target/test-output/plate.stl"))
        .unwrap();
    let bytes = std::fs::read("target/test-output/plate.stl").unwrap();
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    assert_eq!(bytes.len(), 84 + count as usize * 50);
}

#[test]
fn basic_modifiers_and_patterns_are_parametric() {
    let registry = Registry::builtin().unwrap();
    for operation in ["fillet", "chamfer", "mirror", "pattern", "transform"] {
        let mut d = rectangular_part();
        let mut f = registry.create(operation, &mut d).unwrap();
        if operation == "fillet" || operation == "chamfer" {
            f.params["edges"] = json!(["@all"]);
        }
        d.features.push(f);
        let x = runtime().evaluate(&d).unwrap();
        assert!(
            x.scene.error.is_none(),
            "{operation}: {:?}",
            x.scene.statuses
        );
        assert_eq!(
            x.scene.meshes.len(),
            if operation == "mirror" {
                2
            } else if operation == "pattern" {
                3
            } else {
                1
            }
        );
        for part in &x.geometry.parts {
            let report =
                brepkit_operations::validate::validate_solid(&x.geometry.topo, part.solid).unwrap();
            assert!(report.is_valid(), "{operation}: {:?}", report.issues);
        }
        if operation == "chamfer" {
            assert!(x.scene.meshes[0].volume < 24000. && x.scene.meshes[0].volume > 22000.);
        }
        #[cfg(feature = "step")]
        {
            std::fs::create_dir_all("target/test-output").unwrap();
            x.geometry
                .export(
                    "step",
                    std::path::Path::new(&format!("target/test-output/{operation}.step")),
                )
                .unwrap();
        }
    }
}

#[test]
fn shell_and_revolve_make_valid_parametric_solids() {
    let registry = Registry::builtin().unwrap();
    let mut d = rectangular_part();
    let mut shell = registry.create("shell", &mut d).unwrap();
    shell.params["faces"] = json!(["f2:end"]);
    d.features.push(shell);
    let x = runtime().evaluate(&d).unwrap();
    assert!(x.scene.error.is_none(), "{:?}", x.scene.statuses);
    assert!((x.scene.meshes[0].volume - (40. * 30. * 20. - 36. * 26. * 18.)).abs() < 1e-4);
    let report =
        brepkit_operations::validate::validate_solid(&x.geometry.topo, x.geometry.parts[0].solid)
            .unwrap();
    assert!(report.is_valid(), "{:?}", report.issues);
    #[cfg(feature = "step")]
    x.geometry
        .export(
            "step",
            std::path::Path::new("target/test-output/shell.step"),
        )
        .unwrap();
    let mut d = Document::default();
    let mut f = registry.create("sketch", &mut d).unwrap();
    let mut sk = Sketch::default();
    sk.add_rect([10., 0.], [15., 20.]);
    f.set_sketch(&sk);
    d.features.push(f);
    let mut f = registry.create("revolve", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    d.features.push(f);
    let x = runtime().evaluate(&d).unwrap();
    assert!(x.scene.error.is_none(), "{:?}", x.scene.statuses);
    assert!((x.scene.meshes[0].volume - std::f64::consts::PI * (225. - 100.) * 20.).abs() < 1e-3);
    let report =
        brepkit_operations::validate::validate_solid(&x.geometry.topo, x.geometry.parts[0].solid)
            .unwrap();
    assert!(report.is_valid(), "{:?}", report.issues);
    #[cfg(feature = "step")]
    x.geometry
        .export(
            "step",
            std::path::Path::new("target/test-output/revolve.step"),
        )
        .unwrap();
}
