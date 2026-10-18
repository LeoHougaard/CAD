//! One profile list supports faces and regions without changing legacy documents.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, ProfileReference, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{
    f64::consts::PI,
    io::Write,
    process::{Command, Stdio},
};
fn feature(r: &Registry, d: &mut Document, id: &str, p: Value) -> String {
    let mut f = r.create(id, d).unwrap();
    for (k, v) in p.as_object().unwrap() {
        f.params[k] = v.clone();
    }
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn sketch(r: &Registry, d: &mut Document, s: Sketch) -> String {
    let mut f = r.create("sketch", d).unwrap();
    f.set_sketch(&s);
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn rectangle(r: &Registry, d: &mut Document, a: [f64; 2], b: [f64; 2]) -> String {
    let mut s = Sketch::default();
    s.add_rect(a, b);
    sketch(r, d, s)
}
fn eval(d: &Document) -> Evaluation {
    Runtime::new(Registry::builtin().unwrap())
        .evaluate(d)
        .unwrap()
}
fn valid(d: &Document) -> Evaluation {
    let e = eval(d);
    assert!(e.scene.error.is_none(), "{:?}", e.scene.statuses);
    for part in &e.geometry.parts {
        let report =
            brepkit_operations::validate::validate_solid(&e.geometry.topo, part.solid).unwrap();
        assert!(report.is_valid(), "{:?}", report.issues);
    }
    e
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7 * b.abs().max(1.), "{a} != {b}");
}
fn region(e: &Evaluation, sketch: &str, p: [f64; 2]) -> Value {
    json!({"kind":"region","sketch":sketch,"region":e.scene.regions[sketch].iter().find(|r|r.contains(p)).unwrap().key})
}
fn total(e: &Evaluation) -> f64 {
    e.scene.meshes.iter().map(|m| m.volume).sum()
}
fn roundtrip(d: &Document, label: &str) {
    let path = std::env::temp_dir().join(format!(
        "cad-unified-profiles-{}-{}-{label}.cadpart",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    d.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded, *d);
    close(total(&valid(d)), total(&valid(&loaded)));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn one_extrude_combines_two_sketch_regions_and_planar_face_and_follows_all_edits() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let base = rectangle(&r, &mut d, [0., 0.], [6., 4.]);
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":base,"depth":"2"}));
    let a = rectangle(&r, &mut d, [10., 0.], [14., 3.]);
    let mut s = Sketch::default();
    s.add_circle([24., 2.], 2.);
    let b = sketch(&r, &mut d, s);
    let before = valid(&d);
    let profiles = json!([region(&before,&a,[11.,1.]),region(&before,&b,[24.,2.]),{"kind":"face","part":format!("{ex}:0"),"face":format!("{ex}:end")}]);
    let id = feature(
        &r,
        &mut d,
        "extrude",
        json!({"profiles":profiles,"depth":"3"}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 4);
    close(total(&e), 156. + 12. * PI);
    assert_eq!(e.geometry.outputs[&id].len(), 3);
    #[cfg(feature = "step")]
    {
        let folder = std::path::Path::new("target/test-output/geometry-controls");
        std::fs::create_dir_all(folder).unwrap();
        d.save(&folder.join("mixed-profiles.cadpart")).unwrap();
        e.geometry
            .export("step", &folder.join("mixed-profiles.step"))
            .unwrap();
        std::fs::write(
            folder.join("mixed-profiles.json"),
            serde_json::to_vec_pretty(&json!({
            "case":"mixed-profiles","step_exported":true,"expected_solids":[
                {"volume_mm3":48.,"bounds_mm":[[0.,0.,0.],[6.,4.,2.]]},
                {"volume_mm3":36.,"bounds_mm":[[10.,0.,0.],[14.,3.,3.]]},
                {"volume_mm3":12.*PI,"bounds_mm":[[22.,0.,0.],[26.,4.,3.]]},
                {"volume_mm3":72.,"bounds_mm":[[0.,0.,2.],[6.,4.,5.]]}
            ]}))
            .unwrap(),
        )
        .unwrap();
    }
    let mut s = d.features[2].sketch().unwrap();
    s.entities[0].values[2] = "6".into();
    d.features[2].set_sketch(&s);
    let mut s = d.features[3].sketch().unwrap();
    s.entities[0].values[2] = "3".into();
    d.features[3].set_sketch(&s);
    d.features[1].params["depth"] = json!("5");
    let e = valid(&d);
    assert_eq!(d.features[4].params["profiles"], profiles);
    close(total(&e), 246. + 27. * PI);
    close(e.scene.meshes[3].bounds[0][2] as f64, 5.);
    close(e.scene.meshes[3].bounds[1][2] as f64, 8.);
    roundtrip(&d, "mixed");
}
#[test]
fn nested_regions_can_select_annulus_disc_or_their_union_without_losing_holes() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.add_circle([0., 0.], 2.);
    let sk = sketch(&r, &mut d, s);
    let before = valid(&d);
    let ring = region(&before, &sk, [3., 0.]);
    let disc = region(&before, &sk, [0., 0.]);
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"profiles":[ring.clone()],"depth":"3"}),
    );
    close(total(&valid(&d)), 63. * PI);
    d.features[1].params["profiles"] = json!([disc.clone()]);
    close(total(&valid(&d)), 12. * PI);
    d.features[1].params["profiles"] = json!([ring, disc]);
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 1);
    close(total(&e), 75. * PI);
    roundtrip(&d, "nested");
}
#[test]
fn shared_creation_scopes_apply_every_selected_profile() {
    for (mode, volume) in [("Remove", 1920.), ("Add", 2016.), ("Intersect", 80.)] {
        let r = Registry::builtin().unwrap();
        let mut d = Document::default();
        let base = rectangle(&r, &mut d, [0., 0.], [20., 10.]);
        let ex = feature(&r, &mut d, "extrude", json!({"sketch":base,"depth":"10"}));
        let a = rectangle(&r, &mut d, [2., 2.], [4., 4.]);
        let x = if mode == "Intersect" { 4. } else { 10. };
        let b = rectangle(&r, &mut d, [x, 2.], [x + 2., 4.]);
        let e = valid(&d);
        let profiles = json!([region(&e, &a, [3., 3.]), region(&e, &b, [x + 1., 3.])]);
        feature(
            &r,
            &mut d,
            "extrude",
            json!({"profiles":profiles,"depth":if mode=="Add"{"12"}else{"10"},"mode":mode,"targets":[format!("{ex}:0")]}),
        );
        let e = valid(&d);
        assert_eq!(e.scene.meshes.len(), 1);
        close(total(&e), volume);
        roundtrip(&d, mode);
    }
}
#[test]
fn revolve_accepts_regions_from_multiple_sketches_with_one_shared_axis() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = rectangle(&r, &mut d, [2., 0.], [4., 3.]);
    let b = rectangle(&r, &mut d, [-8., 6.], [-6., 9.]);
    let e = valid(&d);
    let profiles = json!([region(&e, &a, [3., 1.]), region(&e, &b, [-7., 7.])]);
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"profiles":profiles,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 2);
    close(total(&e), 120. * PI);
    roundtrip(&d, "revolve");
    d.features[1].params["offset"] = json!("5");
    let e = eval(&d);
    assert!(
        e.scene
            .error
            .as_ref()
            .unwrap()
            .contains("every selected profile plane")
    );
    assert!(e.scene.meshes.is_empty());
}
#[test]
fn explicit_empty_duplicate_missing_and_future_profiles_reject_atomically() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = rectangle(&r, &mut d, [0., 0.], [6., 4.]);
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    let e = valid(&d);
    let good = region(&e, &sk, [2., 2.]);
    feature(&r, &mut d, "extrude", json!({"profiles":[]}));
    for profiles in [
        json!([]),
        json!([good.clone(), good]),
        json!([{"kind":"region","sketch":sk,"region":"missing"}]),
        json!([{"kind":"sketch","sketch":"f999"}]),
        json!([{"kind":"face","part":format!("{ex}:0"),"face":"missing"}]),
    ] {
        d.features.last_mut().unwrap().params["profiles"] = profiles;
        let e = eval(&d);
        assert!(e.scene.error.is_some());
        assert_eq!(e.scene.meshes.len(), 1);
        close(total(&e), 48.);
    }
}
#[test]
fn exact_old_package_hashes_reopen_without_reinterpreting_legacy_inputs() {
    for (plugin, version, hash) in [
        ("extrude", "1.2.0", "0d4a61c8cdf65c59"),
        ("revolve", "1.1.0", "50aaf7062e8e6eca"),
    ] {
        let r = Registry::builtin().unwrap();
        let mut d = Document::default();
        let sk = rectangle(&r, &mut d, [2., 0.], [4., 3.]);
        feature(&r, &mut d, plugin, json!({"sketch":sk}));
        let f = d.features.last_mut().unwrap();
        f.plugin_version = version.into();
        f.plugin_hash = hash.into();
        f.params.as_object_mut().unwrap().remove("profiles");
        let old = f.clone();
        valid(&d);
        roundtrip(&d, plugin);
        assert_eq!(d.features.last().unwrap(), &old);
        let f = d.features.last_mut().unwrap();
        f.plugin_hash = "different".into();
        assert!(eval(&d).scene.error.is_some());
    }
}
#[test]
fn adopting_old_profile_input_preserves_downstream_attached_sketch_references() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = rectangle(&r, &mut d, [0., 0.], [6., 4.]);
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    let old = &mut d.features[1];
    old.plugin_version = "1.2.0".into();
    old.plugin_hash = "0d4a61c8cdf65c59".into();
    old.params.as_object_mut().unwrap().remove("profiles");
    let attached = rectangle(&r, &mut d, [1., 1.], [3., 3.]);
    d.features.last_mut().unwrap().params["support"] = json!(format!("{ex}:end"));
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"sketch":attached,"depth":"3"}),
    );
    let before = valid(&d);
    let references = ProfileReference::from_params(&d.features[1].params).unwrap();
    d.features[1].params["profiles"] = json!(references);
    r.plugins["extrude"].adopt_inputs(&mut d.features[1]);
    assert_eq!(d.features[1].plugin_version, "1.3.0");
    let after = valid(&d);
    close(total(&before), total(&after));
    assert!(
        after.scene.meshes[0]
            .faces
            .iter()
            .any(|f| f.key == format!("{ex}:end"))
    );
    roundtrip(&d, "adoption");
}
#[test]
fn output_reference_namespaces_keep_second_profile_support_when_first_plane_moves() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let plane = feature(&r, &mut d, "plane", json!({"offset":"0"}));
    let a = rectangle(&r, &mut d, [0., 0.], [4., 3.]);
    d.features.last_mut().unwrap().params["support"] = json!(plane);
    let b = rectangle(&r, &mut d, [10., 0.], [14., 3.]);
    let e = valid(&d);
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"profiles":[region(&e,&a,[2.,1.]),region(&e,&b,[12.,1.])],"depth":"2"}),
    );
    let e = valid(&d);
    let mesh = e.scene.meshes.iter().find(|m| m.bounds[0][0] > 9.).unwrap();
    let part = mesh.part.clone();
    let keys: Vec<_> = mesh.faces.iter().map(|f| f.key.clone()).collect();
    let support = mesh
        .faces
        .iter()
        .find(|f| f.key.ends_with(":end"))
        .unwrap()
        .key
        .clone();
    assert!(support.contains(":profile:"), "{support}");
    let child = rectangle(&r, &mut d, [10.5, 0.5], [11.5, 1.5]);
    d.features.last_mut().unwrap().params["support"] = json!(support);
    feature(&r, &mut d, "extrude", json!({"sketch":child,"depth":"1"}));
    for offset in [0., 5., -4.] {
        d.features[0].params["offset"] = json!(offset.to_string());
        let e = valid(&d);
        let mesh = e.scene.meshes.iter().find(|m| m.part == part).unwrap();
        assert_eq!(
            mesh.faces.iter().map(|f| f.key.clone()).collect::<Vec<_>>(),
            keys
        );
        close(e.scene.sketches[&child].plane.origin[2], 2.);
        close(e.scene.meshes.last().unwrap().bounds[1][2] as f64, 3.);
    }
    roundtrip(&d, "profile-namespaces");
}
#[test]
fn source_reorder_and_removal_never_rebind_parts_or_attached_children() {
    for same_sketch in [false, true] {
        let r = Registry::builtin().unwrap();
        let mut d = Document::default();
        let a = rectangle(&r, &mut d, [0., 0.], [4., 3.]);
        let b = if same_sketch {
            let mut sk = d.features[0].sketch().unwrap();
            sk.add_rect([10., 0.], [14., 3.]);
            d.features[0].set_sketch(&sk);
            a.clone()
        } else {
            rectangle(&r, &mut d, [10., 0.], [14., 3.])
        };
        let before = valid(&d);
        let left = region(&before, &a, [2., 1.]);
        let right = region(&before, &b, [12., 1.]);
        let ex_index = d.features.len();
        let ex = feature(
            &r,
            &mut d,
            "extrude",
            json!({"profiles":[left.clone(),right.clone()],"depth":"2"}),
        );
        let before = valid(&d);
        let mesh = before
            .scene
            .meshes
            .iter()
            .find(|m| m.bounds[0][0] > 9.)
            .unwrap();
        let part = mesh.part.clone();
        let cap = mesh
            .faces
            .iter()
            .find(|f| f.key.ends_with(":end"))
            .unwrap()
            .key
            .clone();
        let child = rectangle(&r, &mut d, [10.5, 0.5], [11.5, 1.5]);
        d.features.last_mut().unwrap().params["support"] = json!(cap);
        for profiles in [
            json!([left.clone(), right.clone()]),
            json!([right.clone(), left.clone()]),
            json!([right.clone()]),
        ] {
            d.features[ex_index].params["profiles"] = profiles;
            let e = valid(&d);
            let mesh = e.scene.meshes.iter().find(|m| m.part == part).unwrap();
            close(mesh.bounds[0][0] as f64, 10.);
            assert!(mesh.faces.iter().any(|f| f.key == cap));
            close(e.scene.sketches[&child].plane.origin[2], 2.);
        }
        roundtrip(&d, "source-removal");
        d.features[ex_index].params["profiles"] = json!([left]);
        let e = eval(&d);
        assert!(
            e.scene.error.as_ref().unwrap().contains("missing"),
            "{:?}",
            e.scene.error
        );
        assert!(!e.scene.sketches.contains_key(&child));
        assert!(!e.geometry.outputs[&ex].contains(&part));
    }
}

