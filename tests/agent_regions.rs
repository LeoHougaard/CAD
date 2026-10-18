//! Region workflows through the public offline agent JSON protocol.
use cad::{
    model::{Document, Sketch},
    plugins::{Numbers, Registry},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const LEGACY_PLATE: &str = include_str!("../examples/dimensioned-plate.cadpart");

struct Fixture {
    directory: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new(document: &Document) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "cad-agent-regions-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("part.cadpart");
        document.save(&path).unwrap();
        Self { directory, path }
    }
    fn call(&self, action: Value) -> (bool, Value) {
        let request = json!({"version":1,"path":self.path,"action":action});
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
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
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
    fn inspect(&self) -> Value {
        let (ok, response) = self.call(json!({"op":"inspect"}));
        assert!(ok, "{response}");
        let result = response["result"].clone();
        assert!(result["validation_error"].is_null(), "{result}");
        assert!(result["model"]["error"].is_null(), "{result}");
        result
    }
    fn apply(&self, revision: &Value, edits: Value) -> (bool, Value) {
        self.call(
            json!({"op":"apply","expected_revision":revision,"output":self.path,"edits":edits}),
        )
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
                .starts_with("cad-agent-regions-")
        {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

fn disjoint_document() -> Document {
    let registry = Registry::builtin().unwrap();
    let mut document = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [10., 8.]);
    sketch.add_circle([25., 0.], 4.);
    let mut f = registry.create("sketch", &mut document).unwrap();
    f.set_sketch(&sketch);
    document.features.push(f);
    let mut f = registry.create("extrude", &mut document).unwrap();
    f.params["sketch"] = json!("f1");
    f.params["depth"] = json!("3");
    document.features.push(f);
    document
}
fn model_regions(result: &Value) -> &Vec<Value> {
    result["model"]["sketches"][0]["regions"]
        .as_array()
        .unwrap()
}
fn circle_region_key(result: &Value) -> Value {
    model_regions(result)
        .iter()
        .find(|region| region["outer"][0][0].as_f64().unwrap() > 15.)
        .unwrap()["key"]
        .clone()
}
fn assert_volume(result: &Value, expected: f64, parts: usize) {
    let values = result["model"]["parts"].as_array().unwrap();
    assert_eq!(values.len(), parts);
    let actual: f64 = values
        .iter()
        .map(|p| p["volume_mm3"].as_f64().unwrap())
        .sum();
    assert!(
        (actual - expected).abs() < expected * 1e-7,
        "{actual} vs {expected}"
    );
}

#[test]
fn region_inspection_selection_and_parameter_edits_roundtrip_through_cli() {
    let fixture = Fixture::new(&disjoint_document());
    let before = fixture.inspect();
    assert_eq!(model_regions(&before).len(), 2);
    for region in model_regions(&before) {
        assert!(!region["key"].as_str().unwrap().is_empty());
        assert!(region["outer"].as_array().unwrap().len() >= 4);
        assert_eq!(region["holes"], json!([]));
        assert_eq!(region["whole_sketch"], true);
    }
    assert_volume(&before, (80. + 16. * std::f64::consts::PI) * 3., 2);
    let key = circle_region_key(&before);
    let (ok, response) = fixture.apply(
        &before["revision"],
        json!([
            {"op":"set_param","feature":"f2","key":"regions","value":[key],"before":null}
        ]),
    );
    assert!(ok, "{response}");
    let selected = fixture.inspect();
    assert_eq!(
        selected["document"]["features"][1]["params"]["regions"],
        json!([key])
    );
    assert_volume(&selected, 16. * std::f64::consts::PI * 3., 1);
    let (ok,response)=fixture.apply(&selected["revision"],json!([
        {"op":"set_entity_values","feature":"f1","entity":2,"values":["25","0","5"],"before":["25","0","4"]}
    ]));
    assert!(ok, "{response}");
    let resized = fixture.inspect();
    assert_eq!(circle_region_key(&resized), key);
    assert_volume(&resized, 25. * std::f64::consts::PI * 3., 1);
    let (ok, response) = fixture.apply(
        &resized["revision"],
        json!([
            {"op":"set_param","feature":"f2","key":"regions","value":null,"before":[key]}
        ]),
    );
    assert!(ok, "{response}");
    assert_volume(
        &fixture.inspect(),
        (80. + 25. * std::f64::consts::PI) * 3.,
        2,
    );
}

#[test]
fn empty_unknown_duplicate_and_stale_regions_never_overwrite_the_document() {
    let fixture = Fixture::new(&disjoint_document());
    let before = fixture.inspect();
    let key = circle_region_key(&before);
    for value in [
        json!([]),
        json!(["missing-region"]),
        json!([key, key]),
        json!([1]),
        json!(true),
    ] {
        let bytes = std::fs::read(&fixture.path).unwrap();
        let (ok, response) = fixture.apply(
            &before["revision"],
            json!([
                {"op":"set_param","feature":"f2","key":"regions","value":value}
            ]),
        );
        assert!(!ok, "{response}");
        assert_eq!(bytes, std::fs::read(&fixture.path).unwrap());
    }
    let (ok, response) = fixture.apply(
        &before["revision"],
        json!([
            {"op":"set_param","feature":"f2","key":"regions","value":[key]}
        ]),
    );
    assert!(ok, "{response}");
    let selected = fixture.inspect();
    let bytes = std::fs::read(&fixture.path).unwrap();
    // Moving the circle into the rectangle creates two intersections. Its old
    // whole-circle region no longer exists, so the transaction must fail closed.
    let (ok,response)=fixture.apply(&selected["revision"],json!([
        {"op":"set_entity_values","feature":"f1","entity":2,"values":["9","4","3"],"before":["25","0","4"]}
    ]));
    assert!(!ok, "{response}");
    assert!(
        response.to_string().contains("no longer exists"),
        "{response}"
    );
    assert_eq!(bytes, std::fs::read(&fixture.path).unwrap());
    assert_volume(&fixture.inspect(), 16. * std::f64::consts::PI * 3., 1);
}

#[test]
fn legacy_pinned_extrude_can_inspect_and_select_its_enclosed_disc() {
    let document: Document = serde_json::from_str(LEGACY_PLATE).unwrap();
    assert_eq!(document.features[1].plugin_version, "1.0.0");
    assert!(document.features[1].params.get("regions").is_none());
    let fixture = Fixture::new(&document);
    let before = fixture.inspect();
    assert_volume(&before, (1800. - 25. * std::f64::consts::PI) * 10., 1);
    let regions = model_regions(&before);
    assert_eq!(regions.len(), 2);
    let outer = regions.iter().find(|r| r["whole_sketch"] == true).unwrap();
    assert_eq!(outer["holes"].as_array().unwrap().len(), 1);
    let disc = regions.iter().find(|r| r["whole_sketch"] == false).unwrap();
    assert_eq!(disc["holes"], json!([]));
    let (ok, response) = fixture.apply(
        &before["revision"],
        json!([
            {"op":"set_param","feature":"f2","key":"regions","value":[disc["key"]]}
        ]),
    );
    assert!(ok, "{response}");
    let after = fixture.inspect();
    assert_volume(&after, 25. * std::f64::consts::PI * 10., 1);
    assert_eq!(after["document"]["features"][1]["plugin_version"], "1.0.0");
    assert_eq!(
        after["document"]["features"][1]["plugin_hash"],
        document.features[1].plugin_hash
    );
    // Test data and its pinned package identity remain untouched.
    assert_eq!(
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/dimensioned-plate.cadpart"
        ))
        .unwrap(),
        LEGACY_PLATE
    );
}

#[test]
fn builtin_legacy_compatibility_does_not_accept_a_changed_external_package() {
    let legacy: Document = serde_json::from_str(LEGACY_PLATE).unwrap();
    let feature = &legacy.features[1];
    let mut registry = Registry::builtin().unwrap();
    assert!(registry.plugins["extrude"].matches(feature));
    let numbers = Numbers::new(&BTreeMap::new()).unwrap();
    assert!(registry.evaluate(feature, &numbers).is_ok());
    // Same manifest identity and script behavior, different exact package bytes.
    let changed = format!("{}\n", include_str!("../plugins/extrude.json"));
    registry
        .add(&changed, include_str!("../plugins/extrude.rhai"))
        .unwrap();
    assert!(!registry.plugins["extrude"].matches(feature));
    assert!(
        registry
            .evaluate(feature, &numbers)
            .unwrap_err()
            .contains("Plugin code changed")
    );
}
