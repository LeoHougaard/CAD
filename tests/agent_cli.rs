use cad::{
    file_lock::FileLock,
    model::{Document, Plane, Sketch, SketchConstraint},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    directory: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "cad-agent-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("part.cadpart");
        document().save(&path).unwrap();
        Self { directory, path }
    }
    fn inspect(&self) -> Value {
        let (ok, response) = call(request(&self.path, json!({"op":"inspect"})));
        assert!(ok, "{response}");
        response["result"].clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir();
        if self.directory.parent() == Some(temporary.as_path())
            && self
                .directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cad-agent-")
        {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
fn document() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut document = Document::default();
    document.variables.insert("depth".into(), "10".into());
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [20., 10.]);
    for (kind, value) in [("Origin", ""), ("Width", "20"), ("Height", "10")] {
        sketch.constraints.push(SketchConstraint {
            kind: kind.into(),
            a: 1,
            value: value.into(),
            ..Default::default()
        });
    }
    let mut feature = registry.create("sketch", &mut document).unwrap();
    feature.set_sketch(&sketch);
    document.features.push(feature);
    let mut feature = registry.create("extrude", &mut document).unwrap();
    feature.params["sketch"] = json!("f1");
    feature.params["depth"] = json!("depth");
    document.features.push(feature);
    document
}
fn request(path: &Path, action: Value) -> Value {
    json!({"version":1,"path":path,"action":action})
}
#[test]
fn agent_accepts_face_profiles_axis_references_and_explicit_body_scopes() {
    let fixture = Fixture::new();
    let first = fixture.inspect();
    let (ok, response) = call(apply(
        &fixture.path,
        &first["revision"],
        json!([{"op":"add_feature","plugin":"extrude","params":{"profile_face":{"part":"f2:0","face":"f2:end"},"depth":"5"}}]),
    ));
    assert!(ok, "{response}");
    let next = fixture.inspect();
    assert_eq!(next["model"]["parts"].as_array().unwrap().len(), 2);
    let (ok, response) = call(apply(
        &fixture.path,
        &next["revision"],
        json!([{"op":"add_feature","plugin":"boolean","params":{"mode":"Union","tools":["f2:0","f3:0"],"keep_tools":false}}]),
    ));
    assert!(ok, "{response}");
    let united = fixture.inspect();
    assert_eq!(united["model"]["parts"].as_array().unwrap().len(), 1);
    assert!((united["model"]["parts"][0]["volume_mm3"].as_f64().unwrap() - 3000.).abs() < 1e-5);
    let fresh = Fixture::new();
    let first = fresh.inspect();
    let (ok, response) = call(apply(
        &fresh.path,
        &first["revision"],
        json!([{"op":"add_feature","plugin":"revolve","params":{"profile_face":{"part":"f2:0","face":"f2:start"},"axis_ref":{"kind":"origin","axis":"Y"}}}]),
    ));
    assert!(ok, "{response}");
    assert_eq!(
        fresh.inspect()["model"]["parts"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn agent_uses_the_same_owned_sketch_support_and_rectangle_axes_as_the_gui() {
    let fixture = Fixture::new();
    let mut geometry = Sketch::default();
    geometry.add_rect([2., 2.], [4., 4.]);
    let first = fixture.inspect();
    let (ok, response) = call(apply(&fixture.path, &first["revision"], json!([
        {"op":"add_feature","plugin":"sketch","params":{
            "geometry":geometry,"support_ref":{"kind":"face","part":"f2:0","face":"f2:end"}}},
        {"op":"add_feature","plugin":"extrude","params":{"sketch":"f3","depth":"2"}}
    ])));
    assert!(ok, "{response}");
    let mut document = Document::load(&fixture.path).unwrap();
    assert_eq!(document.features[2].plugin_version, "1.1.0");
    // A legacy support remains readable until explicitly adopting owner identity.
    document.features[2].plugin_version = "1.0.0".into();
    document.features[2].plugin_hash = "1ab529f725215646".into();
    document.features[2].params["support"] = json!("f2:end");
    document.features[2].params.as_object_mut().unwrap().remove("support_ref");
    document.features[2].params["context_plane"] = json!(Plane::named("Right", 30.));
    document.features[2].params["context_ref"] = json!({"context":"previous-context"});
    document.save(&fixture.path).unwrap();
    let legacy = fixture.inspect();
    let (ok, response) = call(apply(&fixture.path, &legacy["revision"], json!([
        {"op":"set_param","feature":"f3","key":"support_ref","value":{"kind":"face","part":"f2:0","face":"f2:end"}},
        {"op":"set_variable","name":"depth","value":"15"}
    ])));
    assert!(ok, "{response}");
    let moved = fixture.inspect();
    assert_eq!(moved["document"]["features"][2]["plugin_version"], "1.1.0");
    let rebound = &moved["document"]["features"][2]["params"];
    assert!(rebound.get("context_plane").is_none());
    assert!(rebound.get("context_ref").is_none());
    assert_eq!(rebound["support"], "f2:end");
    assert_eq!(moved["model"]["sketches"][1]["plane"]["origin"][2], 15.);
    assert!((moved["model"]["parts"][1]["volume_mm3"].as_f64().unwrap() - 8.).abs() < 1e-7);
    let preserved = std::fs::read(&fixture.path).unwrap();
    let (ok, response) = call(apply(&fixture.path, &legacy["revision"], json!([
        {"op":"set_param","feature":"f3","key":"plane","value":"Front","before":"Top"}
    ])));
    assert!(!ok && response["error"]["code"] == "edit_conflict", "{response}");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), preserved);
    let (ok, response) = call(apply(&fixture.path, &moved["revision"], json!([
        {"op":"set_param","feature":"f3","key":"support_ref","value":null}
    ])));
    assert!(ok, "{response}");
    let cleared = fixture.inspect();
    assert_eq!(cleared["document"]["features"][2]["params"]["support"], "");
    assert!(cleared["document"]["features"][2]["params"]["support_ref"].is_null());
    assert_eq!(cleared["model"]["sketches"][1]["plane"], json!(Plane::named("Top", 0.)));
    let (ok, response) = call(apply(&fixture.path, &cleared["revision"], json!([
        {"op":"set_param","feature":"f3","key":"plane","value":"Front","before":"Top"}
    ])));
    assert!(ok, "{response}");
    let origin = fixture.inspect();
    assert_eq!(origin["document"]["features"][2]["params"]["support"], "");
    assert!(origin["document"]["features"][2]["params"].get("support_ref").is_none());
    assert_eq!(origin["model"]["sketches"][1]["plane"], json!(Plane::named("Front", 0.)));

    // An optional default in a new feature must not mask its explicit legacy support.
    let (ok, response) = call(apply(&fixture.path, &origin["revision"], json!([
        {"op":"add_feature","plugin":"sketch","params":{
            "support_ref":null,"support":"f2:end"}}
    ])));
    assert!(ok, "{response}");
    assert_eq!(fixture.inspect()["model"]["sketches"][2]["plane"]["origin"][2], 15.);

    let fresh = Fixture::new();
    let first = fresh.inspect();
    let bytes = std::fs::read(&fresh.path).unwrap();
    for edge in [json!(4), json!(-1), json!(0.5)] {
        let (ok, response) = call(apply(&fresh.path, &first["revision"], json!([
            {"op":"add_feature","plugin":"revolve","params":{"sketch":"f1",
                "axis_ref":{"kind":"sketch_entity","sketch":"f1","entity":1,"edge":edge}}}
        ])));
        assert!(!ok, "{response}");
        assert_eq!(std::fs::read(&fresh.path).unwrap(), bytes);
    }
    let (ok, response) = call(apply(&fresh.path, &first["revision"], json!([
        {"op":"add_feature","plugin":"revolve","params":{"sketch":"f1",
            "axis_ref":{"kind":"sketch_entity","sketch":"f1","entity":1,"edge":3}}}
    ])));
    assert!(ok, "{response}");
    let revolved = fresh.inspect();
    let volume = revolved["model"]["parts"][1]["volume_mm3"].as_f64().unwrap();
    assert!((volume - 4000. * std::f64::consts::PI).abs() < 1e-6);
}
#[test]
fn malformed_new_feature_inputs_fail_without_writing() {
    let fixture = Fixture::new();
    let initial = fixture.inspect();
    let bytes = std::fs::read(&fixture.path).unwrap();
    for edit in [
        json!({"op":"set_param","feature":"f2","key":"profile_face","value":{"part":"f2:0"}}),
        json!({"op":"set_param","feature":"f2","key":"targets","value":["f2:0","f2:0"]}),
        json!({"op":"set_param","feature":"f2","key":"targets","value":["@last"]}),
        json!({"op":"add_feature","plugin":"revolve","params":{"sketch":"f1","axis_ref":{"kind":"edge","part":"f2:0","edge":17}}}),
        json!({"op":"add_feature","plugin":"revolve","params":{"sketch":"f1","axis_ref":{"kind":"sketch_entity","sketch":"f1","entity":999}}}),
        json!({"op":"add_feature","plugin":"revolve","params":{"sketch":"f1","axis_ref":{"kind":"origin","axis":"Q"}}}),
    ] {
        let (ok, response) = call(apply(&fixture.path, &initial["revision"], json!([edit])));
        assert!(!ok, "{response}");
        assert_eq!(std::fs::read(&fixture.path).unwrap(), bytes);
    }
}
fn call(request: Value) -> (bool, Value) {
    raw(&serde_json::to_vec(&request).unwrap())
}
fn raw(bytes: &[u8]) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(response["version"], 1);
    assert_eq!(response["ok"], output.status.success());
    (output.status.success(), response)
}
fn apply(path: &Path, revision: &Value, edits: Value) -> Value {
    request(
        path,
        json!({"op":"apply","expected_revision":revision,"output":path,"edits":edits}),
    )
}

