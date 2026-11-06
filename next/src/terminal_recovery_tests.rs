//! Test-only stdio driver for the public Rust WorkerSession API. The production
//! CLI and native worker protocol are unchanged; native faults live in wrappers.
use crate::{WorkerConfig, WorkerSession};
use serde_json::Value;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[test]
#[ignore = "bounded public-host native regression driver"]
fn public_host() {
    let runtime: Value =
        serde_json::from_slice(&std::fs::read(std::env::var_os("R7_RUNTIME").unwrap()).unwrap())
            .unwrap();
    let mut config = WorkerConfig::new(
        runtime["executable"].as_str().unwrap(),
        PathBuf::from(runtime["script"].as_str().unwrap()),
    );
    config.args = runtime["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().into())
        .collect();
    let mut worker = WorkerSession::spawn(&config).unwrap();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        let response = worker.request(request).unwrap();
        // Separate protocol frames from the Rust test harness's own text.
        println!("\n{}", serde_json::to_string(&response).unwrap());
        io::stdout().flush().unwrap();
    }
}
