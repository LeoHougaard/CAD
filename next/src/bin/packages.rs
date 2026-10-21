use cad_next::{feature_commands, packages::Registry, property_commands};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedRequest {
    #[serde(rename = "type")]
    type_id: String,
    state: Value,
    name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    selected_object: Option<String>,
    #[serde(default)]
    selections: Vec<(String, String)>,
}

fn read_json(argument: String) -> Result<Value, Box<dyn std::error::Error>> {
    let source = if argument == "-" {
        let mut source = String::new();
        io::stdin().read_to_string(&mut source)?;
        source
    } else {
        argument
    };
    Ok(serde_json::from_str(&source)?)
}

enum Mode {
    Catalog,
    Seed(Value),
    Properties(Value),
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut path = None;
    let mut id = None;
    let mut parameters = Value::Null;
    let mut mode = None;
    let mut package_options = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package-dir" => {
                package_options = true;
                path = Some(PathBuf::from(
                    args.next().ok_or("Missing package directory")?,
                ))
            }
            "--id" => {
                package_options = true;
                id = Some(args.next().ok_or("Missing package id")?)
            }
            "--parameters" => {
                package_options = true;
                parameters = serde_json::from_str(&args.next().ok_or("Missing parameters")?)?
            }
            "--feature-catalog" | "--feature-seed" | "--native-properties" => {
                if mode.is_some() {
                    return Err("Choose one feature compiler mode".into());
                }
                mode = Some(match arg.as_str() {
                    "--feature-catalog" => Mode::Catalog,
                    "--feature-seed" => Mode::Seed(read_json(
                        args.next()
                            .ok_or("--feature-seed requires JSON or - for stdin")?,
                    )?),
                    _ => Mode::Properties(read_json(
                        args.next()
                            .ok_or("--native-properties requires JSON or - for stdin")?,
                    )?),
                });
            }
            "--help" | "-h" => {
                println!(
                    "cad-next-packages [--package-dir DIR] [--id ID --parameters JSON]\nLists local packages, or builds native edits for the chosen package.\n--feature-catalog lists native feature definitions.\n--feature-seed JSON suggests native properties from {{type,state,name,body?,selected_object?,selections?}}.\n--native-properties JSON builds the same native property edits as the editor.\nUse - instead of JSON to read a request from stdin. These commands do not mutate documents."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument {arg}").into()),
        }
    }
    if let Some(mode) = mode {
        if package_options {
            return Err("Feature compiler modes cannot be mixed with package options".into());
        }
        let result = match mode {
            Mode::Catalog => serde_json::to_value(feature_commands::definitions())?,
            Mode::Properties(parameters) => {
                serde_json::to_value(property_commands::build(&parameters)?)?
            }
            Mode::Seed(request) => {
                let request: SeedRequest = serde_json::from_value(request)?;
                serde_json::to_value(feature_commands::seed(
                    &request.state,
                    &request.type_id,
                    request.body.as_deref(),
                    &request.name,
                    request.selected_object.as_deref(),
                    &request.selections,
                )?)?
            }
        };
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    let registry = if let Some(path) = path {
        Registry::load(&path)?
    } else {
        Registry::builtin()?
    };
    let result = if let Some(id) = id {
        serde_json::to_value(registry.build(&id, &parameters)?)?
    } else {
        serde_json::to_value(registry.entries())?
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cad-next-packages: {error}");
        std::process::exit(1);
    }
}
