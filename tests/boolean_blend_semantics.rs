//! Onshape-style solid operations must preserve material side and joined faces.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::f64::consts::PI;

fn feature(d: &mut Document, plugin: &str, params: Value) -> String {
    let mut f = Registry::builtin().unwrap().create(plugin, d).unwrap();
    for (key, value) in params.as_object().unwrap() {
        f.params[key] = value.clone();
    }
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn sketch(d: &mut Document, s: Sketch, params: Value) -> String {
    let id = feature(d, "sketch", params);
    d.features.last_mut().unwrap().set_sketch(&s);
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
    e
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < expected.abs().max(1.) * 1e-7,
        "{actual} != {expected}"
    );
}

fn fixture(d: &Document, e: &Evaluation, name: &str, volume: f64, bounds: [[f64; 3]; 2]) {
    fixture_spec(d, e, name, json!({"volume_mm3":volume,"bounds_mm":bounds}));
}
fn fixture_spec(d: &Document, e: &Evaluation, name: &str, solid: Value) {
    let folder = std::path::Path::new("target/test-output/blend-semantics");
    std::fs::create_dir_all(folder).unwrap();
    d.save(&folder.join(format!("{name}.cadpart"))).unwrap();
    #[cfg(feature = "step")]
    e.geometry
        .export("step", &folder.join(format!("{name}.step")))
        .unwrap();
    std::fs::write(
        folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(
            &json!({"case":name,"step_exported":cfg!(feature="step"),"expected_solids":[solid]}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn circular_rim_fillets_remove_material_at_both_ends_and_both_walls() {
    let mut failures = vec![];
    for reverse in [false, true] {
        for symmetric in [false, true] {
            for inner in [false, true] {
                for end in [false, true] {
                    let mut d = Document::default();
                    let mut s = Sketch::default();
                    s.add_circle([0., 0.], 10.);
                    s.add_circle([0., 0.], 4.);
                    let sk = sketch(&mut d, s, json!({}));
                    feature(
                        &mut d,
                        "extrude",
                        json!({"sketch":sk,"depth":"8","reverse":reverse,"symmetric":symmetric}),
                    );
                    let before = valid(&d);
                    let z = if symmetric {
                        if end { 4. } else { -4. }
                    } else if end {
                        if reverse { -8. } else { 8. }
                    } else {
                        0.
                    };
                    let radius = if inner { 4. } else { 10. };
                    let edge = before.scene.meshes[0]
                        .edges
                        .iter()
                        .find(|edge| {
                            !edge.seam
                                && edge.points.iter().all(|p| {
                                    (p[2] as f64 - z).abs() < 1e-5
                                        && (p[0].hypot(p[1]) as f64 - radius).abs() < 1e-4
                                })
                        })
                        .unwrap();
                    feature(&mut d, "fillet", json!({"edges":[edge.key],"radius":"0.5"}));
                    let result = eval(&d);
                    let area = 0.25 * (1. - PI / 4.);
                    let moment = 0.125 * (5. / 6. - PI / 4.);
                    let removed = 2. * PI * (radius * area + if inner { moment } else { -moment });
                    let expected = 84. * PI * 8. - removed;
                    let actual = result.scene.meshes[0].volume;
                    let low = if symmetric {
                        -4.
                    } else if reverse {
                        -8.
                    } else {
                        0.
                    };
                    fixture(
                        &d,
                        &result,
                        &format!("rim-{reverse}-{symmetric}-{inner}-{end}"),
                        expected,
                        [[-10., -10., low], [10., 10., low + 8.]],
                    );
                    if result.scene.error.is_some() || (actual - expected).abs() > 1e-6 {
                        failures.push(format!("reverse={reverse} symmetric={symmetric} inner={inner} end={end}: volume={actual} expected={expected} error={:?}", result.scene.error));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn additions_join_coplanar_faces_before_downstream_blends() {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 10.]);
    let sk = sketch(&mut d, s, json!({}));
    let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
    let mut s = Sketch::default();
    s.add_rect([5., 0.], [15., 10.]);
    let sk = sketch(&mut d, s, json!({}));
    feature(
        &mut d,
        "extrude",
        json!({"sketch":sk,"depth":"8","mode":"Add","targets":[format!("{base}:0")]}),
    );
    let e = valid(&d);
    close(e.scene.meshes[0].volume, 1200.);
    assert_eq!(
        e.scene.meshes[0].faces.len(),
        6,
        "a rectangular union must have six joined faces"
    );
    let edge = e.scene.meshes[0]
        .edges
        .iter()
        .find(|edge| {
            edge.points
                .iter()
                .all(|p| p[1].abs() < 1e-6 && (p[2] - 8.).abs() < 1e-6)
        })
        .unwrap();
    feature(&mut d, "fillet", json!({"edges":[edge.key],"radius":"1"}));
    close(
        valid(&d).scene.meshes[0].volume,
        1200. - 15. * (1. - PI / 4.),
    );
}

#[test]
fn stacked_additions_have_no_internal_cap_or_wall_seam() {
    for circle in [false, true] {
        let mut d = Document::default();
        let mut s = Sketch::default();
        if circle {
            s.add_circle([0., 0.], 5.);
        } else {
            s.add_rect([0., 0.], [10., 10.]);
        }
        let sk = sketch(&mut d, s.clone(), json!({}));
        let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"4"}));
        let sk = sketch(&mut d, s, json!({"offset":"4"}));
        feature(
            &mut d,
            "extrude",
            json!({"sketch":sk,"depth":"4","mode":"Add","targets":[format!("{base}:0")]}),
        );
        let e = valid(&d);
        close(
            e.scene.meshes[0].volume,
            if circle { 200. * PI } else { 800. },
        );
        assert_eq!(
            e.scene.meshes[0].faces.len(),
            if circle { 3 } else { 6 },
            "circle={circle}: {:?}",
            e.scene.meshes[0]
                .faces
                .iter()
                .map(|f| &f.key)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn overlapping_cylinders_join_caps_without_internal_selection_edges() {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    let sk = sketch(&mut d, s, json!({}));
    let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
    let mut s = Sketch::default();
    s.add_circle([6., 0.], 5.);
    let sk = sketch(&mut d, s, json!({}));
    feature(
        &mut d,
        "extrude",
        json!({"sketch":sk,"depth":"8","mode":"Add","targets":[format!("{base}:0")]}),
    );
    let e = valid(&d);
    let lens = 50. * 0.6_f64.acos() - 24.;
    close(e.scene.meshes[0].volume, (50. * PI - lens) * 8.);
    let caps = e.scene.meshes[0]
        .faces
        .iter()
        .filter(|f| f.plane.is_some())
        .count();
    assert_eq!(caps, 2, "each union cap must be one face");
    assert_eq!(
        e.scene.meshes[0].faces.len(),
        4,
        "each surviving cylinder wall must be one face"
    );
}

#[test]
fn direct_face_extrudes_follow_a_pocket_floors_material_normal() {
    for reverse in [false, true] {
        let mut d = Document::default();
        let mut s = Sketch::default();
        s.add_rect([0., 0.], [20., 16.]);
        let sk = sketch(&mut d, s, json!({}));
        let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
        let mut s = Sketch::default();
        s.add_rect([3., 4.], [7., 10.]);
        let sk = sketch(&mut d, s, json!({"offset":"4"}));
        let cut = feature(
            &mut d,
            "extrude",
            json!({"sketch":sk,"depth":"4","mode":"Remove","targets":[format!("{base}:0")]}),
        );
        let e = valid(&d);
        let floor = e.scene.meshes[0]
            .faces
            .iter()
            .find(|f| f.key == format!("{cut}:start"))
            .unwrap();
        feature(
            &mut d,
            "extrude",
            json!({"profile_face":{"part":format!("{base}:0"),"face":floor.key},"depth":"2","reverse":reverse}),
        );
        let result = valid(&d);
        let body = &result.scene.meshes[1];
        close(body.volume, 48.);
        let expected = if reverse {
            [[3., 4., 2.], [7., 10., 4.]]
        } else {
            [[3., 4., 4.], [7., 10., 6.]]
        };
        for (actual, expected) in body.bounds.iter().flatten().zip(expected.iter().flatten()) {
            close(*actual as f64, *expected);
        }
    }
}

#[test]
fn joined_boolean_history_keeps_face_support_after_edit_and_reopen() {
    let mut d = Document::default();
    d.variables.insert("height".into(), "8".into());
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 10.]);
    let sk = sketch(&mut d, s, json!({}));
    let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"height"}));
    let mut s = Sketch::default();
    s.add_rect([5., 0.], [15., 10.]);
    let sk = sketch(&mut d, s, json!({}));
    feature(
        &mut d,
        "extrude",
        json!({"sketch":sk,"depth":"height","mode":"Add","targets":[format!("{base}:0")]}),
    );
    let mut s = Sketch::default();
    s.add_circle([7.5, 5.], 2.);
    let sk = sketch(&mut d, s, json!({}));
    feature(
        &mut d,
        "extrude",
        json!({"sketch":sk,"depth":"height","mode":"Remove","targets":[format!("{base}:0")]}),
    );
    let e = valid(&d);
    let edge = e.scene.meshes[0]
        .edges
        .iter()
        .find(|edge| {
            edge.points
                .iter()
                .all(|p| p[1].abs() < 1e-6 && (p[2] - 8.).abs() < 1e-6)
        })
        .unwrap()
        .key
        .clone();
    feature(&mut d, "fillet", json!({"edges":[edge],"radius":"1"}));
    let mut s = Sketch::default();
    s.add_circle([2., 5.], 1.);
    let child = sketch(&mut d, s, json!({"support":format!("{base}:end")}));
    for height in [8., 10.] {
        d.variables.insert("height".into(), height.to_string());
        let e = valid(&d);
        close(
            e.scene.meshes[0].volume,
            (150. - 4. * PI) * height - 15. * (1. - PI / 4.),
        );
        close(e.geometry.sketches[&child].plane.origin[2], height);
        assert_eq!(
            e.scene.meshes[0]
                .faces
                .iter()
                .filter(|f| f.key == format!("{base}:end"))
                .count(),
            1
        );
        let name = format!("boolean-history-{height}");
        fixture(
            &d,
            &e,
            &name,
            (150. - 4. * PI) * height - 15. * (1. - PI / 4.),
            [[0., 0., 0.], [15., 10., height]],
        );
        let reopened = Document::load(
            &std::path::Path::new("target/test-output/blend-semantics")
                .join(format!("{name}.cadpart")),
        )
        .unwrap();
        assert_eq!(d, reopened);
        close(
            valid(&reopened).geometry.sketches[&child].plane.origin[2],
            height,
        );
    }
}

#[test]
fn analytic_prism_boolean_modes_preserve_arc_material_and_face_names() {
    let lens = 50. * 0.6_f64.acos() - 24.;
    for (mode, area, faces) in [
        ("Add", 50. * PI - lens, 4),
        ("Remove", 25. * PI - lens, 4),
        ("Intersect", lens, 4),
    ] {
        for reverse in [false, true] {
            let mut d = Document::default();
            let mut s = Sketch::default();
            s.add_circle([0., 0.], 5.);
            let sk = sketch(&mut d, s, json!({}));
            let base = feature(
                &mut d,
                "extrude",
                json!({"sketch":sk,"depth":"8","reverse":reverse}),
            );
            let mut s = Sketch::default();
            s.add_circle([6., 0.], 5.);
            let sk = sketch(&mut d, s, json!({}));
            feature(
                &mut d,
                "extrude",
                json!({"sketch":sk,"depth":"8","reverse":reverse,"mode":mode,"targets":[format!("{base}:0")]}),
            );
            let e = valid(&d);
            close(e.scene.meshes[0].volume, area * 8.);
            assert_eq!(e.scene.meshes[0].faces.len(), faces);
            let (lo, hi) = match mode {
                "Add" => (-5., 11.),
                "Remove" => (-5., 3.),
                _ => (1., 5.),
            };
            let z = if reverse { -8. } else { 0. };
            let (x, y) = if mode == "Intersect" {
                (3., 4.)
            } else {
                (0., 5.)
            };
            // OCC's STL box samples arc extrema. Exact projection probes keep
            // all six analytic bounds checked independently at 1e-7 mm.
            fixture_spec(
                &d,
                &e,
                &format!("prism-{mode}-{reverse}"),
                json!({
                    "volume_mm3":area*8.,"bounds_mm":[[lo,-y,z],[hi,y,z+8.]],"bounds_tolerance_mm":0.01,
                    "boundary_points_mm":[[lo,0.,z],[hi,if mode=="Remove" {4.} else {0.},z],[x,y,z],[x,-y,z],[x,y,z+8.],[x,-y,z+8.]]
                }),
            );
            assert!(
                e.scene.meshes[0]
                    .faces
                    .iter()
                    .all(|face| !face.key.contains("untracked:")),
                "{mode} reverse={reverse}: {:?}",
                e.scene.meshes[0]
                    .faces
                    .iter()
                    .map(|f| &f.key)
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn an_oversized_closed_rim_blend_is_rejected_without_changing_the_part() {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_circle([0., 0.], 5.);
    let sk = sketch(&mut d, s, json!({}));
    feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8"}));
    let before = valid(&d);
    let edges: Vec<_> = before.scene.meshes[0]
        .edges
        .iter()
        .filter(|edge| !edge.seam)
        .map(|edge| edge.key.clone())
        .collect();
    feature(&mut d, "fillet", json!({"edges":edges,"radius":"100"}));
    let failed = eval(&d);
    assert!(
        failed.scene.statuses.last().unwrap().error.is_some(),
        "a partial kernel result must not commit"
    );
    close(failed.scene.meshes[0].volume, 200. * PI);
    assert_eq!(failed.geometry.parts[0].feature, "f2");
}

#[test]
fn contained_prism_booleans_are_independent_of_frame_and_extrusion_direction() {
    for plane in ["Top", "Front", "Right"] {
        for reverse in [false, true] {
            for (mode, area, faces) in [
                ("Add", 25. * PI, 3),
                ("Remove", 21. * PI, 4),
                ("Intersect", 4. * PI, 3),
            ] {
                let mut d = Document::default();
                let mut s = Sketch::default();
                s.add_circle([20., -30.], 5.);
                let sk = sketch(&mut d, s, json!({"plane":plane,"offset":"17"}));
                let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8","reverse":reverse}));
                let mut s = Sketch::default();
                s.add_circle([21., -30.], 2.);
                let sk = sketch(&mut d, s, json!({"plane":plane,"offset":"17"}));
                feature(&mut d, "extrude", json!({"sketch":sk,"depth":"8","reverse":reverse,"mode":mode,"targets":[format!("{base}:0")]}));
                let e = valid(&d);
                close(e.scene.meshes[0].volume, area * 8.);
                assert_eq!(e.scene.meshes[0].faces.len(), faces, "{plane}/{mode}/{reverse}");
            }
        }
    }
}

#[test]
fn sequential_planar_fillets_keep_exact_shared_contact_boundaries() {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [20., 20.]);
    let sk = sketch(&mut d, s, json!({}));
    feature(&mut d, "extrude", json!({"sketch":sk,"depth":"10"}));
    for y in [0., 20.] {
        let e = valid(&d);
        let edge = e.scene.meshes[0].edges.iter().find(|edge| !edge.seam
            && edge.points.iter().all(|p| (p[1] - y).abs() < 1e-6 && (p[2] - 10.).abs() < 1e-6))
            .unwrap();
        feature(&mut d, "fillet", json!({"edges":[edge.key],"radius":"2"}));
    }
    close(valid(&d).scene.meshes[0].volume, 4000. - 160. * (1. - PI / 4.));
}

#[test]
fn disconnected_coplanar_faces_keep_distinct_boundary_lineage_after_edits() {
    let mut d = Document::default();
    d.variables.insert("boss_height".into(), "25".into());
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [20., 20.]);
    let sk = sketch(&mut d, s, json!({}));
    let base = feature(&mut d, "extrude", json!({"sketch":sk,"depth":"10","symmetric":true}));
    let part = format!("{base}:0");
    let mut s = Sketch::default();
    s.add_rect([-10., 0.], [0., 20.]);
    let sk = sketch(&mut d, s, json!({}));
    feature(&mut d, "extrude", json!({"sketch":sk,"depth":"boss_height","symmetric":true,"mode":"Add","targets":[part]}));
    let e = valid(&d);
    let faces: Vec<_> = e.scene.meshes[0].faces.iter().filter(|f| f.plane.as_ref().is_some_and(|p|
        p.normal[0].abs() > 1. - 1e-7 && p.origin[0].abs() < 1e-7)).map(|f|f.key.clone()).collect();
    assert_eq!(faces.len(), 2, "The upper and lower exposed wall patches are disconnected");
    assert_ne!(faces[0], faces[1]);
    assert!(faces.iter().all(|key| !key.contains("untracked:")));
    // Existing documents intentionally refer to the common oriented support
    // plane. A through-band cut splits the original -X wall without reversing it.
    let mut legacy = d.clone();
    let mut band = Sketch::default();
    band.add_rect([-1., -1.], [5., 21.]);
    legacy.features[2].set_sketch(&band);
    legacy.features[3].params["mode"] = json!("Remove");
    legacy.features[3].params["depth"] = json!("2");
    let split = valid(&legacy);
    let patches: Vec<_> = split.scene.meshes[0].faces.iter().filter(|f|
        f.plane.as_ref().is_some_and(|p| p.normal[0] < -1. + 1e-7 && p.origin[0].abs() < 1e-7)).collect();
    assert_eq!(patches.len(), 2);
    let support = patches[0].key.rsplit_once(":patch:").unwrap().0;
    let mut s = Sketch::default(); s.add_point([2., 3.]);
    let legacy_child = sketch(&mut legacy, s, json!({"support":support}));
    let expected = patches[0].plane.as_ref().unwrap();
    assert_eq!(&valid(&legacy).geometry.sketches[&legacy_child].plane, expected);
    let path = std::path::Path::new("target/test-output/blend-semantics/legacy-split-support.cadpart");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    legacy.save(path).unwrap();
    assert_eq!(&valid(&Document::load(path).unwrap()).geometry.sketches[&legacy_child].plane, expected);
    for (index, face) in faces.iter().enumerate() {
        let mut child = d.clone();
        feature(&mut child, "extrude", json!({"profiles":[{"kind":"face","part":part,"face":face}],"depth":"2"}));
        let mut s = Sketch::default();
        s.add_circle([2., 3.], 0.5);
        let reference = json!({"kind":"face","part":part,"face":face});
        let child_sketch = sketch(&mut child, s, json!({"support":face,"support_ref":reference,"offset":"1"}));
        feature(&mut child, "extrude", json!({"sketch":child_sketch,"depth":"3"}));
        for height in [25, 29] {
            child.variables.insert("boss_height".into(), height.to_string());
            let evaluated = valid(&child);
            assert_eq!(evaluated.scene.meshes.len(), 3);
            close(evaluated.scene.meshes[0].volume, 4000. + 200. * height as f64);
            close(evaluated.scene.meshes[1].volume, 40. * (height as f64 / 2. - 5.));
            close(evaluated.scene.meshes[2].volume, 0.75 * PI);
            let source = face.rsplit_once(":patch:").unwrap().0;
            // Both exposed boss walls face +X. Preserve the source U axis and
            // origin while reversing its tool normal/V to face out of material.
            let expected = cad::model::Plane {
                origin: [1., 20., 0.], u: [0., -1., 0.],
                v: [0., 0., -1.], normal: [1., 0., 0.],
            };
            assert_eq!(evaluated.geometry.sketches[&child_sketch].plane, expected,
                "The selected patch must retain its original outward parametric frame");
            let path = std::path::PathBuf::from(format!("target/test-output/blend-semantics/face-island-{index}-{height}.cadpart"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            child.save(&path).unwrap();
            let reopened = valid(&Document::load(&path).unwrap());
            close(reopened.scene.meshes[1].volume, evaluated.scene.meshes[1].volume);
            assert_eq!(reopened.geometry.sketches[&child_sketch].plane, expected);
            close(reopened.scene.meshes[2].volume, 0.75 * PI);
            for invalid in [
                json!({"kind":"face","part":"missing:0","face":face}),
                json!({"kind":"face","part":part,"face":format!("{source}:patch:0000000000000000")}),
                json!({"kind":"face","part":part,"face":source}),
            ] {
                let mut broken = child.clone();
                broken.features.iter_mut().find(|f| f.id == child_sketch).unwrap().params["support_ref"] = invalid;
                let result = eval(&broken);
                assert!(!result.geometry.sketches.contains_key(&child_sketch),
                    "Missing owned patch must fail without silently choosing its coplanar sibling");
                assert!(result.scene.error.is_some());
            }
        }
    }
}
