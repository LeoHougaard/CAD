//! One bounded JSON request on stdin, one versioned JSON response on stdout.
use cad::{
    dimensions,
    engine::{Evaluation, Runtime},
    file_lock::FileLock,
    model::{Document, Feature, Plane},
    plugins::{Field, Numbers, Registry},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 32 * 1024 * 1024;
type Result<T> = std::result::Result<T, Failure>;

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
    Inspect,
    Create {
        document: Document,
    },
    Apply {
        expected_revision: Option<String>,
        output: Option<PathBuf>,
        edits: Vec<Edit>,
    },
    Export {
        format: String,
        output: PathBuf,
        expected_revision: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    SetParam {
        feature: String,
        key: String,
        value: Value,
        #[serde(default)]
        before: Before,
    },
    SetVariable {
        name: String,
        value: String,
        #[serde(default)]
        before: Before,
    },
    SetDimension {
        feature: String,
        index: usize,
        value: String,
        binding: Option<cad::model::SketchConstraint>,
        #[serde(default)]
        before: Before,
    },
    SetEntityValues {
        feature: String,
        entity: u64,
        values: Vec<String>,
        kind: Option<String>,
        #[serde(default)]
        before: Before,
    },
    AddFeature {
        plugin: String,
        name: Option<String>,
        params: Value,
        #[serde(default)]
        before: Before,
    },
}

#[derive(Default)]
enum Before {
    #[default]
    Missing,
    Present(Value),
}
impl<'de> Deserialize<'de> for Before {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(Self::Present(Value::deserialize(deserializer)?))
    }
}

struct Failure {
    code: &'static str,
    message: String,
    details: Value,
}
fn failure(code: &'static str, message: impl ToString) -> Failure {
    Failure {
        code,
        message: message.to_string(),
        details: Value::Null,
    }
}
fn input(message: impl ToString) -> Failure {
    failure("invalid_request", message)
}
fn io(error: impl ToString) -> Failure {
    failure("io_error", error)
}

