use crate::model::{Document, Feature, Result};
use rhai::{AST, Dynamic, Engine, Scope};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    #[serde(default)]
    pub editor: String,
    pub script: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub description: String,
}

pub struct Plugin {
    pub manifest: Manifest,
    pub hash: String,
    ast: AST,
}
impl Plugin {
    pub fn adopt_profile_input(&self, feature: &mut Feature) {
        if self.manifest.fields.iter().any(|field| field.kind == "profiles"
            && feature.params.get(&field.key).is_some_and(Value::is_array))
            && self.matches(feature)
        {
            if feature.plugin_hash != self.hash
                && feature.params.get("legacy_profile_source").and_then(Value::as_str).unwrap_or("").is_empty() {
                let mut legacy = feature.params.clone();
                legacy["profiles"] = Value::Null;
                if let Ok(items) = crate::model::ProfileReference::from_params(&legacy)
                    && let Some(item) = items.first() {
                    feature.params["legacy_profile_source"] = json!(item.source_key());
                }
            }
            feature.plugin_version = self.manifest.version.clone();
            feature.plugin_hash = self.hash.clone();
        }
    }
    /// Additive builtin inputs preserve old region, profile and scope semantics.
    /// Compatibility applies only to these exact bundled package identities.
    pub fn matches(&self, feature: &Feature) -> bool {
        (self.hash == feature.plugin_hash && self.manifest.version == feature.plugin_version)
            || (feature.plugin == "extrude"
                && ((feature.plugin_version == "1.0.0"
                    && feature.plugin_hash == "b3447843f1693770")
                    || (feature.plugin_version == "1.1.0"
                        && feature.plugin_hash == "094953e2178e0cde")
                    || (feature.plugin_version == "1.2.0"
                        && feature.plugin_hash == "0d4a61c8cdf65c59"))
                && self.manifest.id == "extrude"
                && self.manifest.version == "1.3.0"
                && self.hash
                    == source_hash(
                        &[
                            include_str!("../plugins/extrude.json"),
                            include_str!("../plugins/extrude.rhai"),
                        ]
                        .concat(),
                    ))
            || (feature.plugin == "revolve"
                && ((feature.plugin_version == "1.0.0" && feature.plugin_hash == "967fd7d645e4df99")
                    || (feature.plugin_version == "1.1.0" && feature.plugin_hash == "50aaf7062e8e6eca"))
                && self.manifest.id == "revolve"
                && self.manifest.version == "1.2.0"
                && self.hash
                    == source_hash(
                        &[
                            include_str!("../plugins/revolve.json"),
                            include_str!("../plugins/revolve.rhai"),
                        ]
                        .concat(),
                    ))
    }
}
pub struct Registry {
    pub engine: Engine,
    pub plugins: BTreeMap<String, Plugin>,
    pub errors: Vec<String>,
}

pub fn script_engine() -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(100_000)
        .set_max_array_size(20_000)
        .set_max_map_size(2000)
        .set_max_string_size(1024 * 1024)
        .set_max_expr_depths(64, 64)
        .set_max_call_levels(32);
    e.set_module_resolver(rhai::module_resolvers::StaticModuleResolver::new());
    e.disable_symbol("eval");
    e
}

