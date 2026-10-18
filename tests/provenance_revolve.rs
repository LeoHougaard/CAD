//! Persistent references are scoped to contributors; Revolve cannot fold a region.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, f64::consts::PI};
fn feature(r: &Registry, d: &mut Document, plugin: &str, params: Value) -> String {
    let mut f = r.create(plugin, d).unwrap();
    for (k, v) in params.as_object().unwrap() {
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
fn eval(d: &Document) -> Evaluation {
    Runtime::new(Registry::builtin().unwrap())
        .evaluate(d)
        .unwrap()
}
fn valid(d: &Document) -> Evaluation {
    let e = eval(d);
    assert!(e.scene.error.is_none(), "{:?}", e.scene.statuses);
    for p in &e.geometry.parts {
        let r = brepkit_operations::validate::validate_solid(&e.geometry.topo, p.solid).unwrap();
        assert!(r.is_valid(), "{:?}", r.issues);
    }
    e
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6 * b.abs().max(1.), "{a} != {b}");
}
fn face_keys(e: &Evaluation, part: &str) -> Vec<String> {
    e.scene
        .meshes
        .iter()
        .find(|m| m.part == part)
        .unwrap()
        .faces
        .iter()
        .map(|f| f.key.clone())
        .collect()
}
fn arc_profile(center: [f64; 2], radius: f64, right: bool) -> Sketch {
    let mut s = Sketch::default();
    let a = [center[0], center[1] - radius];
    let b = [center[0], center[1] + radius];
    s.add_arc(center, a, b, !right);
    s.add_line(b, a);
    s
}
fn box_part(r: &Registry, d: &mut Document, a: [f64; 2], b: [f64; 2], depth: &str) -> String {
    let mut s = Sketch::default();
    s.add_rect(a, b);
    let sk = sketch(r, d, s);
    format!(
        "{}:0",
        feature(r, d, "extrude", json!({"sketch":sk,"depth":depth}))
    )
}
#[test]
fn native_crossing_axis_semicircle_is_rejected_without_changing_earlier_exact_part() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_arc([2.3, -9.], [-17.7, -9.], [22.3, -9.], true);
    s.add_line([22.3, -9.], [-17.7, -9.]);
    let sk = sketch(&r, &mut d, s);
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"10"}));
    let part = format!("{ex}:0");
    let before = valid(&d);
    let names = face_keys(&before, &part);
    assert!(
        names.iter().all(|k| k.starts_with(&format!("{ex}:"))),
        "{names:?}"
    );
    let rev = feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    for angle in [90, 360] {
        d.features.last_mut().unwrap().params["angle"] = json!(angle.to_string());
        let e = eval(&d);
        assert!(
            e.scene
                .error
                .as_ref()
                .unwrap()
                .contains("crosses the interior")
        );
        assert_eq!(e.scene.meshes.len(), 1);
        assert_eq!(face_keys(&e, &part), names);
        close(e.scene.meshes[0].volume, 2000. * PI);
        assert!(!e.geometry.parts[0].source_features.contains(&rev));
        assert!(
            e.geometry
                .anchors
                .iter()
                .all(|(k, _)| !k.starts_with(&format!("{rev}:")))
        );
    }
}
#[test]
fn circular_extrema_detect_crossings_with_endpoints_on_one_side_and_respect_arc_bounds() {
    for right in [false, true] {
        let r = Registry::builtin().unwrap();
        let mut d = Document::default();
        let sk = sketch(&r, &mut d, arc_profile([2., 0.], 3., right));
        feature(
            &r,
            &mut d,
            "revolve",
            json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
        );
        let e = eval(&d);
        if right {
            assert!(e.scene.error.as_ref().unwrap().contains("spindle-torus"));
        } else {
            assert!(
                e.scene
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("crosses the interior")
            );
        }
        assert!(e.geometry.parts.is_empty());
    }
    // An ordinary ring-torus arc has an exact analytic kernel path.
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = sketch(&r, &mut d, arc_profile([5., 0.], 3., true));
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let e = valid(&d);
    let expected = 45. * PI * PI + 36. * PI;
    close(e.scene.meshes[0].volume, expected);
    #[cfg(feature = "step")]
    {
        let folder = std::path::Path::new("target/test-output/geometry-controls");
        std::fs::create_dir_all(folder).unwrap();
        d.save(&folder.join("bounded-arc-revolve.cadpart")).unwrap();
        e.geometry
            .export("step", &folder.join("bounded-arc-revolve.step"))
            .unwrap();
        std::fs::write(folder.join("bounded-arc-revolve.json"),
                    serde_json::to_vec_pretty(&json!({"case":"bounded-arc-revolve","step_exported":true,
                        "expected_solids":[{"volume_mm3":expected,"bounds_mm":[[-8.,-3.,-8.],[8.,3.,8.]]}]})).unwrap()).unwrap();
    }
}
#[test]
fn separate_regions_on_opposite_sides_of_axis_are_independently_valid() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([2., 0.], [4., 3.]);
    s.add_rect([-8., 6.], [-6., 9.]);
    let sk = sketch(&r, &mut d, s);
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 2);
    close(e.scene.meshes.iter().map(|m| m.volume).sum(), 120. * PI);
}
#[test]
fn axis_on_profile_boundary_is_valid_and_upstream_crossing_edit_fails_atomically() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = sketch(&r, &mut d, arc_profile([0., 0.], 3., true));
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 2);
    close(e.scene.meshes[1].volume, 36. * PI);
    let mut s = d.features[0].sketch().unwrap();
    for entity in &mut s.entities {
        for i in (0..if entity.kind == "arc" { 6 } else { 4 }).step_by(2) {
            entity.values[i] = (entity.values[i].parse::<f64>().unwrap() - 1.).to_string();
        }
    }
    d.features[0].set_sketch(&s);
    let e = eval(&d);
    assert!(
        e.scene
            .error
            .as_ref()
            .unwrap()
            .contains("crosses the interior")
    );
    assert_eq!(e.scene.meshes.len(), 1);
    close(e.scene.meshes[0].volume, 9. * PI);
}
#[test]
fn later_valid_revolve_cannot_name_an_earlier_arc_extrusions_faces_or_edges() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = sketch(&r, &mut d, arc_profile([0., 0.], 3., true));
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    let part = format!("{ex}:0");
    let a = valid(&d);
    let faces = face_keys(&a, &part);
    let edges: Vec<_> = a.scene.meshes[0]
        .edges
        .iter()
        .map(|e| e.key.clone())
        .collect();
    let mut unrelated = Sketch::default();
    unrelated.add_rect([1., -3.], [4., 0.]);
    let sk = sketch(&r, &mut d, unrelated);
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Y"}}),
    );
    let b = valid(&d);
    assert_eq!(face_keys(&b, &part), faces);
    assert_eq!(
        b.scene.meshes[0]
            .edges
            .iter()
            .map(|e| e.key.clone())
            .collect::<Vec<_>>(),
        edges
    );
    assert_eq!(b.geometry.parts[0].source_features, BTreeSet::from([ex]));
}
#[test]
fn saved_arc_profile_edge_alias_remains_a_valid_axis_after_side_naming_is_repaired() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let sk = sketch(&r, &mut d, arc_profile([0., 0.], 3., true));
    let ex = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    let e = valid(&d);
    let side = e.scene.meshes[0]
        .faces
        .iter()
        .find(|f| f.key.contains("l2["))
        .unwrap();
    let source = side
        .key
        .strip_prefix(&format!("{ex}:region-side:"))
        .unwrap();
    let old_key = format!("{ex}:profile:{source}:start");
    assert!(
        !e.scene.meshes[0]
            .edges
            .iter()
            .any(|edge| edge.key == old_key)
    );
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{
        "kind":"edge","part":format!("{ex}:0"),"edge":old_key}}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 2);
    close(e.scene.meshes[1].volume, 36. * PI);
}
#[test]
fn untracked_primitive_faces_do_not_borrow_later_unrelated_plane_anchors() {
    let mut r = Registry::builtin().unwrap();
    r.add(r#"{"id":"primitive","name":"Primitive","version":"1","category":"feature","script":"p.rhai"}"#,"fn build(p) { #{op: \"box\", width: 10.0, depth: 10.0, height: 10.0} }").unwrap();
    let mut d = Document::default();
    let id = feature(&r, &mut d, "primitive", json!({}));
    let mut runtime = Runtime::new(r);
    let before = runtime.evaluate(&d).unwrap();
    let keys = face_keys(&before, &format!("{id}:0"));
    assert!(keys.iter().all(|k| k.contains("untracked:")));
    let r = Registry::builtin().unwrap();
    box_part(&r, &mut d, [0., 0.], [10., 10.], "10");
    let after = runtime.evaluate(&d).unwrap();
    assert!(after.scene.error.is_none());
    assert_eq!(face_keys(&after, &format!("{id}:0")), keys);
}
#[test]
fn boolean_kept_tools_keep_their_names_and_result_tracks_actual_contributors() {
    for mode in ["Union", "Subtract", "Intersect"] {
        for keep in [false, true] {
            let r = Registry::builtin().unwrap();
            let mut d = Document::default();
            let a = box_part(&r, &mut d, [0., 0.], [10., 10.], "10");
            let b = box_part(&r, &mut d, [5., 0.], [15., 10.], "10");
            let before = valid(&d);
            let source_b = before.geometry.parts[1].source_features.clone();
            let names_b = face_keys(&before, &b);
            let tools = if mode == "Subtract" {
                json!([b.clone()])
            } else {
                json!([a.clone(), b.clone()])
            };
            let op = feature(
                &r,
                &mut d,
                "boolean",
                json!({"mode":mode,"tools":tools,"targets":[a],"keep_tools":keep}),
            );
            let e = valid(&d);
            let output = e.geometry.outputs[&op][0].clone();
            let result = e.geometry.parts.iter().find(|p| p.key == output).unwrap();
            assert!(source_b.is_subset(&result.source_features));
            assert!(result.source_features.contains(&op));
            if keep {
                assert_eq!(face_keys(&e, &b), names_b);
                assert_eq!(
                    e.geometry
                        .parts
                        .iter()
                        .find(|p| p.key == b)
                        .unwrap()
                        .source_features,
                    source_b
                );
            }
            let mut f = r.create("plane", &mut d).unwrap();
            f.params["reference"] = json!({"kind":"origin","plane":"Right"});
            d.features.push(f);
            assert_eq!(face_keys(&valid(&d), &output), face_keys(&e, &output));
        }
    }
}
#[test]
fn copies_and_patterns_inherit_contributors_without_importing_unrelated_sources() {
    for plugin in ["transform", "pattern", "mirror"] {
        let r = Registry::builtin().unwrap();
        let mut d = Document::default();
        let part = box_part(&r, &mut d, [0., 0.], [10., 10.], "10");
        let original = valid(&d).geometry.parts[0].source_features.clone();
        let id = feature(
            &r,
            &mut d,
            plugin,
            json!({"target":part,"copy":true,"keep":true,"x":"30","spacing":"30","count":"3"}),
        );
        let e = valid(&d);
        for p in e
            .geometry
            .parts
            .iter()
            .filter(|p| p.key.starts_with(&format!("{id}:")))
        {
            assert!(original.is_subset(&p.source_features));
            assert!(p.source_features.contains(&id));
        }
        assert_eq!(e.geometry.parts[0].source_features, original);
    }
}