fn read_bounded(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(LIMIT + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > LIMIT {
        return Err(input("Input exceeds 32 MiB"));
    }
    Ok(bytes)
}
fn revision(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{hash:016x}")
}
fn check_revision(expected: Option<&str>, bytes: &[u8]) -> Result<()> {
    if let Some(expected) = expected
        && expected != revision(bytes)
    {
        return Err(Failure {
            code: "stale_revision",
            message: "The document changed. Inspect it again before applying edits.".into(),
            details: json!({"expected_revision":expected,"actual_revision":revision(bytes)}),
        });
    }
    Ok(())
}
fn output_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path.canonicalize().map_err(io);
    }
    let name = path
        .file_name()
        .ok_or_else(|| input("Output needs a file name"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent.canonicalize().map_err(io)?.join(name))
}
fn lock(path: &Path) -> Result<FileLock> {
    let start = std::time::Instant::now();
    loop {
        match FileLock::try_acquire(path) {
            Ok(lock) => return Ok(lock),
            Err(error) if error == "Document is busy in another process" => {
                if start.elapsed() >= std::time::Duration::from_secs(2) {
                    return Err(failure("document_busy", error));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(io(error)),
        }
    }
}

fn field_value(field: &Field, value: &Value) -> cad::model::Result<()> {
    let valid = match field.kind.as_str() {
        "number" => value.is_string() || value.is_number(),
        "bool" => value.is_boolean(),
        "choice" => value
            .as_str()
            .is_some_and(|s| field.options.iter().any(|v| v == s)),
        "edges" | "faces" => value
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string)),
        "sketches" => value.as_array().is_some_and(|a| {
            (2..=32).contains(&a.len())
                && a.iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128))
                && a.iter()
                    .filter_map(Value::as_str)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == a.len()
        }),
        _ => value.is_string(),
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Invalid value type or choice for parameter '{}'",
            field.key
        ))
    }
}
fn feature_mut<'a>(document: &'a mut Document, id: &str) -> Result<&'a mut Feature> {
    document
        .features
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| input(format!("Feature '{id}' is missing")))
}
fn edit_state<'a>(
    document: &Document,
    edit: &'a Edit,
) -> Result<(String, Value, Value, &'a Before)> {
    let get = |id: &str| {
        document
            .features
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| input(format!("Feature '{id}' is missing")))
    };
    Ok(match edit {
        Edit::SetVariable {
            name,
            value,
            before,
        } => (
            format!("variables/{name}"),
            json!(document.variables.get(name)),
            json!(value),
            before,
        ),
        Edit::SetParam {
            feature,
            key,
            value,
            before,
        } => (
            format!("features/{feature}/params/{key}"),
            get(feature)?
                .params
                .get(key)
                .cloned()
                .unwrap_or(Value::Null),
            value.clone(),
            before,
        ),
        Edit::SetDimension {
            feature,
            index,
            value,
            before,
            ..
        } => {
            let sketch = get(feature)?
                .sketch()
                .ok_or_else(|| input("Feature has no sketch"))?;
            let constraint = sketch
                .constraints
                .get(*index)
                .ok_or_else(|| input("Constraint index is missing"))?;
            (
                format!("features/{feature}/geometry/constraints/{index}/value"),
                json!(constraint.value),
                json!(value),
                before,
            )
        }
        Edit::SetEntityValues {
            feature,
            entity,
            values,
            before,
            ..
        } => {
            let sketch = get(feature)?
                .sketch()
                .ok_or_else(|| input("Feature has no sketch"))?;
            let entity = sketch
                .entities
                .iter()
                .find(|e| e.id == *entity)
                .ok_or_else(|| input("Sketch entity is missing"))?;
            (
                format!("features/{feature}/geometry/entities/{}/values", entity.id),
                json!(entity.values),
                json!(values),
                before,
            )
        }
        Edit::AddFeature {
            plugin,
            name,
            params,
            before,
        } => (
            "features/append".into(),
            json!(document.features.len()),
            json!({"plugin":plugin,"name":name,"params":params}),
            before,
        ),
    })
}
fn apply(
    document: &mut Document,
    registry: &Registry,
    edits: Vec<Edit>,
    stale: bool,
) -> Result<()> {
    if edits.len() > 4000 {
        return Err(input("At most 4000 edits per request"));
    }
    if stale && edits.is_empty() {
        return Err(failure(
            "stale_revision",
            "An empty edit cannot rebase a stale revision",
        ));
    }
    let mut conflicts = Vec::new();
    for edit in edits {
        let feature_id = match &edit {
            Edit::SetParam { feature, .. }
            | Edit::SetDimension { feature, .. }
            | Edit::SetEntityValues { feature, .. } => Some(feature),
            _ => None,
        };
        if let Some(id) = feature_id {
            let (index, feature) = document
                .features
                .iter()
                .enumerate()
                .find(|(_, f)| &f.id == id)
                .ok_or_else(|| input(format!("Feature '{id}' is missing")))?;
            if feature.suppressed || document.rollback.is_some_and(|end| index >= end) {
                return Err(Failure {
                    code: "inactive_feature",
                    message: "Unsuppress the feature and move history to End before editing it."
                        .into(),
                    details: json!({"feature":id,"suppressed":feature.suppressed,"rollback":document.rollback}),
                });
            }
        }
        if matches!(edit, Edit::AddFeature { .. }) && document.rollback.is_some() {
            return Err(failure(
                "inactive_feature",
                "Move history to End before adding a feature.",
            ));
        }
        let (target, current, proposed, before) = edit_state(document, &edit)?;
        let identity_matches = match &edit {
            Edit::SetDimension {
                feature,
                index,
                binding,
                ..
            } => {
                let current = document
                    .features
                    .iter()
                    .find(|f| &f.id == feature)
                    .and_then(Feature::sketch)
                    .and_then(|s| s.constraints.get(*index).cloned());
                match (binding, current) {
                    (Some(expected), Some(actual)) => {
                        expected.kind == actual.kind
                            && expected.a == actual.a
                            && expected.b == actual.b
                            && expected.points == actual.points
                            && expected.reference == actual.reference
                            && expected.negative == actual.negative
                    }
                    (None, _) => !stale,
                    _ => false,
                }
            }
            Edit::SetEntityValues {
                feature,
                entity,
                kind,
                ..
            } => {
                let actual = document
                    .features
                    .iter()
                    .find(|f| &f.id == feature)
                    .and_then(Feature::sketch)
                    .and_then(|s| s.entities.into_iter().find(|e| e.id == *entity))
                    .map(|e| e.kind);
                match kind {
                    Some(kind) => actual.as_ref() == Some(kind),
                    None => !stale,
                }
            }
            _ => true,
        };
        if !identity_matches {
            conflicts.push(json!({"target":target,"current":current,"reason":"Sketch binding changed or is missing. Stale dimension edits require binding; stale entity edits require kind.","proposed":proposed}));
            continue;
        }
        if let Before::Present(before) = before {
            if current != *before && current != proposed {
                conflicts.push(json!({"target":target,"current":current,"expected":before,"proposed":proposed}));
                continue;
            }
        } else if stale {
            conflicts.push(json!({"target":target,"current":current,"reason":"A stale revision requires before on every edit","proposed":proposed}));
            continue;
        }
        match edit {
            Edit::SetVariable { name, value, .. } => {
                document.variables.insert(name, value);
            }
            Edit::SetParam {
                feature,
                key,
                value,
                ..
            } => {
                let f = feature_mut(document, &feature)?;
                let plugin = registry
                    .plugins
                    .get(&f.plugin)
                    .ok_or_else(|| input("Feature plugin is missing"))?;
                let field = plugin
                    .manifest
                    .fields
                    .iter()
                    .find(|field| field.key == key)
                    .ok_or_else(|| {
                        input(format!(
                            "Parameter '{key}' is not declared by the feature plugin"
                        ))
                    })?;
                field_value(field, &value).map_err(input)?;
                f.params[&key] = value;
            }
            Edit::SetDimension {
                feature,
                index,
                value,
                ..
            } => {
                let f = feature_mut(document, &feature)?;
                let mut sketch = f.sketch().ok_or_else(|| input("Feature has no sketch"))?;
                let constraint = sketch
                    .constraints
                    .get_mut(index)
                    .ok_or_else(|| input("Constraint index is missing"))?;
                if constraint.reference || !dimensions::is_dimension(&constraint.kind) {
                    return Err(input(
                        "Only driving dimension values can be edited with set_dimension",
                    ));
                }
                constraint.value = value;
                f.set_sketch(&sketch);
            }
            Edit::SetEntityValues {
                feature,
                entity,
                values,
                ..
            } => {
                let f = feature_mut(document, &feature)?;
                let mut sketch = f.sketch().ok_or_else(|| input("Feature has no sketch"))?;
                let entity = sketch
                    .entities
                    .iter_mut()
                    .find(|e| e.id == entity)
                    .ok_or_else(|| input("Sketch entity is missing"))?;
                if values.len() != entity.values.len() {
                    return Err(input("Entity value count cannot change"));
                }
                entity.values = values;
                f.set_sketch(&sketch);
            }
            Edit::AddFeature {
                plugin,
                name,
                params,
                ..
            } => {
                let params = params
                    .as_object()
                    .ok_or_else(|| input("Feature params must be an object"))?;
                let mut feature = registry.create(&plugin, document).map_err(input)?;
                if registry.plugins[&plugin].manifest.category != "feature" {
                    return Err(input("Only feature plugins can be added to a document"));
                }
                if let Some(name) = name {
                    feature.name = name;
                }
                for (key, value) in params {
                    let field = registry.plugins[&plugin]
                        .manifest
                        .fields
                        .iter()
                        .find(|f| &f.key == key);
                    if key == "geometry" && registry.plugins[&plugin].manifest.editor == "sketch" {
                        let sketch: cad::model::Sketch =
                            serde_json::from_value(value.clone()).map_err(input)?;
                        if serde_json::to_value(&sketch).map_err(input)? != *value {
                            return Err(input(
                                "Sketch geometry must use its complete schema without unknown fields",
                            ));
                        }
                        feature.set_sketch(&sketch);
                    } else {
                        let field = field.ok_or_else(|| {
                            input(format!(
                                "Parameter '{key}' is not declared by the feature plugin"
                            ))
                        })?;
                        field_value(field, value).map_err(input)?;
                        feature.params[key] = value.clone();
                    }
                }
                document.features.push(feature);
            }
        }
    }
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(Failure {
            code: "edit_conflict",
            message: "One or more edit targets changed. The file was not modified.".into(),
            details: json!({"conflicts":conflicts}),
        })
    }
}

