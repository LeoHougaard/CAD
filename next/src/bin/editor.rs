#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cad_next::{WorkerConfig, editor::Editor};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut runtime = None;
    let mut document = None;
    let mut packages = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--runtime") => {
                runtime = Some(PathBuf::from(args.next().ok_or("--runtime needs a path")?))
            }
            Some("--open") => {
                document = Some(PathBuf::from(args.next().ok_or("--open needs a path")?))
            }
            Some("--packages") => {
                packages = Some(PathBuf::from(
                    args.next().ok_or("--packages needs a directory")?,
                ))
            }
            Some("--help" | "-h") => {
                println!(
                    "cad-next-editor --runtime runtime.json [--open part.FCStd] [--packages directory]"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {}", arg.to_string_lossy()).into()),
        }
    }
    let runtime: Value =
        serde_json::from_slice(&std::fs::read(runtime.ok_or("--runtime is required")?)?)?;
    let mut config = WorkerConfig::new(
        runtime["executable"]
            .as_str()
            .ok_or("Runtime executable is required")?,
        runtime["script"]
            .as_str()
            .ok_or("Runtime script is required")?,
    );
    config.args = runtime["args"]
        .as_array()
        .ok_or("Runtime args must be an array")?
        .iter()
        .map(|arg| {
            arg.as_str()
                .map(Into::into)
                .ok_or("Runtime argument must be a string")
        })
        .collect::<Result<_, _>>()?;
    config.response_timeout = Duration::from_secs(120);
    let registry = if let Some(path) = packages {
        cad_next::packages::Registry::load(&path)?
    } else {
        cad_next::packages::Registry::builtin()?
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CAD Next · Part Studio")
            .with_inner_size([1000., 800.])
            .with_min_inner_size([740., 520.]),
        renderer: eframe::Renderer::Glow,
        depth_buffer: 24,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "CAD Next",
        options,
        Box::new(move |cc| Ok(Box::new(Editor::new(cc, config, registry, document)?))),
    )?;
    Ok(())
}
