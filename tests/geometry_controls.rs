//! Explicit Boolean scopes, parametric Revolve references and exact edge blends.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::f64::consts::PI;
fn feature(r: &Registry, d: &mut Document, id: &str, params: Value) -> String {
    let mut f = r.create(id, d).unwrap();
    for (k, v) in params.as_object().unwrap() {
        f.params[k] = v.clone();
    }
    let key = f.id.clone();
    d.features.push(f);
    key
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
    assert!(e.scene.error.is_none(), "{:?}", e.scene.error);
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
fn export_fixture(d: &Document, e: &Evaluation, name: &str, expected: Vec<Value>) {
    let folder = std::path::Path::new("target/test-output/geometry-controls");
    std::fs::create_dir_all(folder).unwrap();
    d.save(&folder.join(format!("{name}.cadpart"))).unwrap();
    #[cfg(feature = "step")]
    e.geometry
        .export("step", &folder.join(format!("{name}.step")))
        .unwrap();
    std::fs::write(
        folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(
            &json!({"case":name,"step_exported":cfg!(feature="step"),"expected_solids":expected}),
        )
        .unwrap(),
    )
    .unwrap();
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
fn two_boxes() -> (Document, String, String) {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = box_part(&r, &mut d, [0., 0.], [10., 10.], "10");
    let b = box_part(&r, &mut d, [5., 0.], [15., 10.], "10");
    (d, a, b)
}
fn roundtrip(d: &Document, label: &str) {
    let folder = std::env::temp_dir().join(format!(
        "cad-geometry-controls-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("source.cadpart");
    d.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(&loaded, d);
    let a = valid(d);
    let b = valid(&loaded);
    close(
        a.scene.meshes.iter().map(|m| m.volume).sum(),
        b.scene.meshes.iter().map(|m| m.volume).sum(),
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(folder).unwrap();
}
#[test]
fn boolean_operations_have_explicit_scopes_and_keep_tools() {
    for (mode, volume) in [("Union", 1500.), ("Subtract", 500.), ("Intersect", 500.)] {
        for keep in [false, true] {
            let (mut d, a, b) = two_boxes();
            let tools = if mode == "Subtract" {
                json!([b])
            } else {
                json!([a.clone(), b])
            };
            feature(
                &Registry::builtin().unwrap(),
                &mut d,
                "boolean",
                json!({"mode":mode,"targets":[a.clone()],"tools":tools,"keep_tools":keep}),
            );
            let e = valid(&d);
            let result = if keep && mode != "Subtract" {
                e.scene.meshes.last().unwrap()
            } else {
                e.scene.meshes.iter().find(|m| m.part == a).unwrap()
            };
            close(result.volume, volume);
            assert_eq!(
                e.scene.meshes.len(),
                if keep {
                    if mode == "Subtract" { 2 } else { 3 }
                } else {
                    1
                }
            );
            if keep {
                assert_eq!(
                    e.scene
                        .meshes
                        .iter()
                        .filter(|m| (m.volume - 1000.).abs() < 1e-5)
                        .count(),
                    if mode == "Subtract" { 1 } else { 2 }
                );
            }
            roundtrip(&d, &format!("{mode}-{keep}"));
        }
    }
}
#[test]
fn subtract_applies_all_tools_to_all_targets_and_retains_other_parts() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let a = box_part(&r, &mut d, [0., 0.], [10., 10.], "10");
    let b = box_part(&r, &mut d, [20., 0.], [30., 10.], "10");
    let tool = box_part(&r, &mut d, [-1., 0.], [31., 5.], "10");
    let untouched = box_part(&r, &mut d, [50., 0.], [55., 5.], "2");
    feature(
        &r,
        &mut d,
        "boolean",
        json!({"mode":"Subtract","targets":[a,b],"tools":[tool]}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 3);
    close(
        e.scene
            .meshes
            .iter()
            .find(|m| m.part == untouched)
            .unwrap()
            .volume,
        50.,
    );
    close(e.scene.meshes.iter().map(|m| m.volume).sum(), 1050.);
}
#[test]
fn invalid_boolean_scopes_preserve_every_committed_part() {
    for params in [
        json!({"tools":[]}),
        json!({"tools":["f2:0","f2:0"]}),
        json!({"tools":["f2:0","missing"]}),
        json!({"mode":"Subtract","targets":["f2:0"],"tools":["f2:0"]}),
        json!({"mode":"Subtract","targets":[],"tools":["f4:0"]}),
    ] {
        let (mut d, _, _) = two_boxes();
        feature(&Registry::builtin().unwrap(), &mut d, "boolean", params);
        let e = eval(&d);
        assert!(e.scene.error.is_some());
        assert_eq!(e.scene.meshes.len(), 2);
        for m in &e.scene.meshes {
            close(m.volume, 1000.);
        }
    }
}
fn radial_profile() -> (Document, String, u64) {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([2., 0.], [4., 3.]);
    s.add_rect([6., 0.], [8., 3.]);
    s.add_line([0., -2.], [0., 6.]);
    let axis = s.entities.last_mut().unwrap();
    axis.construction = true;
    let id = axis.id;
    let sk = sketch(&r, &mut d, s);
    (d, sk, id)
}
#[test]
fn revolve_regions_and_sketch_axis_follow_parametric_edits() {
    let r = Registry::builtin().unwrap();
    let (mut d, sk, axis) = radial_profile();
    let initial = valid(&d);
    let key = initial.scene.regions[&sk]
        .iter()
        .find(|r| r.contains([3., 1.]))
        .unwrap()
        .key
        .clone();
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"regions":[key.clone()],"axis_ref":{"kind":"sketch_line","sketch":sk,"entity":axis}}),
    );
    close(valid(&d).scene.meshes[0].volume, 36. * PI);
    let mut s = d.features[0].sketch().unwrap();
    s.entities[0].values[2] = "3".into();
    d.features[0].set_sketch(&s);
    close(valid(&d).scene.meshes[0].volume, 63. * PI);
    roundtrip(&d, "region-axis");
    d.features.last_mut().unwrap().params["regions"] = json!([]);
    assert!(eval(&d).scene.error.as_ref().unwrap().contains("region"));
    d.features.last_mut().unwrap().params["regions"] = json!(["missing"]);
    assert!(eval(&d).scene.error.is_some());
}
#[test]
fn revolve_planar_face_and_straight_edge_axis_are_persistent() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let axis_part = box_part(&r, &mut d, [-1., 0.], [0., 5.], "2");
    let profile_part = box_part(&r, &mut d, [2., 0.], [4., 3.], "2");
    let before = valid(&d);
    let axis_key = before
        .scene
        .meshes
        .iter()
        .find(|m| m.part == axis_part)
        .unwrap()
        .edges
        .iter()
        .find(|e| {
            e.points
                .iter()
                .all(|p| p[0].abs() < 1e-5 && p[2].abs() < 1e-5)
        })
        .unwrap()
        .key
        .clone();
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"profile_face":{"part":profile_part,"face":"f4:start"},"axis_ref":{"kind":"edge","part":axis_part,"edge":axis_key}}),
    );
    let e = valid(&d);
    assert_eq!(e.scene.meshes.len(), 3);
    close(e.scene.meshes[2].volume, 36. * PI);
    d.features[3].params["depth"] = json!("4");
    let edited = valid(&d);
    close(edited.scene.meshes[2].volume, 36. * PI);
    close(edited.scene.meshes[1].volume, 24.);
    roundtrip(&d, "face-edge-axis");
}
#[test]
fn invalid_revolve_axis_and_face_do_not_change_source_parts() {
    let r = Registry::builtin().unwrap();
    let (mut d, sk, _) = radial_profile();
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Z"}}),
    );
    assert!(eval(&d).scene.error.as_ref().unwrap().contains("plane"));
    d.features.last_mut().unwrap().params["axis_ref"] =
        json!({"kind":"sketch_line","sketch":sk,"entity":999});
    assert!(eval(&d).scene.error.as_ref().unwrap().contains("missing"));
}
#[test]
fn legacy_revolve_package_and_external_package_pinning_are_preserved() {
    let d = Document::load(std::path::Path::new(
        "examples/project-grooved-pulley.cadpart",
    ))
    .unwrap();
    valid(&d);
    let f = d.features.iter().find(|f| f.plugin == "revolve").unwrap();
    assert_eq!(f.plugin_hash, "967fd7d645e4df99");
    let mut r = Registry::builtin().unwrap();
    let changed = include_str!("../plugins/revolve.json")
        .replace("Revolve sketch regions", "Custom revolve sketch regions");
    r.add(&changed, include_str!("../plugins/revolve.rhai"))
        .unwrap();
    assert!(!r.plugins["revolve"].matches(f));
}
fn plate() -> Document {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 30.]);
    s.add_circle([20., 15.], 4.);
    let sk = sketch(&r, &mut d, s);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
    d
}
#[test]
fn outer_edge_blends_preserve_exact_circular_through_hole() {
    for operation in ["chamfer", "fillet"] {
        for size in [1., 2.] {
            let mut d = plate();
            let e = valid(&d);
            let edge = e.scene.meshes[0]
                .edges
                .iter()
                .find(|e| {
                    e.points
                        .iter()
                        .all(|p| p[1].abs() < 1e-5 && (p[2] - 8.).abs() < 1e-5)
                })
                .unwrap()
                .key
                .clone();
            feature(
                &Registry::builtin().unwrap(),
                &mut d,
                operation,
                json!({"target":"f2:0","edges":[edge],"distance":size.to_string(),"radius":size.to_string()}),
            );
            let result = valid(&d);
            let removed = if operation == "chamfer" {
                20. * size * size
            } else {
                40. * size * size * (1. - PI / 4.)
            };
            close(
                result.scene.meshes[0].volume,
                (1200. - 16. * PI) * 8. - removed,
            );
            assert!(
                result
                    .geometry
                    .face_keys(result.geometry.parts[0].solid)
                    .unwrap()
                    .iter()
                    .any(|(id, _)| matches!(
                        result.geometry.topo.face(*id).unwrap().surface(),
                        brepkit_topology::face::FaceSurface::Cylinder(_)
                    ))
            );
            roundtrip(&d, operation);
            if size == 2. {
                export_fixture(
                    &d,
                    &result,
                    &format!("{operation}-plate-hole"),
                    vec![
                        json!({"volume_mm3":(1200.-16.*PI)*8.-removed,"bounds_mm":[[0.,0.,0.],[40.,30.,8.]]}),
                    ],
                );
            }
        }
    }
}
#[test]
fn planar_names_survive_an_unrelated_coplanar_part_moving() {
    let (mut d, first, second) = two_boxes();
    let before = valid(&d);
    let keys = |e: &Evaluation| {
        e.geometry
            .face_keys(
                e.geometry
                    .parts
                    .iter()
                    .find(|p| p.key == second)
                    .unwrap()
                    .solid,
            )
            .unwrap()
            .into_iter()
            .map(|(_, key)| key)
            .collect::<std::collections::BTreeSet<_>>()
    };
    let expected = keys(&before);
    assert!(expected.contains("f4:start"));
    d.features[0].params["offset"] = json!("20");
    let changed = valid(&d);
    assert_eq!(keys(&changed), expected);
    assert!(changed.scene.meshes.iter().any(|m| m.part == first));
}
#[test]
fn circular_sketch_and_edge_axes_use_exact_centers_and_normals() {
    let r = Registry::builtin().unwrap();
    for kind in ["circle", "arc", "edge"] {
        let edge_axis = kind == "edge";
        let mut d = Document::default();
        let mut circle = Sketch::default();
        if kind == "arc" {
            circle.add_arc([0., 0.], [1., 0.], [0., 1.], false);
        } else {
            circle.add_circle([0., 0.], 1.);
        }
        let id = circle.entities[0].id;
        let axis_sketch = sketch(&r, &mut d, circle);
        let source = if edge_axis {
            Some(format!(
                "{}:0",
                feature(
                    &r,
                    &mut d,
                    "extrude",
                    json!({"sketch":axis_sketch,"depth":"5"})
                )
            ))
        } else {
            None
        };
        let axis_ref = if let Some(part) = source {
            let e = valid(&d);
            let edge = e.scene.meshes[0].edges.iter().find(|e| !e.seam).unwrap();
            assert!(edge.axis.is_some());
            json!({"kind":"edge","part":part,"edge":edge.key})
        } else {
            json!({"kind":"sketch_entity","sketch":axis_sketch,"entity":id})
        };
        let mut s = Sketch::default();
        s.add_rect([2., 0.], [4., 3.]);
        let profile = sketch(&r, &mut d, s);
        d.features.last_mut().unwrap().params["plane"] = json!("Front");
        feature(
            &r,
            &mut d,
            "revolve",
            json!({"sketch":profile,"axis_ref":axis_ref}),
        );
        close(valid(&d).scene.meshes.last().unwrap().volume, 36. * PI);
    }
}
#[test]
fn extrude_planar_face_preserves_holes_and_follows_source_depth() {
    let r = Registry::builtin().unwrap();
    let mut d = plate();
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"profile_face":{"part":"f2:0","face":"f2:end"},"depth":"3"}),
    );
    let result = valid(&d);
    assert_eq!(result.scene.meshes.len(), 2);
    let area = 1200. - 16. * PI;
    close(result.scene.meshes[1].volume, area * 3.);
    close(result.scene.meshes[1].bounds[0][2] as f64, 8.);
    close(result.scene.meshes[1].bounds[1][2] as f64, 11.);
    export_fixture(
        &d,
        &result,
        "face-extrude",
        vec![
            json!({"volume_mm3":area*8.,"bounds_mm":[[0.,0.,0.],[40.,30.,8.]]}),
            json!({"volume_mm3":area*3.,"bounds_mm":[[0.,0.,8.],[40.,30.,11.]]}),
        ],
    );
    d.features[1].params["depth"] = json!("10");
    let edited = valid(&d);
    close(edited.scene.meshes[1].bounds[0][2] as f64, 10.);
    close(edited.scene.meshes[1].volume, area * 3.);
    roundtrip(&d, "face-extrude");
}
#[test]
fn explicit_empty_merge_scope_never_uses_the_legacy_target_alias() {
    for mode in ["Add", "Remove", "Intersect"] {
        let r = Registry::builtin().unwrap();
        let mut d = plate();
        feature(
            &r,
            &mut d,
            "extrude",
            json!({"profile_face":{"part":"f2:0","face":"f2:end"},"mode":mode,"depth":"2","targets":[],"target":"f2:0"}),
        );
        let e = eval(&d);
        assert!(e.scene.error.as_ref().unwrap().contains("scope"));
        assert_eq!(e.scene.meshes.len(), 1);
        close(e.scene.meshes[0].volume, (1200. - 16. * PI) * 8.);
    }
}
#[test]
fn legacy_one_point_one_extrudes_keep_their_pins_and_rebuild() {
    let d = Document::load(std::path::Path::new(
        "examples/project-bearing-block.cadpart",
    ))
    .unwrap();
    assert!(d.features.iter().any(|f| f.plugin == "extrude"
        && f.plugin_version == "1.1.0"
        && f.plugin_hash == "094953e2178e0cde"));
    valid(&d);
}
#[test]
fn annular_revolve_boolean_limit_is_explicit_and_atomic() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 6.);
    let sk = sketch(&r, &mut d, s);
    let target = format!(
        "{}:0",
        feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"3"}))
    );
    let mut s = Sketch::default();
    s.add_rect([2., 0.], [4., 9.]);
    let sk = sketch(&r, &mut d, s);
    d.features.last_mut().unwrap().params["plane"] = json!("Front");
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Z"},"mode":"Add","targets":[target]}),
    );
    let e = eval(&d);
    assert!(
        e.scene
            .error
            .as_ref()
            .unwrap()
            .contains("mesh approximation")
    );
    assert_eq!(e.scene.meshes.len(), 1);
    close(e.scene.meshes[0].volume, 108. * PI);
}
#[test]
fn planar_setbacks_work_on_rotated_plates_with_multiple_holes() {
    let r = Registry::builtin().unwrap();
    for plane in ["Top", "Front", "Right"] {
        let mut d = Document::default();
        let mut s = Sketch::default();
        s.add_rect([0., 0.], [40., 30.]);
        s.add_circle([12., 15.], 3.);
        s.add_circle([28., 15.], 4.);
        let sk = sketch(&r, &mut d, s);
        d.features.last_mut().unwrap().params["plane"] = json!(plane);
        feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
        let before = valid(&d);
        let edge = before.scene.meshes[0]
            .edges
            .iter()
            .find(|e| e.points.len() == 2 && e.key.contains("f2:end"))
            .unwrap();
        let a = edge.points[0];
        let b = edge.points[1];
        let length = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f32>().sqrt() as f64;
        feature(
            &r,
            &mut d,
            "chamfer",
            json!({"edges":[edge.key],"distance":"2"}),
        );
        let after = valid(&d);
        close(
            after.scene.meshes[0].volume,
            (1200. - 25. * PI) * 8. - length * 2.,
        );
    }
}
fn revolved_cylinder() -> Document {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [4., 3.]);
    let sk = sketch(&r, &mut d, s);
    d.features[0].params["plane"] = json!("Front");
    feature(
        &r,
        &mut d,
        "revolve",
        json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Z"}}),
    );
    d
}
#[test]
fn revolved_cylinder_references_support_fillet_and_follow_radius_edits() {
    let r = Registry::builtin().unwrap();
    let mut d = revolved_cylinder();
    let before = valid(&d);
    let edges = before.scene.meshes[0]
        .edges
        .iter()
        .filter(|e| !e.seam)
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| !e.key.contains("untracked:")),"{:?}",edges.iter().map(|e|&e.key).collect::<Vec<_>>());
    let edge = edges
        .iter()
        .find(|e| e.points.iter().all(|p| (p[2] - 3.).abs() < 1e-5))
        .unwrap()
        .key
        .clone();
    export_fixture(
        &d,
        &before,
        "revolved-cylinder",
        vec![json!({"volume_mm3":48.*PI,"bounds_mm":[[-4.,-4.,0.],[4.,4.,3.]]})],
    );
    feature(
        &r,
        &mut d,
        "fillet",
        json!({"edges":[edge.clone()],"radius":"0.5"}),
    );
    let blended = valid(&d);
    assert!(blended.scene.meshes[0].volume < 48. * PI);
    let mut s = d.features[0].sketch().unwrap();
    s.entities[0].values[2] = "5".into();
    d.features[0].set_sketch(&s);
    let changed = valid(&d);
    assert!(changed.scene.meshes[0].volume > blended.scene.meshes[0].volume);
    assert_eq!(d.features.last().unwrap().params["edges"][0], edge);
    roundtrip(&d, "revolve-fillet");
}
#[test]
fn revolved_planar_caps_accept_attached_parametric_sketches() {
    let r = Registry::builtin().unwrap();
    let mut d = revolved_cylinder();
    let before = valid(&d);
    let cap = before.scene.meshes[0]
        .faces
        .iter()
        .find(|f| {
            f.plane
                .as_ref()
                .is_some_and(|p| p.normal[2] > 0.9 && (p.origin[2] - 3.).abs() < 1e-7)
        })
        .unwrap()
        .key
        .clone();
    assert!(!cap.contains("untracked:"));
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 1.);
    let sk = sketch(&r, &mut d, s);
    d.features.last_mut().unwrap().params["support"] = json!(cap);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"1"}));
    let result = valid(&d);
    assert_eq!(result.scene.meshes.len(), 2);
    close(result.scene.meshes[1].volume, PI);
    close(result.scene.meshes[1].bounds[0][2] as f64, 3.);
}
#[test]
fn outer_chamfer_preserves_planar_rectangular_hole_wires() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [40., 30.]);
    s.add_rect([16., 12.], [24., 18.]);
    let sk = sketch(&r, &mut d, s);
    feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
    let before = valid(&d);
    let edge = before.scene.meshes[0]
        .edges
        .iter()
        .find(|e| {
            e.points
                .iter()
                .all(|p| p[1].abs() < 1e-5 && (p[2] - 8.).abs() < 1e-5)
        })
        .unwrap()
        .key
        .clone();
    feature(
        &r,
        &mut d,
        "chamfer",
        json!({"edges":[edge],"distance":"2"}),
    );
    close(valid(&d).scene.meshes[0].volume, (1200. - 48.) * 8. - 80.);
}
#[test]
fn circular_chamfer_on_revolved_cylinder_has_exact_conical_volume() {
    let mut d = revolved_cylinder();
    let before = valid(&d);
    let edge = before.scene.meshes[0]
        .edges
        .iter()
        .find(|e| !e.seam && e.points.iter().all(|p| (p[2] - 3.).abs() < 1e-5))
        .unwrap()
        .key
        .clone();
    feature(
        &Registry::builtin().unwrap(),
        &mut d,
        "chamfer",
        json!({"edges":[edge],"distance":"0.5"}),
    );
    let after = valid(&d);
    close(
        after.scene.meshes[0].volume,
        48. * PI - PI * (4. * 0.5_f64.powi(2) - 0.5_f64.powi(3) / 3.),
    );
}
#[test]
fn repeated_face_extrusions_keep_reference_sizes_bounded() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 10.]);
    s.add_circle([5., 5.], 1.);
    let sk = sketch(&r, &mut d, s);
    let mut source = feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"2"}));
    for _ in 0..12 {
        let next = feature(
            &r,
            &mut d,
            "extrude",
            json!({"profile_face":{"part":format!("{source}:0"),"face":format!("{source}:end")},"depth":"2"}),
        );
        let e = valid(&d);
        let mesh = e.scene.meshes.last().unwrap();
        assert!(mesh.faces.iter().all(|f| f.key.len() < 100));
        assert!(mesh.edges.iter().all(|e| e.key.len() < 200));
        close(mesh.volume, (100. - PI) * 2.);
        source = next;
    }
    d.features[1].params["depth"] = json!("3");
    let e = valid(&d);
    close(e.scene.meshes.last().unwrap().bounds[1][2] as f64, 27.);
}
#[test]
fn lens_face_extrusion_preserves_distinct_source_vertex_references() {
    let r = Registry::builtin().unwrap();
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    s.add_circle([6., 0.], 5.);
    let sk = sketch(&r, &mut d, s);
    let initial = valid(&d);
    let region = initial.scene.regions[&sk]
        .iter()
        .find(|r| r.contains([3., 0.]))
        .unwrap()
        .key
        .clone();
    let source = feature(
        &r,
        &mut d,
        "extrude",
        json!({"sketch":sk,"regions":[region],"depth":"3"}),
    );
    feature(
        &r,
        &mut d,
        "extrude",
        json!({"profile_face":{"part":format!("{source}:0"),"face":format!("{source}:end")},"depth":"3"}),
    );
    let e = valid(&d);
    let edges = e
        .scene
        .meshes
        .last()
        .unwrap()
        .edges
        .iter()
        .filter(|e| !e.seam)
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 6);
    assert!(edges.iter().all(|e| !e.key.contains("untracked:")));
    assert_eq!(
        edges
            .iter()
            .map(|e| &e.key)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );
}
#[test]
fn revolve_operation_scopes_preserve_unselected_parts() {
    let r = Registry::builtin().unwrap();
    for (mode, expected, parts) in [
        ("Add", 144. * PI, 2),
        ("Remove", 32. * PI, 3),
        ("Intersect", 16. * PI, 3),
    ] {
        let mut d = Document::default();
        let mut targets = vec![];
        for z in [0., 6.] {
            let mut s = Sketch::default();
            s.add_circle([0., 0.], 4.);
            let sk = sketch(&r, &mut d, s);
            d.features.last_mut().unwrap().params["offset"] = json!(z.to_string());
            targets.push(format!(
                "{}:0",
                feature(&r, &mut d, "extrude", json!({"sketch":sk,"depth":"3"}))
            ));
        }
        let untouched = box_part(&r, &mut d, [20., 20.], [22., 22.], "2");
        let mut s = Sketch::default();
        s.add_rect([0., 2.], [4., 7.]);
        let sk = sketch(&r, &mut d, s);
        d.features.last_mut().unwrap().params["plane"] = json!("Front");
        feature(
            &r,
            &mut d,
            "revolve",
            json!({"sketch":sk,"axis_ref":{"kind":"origin","axis":"Z"},"mode":mode,"targets":targets}),
        );
        let e = valid(&d);
        assert_eq!(e.scene.meshes.len(), parts, "{mode}");
        for mesh in e.scene.meshes.iter().filter(|m| m.part != untouched) {
            close(mesh.volume, expected);
        }
        close(
            e.scene
                .meshes
                .iter()
                .find(|m| m.part == untouched)
                .unwrap()
                .volume,
            8.,
        );
    }
}
#[test]
fn multiple_outer_chamfers_preserve_holes_and_oversized_setbacks_fail_atomically() {
    let r = Registry::builtin().unwrap();
    let mut d = plate();
    let baseline = valid(&d);
    let keys = baseline.scene.meshes[0]
        .edges
        .iter()
        .filter(|e| {
            e.points.iter().all(|p| (p[2] - 8.).abs() < 1e-5)
                && (e.points.iter().all(|p| p[1].abs() < 1e-5)
                    || e.points.iter().all(|p| (p[1] - 30.).abs() < 1e-5))
        })
        .map(|e| e.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 2);
    feature(&r, &mut d, "chamfer", json!({"edges":keys,"distance":"1"}));
    close(
        valid(&d).scene.meshes[0].volume,
        baseline.scene.meshes[0].volume - 40.,
    );
    d.features.last_mut().unwrap().params["distance"] = json!("9");
    let failed = eval(&d);
    assert!(failed.scene.error.is_some());
    close(
        failed.scene.meshes[0].volume,
        baseline.scene.meshes[0].volume,
    );
}
