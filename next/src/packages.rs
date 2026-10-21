//! Local declarative feature packages expand to ordinary native transaction edits.
//!
//! Packages do not own document state, run code, fetch dependencies, or commit.
//! The caller submits the edits under its current native document revision.
//! Templates support parameter substitution, checked integer index offsets and
//! two-term numeric sums. Unknown fields and template operators are errors.
//! Sketch packages append geometry, so callers must supply its native start
//! index when the sketch already contains geometry.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeaturePackage {
    pub version: u64,
    pub id: String,
    pub label: String,
    pub description: String,
    pub parameters: BTreeMap<String, Parameter>,
    pub edits: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    #[serde(rename = "type")]
    pub kind: ParameterType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Value>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    Identifier,
    Edge,
    Number,
    Integer,
    String,
    Boolean,
}

#[derive(Clone, Debug)]
pub struct Registry {
    entries: Vec<FeaturePackage>,
}

impl Registry {
    /// Read only immediate JSON files in a caller-selected local directory.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut paths = std::fs::read_dir(path)
            .map_err(|error| format!("Cannot read package directory {}: {error}", path.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Cannot read package directory: {error}"))?;
        paths.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        });
        paths.sort();
        let mut entries = Vec::new();
        for path in paths {
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("Cannot read package {}: {error}", path.display()))?;
            entries.push(parse(&source).map_err(|error| format!("{}: {error}", path.display()))?);
        }
        Self::from_entries(entries)
    }

    pub fn builtin() -> Result<Self, String> {
        let sources = [
            include_str!("../packages/sketch.rectangle.json"),
            include_str!("../packages/sketch.circle.json"),
            include_str!("../packages/feature.pad.json"),
            include_str!("../packages/feature.pocket.json"),
            include_str!("../packages/feature.fillet.json"),
            include_str!("../packages/body-boolean.json"),
        ];
        Self::from_entries(sources.into_iter().map(parse).collect::<Result<_, _>>()?)
    }

    fn from_entries(mut entries: Vec<FeaturePackage>) -> Result<Self, String> {
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        let mut ids = BTreeSet::new();
        for entry in &entries {
            if !ids.insert(entry.id.as_str()) {
                return Err(format!("Duplicate package id: {}", entry.id));
            }
        }
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[FeaturePackage] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&FeaturePackage> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn build(&self, id: &str, parameters: &Value) -> Result<Vec<Value>, String> {
        let package = self
            .get(id)
            .ok_or_else(|| format!("Unknown feature package: {id}"))?;
        let parameters = parameters
            .as_object()
            .ok_or("Package parameters must be an object")?;
        for name in parameters.keys() {
            if !package.parameters.contains_key(name) {
                return Err(format!("Unknown parameter for {id}: {name}"));
            }
        }
        let mut resolved = Map::new();
        for (name, spec) in &package.parameters {
            let value = parameters
                .get(name)
                .or(spec.default.as_ref())
                .ok_or_else(|| format!("Missing parameter for {id}: {name}"))?;
            validate_parameter(name, spec, value)?;
            resolved.insert(name.clone(), value.clone());
        }
        package
            .edits
            .iter()
            .map(|edit| substitute(edit, &resolved))
            .collect()
    }
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn validate_parameter(name: &str, spec: &Parameter, value: &Value) -> Result<(), String> {
    let valid = match spec.kind {
        ParameterType::Identifier => value.as_str().is_some_and(identifier),
        ParameterType::Edge => value
            .as_str()
            .and_then(|text| text.strip_prefix("Edge"))
            .is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix.bytes().all(|byte| byte.is_ascii_digit())
                    && suffix.parse::<u64>().is_ok_and(|index| index > 0)
            }),
        ParameterType::Number => value.as_f64().is_some_and(f64::is_finite),
        ParameterType::Integer => value.as_i64().is_some(),
        ParameterType::String => value.is_string(),
        ParameterType::Boolean => value.is_boolean(),
    };
    if !valid {
        return Err(format!("Parameter {name} must have type {:?}", spec.kind));
    }
    if spec
        .choices
        .as_ref()
        .is_some_and(|choices| !choices.contains(value))
    {
        return Err(format!(
            "Parameter {name} must be one of its declared choices"
        ));
    }
    if let Some(number) = value.as_f64()
        && (spec.minimum.is_some_and(|minimum| number < minimum)
            || spec
                .exclusive_minimum
                .is_some_and(|minimum| number <= minimum))
    {
        return Err(format!("Parameter {name} is below its allowed minimum"));
    }
    Ok(())
}

