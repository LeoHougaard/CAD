use cad::{
    assembly::*,
    engine::Runtime,
    model::{Document, Plane, Sketch},
    plugins::Registry,
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn registry() -> Arc<Registry> {
    Arc::new(Registry::builtin().unwrap())
}
fn block(r: &Registry, height: &str) -> Document {
    let mut document = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [20., 10.]);
    let mut feature = r.create("sketch", &mut document).unwrap();
    feature.set_sketch(&sketch);
    document.features.push(feature);
    let mut feature = r.create("extrude", &mut document).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!(height);
    document.features.push(feature);
    document
}
fn assembly(r: &Registry, count: usize) -> AssemblyDocument {
    let mut document = AssemblyDocument::default();
    document
        .sources
        .push(Source::new("source".into(), "Block".into(), block(r, "5"), None).unwrap());
    for n in 0..count {
        let mut instance = Instance::new(format!("instance{n}"), "source".into(), "f2:0".into());
        instance.transform = Transform::translated([n as f64 * 30., 0., 0.]);
        instance.fixed = n == 0;
        document.instances.push(instance);
    }
    document
}
fn mate(id: &str, kind: MateKind, a: &str, b: &str) -> Mate {
    let mut mate = Mate::new(kind, Connector::origin(a), Connector::origin(b));
    mate.id = id.into();
    mate
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn rigid_transforms_roundtrip_points_and_every_named_plane() {
    let mut pose = Transform::from_euler_degrees([37., -18., 110.]);
    pose.translation = [3., -9., 12.];
    let point = [2., 11., -7.];
    let result = pose.inverse().point(pose.point(point));
    for i in 0..3 {
        close(result[i], point[i]);
    }
    for name in ["Top", "Front", "Right"] {
        let plane = pose.plane(&Plane::named(name, 4.));
        let frame = Transform::from_plane(&plane).unwrap();
        let rebuilt = frame.plane(&Plane::named("Top", 0.));
        for i in 0..3 {
            close(rebuilt.u[i], plane.u[i]);
            close(rebuilt.v[i], plane.v[i]);
            close(rebuilt.normal[i], plane.normal[i]);
        }
    }
}
#[test]
fn repeated_instances_reuse_source_topology_and_pose_changes_do_not_retessellate() {
    let r = registry();
    let mut document = assembly(&r, 12);
    let mut runtime = AssemblyRuntime::new(r);
    let first = runtime.evaluate(&document).unwrap();
    assert_eq!(first.evaluated_sources, 1);
    assert_eq!(first.scene.meshes.len(), 12);
    assert_eq!(first.scene.meshes[0].faces[0].part, "instance0");
    assert!(
        first.scene.meshes[1].faces[0]
            .key
            .starts_with("instance1::")
    );
    document.instances[1].transform = Transform::translated([80., 20., 4.]);
    let second = runtime.evaluate(&document).unwrap();
    assert_eq!(second.evaluated_sources, 0);
    assert!(Arc::ptr_eq(
        &first.sources["source"],
        &second.sources["source"]
    ));
    close(second.scene.meshes[1].centroid[0], 90.);
    let exact = second.instance_geometry(&document, "instance1").unwrap();
    let mesh = exact.scene().unwrap();
    close(mesh.meshes[0].volume, 1000.);
    close(mesh.meshes[0].centroid[0], 90.);
}
#[test]
fn two_link_hinge_and_slider_solve_deterministically_with_limits() {
    let r = registry();
    let mut document = assembly(&r, 3);
    let mut hinge = mate("hinge", MateKind::Revolute, "instance0", "instance1");
    hinge.rotation_deg[2] = 90.;
    hinge.rotation_limits[2] = Some(Limit { min: 0., max: 120. });
    hinge.offset = Transform::translated([20., 0., 0.]);
    let mut slider = mate("slider", MateKind::Slider, "instance1", "instance2");
    slider.position[2] = 15.;
    slider.translation_limits[2] = Some(Limit { min: 0., max: 20. });
    document.mates = vec![slider, hinge];
    let mut runtime = AssemblyRuntime::new(r);
    let result = runtime.evaluate(&document).unwrap();
    assert_eq!(result.degrees_of_freedom, 2);
    let endpoint = result.poses["instance1"].point([10., 0., 0.]);
    close(endpoint[0], 20.);
    close(endpoint[1], 10.);
    close(result.poses["instance2"].translation[2], 15.);
    document.mates[0].position[2] = 21.;
    assert!(
        runtime
            .evaluate(&document)
            .err()
            .unwrap()
            .contains("limits")
    );
}
#[test]
fn fixed_offsets_are_independent_of_free_motion_and_all_mate_kinds_are_supported() {
    let r = registry();
    let mut runtime = AssemblyRuntime::new(r.clone());
    for kind in [
        MateKind::Fastened,
        MateKind::Revolute,
        MateKind::Slider,
        MateKind::Cylindrical,
        MateKind::Planar,
        MateKind::Ball,
    ] {
        let mut document = assembly(&r, 2);
        let mut link = mate("link", kind, "instance0", "instance1");
        link.offset = Transform::translated([7., 11., 3.]);
        match kind {
            MateKind::Fastened => (),
            MateKind::Revolute => link.rotation_deg[2] = 25.,
            MateKind::Slider => link.position[2] = 9.,
            MateKind::Cylindrical => {
                link.position[2] = 9.;
                link.rotation_deg[2] = 25.;
            }
            MateKind::Planar => {
                link.position = [4., 5., 0.];
                link.rotation_deg[2] = 25.;
            }
            MateKind::Ball => link.rotation_deg = [10., 20., 30.],
        }
        document.mates.push(link);
        let evaluated = runtime.evaluate(&document).unwrap();
        assert_eq!(evaluated.degrees_of_freedom, kind.degrees_of_freedom());
        close(
            evaluated.poses["instance1"].translation[2],
            if matches!(kind, MateKind::Slider | MateKind::Cylindrical) {
                12.
            } else {
                3.
            },
        );
    }
}
#[test]
fn consistent_rigid_cycles_validate_but_conflicts_and_articulated_loops_report_errors() {
    let r = registry();
    let mut document = assembly(&r, 3);
    document.mates = vec![
        mate("a", MateKind::Fastened, "instance0", "instance1"),
        mate("b", MateKind::Fastened, "instance1", "instance2"),
        mate("c", MateKind::Fastened, "instance2", "instance0"),
    ];
    let mut runtime = AssemblyRuntime::new(r);
    assert_eq!(runtime.evaluate(&document).unwrap().degrees_of_freedom, 0);
    document.mates[2].offset.translation[0] = 2.;
    assert!(
        runtime
            .evaluate(&document)
            .err()
            .unwrap()
            .contains("conflicts")
    );
    document.mates[2].offset = Transform::default();
    document.mates[2].kind = MateKind::Revolute;
    assert!(runtime.evaluate(&document).err().unwrap().contains("loop"));
}
#[test]
fn stable_face_mates_follow_upstream_dimensions_and_failed_source_updates_are_atomic() {
    let r = registry();
    let mut document = assembly(&r, 2);
    let mut link = Mate::new(
        MateKind::Fastened,
        Connector::on_face("instance0", "instance0::f2:end"),
        Connector::on_face("instance1", "instance1::f2:start"),
    );
    link.id = "face_mate".into();
    document.mates.push(link);
    let mut runtime = AssemblyRuntime::new(r.clone());
    close(
        runtime.evaluate(&document).unwrap().poses["instance1"].translation[2],
        5.,
    );
    runtime
        .update_source(&mut document, "source", block(&r, "8"))
        .unwrap();
    close(
        runtime.evaluate(&document).unwrap().poses["instance1"].translation[2],
        8.,
    );
    let before = document.clone();
    let empty = Document::default();
    assert!(
        runtime
            .update_source(&mut document, "source", empty)
            .is_err()
    );
    assert_eq!(document, before);
    document.mates[0].a.face = Some("missing".into());
    assert!(
        runtime
            .evaluate(&document)
            .err()
            .unwrap()
            .contains("missing or ambiguous")
    );
}
#[test]
fn frozen_contexts_bind_real_geometry_and_only_explicit_update_changes_primary_part() {
    let r = registry();
    let mut document = assembly(&r, 1);
    document.sources[0].id = "primary_source".into();
    document.instances[0].source = "primary_source".into();
    document.instances[0].transform = Transform::translated([10., 0., 0.]);
    document.sources.push(
        Source::new(
            "reference_source".into(),
            "Reference".into(),
            block(&r, "5"),
            None,
        )
        .unwrap(),
    );
    let mut reference = Instance::new("reference".into(), "reference_source".into(), "f2:0".into());
    reference.transform = Transform::translated([10., 0., 20.]);
    reference.fixed = true;
    document.instances.push(reference);
    let mut runtime = AssemblyRuntime::new(r.clone());
    let mut context = runtime.capture_context(&document, "instance0").unwrap();
    close(
        runtime
            .reference_plane(&context, "reference", "f2:end")
            .unwrap()
            .origin[2],
        25.,
    );
    let ghosts = runtime.context_scene(&context).unwrap();
    assert_eq!(ghosts.meshes.len(), 1);
    close(ghosts.meshes[0].centroid[0], 10.);
    let mut primary = document.sources[0].document.clone();
    runtime
        .bind_context(
            &mut context,
            &mut primary,
            ContextBinding {
                feature: "f2".into(),
                instance: "reference".into(),
                face: "f2:end".into(),
                kind: BindingKind::ExtrudeUpToFace,
                offset: 0.,
            },
        )
        .unwrap();
    let evaluated = Runtime::shared(r.clone()).evaluate(&primary).unwrap();
    assert!(evaluated.scene.error.is_none());
    close(evaluated.scene.meshes[0].volume, 5000.);
    runtime
        .update_source(&mut document, "primary_source", primary)
        .unwrap();
    let id = context.id.clone();
    document.contexts.push(context.clone());
    runtime
        .update_source(&mut document, "reference_source", block(&r, "9"))
        .unwrap();
    assert!(context.is_stale(&document));
    close(
        runtime
            .reference_plane(&context, "reference", "f2:end")
            .unwrap()
            .origin[2],
        25.,
    );
    close(
        runtime.evaluate(&document).unwrap().scene.meshes[0].volume,
        5000.,
    );
    runtime.update_context(&mut document, &id).unwrap();
    close(
        runtime.evaluate(&document).unwrap().scene.meshes[0].volume,
        5800.,
    );
    assert_eq!(
        document.sources[0].document.features[1].params["context_ref"]["context"],
        id
    );
    let before = document.clone();
    document.contexts[0].bindings[0].face = "missing".into();
    let invalid = document.clone();
    assert!(runtime.update_context(&mut document, &id).is_err());
    assert_eq!(document, invalid);
    document = before;
    assert!(!document.contexts[0].is_stale(&document));
}
#[test]
fn context_sketch_attachment_survives_reopen_and_guarded_save_rejects_stale_agents() {
    let r = registry();
    let mut document = assembly(&r, 2);
    document.instances[1].fixed = true;
    document.instances[1].transform = Transform::translated([0., 0., 20.]);
    let mut runtime = AssemblyRuntime::new(r);
    let mut context = runtime.capture_context(&document, "instance0").unwrap();
    let mut part = document.sources[0].document.clone();
    runtime
        .bind_context(
            &mut context,
            &mut part,
            ContextBinding {
                feature: "f1".into(),
                instance: "instance1".into(),
                face: "f2:end".into(),
                kind: BindingKind::SketchPlane,
                offset: 2.,
            },
        )
        .unwrap();
    close(
        part.features[0].params["context_plane"]["origin"][2]
            .as_f64()
            .unwrap(),
        27.,
    );
    document.contexts.push(context);
    let dir = std::env::temp_dir().join(format!(
        "cad-assembly-tests-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join("test.cadasm");
    document.save_checked(&path, None).unwrap();
    let loaded = AssemblyDocument::load(&path).unwrap();
    assert_eq!(loaded, document);
    let revision = loaded.revision().unwrap();
    let mut agent = loaded.clone();
    agent.name = "Other edit".into();
    agent.save_checked(&path, Some(&revision)).unwrap();
    assert!(document.save_checked(&path, Some(&revision)).is_err());
    assert_eq!(AssemblyDocument::load(&path).unwrap(), agent);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn all_instance_export_preserves_exact_solids_and_transformed_volumes() {
    let r = registry();
    let mut document = assembly(&r, 3);
    document.instances[1].transform = Transform::from_euler_degrees([20., 30., 40.]);
    document.instances[1].transform.translation = [50., 20., 10.];
    let evaluated = AssemblyRuntime::new(r).evaluate(&document).unwrap();
    let exact = evaluated.geometry(&document).unwrap();
    assert_eq!(exact.parts.len(), 3);
    let scene = exact.scene().unwrap();
    for part in scene.meshes {
        close(part.volume, 1000.);
    }
    #[cfg(feature = "step")]
    {
        let directory = PathBuf::from("output/assembly-tests");
        std::fs::create_dir_all(&directory).unwrap();
        evaluated
            .export(&document, "step", &directory.join("three-instances.step"))
            .unwrap();
        document
            .save(&directory.join("three-instances.cadasm"))
            .unwrap();
    }
}

#[test]
fn installed_mate_plugins_extend_core_kinds_and_saved_packages_are_pinned() {
    let mut packages = Registry::builtin().unwrap();
    packages.add(r#"{"id":"custom-rack-slide","name":"Rack slide","version":"1","category":"mate","script":"custom.rhai","fields":[{"key":"travel","label":"Travel","kind":"number","default":"12"}]}"#,r#"fn build(p) { #{op:"mate",kind:"Slider",position:[0.0,0.0,p.travel]} }"#).unwrap();
    let r = Arc::new(packages);
    let mut runtime = AssemblyRuntime::new(r.clone());
    let mut document = assembly(&r, 2);
    let mut link = runtime
        .mate_from_plugin(
            "custom-rack-slide",
            json!({"travel":"18"}),
            Connector::origin("instance0"),
            Connector::origin("instance1"),
        )
        .unwrap();
    link.id = "rack_slide".into();
    document.mates.push(link);
    close(
        runtime.evaluate(&document).unwrap().poses["instance1"].translation[2],
        18.,
    );
    let mut changed = Registry::builtin().unwrap();
    changed.add(r#"{"id":"custom-rack-slide","name":"Rack slide","version":"1","category":"mate","script":"custom.rhai","fields":[]}"#,r#"fn build(p) { #{op:"mate",kind:"Fastened"} }"#).unwrap();
    let error = AssemblyRuntime::new(Arc::new(changed))
        .evaluate(&document)
        .err()
        .unwrap();
    assert!(error.contains("Plugin code changed"));
    assert_eq!(
        r.plugins
            .values()
            .filter(|p| p.manifest.category == "mate")
            .count(),
        7
    );
}

#[test]
fn circular_edge_mates_use_exact_axis_centers_and_survive_radius_edits() {
    let r = registry();
    let cylinder = |radius: f64| {
        let mut document = Document::default();
        let mut sketch = Sketch::default();
        sketch.add_circle([4., -3.], radius);
        let mut s = r.create("sketch", &mut document).unwrap();
        s.set_sketch(&sketch);
        document.features.push(s);
        let mut e = r.create("extrude", &mut document).unwrap();
        e.params["sketch"] = json!("f1");
        e.params["depth"] = json!("12");
        document.features.push(e);
        document
    };
    let mut document = assembly(&r, 2);
    document.sources[0] =
        Source::new("source".into(), "Cylinder".into(), cylinder(3.), None).unwrap();
    let mut runtime = AssemblyRuntime::new(r.clone());
    let initial = runtime.evaluate(&document).unwrap();
    let source = &initial.sources["source"];
    let part = &source.geometry.parts[0];
    let key = source
        .geometry
        .edge_keys(part.solid)
        .unwrap()
        .into_iter()
        .find_map(|(id, key)| {
            if let brepkit_topology::edge::EdgeCurve::Circle(circle) =
                source.geometry.topo.edge(id).unwrap().curve()
                && circle.center().z().abs() < 1e-7
            {
                return Some(key);
            }
            None
        })
        .unwrap();
    assert!(!key.starts_with("untracked:"));
    let frame = source.edge_frames[&("f2:0".into(), key.clone())];
    close(frame.translation[0], 4.);
    close(frame.translation[1], -3.);
    let mut mate = Mate::new(
        MateKind::Cylindrical,
        Connector::on_edge("instance0", format!("instance0::{key}")),
        Connector::on_edge("instance1", &key),
    );
    mate.id = "bore_axis".into();
    mate.position[2] = 6.;
    mate.rotation_deg[2] = 35.;
    document.mates.push(mate);
    let pose = runtime.evaluate(&document).unwrap().poses["instance1"];
    let center = pose.point([4., -3., 0.]);
    close(center[0], 4.);
    close(center[1], -3.);
    close(center[2], 6.);
    runtime
        .update_source(&mut document, "source", cylinder(5.))
        .unwrap();
    assert!(runtime.evaluate(&document).unwrap().poses["instance1"].approximately(&pose));
    let mut block_document = assembly(&r, 2);
    let evaluation = runtime.evaluate(&block_document).unwrap();
    let source = &evaluation.sources["source"];
    let part = &source.geometry.parts[0];
    let line = source
        .geometry
        .edge_keys(part.solid)
        .unwrap()
        .into_iter()
        .find(|(id, _)| {
            matches!(
                source.geometry.topo.edge(*id).unwrap().curve(),
                brepkit_topology::edge::EdgeCurve::Line
            )
        })
        .unwrap()
        .1;
    let mut invalid = Mate::new(
        MateKind::Revolute,
        Connector::on_edge("instance0", &line),
        Connector::origin("instance1"),
    );
    invalid.id = "straight_edge".into();
    block_document.mates.push(invalid);
    assert!(
        runtime
            .evaluate(&block_document)
            .err()
            .unwrap()
            .contains("circular edge")
    );
}

#[test]
fn updating_old_context_cannot_restore_detached_or_rebound_feature_references() {
    let r = registry();
    let mut document = assembly(&r, 2);
    document.instances[1].fixed = true;
    document.instances[1].transform = Transform::translated([0., 0., 20.]);
    let mut runtime = AssemblyRuntime::new(r);
    let mut original = runtime.capture_context(&document, "instance0").unwrap();
    let mut part = document.sources[0].document.clone();
    let binding = ContextBinding {
        feature: "f2".into(),
        instance: "instance1".into(),
        face: "f2:end".into(),
        kind: BindingKind::ExtrudeUpToFace,
        offset: 0.,
    };
    runtime
        .bind_context(&mut original, &mut part, binding.clone())
        .unwrap();
    runtime
        .update_source(&mut document, "source", part)
        .unwrap();
    let id = original.id.clone();
    document.contexts.push(original);
    let bound = document.clone();

    let mut newer = runtime.capture_context(&document, "instance0").unwrap();
    let mut part = document.sources[0].document.clone();
    runtime
        .bind_context(&mut newer, &mut part, binding.clone())
        .unwrap();
    runtime
        .update_source(&mut document, "source", part)
        .unwrap();
    document.contexts.push(newer);
    let mut old = document.contexts[0].clone();
    let old_before = old.clone();
    let mut part = document.sources[0].document.clone();
    let part_before = part.clone();
    let error = runtime
        .bind_context(
            &mut old,
            &mut part,
            ContextBinding {
                feature: "f1".into(),
                kind: BindingKind::SketchPlane,
                ..binding.clone()
            },
        )
        .unwrap_err();
    assert!(error.contains("no longer belongs"), "{error}");
    assert_eq!(old, old_before);
    assert_eq!(part, part_before);
    // Explicitly rebinding this same feature is allowed.
    runtime.bind_context(&mut old, &mut part, binding).unwrap();
    assert_eq!(part.features[1].params["context_ref"]["context"], id);
    let before = document.clone();
    let error = runtime.update_context(&mut document, &id).unwrap_err();
    assert!(error.contains("no longer belongs"), "{error}");
    assert_eq!(document, before);

    let mut document = bound;
    let mut part = document.sources[0].document.clone();
    let params = part.features[1].params.as_object_mut().unwrap();
    params.remove("context_ref");
    params.remove("context_up_to");
    params.insert("depth".into(), json!("6"));
    runtime
        .update_source(&mut document, "source", part)
        .unwrap();
    let before = document.clone();
    let error = runtime.update_context(&mut document, &id).unwrap_err();
    assert!(error.contains("no longer belongs"), "{error}");
    assert_eq!(document, before);
}
