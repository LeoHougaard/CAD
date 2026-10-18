//! References retain their owner and local frame through placements and joins.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, PlaneReference, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};

fn feature(d: &mut Document, plugin: &str, params: Value) -> String {
    let mut f = Registry::builtin().unwrap().create(plugin, d).unwrap();
    for (key, value) in params.as_object().unwrap() {
        f.params[key] = value.clone();
    }
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn valid(d: &Document) -> Evaluation {
    let e = Runtime::new(Registry::builtin().unwrap())
        .evaluate(d)
        .unwrap();
    assert!(e.scene.error.is_none(), "{:?}", e.scene.statuses);
    e
}
fn base(circle: bool) -> Document {
    let mut d = Document::default();
    d.variables.insert("shift".into(), "30".into());
    let mut s = Sketch::default();
    if circle {
        s.add_circle([0., 0.], 5.);
    } else {
        s.add_rect([0., 0.], [10., 10.]);
    }
    let sk = feature(&mut d, "sketch", json!({}));
    d.features.last_mut().unwrap().set_sketch(&s);
    feature(&mut d, "extrude", json!({"sketch":sk,"depth":"10"}));
    d
}
fn boss(d: &mut Document, part: &str, face: &str, negative_v: bool) {
    let mut s = Sketch::default();
    s.add_rect(
        if negative_v { [2., -4.] } else { [2., 2.] },
        if negative_v { [4., -2.] } else { [4., 4.] },
    );
    let sk = feature(
        d,
        "sketch",
        json!({"support_ref":{"kind":"face","part":part,"face":face}}),
    );
    d.features.last_mut().unwrap().set_sketch(&s);
    feature(d, "extrude", json!({"sketch":sk,"depth":"2"}));
}
fn bounds(e: &Evaluation, index: usize, expected: [[f64; 3]; 2]) {
    for (actual, expected) in e.scene.meshes[index]
        .bounds
        .into_iter()
        .flatten()
        .zip(expected.into_iter().flatten())
    {
        assert!(
            (actual as f64 - expected).abs() < 1e-5,
            "{actual} != {expected}"
        );
    }
}
fn reopened(d: &Document, name: &str) -> Evaluation {
    let p = std::path::PathBuf::from(format!(
        "target/test-output/reference-placement/{name}.cadpart"
    ));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    d.save(&p).unwrap();
    valid(&Document::load(&p).unwrap())
}

#[test]
fn translated_faces_and_attached_sketches_follow_edits_without_moving_the_original() {
    for copy in [false, true] {
        for axis in [0, 2] {
            let mut d = base(false);
            feature(
                &mut d,
                "transform",
                json!({"target":"f2:0","copy":copy,"x":if axis==0 {"shift"}else{"0"},"z":if axis==2 {"shift"}else{"0"}}),
            );
            let owner = if copy { "f3:0" } else { "f2:0" };
            let named = if copy {
                "f3:instance:0::f2:end"
            } else {
                "f2:end"
            };
            for key in [named, "f2:end"] {
                let mut child = d.clone();
                boss(&mut child, owner, key, false);
                for shift in [30., 45.] {
                    child.variables.insert("shift".into(), shift.to_string());
                    let e = valid(&child);
                    let mut lo = [2., 2., 10.];
                    let mut hi = [4., 4., 12.];
                    lo[axis] += shift;
                    hi[axis] += shift;
                    bounds(&e, e.scene.meshes.len() - 1, [lo, hi]);
                    if copy {
                        bounds(&e, 0, [[0., 0., 0.], [10., 10., 10.]]);
                    }
                    let frame = e
                        .geometry
                        .resolve_plane_reference(&PlaneReference::Face {
                            part: owner.into(),
                            face: key.into(),
                        })
                        .unwrap();
                    let mut origin = [0., 0., 10.];
                    origin[axis] += shift;
                    assert_eq!(frame.origin, origin);
                    let fresh = reopened(&child, &format!("translate-{copy}-{axis}-{shift}"));
                    bounds(&fresh, fresh.scene.meshes.len() - 1, [lo, hi]);
                }
            }
            let mut direct = d.clone();
            feature(
                &mut direct,
                "extrude",
                json!({"profiles":[{"kind":"face","part":owner,"face":"f2:end"}],"depth":"2"}),
            );
            let e = valid(&direct);
            let mut lo = [0., 0., 10.];
            let mut hi = [10., 10., 12.];
            lo[axis] += 30.;
            hi[axis] += 30.;
            bounds(&e, e.scene.meshes.len() - 1, [lo, hi]);
            if copy {
                let mut both = d.clone();
                boss(&mut both, "f2:0", "f2:end", false);
                boss(&mut both, owner, named, false);
                both.variables.insert("shift".into(), "45".into());
                let e = valid(&both);
                bounds(&e, 2, [[2., 2., 10.], [4., 4., 12.]]);
                let mut lo = [2., 2., 10.];
                let mut hi = [4., 4., 12.];
                lo[axis] += 45.;
                hi[axis] += 45.;
                bounds(&e, 3, [lo, hi]);
            } else {
                let mut legacy = d.clone();
                let mut s = Sketch::default();
                s.add_rect([2., 2.], [4., 4.]);
                let sk = feature(&mut legacy, "sketch", json!({"support":"f2:end"}));
                legacy.features.last_mut().unwrap().set_sketch(&s);
                feature(&mut legacy, "extrude", json!({"sketch":sk,"depth":"2"}));
                let e = valid(&legacy);
                let mut lo = [2., 2., 10.];
                let mut hi = [4., 4., 12.];
                lo[axis] += 30.;
                hi[axis] += 30.;
                bounds(&e, e.scene.meshes.len() - 1, [lo, hi]);
            }
        }
    }
}

#[test]
fn pattern_instances_and_mirrors_have_independent_right_handed_frames() {
    for axis in ["X", "Z"] {
        let mut d = base(false);
        feature(
            &mut d,
            "pattern",
            json!({"target":"f2:0","axis":axis,"spacing":"shift","count":"3"}),
        );
        for instance in [1, 2] {
            let mut child = d.clone();
            let owner = format!("f3:{instance}");
            boss(&mut child, &owner, "f2:end", false);
            for shift in [30., 45.] {
                child.variables.insert("shift".into(), shift.to_string());
                let e = valid(&child);
                let coordinate = if axis == "X" { 0 } else { 2 };
                let mut lo = [2., 2., 10.];
                let mut hi = [4., 4., 12.];
                lo[coordinate] += instance as f64 * shift;
                hi[coordinate] += instance as f64 * shift;
                bounds(&e, 3, [lo, hi]);
                assert!(
                    e.scene.meshes[instance]
                        .faces
                        .iter()
                        .all(|f| !f.key.contains("untracked:"))
                );
            }
        }
    }
    for keep in [false, true] {
        for plane in ["Right", "Top"] {
            let mut d = base(false);
            feature(
                &mut d,
                "mirror",
                json!({"target":"f2:0","plane":plane,"offset":"shift","keep":keep}),
            );
            let owner = if keep { "f3:0" } else { "f2:0" };
            boss(&mut d, owner, "f2:end", true);
            for shift in [30., 45.] {
                d.variables.insert("shift".into(), shift.to_string());
                let e = valid(&d);
                let expected = if plane == "Right" {
                    [[2. * shift - 4., 2., 10.], [2. * shift - 2., 4., 12.]]
                } else {
                    [[2., 2., 2. * shift - 12.], [4., 4., 2. * shift - 10.]]
                };
                bounds(&e, e.scene.meshes.len() - 1, expected);
                let frame = e
                    .geometry
                    .resolve_plane_reference(&PlaneReference::Face {
                        part: owner.into(),
                        face: "f2:end".into(),
                    })
                    .unwrap();
                frame.validate().unwrap();
            }
        }
    }
}

#[test]
fn placed_circle_edges_remain_selectable_for_fillets() {
    for axis in [0, 2] {
        let mut d = base(true);
        feature(
            &mut d,
            "transform",
            json!({"target":"f2:0","copy":true,"x":if axis==0{"shift"}else{"0"},"z":if axis==2{"shift"}else{"0"}}),
        );
        let e = valid(&d);
        let top = if axis == 2 { 40. } else { 10. };
        assert!(
            e.scene.meshes[1]
                .faces
                .iter()
                .all(|f| !f.key.contains("untracked:"))
        );
        let edge = e.scene.meshes[1]
            .edges
            .iter()
            .find(|edge| !edge.seam && edge.points.iter().all(|p| (p[2] - top).abs() < 1e-5))
            .unwrap();
        feature(
            &mut d,
            "fillet",
            json!({"target":"f3:0","edges":[edge.key],"radius":"0.5"}),
        );
        for shift in [30, 45] {
            d.variables.insert("shift".into(), shift.to_string());
            let e = valid(&d);
            assert!(e.scene.meshes[1].volume < 250. * std::f64::consts::PI);
        }
    }
}

#[test]
fn boolean_joins_keep_copied_boundary_identity_and_reject_ambiguous_old_aliases() {
    let mut d = base(false);
    feature(
        &mut d,
        "transform",
        json!({"target":"f2:0","x":"shift","z":"0"}),
    );
    feature(
        &mut d,
        "transform",
        json!({"target":"f2:0","copy":true,"x":"10","z":"0"}),
    );
    feature(
        &mut d,
        "boolean",
        json!({"mode":"Union","tools":["f2:0","f4:0"]}),
    );
    let wall = "f4:instance:0::f2:side:1:1";
    boss(&mut d, "f2:0", wall, false);
    for shift in [30., 45.] {
        d.variables.insert("shift".into(), shift.to_string());
        let e = valid(&d);
        bounds(&e, 1, [[shift + 20., 2., 2.], [shift + 22., 4., 4.]]);
    }
    let mut d = base(false);
    feature(
        &mut d,
        "pattern",
        json!({"target":"f2:0","axis":"X","spacing":"10","count":"3"}),
    );
    feature(
        &mut d,
        "boolean",
        json!({"mode":"Union","tools":["f3:1","f3:2"]}),
    );
    let e = valid(&d);
    assert!(
        e.geometry
            .resolve_plane_reference(&PlaneReference::Face {
                part: "f3:1".into(),
                face: "f2:end".into()
            })
            .is_err()
    );
    assert!(
        e.geometry
            .resolve_plane_reference(&PlaneReference::Face {
                part: "f3:1".into(),
                face: "f3:instance:1::f2:end".into()
            })
            .is_ok()
    );
}

#[test]
fn kept_boolean_results_are_independent_occurrences_when_moved_and_rejoined() {
    let mut d = base(false);
    feature(
        &mut d,
        "transform",
        json!({"target":"f2:0","copy":true,"x":"10","z":"0"}),
    );
    let before = valid(&d);
    let names: Vec<Vec<_>> = before
        .scene
        .meshes
        .iter()
        .map(|m| m.faces.iter().map(|f| f.key.clone()).collect())
        .collect();
    feature(
        &mut d,
        "boolean",
        json!({"mode":"Union","tools":["f2:0","f3:0"],"keep_tools":true}),
    );
    feature(
        &mut d,
        "transform",
        json!({"target":"f4:0","x":"shift","z":"0"}),
    );
    for shift in [20., 30.] {
        d.variables.insert("shift".into(), shift.to_string());
        let e = valid(&d);
        bounds(&e, 0, [[0., 0., 0.], [10., 10., 10.]]);
        bounds(&e, 1, [[10., 0., 0.], [20., 10., 10.]]);
        bounds(&e, 2, [[shift, 0., 0.], [shift + 20., 10., 10.]]);
        for (mesh, expected) in e.scene.meshes.iter().zip(&names) {
            assert_eq!(
                mesh.faces.iter().map(|f| f.key.clone()).collect::<Vec<_>>(),
                *expected
            );
        }
        for face in ["f4:instance:0::f2:end", "f2:end"] {
            let frame = e
                .geometry
                .resolve_plane_reference(&PlaneReference::Face {
                    part: "f4:0".into(),
                    face: face.into(),
                })
                .unwrap();
            assert_eq!(frame.origin, [shift, 0., 10.]);
        }
    }
    d.variables.insert("shift".into(), "20".into());
    feature(
        &mut d,
        "boolean",
        json!({"mode":"Union","tools":["f2:0","f3:0","f4:0"]}),
    );
    boss(
        &mut d,
        "f2:0",
        "f4:instance:0::f3:instance:0::f2:side:1:1",
        false,
    );
    let e = reopened(&d, "kept-boolean-moved-rejoined");
    bounds(&e, 0, [[0., 0., 0.], [40., 10., 10.]]);
    bounds(&e, 1, [[40., 2., 2.], [42., 4., 4.]]);
}