fn parse(source: &str) -> Result<FeaturePackage, String> {
    let package: FeaturePackage =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    if package.version != 1 {
        return Err(format!("Unsupported package version: {}", package.version));
    }
    if package.id.split('.').count() < 2 || !package.id.split('.').all(identifier) {
        return Err("Package id must contain dot-separated ASCII identifiers".into());
    }
    if package.label.trim().is_empty() || package.edits.is_empty() {
        return Err("Packages need a label and at least one edit".into());
    }
    for (name, spec) in &package.parameters {
        if !identifier(name) {
            return Err(format!("Invalid parameter name: {name}"));
        }
        if (spec.minimum.is_some() || spec.exclusive_minimum.is_some())
            && !matches!(spec.kind, ParameterType::Number | ParameterType::Integer)
        {
            return Err(format!("Only numeric parameter {name} can have a minimum"));
        }
        if let Some(choices) = &spec.choices {
            if choices.is_empty() {
                return Err(format!("Parameter {name} needs at least one enum choice"));
            }
            for (index, choice) in choices.iter().enumerate() {
                validate_parameter(name, spec, choice)?;
                if choices[..index].contains(choice) {
                    return Err(format!("Parameter {name} has duplicate enum choices"));
                }
            }
        }
        if let Some(default) = &spec.default {
            validate_parameter(name, spec, default)?;
        }
    }
    for edit in &package.edits {
        match edit.get("op").and_then(Value::as_str) {
            Some(
                "add_object" | "set" | "expression" | "geometry" | "constraint" | "datum"
                | "move_geometry" | "construction" | "delete_geometry" | "delete_constraint"
                | "constraint_state" | "trim_geometry",
            ) => (),
            _ => return Err("Package edits must use a supported native edit operation".into()),
        }
        validate_template(edit, &package.parameters)?;
    }
    Ok(package)
}

