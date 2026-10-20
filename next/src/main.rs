use cad_next::{Error, WorkerConfig, WorkerSession};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

fn config() -> Result<Option<WorkerConfig>, String> {
    let mut executable = std::env::var_os("CAD_NEXT_WORKER_EXE");
    let mut script = std::env::var_os("CAD_NEXT_WORKER_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend/freecad_worker.py")
        });
    let mut worker_args = Vec::<OsString>::new();
    let mut timeout = Duration::from_secs(120);
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "cad-next --executable PATH [--arg ARG]... [--script PATH] [--timeout-ms N]\n\nPersistent local FreeCAD host. Reads one JSON operation per stdin line; writes\none versioned response per stdout line. Missing version and ID are supplied.\nEnvironment: CAD_NEXT_WORKER_EXE, CAD_NEXT_WORKER_SCRIPT. Diagnostics use stderr."
                );
                return Ok(None);
            }
            Some("--executable") => {
                executable = Some(args.next().ok_or("--executable needs a path")?)
            }
            Some("--script") => script = args.next().ok_or("--script needs a path")?.into(),
            Some("--arg") => worker_args.push(args.next().ok_or("--arg needs a value")?),
            Some("--timeout-ms") => {
                let raw = args.next().ok_or("--timeout-ms needs a positive integer")?;
                let millis = raw
                    .to_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| *value > 0)
                    .ok_or("--timeout-ms needs a positive integer")?;
                timeout = Duration::from_millis(millis);
            }
            _ => return Err(format!("unknown argument {}", arg.to_string_lossy())),
        }
    }
    let executable = executable
        .ok_or("set --executable or CAD_NEXT_WORKER_EXE to the native worker launcher")?;
    let mut config = WorkerConfig::new(executable, script);
    config.args = worker_args;
    config.response_timeout = timeout;
    Ok(Some(config))
}

fn run(config: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut session = WorkerSession::spawn(&config)?;
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Result<Value, _> = serde_json::from_str(&line);
        let id = request
            .as_ref()
            .ok()
            .and_then(|value| value.get("id"))
            .cloned()
            .unwrap_or(Value::Null);
        let response = match request {
            Ok(request) => match session.request(request) {
                Ok(response) => response,
                Err(Error::InvalidRequest(message)) => {
                    json!({"version":1,"id":id,"ok":false,"error":{"code":"invalid_request","message":message}})
                }
                Err(error) => return Err(Box::new(error)),
            },
            Err(error) => {
                json!({"version":1,"id":id,"ok":false,"error":{"code":"invalid_json","message":error.to_string()}})
            }
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn main() {
    let result = match config() {
        Ok(Some(config)) => run(config),
        Ok(None) => return,
        Err(message) => Err(message.into()),
    };
    if let Err(error) = result {
        eprintln!("cad-next: {error}");
        std::process::exit(1);
    }
}
