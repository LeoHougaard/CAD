//! One offline, revision-guarded assembly request per process.
use cad::{
    assembly::{AssemblyDocument, AssemblyEvaluation, AssemblyRuntime, ContextBinding},
    file_lock::FileLock,
    model::Result,
    plugins::Registry,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    path: PathBuf,
    plugins: Option<PathBuf>,
    action: Action,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Create {
        document: AssemblyDocument,
    },
    Inspect,
    Apply {
        expected_revision: String,
        document: AssemblyDocument,
    },
    ReloadSource {
        expected_revision: String,
        source: String,
    },
    CaptureContext {
        expected_revision: String,
        primary: String,
    },
    BindContext {
        expected_revision: String,
        context: String,
        binding: ContextBinding,
    },
    UpdateContext {
        expected_revision: String,
        context: String,
    },
    ReferencePlane {
        context: String,
        instance: String,
        face: String,
    },
    Export {
        expected_revision: String,
        format: String,
        output: PathBuf,
        instance: Option<String>,
    },
}
fn path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let file = path.file_name().ok_or("Choose a file name")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent.canonicalize().map_err(|e| e.to_string())?.join(file))
}
fn summary(e: &AssemblyEvaluation) -> Value {
    json!({"poses":e.poses,"degrees_of_freedom":e.degrees_of_freedom,"evaluated_sources":e.evaluated_sources,"parts":e.scene.meshes.iter().map(|m|json!({"id":m.part,"name":m.name,"volume":m.volume,"bounds":m.bounds,"faces":m.faces.iter().map(|f|json!({"key":f.key,"source_key":f.key.strip_prefix(&format!("{}::",m.part)).unwrap_or(&f.key),"plane":f.plane})).collect::<Vec<_>>(),"edges":m.edges.iter().map(|edge|json!({"key":edge.key,"source_key":edge.key.strip_prefix(&format!("{}::",m.part)).unwrap_or(&edge.key),"seam":edge.seam})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn revision(expected: &str, document: &AssemblyDocument) -> Result<()> {
    if expected != document.revision()? {
        return Err(
            "stale_revision: assembly changed; inspect the current revision before applying".into(),
        );
    }
    Ok(())
}
fn save(
    document: &AssemblyDocument,
    path: &Path,
    expected: &str,
    runtime: &mut AssemblyRuntime,
) -> Result<Value> {
    let e = runtime.evaluate(document)?;
    document.save_checked(path, Some(expected))?;
    Ok(json!({"path":path,"revision":document.revision()?,"document":document,"model":summary(&e)}))
}
fn run(request: Request) -> Result<Value> {
    if request.version != 1 {
        return Err("Unsupported request version".into());
    }
    let path = path(&request.path)?;
    let registry = Arc::new(
        request
            .plugins
            .as_deref()
            .map(Registry::load)
            .unwrap_or(Registry::builtin()?),
    );
    let mut runtime = AssemblyRuntime::new(registry);
    if let Action::Create { document } = request.action {
        let evaluated = runtime.evaluate(&document)?;
        document.save_checked(&path, None)?;
        return Ok(
            json!({"path":path,"revision":document.revision()?,"document":document,"model":summary(&evaluated)}),
        );
    }
    let mut document = AssemblyDocument::load(&path)?;
    match request.action {
        Action::Create { .. } => unreachable!(),
        Action::Inspect => {
            let evaluated = runtime.evaluate(&document)?;
            let links:Vec<_>=document.sources.iter().filter_map(|s|s.path.as_ref().map(|p|{let linked=if p.is_absolute(){p.clone()}else{path.parent().unwrap().join(p)};json!({"source":s.id,"path":p,"resolved_path":linked,"changed":cad::model::Document::load(&linked).and_then(|d|cad::assembly::document_revision(&d)).map(|r|r!=s.revision).ok()})})).collect();
            Ok(
                json!({"path":path,"revision":document.revision()?,"document":document,"links":links,"model":summary(&evaluated)}),
            )
        }
        Action::Apply {
            expected_revision,
            document: candidate,
        } => {
            revision(&expected_revision, &document)?;
            save(&candidate, &path, &expected_revision, &mut runtime)
        }
        Action::ReloadSource {
            expected_revision,
            source,
        } => {
            revision(&expected_revision, &document)?;
            runtime.reload_source_at(&mut document, &source, &path)?;
            save(&document, &path, &expected_revision, &mut runtime)
        }
        Action::CaptureContext {
            expected_revision,
            primary,
        } => {
            revision(&expected_revision, &document)?;
            let context = runtime.capture_context(&document, &primary)?;
            document.contexts.push(context);
            save(&document, &path, &expected_revision, &mut runtime)
        }
        Action::BindContext {
            expected_revision,
            context,
            binding,
        } => {
            revision(&expected_revision, &document)?;
            let index = document
                .contexts
                .iter()
                .position(|c| c.id == context)
                .ok_or("Missing context")?;
            let mut context = document.contexts[index].clone();
            let source = document
                .sources
                .iter_mut()
                .find(|s| s.id == context.primary_source)
                .ok_or("Missing primary source")?;
            runtime.bind_context(&mut context, &mut source.document, binding)?;
            source.refresh_revision()?;
            document.contexts[index] = context;
            save(&document, &path, &expected_revision, &mut runtime)
        }
        Action::UpdateContext {
            expected_revision,
            context,
        } => {
            revision(&expected_revision, &document)?;
            runtime.update_context(&mut document, &context)?;
            save(&document, &path, &expected_revision, &mut runtime)
        }
        Action::ReferencePlane {
            context,
            instance,
            face,
        } => {
            let context = document
                .contexts
                .iter()
                .find(|c| c.id == context)
                .ok_or("Missing context")?;
            Ok(
                json!({"plane":runtime.reference_plane(context,&instance,&face)?,"context":context.id,"stale":context.is_stale(&document)}),
            )
        }
        Action::Export {
            expected_revision,
            format,
            output,
            instance,
        } => {
            revision(&expected_revision, &document)?;
            if !matches!(format.as_str(), "step" | "stl") {
                return Err("Export format must be step or stl".into());
            }
            let output = crate::path(&output)?;
            let _lock = FileLock::try_acquire(&output)?;
            if output.exists() {
                return Err("Export requires a new output file".into());
            }
            let evaluated = runtime.evaluate(&document)?;
            if let Some(instance) = instance {
                evaluated
                    .instance_geometry(&document, &instance)?
                    .export(&format, &output)?;
            } else {
                evaluated.export(&document, &format, &output)?;
            }
            Ok(json!({"path":output,"source_revision":expected_revision,"format":format}))
        }
    }
}
fn main() {
    let result = std::panic::catch_unwind(|| -> Result<Value> {
        let mut bytes = vec![];
        std::io::stdin()
            .lock()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Request exceeds 32 MiB".into());
        }
        let request =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid request: {e}"))?;
        run(request)
    })
    .unwrap_or_else(|_| Err("Internal assembly error; no unvalidated edits were saved".into()));
    let response = match result {
        Ok(result) => json!({"version":1,"ok":true,"result":result}),
        Err(error) => json!({"version":1,"ok":false,"error":error}),
    };
    let mut stdout = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, &response);
    let _ = stdout.write_all(b"\n");
    if response["ok"] == false {
        std::process::exit(1);
    }
}
