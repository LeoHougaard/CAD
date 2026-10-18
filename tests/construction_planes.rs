//! Construction planes remain live references throughout parametric regeneration.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Plane, PlaneReference, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn feature(r: &Registry, d: &mut Document, plugin: &str, params: Value) -> String {
    let mut f = r.create(plugin, d).unwrap();
    for (key, value) in params.as_object().unwrap() {
        f.params[key] = value.clone();
    }
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn profile(r: &Registry, d: &mut Document, support: &str) -> String {
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [6., 4.]);
    let mut f = r.create("sketch", d).unwrap();
    f.params["support"] = json!(support);
    f.set_sketch(&s);
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn evaluate(d: &Document) -> Evaluation {
    Runtime::new(Registry::builtin().unwrap())
        .evaluate(d)
        .unwrap()
}
fn valid(d: &Document) -> Evaluation {
    let e = evaluate(d);
    assert!(e.scene.error.is_none(), "{:?}", e.scene.error);
    for p in &e.geometry.parts {
        let report =
            brepkit_operations::validate::validate_solid(&e.geometry.topo, p.solid).unwrap();
        assert!(report.is_valid(), "{:?}", report.issues);
    }
    for p in e.scene.planes.values() {
        p.validate().unwrap();
    }
    e
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7 * b.abs().max(1.), "{a} != {b}");
}
fn xyz(a: [f64; 3], b: [f64; 3]) {
    for i in 0..3 {
        close(a[i], b[i]);
    }
}
fn set(d: &mut Document, id: &str, key: &str, value: Value) {
    d.features.iter_mut().find(|f| f.id == id).unwrap().params[key] = value;
}
fn roundtrip(d: &Document, label: &str) {
    let path = std::env::temp_dir().join(format!(
        "cad-construction-planes-{}-{label}.cadpart",
        std::process::id()
    ));
    d.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded, *d);
    let a = valid(d);
    let b = valid(&loaded);
    assert_eq!(
        serde_json::to_value(&a.scene.planes).unwrap(),
        serde_json::to_value(&b.scene.planes).unwrap()
    );
    close(
        a.scene.meshes.iter().map(|m| m.volume).sum(),
        b.scene.meshes.iter().map(|m| m.volume).sum(),
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn chained_offsets_drive_sketch_and_solid_after_edit_and_reopen() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    d.variables.insert("height".into(), "10".into());
    let a = feature(&r, &mut d, "plane", json!({"offset":"height"}));
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"datum","feature":a},"offset":"5"}),
    );
    let prefix = valid(&d);
    assert!(prefix.geometry.parts.is_empty());
    assert!(prefix.geometry.anchors.is_empty());
    let sk = profile(&r, &mut d, &b);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"3"}));
    for height in [10., 20.] {
        d.variables.insert("height".into(), height.to_string());
        let e = valid(&d);
        xyz(e.scene.planes[&a].origin, [0., 0., height]);
        xyz(e.scene.planes[&b].origin, [0., 0., height + 5.]);
        close(e.scene.meshes[0].volume, 72.);
        close(e.scene.meshes[0].bounds[0][2] as f64, height + 5.);
        close(e.scene.meshes[0].bounds[1][2] as f64, height + 8.);
    }
    roundtrip(&d, "offset");
}
#[test]
fn face_planes_follow_depth_and_do_not_steal_solid_face_names() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = profile(&r, &mut d, "");
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"10"}));
    let part = format!("{ex}:0");
    let before = valid(&d);
    let keys: Vec<_> = before.scene.meshes[0]
        .faces
        .iter()
        .map(|f| f.key.clone())
        .collect();
    let end = json!({"kind":"face","part":part,"face":format!("{ex}:end")});
    let start = json!({"kind":"face","part":part,"face":format!("{ex}:start")});
    let mid = feature(
        &r,
        &mut d,
        "plane",
        json!({"mode":"Midplane","reference":end,"second":start}),
    );
    let offset = feature(&r, &mut d, "plane", json!({"reference":end,"offset":"5"}));
    let bottom = feature(&r, &mut d, "plane", json!({"reference":start,"offset":"5"}));
    feature(&r, &mut d, "plane", json!({"reference":end,"offset":"0"}));
    for depth in [10., 20.] {
        set(&mut d, &ex, "depth", json!(depth.to_string()));
        let e = valid(&d);
        close(e.scene.planes[&mid].origin[2], depth / 2.);
        close(e.scene.planes[&offset].origin[2], depth + 5.);
        close(e.scene.planes[&bottom].origin[2], -5.);
        xyz(e.scene.planes[&bottom].normal, [0., 0., -1.]);
        assert_eq!(
            keys,
            e.scene.meshes[0]
                .faces
                .iter()
                .map(|f| f.key.clone())
                .collect::<Vec<_>>()
        );
    }
    roundtrip(&d, "faces");
}
fn signed_distance(p: &Plane, point: [f64; 3]) -> f64 {
    (0..3).map(|i| (point[i] - p.origin[i]) * p.normal[i]).sum()
}
#[test]
fn almost_parallel_midplane_retains_the_real_angular_bisector() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = feature(&r, &mut d, "plane", json!({"offset":"1"}));
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"mode":"Angle","axis_ref":{"kind":"origin","axis":"X"},"angle":"0.0001"}),
    );
    let mid = feature(
        &r,
        &mut d,
        "plane",
        json!({"mode":"Midplane","reference":{"kind":"datum","feature":a},"second":{"kind":"datum","feature":b}}),
    );
    for flip in [false, true] {
        set(&mut d, &mid, "flip_alignment", json!(flip));
        let e = valid(&d);
        for local in [[0., 0.], [0., 1_000_000.]] {
            let p = e.scene.planes[&mid].at(local);
            close(
                signed_distance(&e.scene.planes[&a], p).abs(),
                signed_distance(&e.scene.planes[&b], p).abs(),
            );
        }
    }
}
#[test]
fn nonparallel_midplanes_have_two_correct_bisectors_and_flip_normal() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"origin","plane":"Top"},"offset":"12"}),
    );
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"origin","plane":"Front"},"offset":"8"}),
    );
    let mid = feature(
        &r,
        &mut d,
        "plane",
        json!({"mode":"Midplane","reference":{"kind":"datum","feature":a},"second":{"kind":"datum","feature":b}}),
    );
    let mut normals = vec![];
    for flip in [false, true] {
        set(&mut d, &mid, "flip_alignment", json!(flip));
        let e = valid(&d);
        let p = &e.scene.planes[&mid];
        normals.push(p.normal);
        for q in [[0., 0.], [3., -7.], [-12., 4.]] {
            let point = p.at(q);
            close(
                signed_distance(&e.scene.planes[&a], point).abs(),
                signed_distance(&e.scene.planes[&b], point).abs(),
            );
        }
        let expected = p.normal.map(|x| -x);
        set(&mut d, &mid, "flip_normal", json!(true));
        xyz(valid(&d).scene.planes[&mid].normal, expected);
        set(&mut d, &mid, "flip_normal", json!(false));
    }
    close((0..3).map(|i| normals[0][i] * normals[1][i]).sum(), 0.);
    roundtrip(&d, "bisector");
}
#[test]
fn angle_rotates_about_referenced_line_and_follows_its_upstream_plane() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let base = feature(&r, &mut d, "plane", json!({"offset":"10"}));
    let mut s = Sketch::default();
    s.add_line([10., -5.], [10., 5.]);
    s.entities[0].construction = true;
    let entity = s.entities[0].id;
    let mut f = r.create("sketch", &mut d).unwrap();
    f.params["support"] = json!(base);
    f.set_sketch(&s);
    let axis = f.id.clone();
    d.features.push(f);
    let angle = feature(
        &r,
        &mut d,
        "plane",
        json!({"mode":"Angle","reference":{"kind":"datum","feature":base},"axis_ref":{"kind":"sketch_entity","sketch":axis,"entity":entity},"angle":"90"}),
    );
    let sk = profile(&r, &mut d, &angle);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    for height in [10., 20.] {
        set(&mut d, &base, "offset", json!(height.to_string()));
        let e = valid(&d);
        xyz(e.scene.planes[&angle].origin, [10., 0., height + 10.]);
        xyz(e.scene.planes[&angle].normal, [1., 0., 0.]);
        close(e.scene.meshes[0].volume, 48.);
        close(e.scene.meshes[0].bounds[0][2] as f64, height + 4.);
        close(e.scene.meshes[0].bounds[1][0] as f64, 12.);
    }
    set(&mut d, &angle, "angle", json!("-90"));
    xyz(valid(&d).scene.planes[&angle].normal, [-1., 0., 0.]);
    roundtrip(&d, "angle");
    set(
        &mut d,
        &angle,
        "axis_ref",
        json!({"kind":"origin","axis":"X"}),
    );
    let failed = evaluate(&d);
    assert!(failed.scene.error.as_ref().unwrap().contains("must lie"));
    assert!(!failed.scene.planes.contains_key(&angle));
    assert!(failed.scene.planes.contains_key(&base));
}
#[test]
fn missing_suppressed_deleted_reordered_and_rolled_back_datums_never_fall_back() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = profile(&r, &mut d, "");
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    let a = feature(&r, &mut d, "plane", json!({"offset":"10"}));
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"datum","feature":a}}),
    );
    let base = d.clone();
    d.features[2].suppressed = true;
    let e = evaluate(&d);
    assert!(e.scene.error.as_ref().unwrap().contains("missing"));
    assert_eq!(e.geometry.parts.len(), 1);
    assert!(e.scene.planes.is_empty());
    d = base.clone();
    d.features.remove(2);
    assert!(evaluate(&d).scene.error.is_some());
    d = base.clone();
    d.features.swap(2, 3);
    let e = evaluate(&d);
    assert!(e.scene.error.is_some());
    assert!(e.scene.planes.is_empty());
    d = base.clone();
    d.rollback = Some(3);
    let e = valid(&d);
    assert!(e.scene.planes.contains_key(&a));
    assert!(!e.scene.planes.contains_key(&b));
    d = base;
    set(
        &mut d,
        &b,
        "reference",
        json!({"kind":"datum","feature":"f999"}),
    );
    let e = evaluate(&d);
    assert!(e.scene.error.is_some());
    assert!(e.scene.planes.contains_key(&a));
    assert!(!e.scene.planes.contains_key(&b));
}
#[test]
fn malformed_plane_references_and_curved_faces_are_explicit_errors() {
    for value in [
        json!({"kind":"origin","plane":"Side"}),
        json!({"kind":"datum","feature":""}),
        json!({"kind":"face","part":"@last","face":"f1:end"}),
        json!({"kind":"face","part":"f1:0","face":"untracked:1"}),
    ] {
        assert!(
            serde_json::from_value::<PlaneReference>(value)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert!(
        serde_json::from_value::<PlaneReference>(
            json!({"kind":"origin","plane":"Top","origin":[0,0,0]})
        )
        .is_err()
    );
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    let mut f = r.create("sketch", &mut d).unwrap();
    f.set_sketch(&s);
    let sk = f.id.clone();
    d.features.push(f);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"5"}));
    let e = valid(&d);
    let side = e.scene.meshes[0]
        .faces
        .iter()
        .find(|f| f.plane.is_none())
        .unwrap();
    let part = side.part.clone();
    let key = side.key.clone();
    feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"face","part":part,"face":key}}),
    );
    assert!(
        evaluate(&d)
            .scene
            .error
            .as_ref()
            .unwrap()
            .contains("planar face")
    );
}
fn call(path: &std::path::Path, mut action: Value) -> (bool, Value) {
    if action["op"] == "apply" {
        let bytes = std::fs::read(path).unwrap();
        let revision = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
        action["expected_revision"] = json!(format!("fnv1a64:{revision:016x}"));
        action["output"] = json!(path);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({"version":1,"path":path,"action":action})).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let response = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    (out.status.success(), response)
}
#[test]
fn agent_creates_live_plane_sketch_solid_and_rejects_bad_references_atomically() {
    let path = std::env::temp_dir().join(format!(
        "cad-construction-planes-{}-agent.cadpart",
        std::process::id()
    ));
    Document::default().save(&path).unwrap();
    let (ok, response) = call(
        &path,
        json!({"op":"apply","edits":[{"op":"add_feature","plugin":"plane","params":{"offset":"12"}}]}),
    );
    assert!(ok, "{response}");
    let (ok, response) = call(&path, json!({"op":"inspect"}));
    assert!(ok, "{response}");
    assert_eq!(response["result"]["model"]["planes"][0]["id"], "f1");
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [6., 4.]);
    let (ok, response) = call(
        &path,
        json!({"op":"apply","edits":[{"op":"add_feature","plugin":"sketch","params":{"support":"f1","geometry":s}}]}),
    );
    assert!(ok, "{response}");
    let (ok, response) = call(
        &path,
        json!({"op":"apply","edits":[{"op":"add_feature","plugin":"extrude","params":{"sketch":"f2","depth":"3"}}]}),
    );
    assert!(ok, "{response}");
    close(
        response["result"]["model"]["parts"][0]["volume_mm3"]
            .as_f64()
            .unwrap(),
        72.,
    );
    for reference in [
        json!({"kind":"datum","feature":"f999"}),
        json!({"kind":"origin","plane":"Side"}),
        json!({"kind":"origin","plane":"Top","extra":0}),
    ] {
        let before = std::fs::read(&path).unwrap();
        let (ok, _) = call(
            &path,
            json!({"op":"apply","edits":[{"op":"set_param","feature":"f1","key":"reference","value":reference}]}),
        );
        assert!(!ok);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let (ok, response) = call(
        &path,
        json!({"op":"apply","edits":[{"op":"set_param","feature":"f1","key":"offset","value":"20"}]}),
    );
    assert!(ok, "{response}");
    close(
        response["result"]["model"]["parts"][0]["bounds_mm"][0][2]
            .as_f64()
            .unwrap(),
        20.,
    );
    valid(&Document::load(&path).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn datum_creation_rolls_back_failed_multi_operation_plugins_and_reuses_prefix_cache() {
    let mut r = Registry::builtin().unwrap();
    r.add(r#"{"id":"broken-plane","name":"Broken plane","version":"1","category":"feature","script":"broken-plane.rhai"}"#,
        "fn build(p) { [#{op: \"plane\", mode: \"Offset\", reference: #{kind: \"origin\", plane: \"Top\"}, offset: 42.0}, #{op: \"missing\"}] }").unwrap();
    let mut d = Document::default();
    let a = feature(&r, &mut d, "plane", json!({"offset":"10"}));
    let b = feature(&r, &mut d, "broken-plane", json!({}));
    let mut runtime = Runtime::new(r);
    let e = runtime.evaluate(&d).unwrap();
    assert!(e.scene.error.is_some());
    assert!(e.scene.planes.contains_key(&a));
    assert!(!e.scene.planes.contains_key(&b));
    assert!(e.geometry.anchors.is_empty());
    d.features.pop();
    let r = Registry::builtin().unwrap();
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"datum","feature":a},"offset":"5"}),
    );
    let mut runtime = Runtime::new(r);
    runtime.evaluate(&d).unwrap();
    set(&mut d, &b, "offset", json!("7"));
    let e = runtime.evaluate(&d).unwrap();
    assert!(e.scene.error.is_none());
    assert_eq!(e.scene.reused_features, 1);
    close(e.scene.planes[&b].origin[2], 17.);
}

#[test]
fn expression_format_transitions_invalidate_cached_geometry_with_identical_feature_text() {
    let r = Registry::builtin().unwrap();
    let mut d = Document {
        format: 1,
        ..Document::default()
    };
    let a = feature(&r, &mut d, "plane", json!({"offset":"sin(30)*10"}));
    let b = feature(
        &r,
        &mut d,
        "plane",
        json!({"reference":{"kind":"datum","feature":a},"offset":"5"}),
    );
    let original = d.features.clone();
    let mut runtime = Runtime::new(r);
    let old = runtime.evaluate(&d).unwrap();
    assert!(old.scene.error.is_none());
    close(old.scene.planes[&a].origin[2], 30_f64.sin() * 10.);
    d.format = 2;
    let new = runtime.evaluate(&d).unwrap();
    assert!(new.scene.error.is_none());
    assert_eq!(new.scene.reused_features, 0);
    close(new.scene.planes[&a].origin[2], 5.);
    close(new.scene.planes[&b].origin[2], 10.);
    assert_eq!(d.features, original);
    d.format = 1;
    let old = runtime.evaluate(&d).unwrap();
    assert_eq!(old.scene.reused_features, 0);
    close(old.scene.planes[&a].origin[2], 30_f64.sin() * 10.);
}
