use cad_next::packages::Registry;
use serde_json::Value;
use std::path::PathBuf;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut path = None;
    let mut id = None;
    let mut parameters = Value::Null;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package-dir" => {
                path = Some(PathBuf::from(
                    args.next().ok_or("Missing package directory")?,
                ))
            }
            "--id" => id = Some(args.next().ok_or("Missing package id")?),
            "--parameters" => {
                parameters = serde_json::from_str(&args.next().ok_or("Missing parameters")?)?
            }
            "--help" | "-h" => {
                println!(
                    "cad-next-packages [--package-dir DIR] [--id ID --parameters JSON]\nLists local packages, or builds native edits for the chosen package."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument {arg}").into()),
        }
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