fn validate(document: &Document, registry: &Registry) -> cad::model::Result<()> {
    document.validate()?;
    let numbers = Numbers::new(&document.variables)?;
    for (index, feature) in document.features.iter().enumerate() {
        let plugin = registry
            .plugins
            .get(&feature.plugin)
            .ok_or_else(|| format!("Missing plugin {}", feature.plugin))?;
        if plugin.hash != feature.plugin_hash || plugin.manifest.version != feature.plugin_version {
            return Err(format!(
                "Pinned plugin does not match feature {}",
                feature.id
            ));
        }
        for field in &plugin.manifest.fields {
            let value = feature.params.get(&field.key).unwrap_or(&field.default);
            field_value(field, value)?;
            if field.kind == "number" {
                numbers.value(value)?;
            }
            if field.kind == "sketch" || field.kind == "sketches" {
                let references = if field.kind == "sketches" {
                    value
                        .as_array()
                        .expect("sketch array type checked above")
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                } else {
                    vec![value.as_str().expect("sketch string type checked above")]
                };
                for reference in references {
                    if !document.features[..index].iter().any(|f| {
                        f.id == reference
                            && !f.suppressed
                            && registry
                                .plugins
                                .get(&f.plugin)
                                .is_some_and(|p| p.manifest.editor == "sketch")
                    }) {
                        return Err(format!(
                            "Parameter '{}' needs earlier, unsuppressed sketches; '{reference}' is unavailable",
                            field.key
                        ));
                    }
                }
            }
        }
        if let Some(sketch) = feature.sketch() {
            // Include suppressed and rolled-back sketches, which regeneration skips.
            let resolved = cad::sketch::resolve(&sketch, Plane::named("Top", 0.), &numbers)?;
            for constraint in sketch.constraints.iter().filter(|c| c.reference) {
                if !dimensions::is_dimension(&constraint.kind) {
                    return Err("Only dimensions can be reference constraints".into());
                }
                if matches!(constraint.kind.as_str(), "Width" | "Height")
                    && !sketch
                        .entities
                        .iter()
                        .any(|e| e.id == constraint.a && e.kind == "rectangle")
                {
                    return Err("Width and height dimensions need a rectangle".into());
                }
                dimensions::measure(&resolved, constraint)?;
            }
        }
    }
    Ok(())
}

