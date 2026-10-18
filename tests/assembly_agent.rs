use cad::{
    assembly::{AssemblyDocument, Instance, Source, Transform},
    model::{Document, Sketch},
    plugins::Registry,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cad-assembly-agent-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn invoke(request: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-assembly-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&request).unwrap().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&result.stdout).unwrap_or_else(|_| {
        panic!(
            "Invalid stdout: {}",
            String::from_utf8_lossy(&result.stdout)
        )
    });
    assert_eq!(result.status.success(), response["ok"].as_bool().unwrap());
    response
}
fn model() -> AssemblyDocument {
    let registry = Registry::builtin().unwrap();
    let mut part = Document::default();
    let mut sketch = Sketch::default();
    sketch.add_rect([0., 0.], [8., 6.]);
    let mut f = registry.create("sketch", &mut part).unwrap();
    f.set_sketch(&sketch);
    part.features.push(f);
    let mut f = registry.create("extrude", &mut part).unwrap();
    f.params["sketch"] = json!("f1");
    part.features.push(f);
    let mut document = AssemblyDocument::default();
    document
        .sources
        .push(Source::new("source".into(), "Block".into(), part, None).unwrap());
    for n in 0..2 {
        let mut i = Instance::new(format!("instance{n}"), "source".into(), "f2:0".into());
        i.fixed = true;
        i.transform = Transform::translated([n as f64 * 20., 0., 0.]);
        document.instances.push(i);
    }
    document
}
#[test]
fn agent_creates_inspects_and_guards_concurrent_source_edits() {
    let directory = Directory::new();
    let path = directory.0.join("assembly.cadasm");
    let document = model();
    let created =
        invoke(json!({"version":1,"path":path,"action":{"op":"create","document":document}}));
    assert_eq!(created["ok"], true);
    let revision = created["result"]["revision"].clone();
    let mut first = document.clone();
    first.name = "First agent".into();
    let result = invoke(
        json!({"version":1,"path":path,"action":{"op":"apply","expected_revision":revision,"document":first}}),
    );
    assert_eq!(result["ok"], true);
    let bytes = std::fs::read(&path).unwrap();
    let stale = invoke(
        json!({"version":1,"path":path,"action":{"op":"apply","expected_revision":revision,"document":document}}),
    );
    assert_eq!(stale["ok"], false);
    assert!(stale["error"].as_str().unwrap().contains("stale_revision"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let inspect = invoke(json!({"version":1,"path":path,"action":{"op":"inspect"}}));
    assert_eq!(
        inspect["result"]["model"]["parts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn context_agent_binds_updates_and_preserves_failed_edits() {
    let directory = Directory::new();
    let path = directory.0.join("context.cadasm");
    let document = model();
    let result =
        invoke(json!({"version":1,"path":path,"action":{"op":"create","document":document}}));
    let result = invoke(
        json!({"version":1,"path":path,"action":{"op":"capture_context","expected_revision":result["result"]["revision"],"primary":"instance0"}}),
    );
    assert_eq!(result["ok"], true);
    let context = result["result"]["document"]["contexts"][0]["id"].clone();
    let before = std::fs::read(&path).unwrap();
    let invalid = invoke(
        json!({"version":1,"path":path,"action":{"op":"bind_context","expected_revision":result["result"]["revision"],"context":context,"binding":{"feature":"f1","instance":"instance1","face":"missing","kind":"SketchPlane","offset":0}}}),
    );
    assert_eq!(invalid["ok"], false);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let valid = invoke(
        json!({"version":1,"path":path,"action":{"op":"reference_plane","context":context,"instance":"instance1","face":"f2:end"}}),
    );
    assert_eq!(valid["ok"], true);
    assert_eq!(valid["result"]["plane"]["origin"][2], 10.);
}
#[cfg(feature = "step")]
#[test]
fn agent_exports_all_exact_instances_to_step() {
    let directory = Directory::new();
    let path = directory.0.join("export.cadasm");
    let result =
        invoke(json!({"version":1,"path":path,"action":{"op":"create","document":model()}}));
    let output = directory.0.join("assembly.step");
    let exported = invoke(
        json!({"version":1,"path":path,"action":{"op":"export","expected_revision":result["result"]["revision"],"format":"step","output":output}}),
    );
    assert_eq!(exported["ok"], true);
    let text = std::fs::read_to_string(output).unwrap();
    assert!(text.starts_with("ISO-10303-21;"));
    assert_eq!(text.matches("MANIFOLD_SOLID_BREP(").count(), 2);
}