#[test]
fn changing_adopted_legacy_boundary_set_fails_closed_instead_of_reusing_part_ordinals() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut sk = Sketch::default();
    sk.add_rect([0., 0.], [4., 3.]);
    sk.add_rect([10., 0.], [14., 3.]);
    let sk = sketch(&r, &mut d, sk);
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    d.features[1].plugin_version = "1.2.0".into();
    d.features[1].plugin_hash = "0d4a61c8cdf65c59".into();
    d.features[1]
        .params
        .as_object_mut()
        .unwrap()
        .remove("profiles");
    let before = valid(&d);
    let right = region(&before, &sk, [12., 1.]);
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"profile_face":{"part":format!("{ex}:0"),"face":format!("{ex}:end")},"depth":"1"}),
    );
    valid(&d);
    d.features[1].params["profiles"] = json!([{"kind":"sketch","sketch":sk}]);
    r.plugins["extrude"].adopt_inputs(&mut d.features[1]);
    valid(&d);
    d.features[1].params["profiles"] = json!([right]);
    let after = eval(&d);
    assert!(after.scene.error.is_some());
    assert_eq!(after.geometry.parts.len(), 1);
    assert_ne!(after.geometry.parts[0].key, format!("{ex}:0"));
    close(after.scene.meshes[0].bounds[0][0] as f64, 10.);
}