fn summary(evaluation: &Evaluation) -> Value {
    let scene = &evaluation.scene;
    json!({
        "error":scene.error,
        "statuses":scene.statuses.iter().map(|s| json!({"id":s.id,"error":s.error,"suppressed":s.suppressed,"rolled_back":s.rolled_back})).collect::<Vec<_>>(),
        "parts":scene.meshes.iter().map(|m| json!({
            "key":m.part,"name":m.name,"volume_mm3":m.volume,"bounds_mm":m.bounds,
            "faces":m.faces.iter().map(|f| json!({"key":f.key,"plane":f.plane})).collect::<Vec<_>>(),
            "edges":m.edges.iter().filter(|e| !e.seam).map(|e| &e.key).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "sketches":scene.sketches.iter().map(|(id,s)| json!({"id":id,"degrees_of_freedom":s.dof,"plane":s.plane,
            "entities":s.entities.iter().map(|e| json!({"id":e.id,"kind":e.kind,"values":e.values,"construction":e.construction,"fixed":e.fixed})).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}
fn schemas(registry: &Registry) -> Value {
    json!(registry.plugins.values().map(|p| json!({
        "id":p.manifest.id,"name":p.manifest.name,"version":p.manifest.version,"hash":p.hash,
        "category":p.manifest.category,"editor":p.manifest.editor,
        "fields":p.manifest.fields.iter().map(|f| json!({"key":f.key,"label":f.label,"kind":f.kind,"default":f.default,"options":f.options})).collect::<Vec<_>>()
    })).collect::<Vec<_>>())
}
fn successful(evaluation: &Evaluation) -> Result<()> {
    if let Some(error) = &evaluation.scene.error {
        return Err(Failure {
            code: "regeneration_failed",
            message: error.clone(),
            details: summary(evaluation),
        });
    }
    Ok(())
}

fn run(request: Request) -> Result<Value> {
    if request.version != 1 {
        return Err(input("Unsupported protocol version. Use version 1."));
    }
    let mut registry = if let Some(plugins) = request.plugins {
        if !plugins.is_dir() {
            return Err(input("Plugin directory is missing"));
        }
        Registry::load(&plugins)
    } else {
        Registry::builtin().map_err(input)?
    };
    if !registry.errors.is_empty() {
        return Err(input(registry.errors.join("\n")));
    }
    // Plugin diagnostics must not corrupt or flood the single JSON response.
    registry.engine.on_print(|_| {});
    registry.engine.on_debug(|_, _, _| {});
    let mut runtime = Runtime::new(registry);
    if let Action::Create { document } = request.action {
        let path = output_path(&request.path)?;
        let _lock = lock(&path)?;
        if path.exists() {
            return Err(failure("output_exists", "Create needs a new file"));
        }
        validate(&document, &runtime.registry).map_err(|e| failure("validation_failed", e))?;
        let evaluation = runtime
            .evaluate(&document)
            .map_err(|e| failure("regeneration_failed", e))?;
        successful(&evaluation)?;
        let bytes = serde_json::to_vec_pretty(&document).map_err(input)?;
        cad::model::atomic_write(&path, |file| {
            file.write_all(&bytes).map_err(|e| e.to_string())
        })
        .map_err(io)?;
        return Ok(
            json!({"revision":revision(&bytes),"saved_path":path,"document":document,"model":summary(&evaluation)}),
        );
    }
    let path = request.path.canonicalize().map_err(io)?;
    let destination = match &request.action {
        Action::Apply {
            output: Some(output),
            ..
        }
        | Action::Export { output, .. } => Some(output_path(output)?),
        _ => None,
    };
    // All cooperating writers serialize before reading, so the merge is based on
    // the latest complete document and cannot race its final atomic replacement.
    let _lock = destination.as_deref().map(lock).transpose()?;
    let original = read_bounded(fs::File::open(&path).map_err(io)?)?;
    let mut document: Document = serde_json::from_slice(&original).map_err(input)?;
    document.validate().map_err(input)?;
    match request.action {
        Action::Create { .. } => unreachable!(),
        Action::Inspect => {
            let validation_error = validate(&document, &runtime.registry).err();
            match runtime.evaluate(&document) {
                Ok(evaluation) => Ok(
                    json!({"path":path,"revision":revision(&original),"document":document,"plugins":schemas(&runtime.registry),"validation_error":validation_error,"model":summary(&evaluation)}),
                ),
                Err(error) => Ok(
                    json!({"path":path,"revision":revision(&original),"document":document,"plugins":schemas(&runtime.registry),"validation_error":validation_error,"model":{"error":error}}),
                ),
            }
        }
        Action::Apply {
            expected_revision,
            output,
            edits,
        } => {
            if destination.as_ref() == Some(&path) && expected_revision.is_none() {
                return Err(input("Overwriting the source requires expected_revision"));
            }
            let stale = expected_revision
                .as_ref()
                .is_some_and(|expected| *expected != revision(&original));
            apply(&mut document, &runtime.registry, edits, stale)?;
            validate(&document, &runtime.registry).map_err(|e| failure("validation_failed", e))?;
            let evaluation = runtime
                .evaluate(&document)
                .map_err(|e| failure("regeneration_failed", e))?;
            successful(&evaluation)?;
            let bytes = serde_json::to_vec_pretty(&document).map_err(input)?;
            let mut saved_path = None;
            if output.is_some() {
                let output = destination.expect("apply output was resolved before locking");
                if output != path && output.exists() {
                    return Err(failure(
                        "output_exists",
                        "A preview output must be a new file",
                    ));
                }
                // Also detect a non-cooperating external writer before committing.
                if output == path && fs::read(&path).map_err(io)? != original {
                    return Err(failure(
                        "stale_revision",
                        "The source changed during regeneration",
                    ));
                }
                cad::model::atomic_write(&output, |file| {
                    file.write_all(&bytes).map_err(|e| e.to_string())
                })
                .map_err(io)?;
                saved_path = Some(output);
            }
            Ok(
                json!({"source_revision":revision(&original),"revision":revision(&bytes),"rebased":stale,"saved_path":saved_path,"document":document,"model":summary(&evaluation)}),
            )
        }
        Action::Export {
            format,
            output,
            expected_revision,
        } => {
            if !["step", "stl"].contains(&format.as_str()) {
                return Err(input("Export format must be step or stl"));
            }
            check_revision(expected_revision.as_deref(), &original)?;
            let output = output_path(&output)?;
            if output == path || output.exists() {
                return Err(failure("output_exists", "Export needs a new output file"));
            }
            validate(&document, &runtime.registry).map_err(|e| failure("validation_failed", e))?;
            let evaluation = runtime
                .evaluate(&document)
                .map_err(|e| failure("regeneration_failed", e))?;
            successful(&evaluation)?;
            evaluation.geometry.export(&format, &output).map_err(io)?;
            Ok(
                json!({"source_revision":revision(&original),"saved_path":output,"format":format,"model":summary(&evaluation)}),
            )
        }
    }
}

fn main() {
    let result = std::panic::catch_unwind(|| {
        let bytes = read_bounded(std::io::stdin().lock())?;
        let request: Request = serde_json::from_slice(&bytes).map_err(input)?;
        run(request)
    })
    .unwrap_or_else(|_| {
        Err(failure(
            "internal_error",
            "The request panicked. No successful result was committed.",
        ))
    });
    let (ok, response) = match result {
        Ok(result) => (true, json!({"version":1,"ok":true,"result":result})),
        Err(error) => (
            false,
            json!({"version":1,"ok":false,"error":{"code":error.code,"message":error.message,"details":error.details}}),
        ),
    };
    if serde_json::to_writer(std::io::stdout().lock(), &response).is_err() {
        std::process::exit(1);
    }
    println!();
    if !ok {
        std::process::exit(1);
    }
}