pub fn source_hash(s: &str) -> String {
    // Stable content identity, not a security signature.
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

impl Registry {
    pub fn builtin() -> Result<Self> {
        let mut r = Self {
            engine: script_engine(),
            plugins: BTreeMap::new(),
            errors: vec![],
        };
        for (manifest, script) in [
            (
                include_str!("../plugins/sketch.json"),
                include_str!("../plugins/sketch.rhai"),
            ),
            (
                include_str!("../plugins/extrude.json"),
                include_str!("../plugins/extrude.rhai"),
            ),
            (
                include_str!("../plugins/revolve.json"),
                include_str!("../plugins/revolve.rhai"),
            ),
            (
                include_str!("../plugins/boolean.json"),
                include_str!("../plugins/boolean.rhai"),
            ),
            (
                include_str!("../plugins/plane.json"),
                include_str!("../plugins/plane.rhai"),
            ),
            (
                include_str!("../plugins/sweep.json"),
                include_str!("../plugins/sweep.rhai"),
            ),
            (
                include_str!("../plugins/loft.json"),
                include_str!("../plugins/loft.rhai"),
            ),
            (
                include_str!("../plugins/fillet.json"),
                include_str!("../plugins/fillet.rhai"),
            ),
            (
                include_str!("../plugins/chamfer.json"),
                include_str!("../plugins/chamfer.rhai"),
            ),
            (
                include_str!("../plugins/shell.json"),
                include_str!("../plugins/shell.rhai"),
            ),
            (
                include_str!("../plugins/mirror.json"),
                include_str!("../plugins/mirror.rhai"),
            ),
            (
                include_str!("../plugins/pattern.json"),
                include_str!("../plugins/pattern.rhai"),
            ),
            (
                include_str!("../plugins/transform.json"),
                include_str!("../plugins/transform.rhai"),
            ),
            (
                include_str!("../plugins/step.json"),
                include_str!("../plugins/step.rhai"),
            ),
            (
                include_str!("../plugins/stl.json"),
                include_str!("../plugins/stl.rhai"),
            ),
            (include_str!("../plugins/mate-fastened.json"), include_str!("../plugins/mate-fastened.rhai")),
            (include_str!("../plugins/mate-revolute.json"), include_str!("../plugins/mate-revolute.rhai")),
            (include_str!("../plugins/mate-slider.json"), include_str!("../plugins/mate-slider.rhai")),
            (include_str!("../plugins/mate-cylindrical.json"), include_str!("../plugins/mate-cylindrical.rhai")),
            (include_str!("../plugins/mate-planar.json"), include_str!("../plugins/mate-planar.rhai")),
            (include_str!("../plugins/mate-ball.json"), include_str!("../plugins/mate-ball.rhai")),
        ] {
            r.add(manifest, script)?;
        }
        Ok(r)
    }
    pub fn load(path: &Path) -> Self {
        let mut r = Self::available(path);
        if let Ok(disabled) = std::fs::read_to_string(path.join("disabled.txt")) {
            for id in disabled.lines() {
                r.plugins.remove(id.trim());
            }
        }
        r
    }
    pub fn available(path: &Path) -> Self {
        // External packages replace bundled defaults by ID and can add feature types.
        let mut r = Self::builtin().expect("bundled plugins are validated by tests");
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut files: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect();
            files.sort();
            for f in files {
                let result = (|| {
                    let text = std::fs::read_to_string(&f).map_err(|e| e.to_string())?;
                    let m: Manifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if Path::new(&m.script).components().count() != 1 {
                        return Err("Script must be in the package directory".into());
                    }
                    let script =
                        std::fs::read_to_string(path.join(&m.script)).map_err(|e| e.to_string())?;
                    r.add(&text, &script)
                })();
                if let Err(e) = result {
                    r.errors.push(format!("{}: {e}", f.display()));
                }
            }
        }
        r
    }
    pub fn add(&mut self, text: &str, script: &str) -> Result<()> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if m.id.is_empty() || m.fields.len() > 64 || script.len() > 1024 * 1024 {
            return Err("Invalid plugin package size or identity".into());
        }
        let ast = self.engine.compile(script).map_err(|e| e.to_string())?;
        let hash = source_hash(&(text.to_owned() + script));
        self.plugins.insert(
            m.id.clone(),
            Plugin {
                manifest: m,
                hash,
                ast,
            },
        );
        Ok(())
    }
    pub fn create(&self, id: &str, doc: &mut Document) -> Result<Feature> {
        let p = self
            .plugins
            .get(id)
            .ok_or_else(|| format!("Plugin {id} is missing"))?;
        let mut params = serde_json::Map::new();
        for f in &p.manifest.fields {
            params.insert(f.key.clone(), f.default.clone());
        }
        if p.manifest.editor == "sketch" {
            params.insert(
                "geometry".into(),
                json!({"entities":[],"constraints":[],"next_id":0}),
            );
        }
        let count = doc.features.iter().filter(|f| f.plugin == id).count() + 1;
        Ok(Feature {
            id: doc.allocate_id(),
            plugin: id.into(),
            plugin_version: p.manifest.version.clone(),
            plugin_hash: p.hash.clone(),
            name: format!("{} {count}", p.manifest.name),
            suppressed: false,
            params: Value::Object(params),
        })
    }
    pub fn evaluate(&self, feature: &Feature, numbers: &Numbers) -> Result<Value> {
        let p = self
            .plugins
            .get(&feature.plugin)
            .ok_or_else(|| format!("Missing plugin {}", feature.plugin))?;
        if !p.matches(feature) {
            return Err("Plugin code changed. Restore the pinned package or explicitly adopt the new version in the feature editor.".into());
        }
        let mut params = feature.params.clone();
        if !params.is_object() {
            return Err("Feature parameters must be an object".into());
        }
        for f in &p.manifest.fields {
            if f.kind == "number" {
                params[&f.key] = json!(numbers.value(params.get(&f.key).unwrap_or(&f.default))?);
            }
        }
        let data = rhai::serde::to_dynamic(params).map_err(|e| e.to_string())?;
        let output: Dynamic = self
            .engine
            .call_fn(&mut Scope::new(), &p.ast, "build", (data,))
            .map_err(|e| e.to_string())?;
        rhai::serde::from_dynamic(&output).map_err(|e| e.to_string())
    }
}