#[test]
fn mixed_rectangle_and_full_circle_revolve_produce_exact_annular_cylinder_and_torus() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let rect = rectangle(&r, &mut d, [5., 0.], [20., 10.]);
    let mut circle = Sketch::default();
    circle.add_circle([30., 5.], 3.);
    let circle = sketch(&r, &mut d, circle);
    let before = valid(&d);
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"profiles":[region(&before,&rect,[10.,5.]),region(&before,&circle,[30.,5.])],"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let e = valid(&d);
    assert_eq!(e.geometry.parts.len(), 2);
    close(total(&e), 3750. * PI + 540. * PI * PI);
    let torus = e
        .scene
        .meshes
        .iter()
        .find(|m| (m.volume - 540. * PI * PI).abs() < 1e-7)
        .unwrap();
    assert_eq!(torus.faces.len(), 1);
    assert!(
        torus.edges.is_empty(),
        "collapsed torus seams must not be pickable ghost points"
    );
    let torus_part = torus.part.clone();
    #[cfg(feature = "step")]
    {
        let folder = std::path::Path::new("target/test-output/geometry-controls");
        std::fs::create_dir_all(folder).unwrap();
        d.save(&folder.join("mixed-revolve.cadpart")).unwrap();
        e.geometry
            .export("step", &folder.join("mixed-revolve.step"))
            .unwrap();
        std::fs::write(
            folder.join("mixed-revolve.json"),
            serde_json::to_vec_pretty(&json!({
            "case":"mixed-revolve","step_exported":true,"expected_solids":[
                {"volume_mm3":3750.*PI,"bounds_mm":[[-20.,0.,-20.],[20.,10.,20.]]},
                {"volume_mm3":540.*PI*PI,"bounds_mm":[[-33.,2.,-33.],[33.,8.,33.]],
                 "bounds_tolerance_mm":0.01,"boundary_points_mm":[[33.,5.,0.],[-33.,5.,0.],[0.,5.,33.],[0.,5.,-33.],[30.,2.,0.],[30.,8.,0.]]}
            ]}))
            .unwrap(),
        )
        .unwrap();
    }
    d.features[2].params["profiles"] = json!([region(&before, &circle, [30., 5.])]);
    let alone = valid(&d);
    assert_eq!(alone.geometry.parts.len(), 1);
    assert_eq!(alone.geometry.parts[0].key, torus_part);
    close(total(&alone), 540. * PI * PI);
    roundtrip(&d, "full-circle-revolve");
    d.features[2].params["profiles"] = json!([
        region(&before, &rect, [10., 5.]),
        region(&before, &circle, [30., 5.])
    ]);
    let mut sk = d.features[1].sketch().unwrap();
    sk.entities[0].values[0] = "35".into();
    sk.entities[0].values[2] = "4".into();
    d.features[1].set_sketch(&sk);
    let edited = valid(&d);
    close(total(&edited), 3750. * PI + 1120. * PI * PI);
    assert!(edited.geometry.parts.iter().any(|p| p.key == torus_part));
    roundtrip(&d, "mixed-full-circle-revolve");
}