fn validate_template(
    value: &Value,
    parameters: &BTreeMap<String, Parameter>,
) -> Result<(), String> {
    match value {
        Value::Object(object) if object.contains_key("$sum") => {
            if object.len() != 1 {
                return Err("$sum cannot have additional fields".into());
            }
            let terms = object["$sum"]
                .as_array()
                .ok_or("$sum requires two numeric terms")?;
            if terms.len() != 2 {
                return Err("$sum requires two numeric terms".into());
            }
            for term in terms {
                validate_template(term, parameters)?;
                let numeric = term.is_number()
                    || term.get("$sum").is_some()
                    || term
                        .get("$param")
                        .and_then(Value::as_str)
                        .and_then(|name| parameters.get(name))
                        .is_some_and(|spec| {
                            matches!(spec.kind, ParameterType::Number | ParameterType::Integer)
                        });
                if !numeric {
                    return Err("$sum requires numeric literals or numeric parameters".into());
                }
            }
        }
        Value::Object(object) if object.contains_key("$param") => {
            if object.keys().any(|key| key != "$param" && key != "offset") {
                return Err("Parameter substitution accepts only $param and integer offset".into());
            }
            let name = object["$param"]
                .as_str()
                .ok_or("$param must name a declared parameter")?;
            let spec = parameters
                .get(name)
                .ok_or_else(|| format!("Undeclared parameter: {name}"))?;
            if let Some(offset) = object.get("offset")
                && (!matches!(spec.kind, ParameterType::Integer) || offset.as_i64().is_none())
            {
                return Err(
                    "Parameter offsets require an integer parameter and integer offset".into(),
                );
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                if key.starts_with('$') {
                    return Err(format!("Unknown package template operator: {key}"));
                }
                validate_template(value, parameters)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_template(value, parameters)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn substitute(value: &Value, parameters: &Map<String, Value>) -> Result<Value, String> {
    match value {
        Value::Object(object) if object.contains_key("$sum") => {
            let terms = object["$sum"].as_array().ok_or("Invalid numeric sum")?;
            let mut result = 0.0;
            for term in terms {
                result += substitute(term, parameters)?
                    .as_f64()
                    .ok_or("Invalid numeric term")?;
            }
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| "Package numeric sum overflow".into())
        }
        Value::Object(object) if object.contains_key("$param") => {
            let name = object["$param"]
                .as_str()
                .ok_or("Invalid parameter substitution")?;
            let value = parameters.get(name).ok_or("Missing resolved parameter")?;
            if let Some(offset) = object.get("offset") {
                let result = value
                    .as_i64()
                    .and_then(|value| value.checked_add(offset.as_i64()?))
                    .ok_or("Package integer offset overflow")?;
                Ok(Value::from(result))
            } else {
                Ok(value.clone())
            }
        }
        Value::Object(object) => Ok(Value::Object(
            object
                .iter()
                .map(|(key, value)| Ok((key.clone(), substitute(value, parameters)?)))
                .collect::<Result<_, String>>()?,
        )),
        Value::Array(values) => Ok(Value::Array(
            values
                .iter()
                .map(|value| substitute(value, parameters))
                .collect::<Result<_, _>>()?,
        )),
        _ => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bundled_packages_validate_and_expand_native_edits() {
        let registry = Registry::builtin().unwrap();
        assert_eq!(registry.entries().len(), 6);
        let rectangle = registry
            .build(
                "sketch.rectangle",
                &json!({
                    "object":"Sketch", "width":40, "height":30, "index":5
                }),
            )
            .unwrap();
        assert_eq!(rectangle.len(), 16);
        assert_eq!(rectangle[4]["args"], json!([5, 2, 6, 1]));
        assert_eq!(rectangle[15]["args"], json!([6, 30]));
        assert_eq!(rectangle[12]["args"], json!([5, 1, 0]));
        assert_eq!(rectangle[2]["start"], json!([40.0, 30.0, 0]));
        let pad = registry
            .build(
                "feature.pad",
                &json!({
                    "name":"Pad", "body":"Body", "profile":"Sketch", "length":20
                }),
            )
            .unwrap();
        assert_eq!(pad[1]["value"], json!({"link":"Sketch"}));
        let circle = registry
            .build(
                "sketch.circle",
                &json!({
                    "object":"Sketch", "radius":3
                }),
            )
            .unwrap();
        assert_eq!(circle[3]["args"], json!([0, 3]));
    }

    #[test]
    fn rejects_bad_parameters_before_any_native_transaction() {
        let registry = Registry::builtin().unwrap();
        for parameters in [
            json!({"object":"Sketch", "width":0, "height":30}),
            json!({"object":"Sketch", "width":40}),
            json!({"object":"Sketch", "width":40, "height":30, "surprise":true}),
            json!({"object":"bad/name", "width":40, "height":30}),
            json!({"object":"Sketch", "width":40, "height":30, "index":-1}),
            json!({"object":"Sketch", "width":40, "height":30, "index":i64::MAX}),
            json!({"object":"Sketch", "width":true, "height":30}),
            json!({"object":"Sketch", "x":1e308, "width":1e308, "height":30}),
        ] {
            assert!(
                registry.build("sketch.rectangle", &parameters).is_err(),
                "{parameters}"
            );
        }
        assert!(registry.build("unknown.package", &json!({})).is_err());
    }

    #[test]
    fn rejects_unknown_versions_dependencies_code_and_duplicate_ids() {
        let source = include_str!("../packages/feature.pad.json");
        let mut value: Value = serde_json::from_str(source).unwrap();
        value["version"] = json!(2);
        assert!(parse(&value.to_string()).is_err());
        value["version"] = json!(1);
        value["dependencies"] = json!(["https://example.org/package"]);
        assert!(parse(&value.to_string()).is_err());
        value.as_object_mut().unwrap().remove("dependencies");
        value["edits"][0]["name"] = json!({"$eval":"process()"});
        assert!(parse(&value.to_string()).is_err());
        value["edits"][0]["name"] = json!({"$param":"undeclared"});
        assert!(parse(&value.to_string()).is_err());
        let entry = parse(source).unwrap();
        assert!(Registry::from_entries(vec![entry.clone(), entry]).is_err());
    }

    #[test]
    fn enum_parameters_require_declared_typed_unique_choices() {
        let registry = Registry::builtin().unwrap();
        for operation in [json!("Fuse"), json!("Cut"), json!("Common")] {
            let edits = registry.build("body.boolean", &json!({"name":"Boolean","target":"Target","tool":"Tool","operation":operation})).unwrap();
            assert_eq!(edits[2]["value"], operation);
        }
        assert!(
            registry
                .build(
                    "body.boolean",
                    &json!({"name":"Boolean","target":"Target","tool":"Tool","operation":"Union"})
                )
                .is_err()
        );
        let mut manifest: Value =
            serde_json::from_str(include_str!("../packages/body-boolean.json")).unwrap();
        for choices in [json!([]), json!(["Fuse", "Fuse"]), json!([1])] {
            manifest["parameters"]["operation"]["enum"] = choices;
            assert!(parse(&manifest.to_string()).is_err());
        }
    }

    #[test]
    fn loads_local_extension_and_rejects_duplicate_files() {
        let bundled =
            Registry::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("packages")).unwrap();
        assert_eq!(
            serde_json::to_value(bundled.entries()).unwrap(),
            serde_json::to_value(Registry::builtin().unwrap().entries()).unwrap()
        );
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("cad-next-packages-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let mut custom: Value =
            serde_json::from_str(include_str!("../packages/feature.pad.json")).unwrap();
        custom["id"] = json!("local.extrude");
        let source = custom.to_string();
        std::fs::write(path.join("extrude.json"), &source).unwrap();
        let registry = Registry::load(&path).unwrap();
        let edits = registry
            .build(
                "local.extrude",
                &json!({
                    "name":"CustomPad", "body":"Body", "profile":"Sketch", "length":8
                }),
            )
            .unwrap();
        assert_eq!(edits[0]["name"], "CustomPad");
        std::fs::write(path.join("duplicate.json"), source).unwrap();
        assert!(
            Registry::load(&path)
                .unwrap_err()
                .contains("Duplicate package id")
        );
        std::fs::remove_file(path.join("duplicate.json")).unwrap();
        std::fs::remove_file(path.join("extrude.json")).unwrap();
        std::fs::remove_dir(&path).unwrap();
    }
}
