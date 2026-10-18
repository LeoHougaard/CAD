//! Independent regressions for document safety, mate ordering and linked files.
use cad::{
    assembly::*,
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cad-assembly-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cad-assembly-review-")
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
fn block(r: &Registry, height: &str) -> Document {
    let mut d = Document::default();
    let mut s = Sketch::default();
    s.add_rect([0., 0.], [10., 8.]);
    let mut f = r.create("sketch", &mut d).unwrap();
    f.set_sketch(&s);
    d.features.push(f);
    let mut f = r.create("extrude", &mut d).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!(height);
    d.features.push(f);
    d
}
fn assembly(r: &Registry, count: usize) -> AssemblyDocument {
    let mut d = AssemblyDocument::default();
    d.sources
        .push(Source::new("source1".into(), "Block".into(), block(r, "5"), None).unwrap());
    for n in 0..count {
        let mut i = Instance::new(format!("instance{n}"), "source1".into(), "f2:0".into());
        i.fixed = n == 0;
        d.instances.push(i);
    }
    d
}
fn link(id: &str, kind: MateKind, a: &str, b: &str) -> Mate {
    let mut m = Mate::new(kind, Connector::origin(a), Connector::origin(b));
    m.id = id.into();
    m
}
#[test]
fn articulated_loop_rejection_does_not_depend_on_feature_order() {
    let r = Arc::new(Registry::builtin().unwrap());
    let mut d = assembly(&r, 3);
    let links = [
        link("a", MateKind::Revolute, "instance0", "instance1"),
        link("b", MateKind::Fastened, "instance1", "instance2"),
        link("c", MateKind::Fastened, "instance2", "instance0"),
    ];
    let mut runtime = AssemblyRuntime::new(r);
    for p in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        d.mates = p.map(|i| links[i].clone()).to_vec();
        assert!(
            runtime.evaluate(&d).err().unwrap().contains("loop"),
            "order {p:?}"
        );
    }
}
#[test]
fn rigid_cycle_does_not_prevent_an_independent_revolute_branch() {
    let r = Arc::new(Registry::builtin().unwrap());
    let mut d = assembly(&r, 4);
    d.mates = vec![
        link("a", MateKind::Fastened, "instance0", "instance1"),
        link("b", MateKind::Fastened, "instance1", "instance2"),
        link("c", MateKind::Fastened, "instance2", "instance0"),
        link("hinge", MateKind::Revolute, "instance2", "instance3"),
    ];
    d.mates[3].rotation_deg[2] = 30.;
    let mut runtime = AssemblyRuntime::new(r);
    let result = runtime.evaluate(&d).unwrap();
    assert_eq!(result.degrees_of_freedom, 1);
    let p = result.poses["instance3"].point([2., 0., 0.]);
    assert!((p[0] - 3_f64.sqrt()).abs() < 1e-7);
    assert!((p[1] - 1.).abs() < 1e-7);
}
#[test]
fn context_primary_frame_must_equal_its_frozen_pose() {
    let r = Arc::new(Registry::builtin().unwrap());
    let d = assembly(&r, 2);
    let mut runtime = AssemblyRuntime::new(r);
    let mut c = runtime.capture_context(&d, "instance0").unwrap();
    c.primary_transform.translation[0] = 5.;
    assert!(c.validate().is_err());
}
#[test]
fn duplicate_context_feature_bindings_are_rejected_before_application() {
    let r = Arc::new(Registry::builtin().unwrap());
    let d = assembly(&r, 2);
    let mut runtime = AssemblyRuntime::new(r);
    let mut c = runtime.capture_context(&d, "instance0").unwrap();
    let binding = ContextBinding {
        feature: "f2".into(),
        instance: "instance1".into(),
        face: "f2:end".into(),
        kind: BindingKind::ExtrudeUpToFace,
        offset: 0.,
    };
    c.bindings = vec![
        binding.clone(),
        ContextBinding {
            offset: 3.,
            ..binding
        },
    ];
    assert!(c.validate().is_err());
    let mut part = d.sources[0].document.clone();
    let before = part.clone();
    assert!(runtime.apply_context(&c, &mut part).is_err());
    assert_eq!(part, before);
}
#[test]
fn loading_a_part_as_an_assembly_cannot_erase_its_features() {
    let r = Registry::builtin().unwrap();
    let t = Temp::new();
    let path = t.0.join("wrong.cadasm");
    block(&r, "5").save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(AssemblyDocument::load(&path).is_err());
    assert!(
        assembly(&r, 1)
            .save_checked(&path, Some("irrelevant"))
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn lowered_identity_counter_does_not_reuse_existing_identity() {
    let r = Registry::builtin().unwrap();
    let mut d = assembly(&r, 1);
    d.next_id = 1;
    if d.validate().is_ok() {
        let id = d.allocate_id("source");
        assert_ne!(id, "source1");
    }
}
#[test]
fn concurrent_revision_guarded_writers_preserve_one_complete_winner() {
    let r = Registry::builtin().unwrap();
    let t = Temp::new();
    let path = t.0.join("race.cadasm");
    let d = assembly(&r, 1);
    d.save_checked(&path, None).unwrap();
    let revision = d.revision().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let mut candidate = d.clone();
            candidate.name = format!("Writer{n}");
            let path = path.clone();
            let revision = revision.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                candidate
                    .save_checked(&path, Some(&revision))
                    .map(|()| candidate.name)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    let loaded = AssemblyDocument::load(&path).unwrap();
    assert_eq!(loaded.name, winner);
    assert_eq!(loaded.sources, d.sources);
}
fn call(path: &Path, cwd: &Path, action: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-assembly-agent"))
        .current_dir(cwd)
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
    let output = child.wait_with_output().unwrap();
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stderr)))
}
#[test]
fn linked_source_inspection_and_reload_use_assembly_directory() {
    let r = Registry::builtin().unwrap();
    let t = Temp::new();
    let correct = t.0.join("assembly");
    let wrong = t.0.join("cwd");
    std::fs::create_dir(&correct).unwrap();
    std::fs::create_dir(&wrong).unwrap();
    block(&r, "5")
        .save(&correct.join("linked.cadpart"))
        .unwrap();
    block(&r, "99").save(&wrong.join("linked.cadpart")).unwrap();
    let mut d = assembly(&r, 1);
    d.sources[0].path = Some("linked.cadpart".into());
    let path = correct.join("test.cadasm");
    d.save(&path).unwrap();
    let inspected = call(&path, &wrong, json!({"op":"inspect"}));
    assert_eq!(inspected["ok"], true, "{inspected}");
    assert_eq!(inspected["result"]["links"][0]["changed"], false);
    block(&r, "8")
        .save(&correct.join("linked.cadpart"))
        .unwrap();
    let reloaded = call(
        &path,
        &wrong,
        json!({"op":"reload_source","source":"source1","expected_revision":d.revision().unwrap()}),
    );
    assert_eq!(reloaded["ok"], true, "{reloaded}");
    let loaded = AssemblyDocument::load(&path).unwrap();
    assert_eq!(loaded.sources[0].document.features[1].params["depth"], "8");
    assert_eq!(
        loaded.sources[0]
            .path
            .as_deref()
            .unwrap()
            .file_name()
            .unwrap(),
        "linked.cadpart"
    );
}

#[test]
fn assembly_transforms_exact_edge_axes_with_the_display_geometry() {
    let r = Registry::builtin().unwrap();
    let document = block(&r, "5");
    let mut runtime = cad::engine::Runtime::new(r);
    let evaluation = runtime.evaluate(&document).unwrap();
    let original = &evaluation.scene.meshes[0];
    let mut pose = Transform::from_euler_degrees([31., 52., 90.]);
    pose.translation = [17., -9., 23.];
    let displayed = transform_mesh(original, &pose);
    let mut checked = 0;
    for (before, after) in original.edges.iter().zip(&displayed.edges) {
        if let Some((point, direction)) = before.axis {
            let (actual_point, actual_direction) = after.axis.unwrap();
            for (actual, expected) in actual_point.into_iter().zip(pose.point(point)) {
                assert!((actual - expected).abs() < 1e-10);
            }
            for (actual, expected) in actual_direction.into_iter().zip(pose.vector(direction)) {
                assert!((actual - expected).abs() < 1e-10);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 12);
}