#[test]
fn inspect_exposes_stable_revision_schemas_solved_geometry_and_references() {
    let fixture = Fixture::new();
    let first = fixture.inspect();
    assert_eq!(first["revision"], fixture.inspect()["revision"]);
    assert!(first["revision"].as_str().unwrap().starts_with("fnv1a64:"));
    assert_eq!(first["document"]["features"][0]["id"], "f1");
    assert!(first["plugins"].as_array().unwrap().iter().any(|p| {
        p["id"] == "extrude"
            && p["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["key"] == "depth")
    }));
    assert_eq!(first["model"]["sketches"][0]["degrees_of_freedom"], 0);
    assert!((first["model"]["parts"][0]["volume_mm3"].as_f64().unwrap() - 2000.).abs() < 1e-5);
    assert!(
        first["model"]["parts"][0]["faces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "f2:end")
    );
    assert!(
        first["model"]["parts"][0]["edges"]
            .as_array()
            .unwrap()
            .len()
            >= 12
    );
    let bytes = std::fs::read(&fixture.path).unwrap();
    let mut modified = bytes.clone();
    modified.push(b'\n');
    std::fs::write(&fixture.path, modified).unwrap();
    assert_ne!(
        first["revision"],
        fixture.inspect()["revision"],
        "Revision must include all bytes, including whitespace"
    );
}

#[test]
fn dimension_param_variable_and_entity_edits_regenerate_save_and_reopen() {
    let fixture = Fixture::new();
    let before = fixture.inspect();
    let edits = json!([
        {"op":"set_dimension","feature":"f1","index":1,"value":"30","before":"20"},
        {"op":"set_variable","name":"depth","value":"12","before":"10"},
        {"op":"set_param","feature":"f2","key":"depth","value":"depth+1","before":"depth"},
        {"op":"set_entity_values","feature":"f1","entity":1,"values":["0","0","30","10"],"before":["0","0","20","10"]}
    ]);
    let (ok, response) = call(apply(&fixture.path, &before["revision"], edits));
    assert!(ok, "{response}");
    let after = fixture.inspect();
    assert_eq!(after["revision"], response["result"]["revision"]);
    assert!((after["model"]["parts"][0]["volume_mm3"].as_f64().unwrap() - 3900.).abs() < 1e-5);
    assert_eq!(after["model"]["sketches"][0]["degrees_of_freedom"], 0);
    assert_eq!(
        after["document"]["features"][0]["plugin_hash"],
        before["document"]["features"][0]["plugin_hash"]
    );
}

#[test]
fn previews_create_distinct_files_and_never_overwrite_without_guard() {
    let fixture = Fixture::new();
    let original = std::fs::read(&fixture.path).unwrap();
    let edit = json!([{"op":"set_dimension","feature":"f1","index":1,"value":"25"}]);
    let (ok, preview) = call(request(&fixture.path, json!({"op":"apply","edits":edit})));
    assert!(ok, "{preview}");
    assert!(preview["result"]["saved_path"].is_null());
    assert_eq!(original, std::fs::read(&fixture.path).unwrap());
    let output = fixture.directory.join("preview.cadpart");
    let (ok, saved) = call(request(
        &fixture.path,
        json!({"op":"apply","output":output,"edits":edit}),
    ));
    assert!(ok, "{saved}");
    assert_eq!(
        Document::load(&output).unwrap().features[0]
            .sketch()
            .unwrap()
            .constraints[1]
            .value,
        "25"
    );
    let (ok, response) = call(request(
        &fixture.path,
        json!({"op":"apply","output":fixture.path,"edits":edit}),
    ));
    assert!(!ok);
    assert_eq!(response["error"]["code"], "invalid_request");
    let (ok, response) = call(request(
        &fixture.path,
        json!({"op":"apply","output":output,"edits":edit}),
    ));
    assert!(!ok);
    assert_eq!(response["error"]["code"], "output_exists");
    assert_eq!(original, std::fs::read(&fixture.path).unwrap());
}

#[test]
fn malformed_failed_and_identity_edits_leave_the_source_unchanged() {
    let fixture = Fixture::new();
    let revision = fixture.inspect()["revision"].clone();
    let original = std::fs::read(&fixture.path).unwrap();
    let requests = [
        json!([{"op":"set_param","feature":"f2","key":"plugin_hash","value":"fake"}]),
        json!([{"op":"set_param","feature":"f2","key":"sketch","value":"missing"}]),
        json!([{"op":"set_param","feature":"f2","key":"symmetric","value":"false"}]),
        json!([{"op":"set_dimension","feature":"f1","index":1,"value":"-5"}]),
        json!([{"op":"set_dimension","feature":"f1","index":0,"value":"5"}]),
        json!([{"op":"set_entity_values","feature":"f1","entity":400,"values":["1"]}]),
        json!([{"op":"set_param","feature":"f2","key":"depth","value":"12","plugin":"fake"}]),
    ];
    for edits in requests {
        let (ok, response) = call(apply(&fixture.path, &revision, edits));
        assert!(!ok, "{response}");
        assert_eq!(original, std::fs::read(&fixture.path).unwrap());
    }
    assert!(!raw(b"not json").0);
    let (ok, response) = call(apply(
        &fixture.path,
        &json!("fnv1a64:obsolete"),
        json!([{"op":"set_variable","name":"depth","value":"11"}]),
    ));
    assert!(!ok);
    assert_eq!(response["error"]["code"], "edit_conflict");
    assert_eq!(original, std::fs::read(&fixture.path).unwrap());
}

#[test]
fn inactive_feature_edits_and_appends_are_rejected_without_changing_the_file() {
    let fixture = Fixture::new();
    for (rollback, suppressed) in [(Some(1), false), (None, true)] {
        let mut document = document();
        document.rollback = rollback;
        document.features[1].suppressed = suppressed;
        document.save(&fixture.path).unwrap();
        let revision = fixture.inspect()["revision"].clone();
        let original = std::fs::read(&fixture.path).unwrap();
        for value in ["-1", "not_a_number", "12"] {
            let (ok, response) = call(apply(
                &fixture.path,
                &revision,
                json!([
                    {"op":"set_param","feature":"f2","key":"depth","value":value}
                ]),
            ));
            assert!(!ok, "{response}");
            assert_eq!(response["error"]["code"], "inactive_feature");
            assert_eq!(original, std::fs::read(&fixture.path).unwrap());
        }
    }
    for (rollback, suppressed) in [(Some(0), false), (None, true)] {
        let mut document = document();
        document.rollback = rollback;
        document.features[0].suppressed = suppressed;
        document.features[1].suppressed = suppressed;
        document.save(&fixture.path).unwrap();
        let revision = fixture.inspect()["revision"].clone();
        let original = std::fs::read(&fixture.path).unwrap();
        for edit in [
            json!({"op":"set_dimension","feature":"f1","index":1,"value":"-1"}),
            json!({"op":"set_entity_values","feature":"f1","entity":1,"values":["0","0","-1","10"]}),
        ] {
            let (ok, response) = call(apply(&fixture.path, &revision, json!([edit])));
            assert!(!ok, "{response}");
            assert_eq!(response["error"]["code"], "inactive_feature");
            assert_eq!(original, std::fs::read(&fixture.path).unwrap());
        }
    }
    // Even rollback == old length would hide a newly appended feature.
    for rollback in [1, 2] {
        let mut document = document();
        document.rollback = Some(rollback);
        document.save(&fixture.path).unwrap();
        let revision = fixture.inspect()["revision"].clone();
        let original = std::fs::read(&fixture.path).unwrap();
        let (ok, response) = call(apply(
            &fixture.path,
            &revision,
            json!([
                {"op":"add_feature","plugin":"extrude","params":{"sketch":"f1","depth":"-1"}}
            ]),
        ));
        assert!(!ok, "{response}");
        assert_eq!(response["error"]["code"], "inactive_feature");
        assert_eq!(original, std::fs::read(&fixture.path).unwrap());
    }
}

#[test]
fn variable_edits_regenerate_the_active_history_without_claiming_inactive_validity() {
    let fixture = Fixture::new();
    let mut document = document();
    document.rollback = Some(1);
    document.save(&fixture.path).unwrap();
    let revision = fixture.inspect()["revision"].clone();
    let (ok, response) = call(apply(
        &fixture.path,
        &revision,
        json!([
            {"op":"set_variable","name":"depth","value":"-1","before":"10"}
        ]),
    ));
    assert!(ok, "{response}");
    assert_eq!(
        response["result"]["model"]["statuses"][1]["rolled_back"],
        true
    );
    assert!(
        response["result"]["model"]["parts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut reopened = Document::load(&fixture.path).unwrap();
    reopened.rollback = None;
    assert!(
        cad::engine::Runtime::new(Registry::builtin().unwrap())
            .evaluate(&reopened)
            .unwrap()
            .scene
            .error
            .is_some()
    );
}

#[test]
fn concurrent_disjoint_edits_merge_and_same_field_conflicts_are_reported() {
    let fixture = Fixture::new();
    let base = fixture.inspect()["revision"].clone();
    let first = apply(
        &fixture.path,
        &base,
        json!([{"op":"set_dimension","feature":"f1","index":1,"value":"30","before":"20","binding":{"kind":"Width","a":1}}]),
    );
    let second = apply(
        &fixture.path,
        &base,
        json!([{"op":"set_param","feature":"f2","key":"depth","value":"20","before":"depth"}]),
    );
    let a = std::thread::spawn(move || call(first));
    let b = std::thread::spawn(move || call(second));
    let (ok_a, a) = a.join().unwrap();
    let (ok_b, b) = b.join().unwrap();
    assert!(ok_a && ok_b, "{a}\n{b}");
    assert_ne!(a["result"]["rebased"], b["result"]["rebased"]);
    assert!(
        (fixture.inspect()["model"]["parts"][0]["volume_mm3"]
            .as_f64()
            .unwrap()
            - 6000.)
            .abs()
            < 1e-5
    );

    let fixture = Fixture::new();
    let base = fixture.inspect()["revision"].clone();
    let first = apply(
        &fixture.path,
        &base,
        json!([{"op":"set_param","feature":"f2","key":"depth","value":"15","before":"depth"}]),
    );
    let second = apply(
        &fixture.path,
        &base,
        json!([{"op":"set_param","feature":"f2","key":"depth","value":"20","before":"depth"}]),
    );
    let a = std::thread::spawn(move || call(first));
    let b = std::thread::spawn(move || call(second));
    let a = a.join().unwrap();
    let b = b.join().unwrap();
    assert_ne!(a.0, b.0);
    let failure = if a.0 { b.1 } else { a.1 };
    assert_eq!(failure["error"]["code"], "edit_conflict");
    assert_eq!(
        failure["error"]["details"]["conflicts"][0]["target"],
        "features/f2/params/depth"
    );
    assert_eq!(
        failure["error"]["details"]["conflicts"][0]["expected"],
        "depth"
    );
}

#[test]
fn repeated_stale_edit_is_idempotent_and_dropped_lock_is_reusable() {
    let fixture = Fixture::new();
    let first_lock = FileLock::try_acquire(&fixture.path).unwrap();
    assert!(FileLock::try_acquire(&fixture.path).is_err());
    drop(first_lock);
    drop(FileLock::try_acquire(&fixture.path).unwrap());
    let base = fixture.inspect()["revision"].clone();
    let request = apply(
        &fixture.path,
        &base,
        json!([{"op":"set_variable","name":"depth","value":"15","before":"10"}]),
    );
    let (ok, first) = call(request.clone());
    assert!(ok, "{first}");
    let (ok, repeated) = call(request);
    assert!(ok, "{repeated}");
    assert_eq!(first["result"]["revision"], repeated["result"]["revision"]);
    assert_eq!(repeated["result"]["rebased"], true);
}

#[test]
fn stale_dimension_binding_cannot_target_a_different_constraint() {
    let fixture = Fixture::new();
    let base = fixture.inspect()["revision"].clone();
    let mut changed = Document::load(&fixture.path).unwrap();
    let mut sketch = changed.features[0].sketch().unwrap();
    sketch.constraints[1].kind = "Height".into();
    sketch.constraints[2].kind = "Width".into();
    changed.features[0].set_sketch(&sketch);
    changed.save(&fixture.path).unwrap();
    let original = std::fs::read(&fixture.path).unwrap();
    let (ok, response) = call(apply(
        &fixture.path,
        &base,
        json!([
            {"op":"set_dimension","feature":"f1","index":1,"value":"30","before":"20","binding":{"kind":"Width","a":1}}
        ]),
    ));
    assert!(!ok);
    assert_eq!(response["error"]["code"], "edit_conflict");
    assert_eq!(original, std::fs::read(&fixture.path).unwrap());
}

#[test]
fn operating_system_releases_the_lock_after_a_process_crashes() {
    if let Some(path) = std::env::var_os("CAD_AGENT_LOCK_TEST_CHILD") {
        let path = PathBuf::from(path);
        let _lock = FileLock::try_acquire(&path).unwrap();
        std::fs::write(path.with_extension("ready"), b"locked").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(10));
        return;
    }
    let fixture = Fixture::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "operating_system_releases_the_lock_after_a_process_crashes",
        ])
        .env("CAD_AGENT_LOCK_TEST_CHILD", &fixture.path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    while !fixture.path.with_extension("ready").exists() && start.elapsed().as_secs() < 3 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let acquired = fixture.path.with_extension("ready").exists();
    let blocked = FileLock::try_acquire(&fixture.path).is_err();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        acquired && blocked,
        "Child must hold the lock before termination"
    );
    drop(FileLock::try_acquire(&fixture.path).unwrap());
}

#[test]
fn create_add_feature_and_export_are_validated_and_cannot_overwrite() {
    let fixture = Fixture::new();
    let output = fixture.directory.join("created.cadpart");
    let (ok, created) = call(request(
        &output,
        json!({"op":"create","document":Document::default()}),
    ));
    assert!(ok, "{created}");
    let (ok, added) = call(apply(
        &output,
        &created["result"]["revision"],
        json!([
            {"op":"add_feature","plugin":"sketch","name":"Profile","params":{"geometry":document().features[0].params["geometry"]}},
            {"op":"add_feature","plugin":"extrude","params":{"sketch":"f1","depth":"8"}}
        ]),
    ));
    assert!(ok, "{added}");
    let reopened = Document::load(&output).unwrap();
    assert_eq!(reopened.features[0].id, "f1");
    assert_eq!(reopened.features[1].id, "f2");
    assert_eq!(reopened.next_id, 3);
    let previous = std::fs::read(&output).unwrap();
    assert!(
        !call(request(
            &output,
            json!({"op":"create","document":Document::default()})
        ))
        .0
    );
    assert_eq!(previous, std::fs::read(&output).unwrap());
    let stl = fixture.directory.join("part.stl");
    let (ok, exported) = call(request(
        &output,
        json!({"op":"export","format":"stl","output":stl}),
    ));
    assert!(ok, "{exported}");
    let bytes = std::fs::read(&stl).unwrap();
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    assert_eq!(bytes.len(), 84 + count as usize * 50);
    assert!(count > 0);
    assert!(
        !call(request(
            &output,
            json!({"op":"export","format":"stl","output":stl})
        ))
        .0
    );
    #[cfg(feature = "step")]
    {
        let step = fixture.directory.join("part.step");
        let (ok, response) = call(request(
            &output,
            json!({"op":"export","format":"step","output":step}),
        ));
        assert!(ok, "{response}");
        assert!(
            std::fs::read_to_string(step)
                .unwrap()
                .contains("MANIFOLD_SOLID_BREP")
        );
    }
}

#[test]
fn custom_plugin_diagnostics_do_not_corrupt_the_json_protocol() {
    let fixture = Fixture::new();
    let plugins = fixture.directory.join("plugins");
    std::fs::create_dir(&plugins).unwrap();
    std::fs::write(plugins.join("probe.json"), serde_json::to_vec(&json!({
        "id":"probe","name":"Probe","version":"1","category":"feature","script":"probe.rhai","fields":[]
    })).unwrap()).unwrap();
    std::fs::write(plugins.join("probe.rhai"),
        "fn build(p) { print(\"plugin output\"); debug(\"plugin debug\"); #{op: \"box\", width: 20.0, depth: 10.0, height: 5.0} }").unwrap();
    let registry = Registry::load(&plugins);
    assert!(registry.errors.is_empty(), "{:?}", registry.errors);
    let mut document = Document::default();
    let feature = registry.create("probe", &mut document).unwrap();
    document.features.push(feature);
    document.save(&fixture.path).unwrap();
    let (ok, response) =
        call(json!({"version":1,"path":fixture.path,"plugins":plugins,"action":{"op":"inspect"}}));
    assert!(ok, "{response}");
    assert!(response["result"]["model"]["error"].is_null(), "{response}");
    assert_eq!(response["result"]["model"]["parts"][0]["volume_mm3"], 1000.);
}

#[test]
fn ordered_sketch_parameters_preserve_order_and_reject_invalid_or_future_references() {
    let fixture = Fixture::new();
    let plugins = fixture.directory.join("plugins");
    std::fs::create_dir(&plugins).unwrap();
    std::fs::write(plugins.join("ordered.json"), serde_json::to_vec(&json!({
        "id":"ordered","name":"Ordered profiles","version":"1","category":"feature","script":"ordered.rhai",
        "fields":[{"key":"profiles","label":"Profiles","kind":"sketches","default":[]}]
    })).unwrap()).unwrap();
    std::fs::write(
        plugins.join("ordered.rhai"),
        "fn build(p) { #{op: \"box\", width: 20.0, depth: 10.0, height: 5.0} }",
    )
    .unwrap();
    let registry = Registry::load(&plugins);
    let sketch = document().features[0].sketch().unwrap();
    let mut document = Document::default();
    for _ in 0..2 {
        let mut feature = registry.create("sketch", &mut document).unwrap();
        feature.set_sketch(&sketch);
        document.features.push(feature);
    }
    let mut feature = registry.create("ordered", &mut document).unwrap();
    feature.params["profiles"] = json!(["f1", "f2"]);
    document.features.push(feature);
    let mut feature = registry.create("sketch", &mut document).unwrap();
    feature.set_sketch(&sketch);
    document.features.push(feature);
    document.save(&fixture.path).unwrap();
    let mut inspect = request(&fixture.path, json!({"op":"inspect"}));
    inspect["plugins"] = json!(plugins);
    let (ok, inspected) = call(inspect);
    assert!(ok, "{inspected}");
    let base = inspected["result"]["revision"].clone();
    let original = std::fs::read(&fixture.path).unwrap();
    for value in [
        json!("f1"),
        json!(["f1"]),
        json!(["f1", 2]),
        json!(["f1", "f1"]),
        json!(["f1", "f3"]),
        json!(["f1", "f4"]),
        json!(["f1", "missing"]),
        json!((0..33).map(|i| format!("s{i}")).collect::<Vec<_>>()),
    ] {
        let mut change = apply(
            &fixture.path,
            &base,
            json!([
                {"op":"set_param","feature":"f3","key":"profiles","value":value}
            ]),
        );
        change["plugins"] = json!(plugins);
        let (ok, response) = call(change);
        assert!(!ok, "{response}");
        assert_eq!(original, std::fs::read(&fixture.path).unwrap());
    }
    let mut change = apply(
        &fixture.path,
        &base,
        json!([
            {"op":"set_param","feature":"f3","key":"profiles","value":["f2","f1"],"before":["f1","f2"]}
        ]),
    );
    change["plugins"] = json!(plugins);
    let (ok, response) = call(change);
    assert!(ok, "{response}");
    assert_eq!(
        response["result"]["document"]["features"][2]["params"]["profiles"],
        json!(["f2", "f1"])
    );
    assert_eq!(
        Document::load(&fixture.path).unwrap().features[2].params["profiles"],
        json!(["f2", "f1"])
    );
}

#[test]
fn installed_conditional_plugin_round_trips_multistep_edits_and_rejects_incomplete_mode_changes() {
    let fixture = Fixture::new();
    let plugins = fixture.directory.join("plugins");
    std::fs::create_dir(&plugins).unwrap();
    std::fs::write(plugins.join("conditional.json"), include_str!("fixtures/conditional-plugin.json")).unwrap();
    std::fs::write(plugins.join("conditional.rhai"), include_str!("fixtures/conditional-plugin.rhai")).unwrap();
    let inspect = || {
        let mut request = request(&fixture.path, json!({"op":"inspect"}));
        request["plugins"] = json!(plugins);
        let (ok, response) = call(request);
        assert!(ok, "{response}");
        assert!(response["result"]["validation_error"].is_null(), "{response}");
        response["result"].clone()
    };
    let change = |revision: &Value, edits: Value| {
        let mut request = apply(&fixture.path, revision, edits);
        request["plugins"] = json!(plugins);
        call(request)
    };
    let first = inspect();
    let schema = first["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "conditional-operation").unwrap();
    let fields = schema["fields"].as_array().unwrap();
    assert_eq!(fields.iter().find(|f| f["key"] == "profile").unwrap()["when"],
        json!([{"op":"equals","key":"mode","value":"Cut"}]));
    assert_eq!(fields.iter().find(|f| f["key"] == "guides").unwrap()["required"], false);
    let pins = first["document"]["features"].clone();
    let (ok, added) = change(&first["revision"], json!([
        {"op":"add_feature","plugin":"conditional-operation","params":{"optional_axis":null,"optional_plane":null,"guides":["f1"]}}
    ]));
    assert!(ok, "{added}");
    let reopened = inspect();
    assert_eq!(reopened["document"]["features"][2]["params"]["guides"], json!(["f1"]));
    assert_eq!(&reopened["document"]["features"].as_array().unwrap()[..2], pins.as_array().unwrap());
    assert_eq!(reopened["model"]["parts"].as_array().unwrap().len(), 2);
    assert!((reopened["model"]["parts"][1]["volume_mm3"].as_f64().unwrap() - 60.).abs() < 1e-6,
        "Hidden numeric width must still be evaluated: {reopened}");
    let before = std::fs::read(&fixture.path).unwrap();
    let (ok, rejected) = change(&reopened["revision"], json!([
        {"op":"set_param","feature":"f3","key":"mode","value":"Cut"}
    ]));
    assert!(!ok, "{rejected}");
    assert!(rejected["error"]["message"].as_str().unwrap().contains("cut profile"), "{rejected}");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
    let (ok, cut) = change(&reopened["revision"], json!([
        {"op":"set_param","feature":"f3","key":"mode","value":"Cut"},
        {"op":"set_param","feature":"f3","key":"profile","value":"f1"},
        {"op":"set_param","feature":"f3","key":"target","value":"f2:0"},
        {"op":"set_param","feature":"f3","key":"cut_depth","value":"2"},
        {"op":"add_feature","plugin":"conditional-operation","params":{"height":"2"}}
    ]));
    assert!(ok, "{cut}");
    let cut = inspect();
    let (ok, resized) = change(&cut["revision"], json!([
        {"op":"set_dimension","feature":"f1","index":1,"value":"30"}
    ]));
    assert!(ok, "{resized}");
    let resized = inspect();
    let parts = resized["model"]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert!((parts[0]["volume_mm3"].as_f64().unwrap() - 2400.).abs() < 1e-6);
    assert!((parts[1]["volume_mm3"].as_f64().unwrap() - 40.).abs() < 1e-6);
    let (ok, dormant) = change(&resized["revision"], json!([
        {"op":"set_param","feature":"f3","key":"mode","value":"New"},
        {"op":"set_param","feature":"f3","key":"profile","value":"gone"},
        {"op":"set_param","feature":"f3","key":"cut_depth","value":"not_defined"}
    ]));
    assert!(ok, "{dormant}");
    let dormant = inspect();
    assert_eq!(dormant["model"]["parts"].as_array().unwrap().len(), 3);
    let before = std::fs::read(&fixture.path).unwrap();
    let (ok, rejected) = change(&dormant["revision"], json!([
        {"op":"set_param","feature":"f3","key":"mode","value":"Cut"}
    ]));
    assert!(!ok, "{rejected}");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before, "Invalid dormant references cannot become live silently");
}