mod expressions;

pub struct Numbers {
    engine: Engine,
    values: BTreeMap<String, f64>,
    format: u32,
}
impl Numbers {
    pub fn new(vars: &BTreeMap<String, String>) -> Result<Self> {
        Self::for_format(vars, 2)
    }
    pub fn for_document(document: &Document) -> Result<Self> {
        Self::for_format(&document.variables, document.format)
    }
    pub fn for_format(vars: &BTreeMap<String, String>, format: u32) -> Result<Self> {
        if !matches!(format, 1 | 2) { return Err(format!("Unsupported expression format {format}")); }
        let mut engine = script_engine();
        if format >= 2 { expressions::functions(&mut engine); }
        // CAD dimensions use real division even when both literals are integers.
        // Feature scripts retain normal Rhai integer semantics for indexes/loops.
        #[allow(deprecated)]
        engine.on_parse_token(move |token, _, _| match token {
            rhai::Token::XOr if format >= 2 => rhai::Token::PowerOf,
            rhai::Token::IntegerConstant(n) => {
                rhai::Token::FloatConstant(((n as f64).into(), n.to_string().into()).into())
            }
            other => other,
        });
        let mut n = Self {
            engine,
            format,
            values: BTreeMap::from([
                ("mm".into(), 1.),
                ("cm".into(), 10.),
                ("m".into(), 1000.),
                ("inch".into(), 25.4),
                ("deg".into(), 1.),
                ("PI".into(), std::f64::consts::PI),
            ]),
        };
        if format >= 2 {
            n.values.extend([
                ("pi".into(), std::f64::consts::PI),
                ("e".into(), std::f64::consts::E),
                ("rad".into(), 180. / std::f64::consts::PI),
            ]);
        }
        let mut pending = vars.clone();
        while !pending.is_empty() {
            let mut progress = false;
            for (name, expr) in pending.clone() {
                if name.is_empty()
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || name.chars().next().unwrap().is_ascii_digit()
                {
                    return Err(format!("Invalid variable name '{name}'"));
                }
                if ["mm", "cm", "m", "inch", "deg", "PI"].contains(&name.as_str())
                    || (format >= 2 && ["pi", "e", "rad"].contains(&name.as_str())) {
                    return Err(format!("'{name}' is a reserved unit name"));
                }
                if let Ok(x) = n.eval(&expr) {
                    n.values.insert(name.clone(), x);
                    pending.remove(&name);
                    progress = true;
                }
            }
            if !progress {
                return Err(format!(
                    "Unresolved or circular variables: {}",
                    pending.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
        Ok(n)
    }
    pub fn eval(&self, expr: &str) -> Result<f64> {
        if expr.len() > 2048 {
            return Err("Expression is too long".into());
        }
        let text = expr.trim().trim_start_matches('=');
        let expression = if self.format >= 2 { expressions::normalize(text)? } else { text.replace('#', "") };
        let mut scope = Scope::new();
        for (k, v) in &self.values {
            scope.push_constant(k.as_str(), *v);
        }
        let result = self
            .engine
            .eval_expression_with_scope::<Dynamic>(&mut scope, &expression)
            .map_err(|e| format!("{expr}: {e}"))?;
        let x = if result.is::<f64>() {
            result.cast::<f64>()
        } else if result.is::<i64>() {
            result.cast::<i64>() as f64
        } else {
            return Err(format!("'{expr}' is not a number"));
        };
        if !x.is_finite() || x.abs() > 1e8 {
            return Err("Value must be finite and within the modeling range".into());
        }
        Ok(x)
    }
    pub fn value(&self, v: &Value) -> Result<f64> {
        if let Some(x) = v.as_f64() {
            if x.is_finite() && x.abs() <= 1e8 {
                Ok(x)
            } else {
                Err("Invalid number".into())
            }
        } else {
            self.eval(v.as_str().ok_or("Expected a dimension expression")?)
        }
    }
}