fn call(path: &std::path::Path, action: Value) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({"version":1,"path":path,"action":action})).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}
#[test]
fn agent_unified_profiles_regenerate_and_invalid_edits_never_write() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = rectangle(&r, &mut d, [0., 0.], [6., 4.]);
    let b = rectangle(&r, &mut d, [10., 0.], [14., 3.]);
    let e = valid(&d);
    let profiles = json!([region(&e, &a, [2., 2.]), region(&e, &b, [11., 1.])]);
    let path = std::env::temp_dir().join(format!(
        "cad-unified-profiles-{}-agent.cadpart",
        std::process::id()
    ));
    d.save(&path).unwrap();
    let (ok, inspected) = call(&path, json!({"op":"inspect"}));
    assert!(ok, "{inspected}");
    let (ok, added) = call(
        &path,
        json!({"op":"apply","output":path,"expected_revision":inspected["result"]["revision"],"edits":[{"op":"add_feature","plugin":"extrude","params":{"profiles":profiles,"depth":"3"}}]}),
    );
    assert!(ok, "{added}");
    close(
        added["result"]["model"]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["volume_mm3"].as_f64().unwrap())
            .sum(),
        108.,
    );
    let before = std::fs::read(&path).unwrap();
    for bad in [
        json!([]),
        json!([{"kind":"region","sketch":a,"region":"missing"}]),
        json!([{"kind":"face","part":"f99:0","face":"f99:end","extra":0}]),
    ] {
        let (ok, result) = call(
            &path,
            json!({"op":"apply","output":path,"expected_revision":added["result"]["revision"],"edits":[{"op":"set_param","feature":"f3","key":"profiles","value":bad}]}),
        );
        assert!(!ok, "{result}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    valid(&Document::load(&path).unwrap());
    std::fs::remove_file(path).unwrap();
    assert!(
        ProfileReference::from_params(&json!({"profiles":[{"kind":"sketch","sketch":""}]}))
            .is_err()
    );
}
