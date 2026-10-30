//! Persistent local modeling session. Native documents stay inside the worker.

pub mod assembly_commands;
pub mod body_commands;
pub mod document_units;
#[cfg(feature = "desktop")]
pub mod editor;
pub mod extrusion_commands;
pub mod feature_commands;
mod gesture_leases;
pub mod history_commands;
pub mod hole_commands;
pub mod measurement_commands;
pub mod measurement_types;
pub mod packages;
pub mod profile_reference_commands;
pub mod property_commands;
mod revision_registry;
pub mod sketch_commands;
pub mod sketch_creation_commands;
pub mod sketch_curve_commands;
pub mod sketch_dimensions;
mod sketch_journal;
pub mod sketch_operation_commands;
#[cfg(test)]
mod terminal_recovery_tests;
pub mod transform_commands;
#[cfg(feature = "desktop")]
pub mod viewport;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub executable: OsString,
    pub args: Vec<OsString>,
    pub script: PathBuf,
    pub response_timeout: Duration,
}

impl WorkerConfig {
    pub fn new(executable: impl AsRef<OsStr>, script: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.as_ref().to_owned(),
            args: Vec::new(),
            script: script.into(),
            response_timeout: Duration::from_secs(120),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidRequest(String),
    Protocol(String),
    WorkerExited(String),
    Timeout(Duration),
    Closed,
    Remote { code: String, message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "worker I/O: {error}"),
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::Protocol(message) => write!(f, "worker protocol: {message}"),
            Self::WorkerExited(message) => write!(f, "worker exited: {message}"),
            Self::Timeout(timeout) => write!(f, "worker response timed out after {timeout:?}"),
            Self::Closed => write!(f, "worker session is closed"),
            Self::Remote { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Deserialize)]
struct RemoteError {
    code: String,
    message: String,
}

struct FrameWrite {
    bytes: Vec<u8>,
    complete: SyncSender<io::Result<()>>,
}

/// One worker, one authoritative native document session, one request at a time.
/// A failed transaction is a normal remote error and leaves the session usable.
/// Transport/protocol failures terminate the child because mutation outcome may
/// be unknown. Callers must reopen the last saved native document explicitly.
pub struct WorkerSession {
    child: Child,
    input: Option<SyncSender<FrameWrite>>,
    output: Receiver<io::Result<String>>,
    timeout: Duration,
    next_id: u64,
    closed: bool,
    capabilities: Value,
    journals: sketch_journal::Journals,
    callback_sequence: u64,
    revisions: revision_registry::Revisions,
    leases: gesture_leases::Leases,
}

impl WorkerSession {
    pub fn spawn(config: &WorkerConfig) -> Result<Self, Error> {
        if config.response_timeout.is_zero() {
            return Err(Error::InvalidRequest(
                "response timeout must be positive".into(),
            ));
        }
        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .arg(&config.script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let mut input = child.stdin.take().expect("piped worker stdin");
        let (writes, pending) = mpsc::sync_channel::<FrameWrite>(1);
        std::thread::spawn(move || {
            while let Ok(frame) = pending.recv() {
                let result = input.write_all(&frame.bytes).and_then(|_| input.flush());
                let failed = result.is_err();
                if frame.complete.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let output = child.stdout.take().expect("piped worker stdout");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let failed = line.is_err();
                if sender.send(line).is_err() || failed {
                    break;
                }
            }
        });
        let mut session = Self {
            child,
            input: Some(writes),
            output: receiver,
            timeout: config.response_timeout,
            next_id: 1,
            closed: false,
            capabilities: Value::Null,
            journals: sketch_journal::Journals::default(),
            callback_sequence: 0,
            revisions: revision_registry::Revisions::default(),
            leases: gesture_leases::Leases::default(),
        };
        session.capabilities = session.call("capabilities", Map::new())?;
        Ok(session)
    }

    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    /// Send an operation and return its result, preserving structured errors.
    pub fn call(&mut self, op: &str, parameters: Map<String, Value>) -> Result<Value, Error> {
        if parameters.contains_key("op")
            || parameters.contains_key("id")
            || parameters.contains_key("version")
        {
            return Err(Error::InvalidRequest(
                "operation parameters contain reserved envelope fields".into(),
            ));
        }
        let mut request = parameters;
        request.insert("op".into(), Value::String(op.into()));
        let response = self.request(Value::Object(request))?;
        if response["ok"] == true {
            Ok(response["result"].clone())
        } else {
            let error: RemoteError = serde_json::from_value(response["error"].clone())
                .map_err(|error| Error::Protocol(error.to_string()))?;
            Err(Error::Remote {
                code: error.code,
                message: error.message,
            })
        }
    }

    /// Exchange one request envelope. Missing version and ID are supplied here.
    /// Explicit caller IDs are retained. Remote failures remain response values.
    pub fn request(&mut self, mut request: Value) -> Result<Value, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        let object = request
            .as_object_mut()
            .ok_or_else(|| Error::InvalidRequest("expected an object".into()))?;
        if object
            .get("op")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(Error::InvalidRequest("op must be a nonempty string".into()));
        }
        let version = object.entry("version").or_insert(json!(PROTOCOL_VERSION));
        if version.as_u64() != Some(PROTOCOL_VERSION) {
            return Err(Error::InvalidRequest("unsupported protocol version".into()));
        }
        let id = object.entry("id").or_insert(json!(self.next_id)).clone();
        if !id.is_i64() && !id.is_u64() {
            return Err(Error::InvalidRequest("id must be an integer".into()));
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::InvalidRequest("request IDs exhausted".into()))?;
        let result = self.exchange(&request, &id);
        if result.is_err() {
            self.shutdown();
        }
        result
    }

    /// A blocked pipe writer must not hold the model thread past its deadline.
    /// Only one frame is outstanding; expiry closes and kills the native worker.
    fn write_frame(&self, frame: &Value, start: Instant) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(frame).map_err(io::Error::other)?;
        bytes.push(b'\n');
        let remaining = self
            .timeout
            .checked_sub(start.elapsed())
            .ok_or(Error::Timeout(self.timeout))?;
        let (complete, result) = mpsc::sync_channel(1);
        self.input
            .as_ref()
            .ok_or(Error::Closed)?
            .try_send(FrameWrite { bytes, complete })
            .map_err(|error| Error::Protocol(format!("worker write unavailable: {error}")))?;
        match result.recv_timeout(remaining) {
            Ok(result) => result.map_err(Error::Io),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Timeout(self.timeout)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(Error::WorkerExited("stdin writer closed".into()))
            }
        }
    }

    fn exchange(&mut self, request: &Value, id: &Value) -> Result<Value, Error> {
        let start = Instant::now();
        self.write_frame(request, start)?;
        let mut callbacks = 0;
        loop {
            let remaining = self
                .timeout
                .checked_sub(start.elapsed())
                .ok_or(Error::Timeout(self.timeout))?;
            let line = match self.output.recv_timeout(remaining) {
                Ok(line) => line?,
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(Error::Timeout(self.timeout)),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let status = self
                        .child
                        .try_wait()?
                        .map(|status| status.to_string())
                        .unwrap_or_else(|| "stdout closed before a response".into());
                    return Err(Error::WorkerExited(status));
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // FreeCADCmd can emit startup text before Python redirects stdout.
            // JSON-looking output must satisfy the protocol; never hide it.
            let response: Value = match serde_json::from_str(trimmed) {
                Ok(response) => response,
                Err(error) if trimmed.starts_with('{') || trimmed.starts_with('[') => {
                    return Err(Error::Protocol(format!("malformed JSON: {error}")));
                }
                Err(_) => {
                    eprintln!("[worker] {line}");
                    continue;
                }
            };
            if response["kind"] == sketch_journal::CHANNEL
                || response["kind"] == revision_registry::CHANNEL
                || response["kind"] == gesture_leases::CHANNEL
            {
                callbacks += 1;
                let sequence = self
                    .callback_sequence
                    .checked_add(1)
                    .ok_or_else(|| Error::Protocol("session callback sequence exhausted".into()))?;
                if callbacks > 4096
                    || response["version"] != PROTOCOL_VERSION
                    || response["id"] != *id
                    || response["sequence"].as_u64() != Some(sequence)
                {
                    return Err(Error::Protocol(
                        "invalid session callback correlation".into(),
                    ));
                }
                self.callback_sequence = sequence;
                let mut reply = if response["kind"] == sketch_journal::CHANNEL {
                    self.journals.respond(&response)
                } else if response["kind"] == revision_registry::CHANNEL {
                    self.revisions.respond(&response)
                } else {
                    self.leases.respond(&response)
                };
                let reply_object = reply
                    .as_object_mut()
                    .expect("session callback response object");
                for key in ["kind", "version", "id", "sequence"] {
                    reply_object.insert(key.into(), response[key].clone());
                }
                self.write_frame(&reply, start)?;
                continue;
            }
            validate_response(&response, id)?;
            return Ok(response);
        }
    }

    /// On Windows, close stdin and wait for stdout EOF and the actual child exit.
    /// Unix refuses this operation without changing the session; Drop retains
    /// its existing private-process-group cleanup until graceful group lifetime
    /// ownership is independently implemented and tested there.
    /// Does not save documents. A timeout or transport error forces cleanup and
    /// remains an error; a nonzero child status is returned without reinterpretation.
    pub fn shutdown_gracefully(&mut self, timeout: Duration) -> Result<ExitStatus, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .filter(|_| !timeout.is_zero())
            .ok_or_else(|| {
                Error::InvalidRequest("shutdown timeout must be positive and finite".into())
            })?;
        #[cfg(unix)]
        {
            let _ = deadline;
            Err(Error::InvalidRequest(
                "graceful process-group shutdown is not supported on Unix".into(),
            ))
        }
        #[cfg(not(unix))]
        {
            self.input.take();
            let mut stdout_closed = false;
            loop {
                let status = match self.child.try_wait() {
                    Ok(status) => status,
                    Err(error) => {
                        self.shutdown();
                        return Err(Error::Io(error));
                    }
                };
                let received = match self.output.try_recv() {
                    Ok(Ok(_)) => true,
                    Ok(Err(error)) => {
                        self.shutdown();
                        return Err(Error::Io(error));
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        stdout_closed = true;
                        false
                    }
                    Err(mpsc::TryRecvError::Empty) => false,
                };
                if let Some(status) = status.filter(|_| stdout_closed) {
                    self.closed = true;
                    return Ok(status);
                }
                if Instant::now() >= deadline {
                    self.shutdown();
                    return Err(Error::Timeout(timeout));
                }
                if !received {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    /// Stop the child. Does not save native documents implicitly.
    pub fn shutdown(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.input.take();
        #[cfg(unix)]
        {
            // The configured launcher and its native worker share the private
            // process group created at spawn. AppRun may retain a shell parent.
            // SAFETY: this positive child PID is our private process-group ID.
            if let Ok(pid) = libc::pid_t::try_from(self.child.id()) {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        #[cfg(unix)]
        self.shutdown();
        #[cfg(not(unix))]
        let _ = self.shutdown_gracefully(Duration::from_secs(1));
    }
}

fn validate_response(response: &Value, id: &Value) -> Result<(), Error> {
    let object = response
        .as_object()
        .ok_or_else(|| Error::Protocol("response is not an object".into()))?;
    if object.get("version").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
        return Err(Error::Protocol("response version mismatch".into()));
    }
    if object.get("id") != Some(id) {
        return Err(Error::Protocol(format!(
            "response ID mismatch, expected {id}"
        )));
    }
    match object.get("ok").and_then(Value::as_bool) {
        Some(true) if object.contains_key("result") && !object.contains_key("error") => Ok(()),
        Some(false) if object.contains_key("error") && !object.contains_key("result") => {
            serde_json::from_value::<RemoteError>(object["error"].clone())
                .map(|_| ())
                .map_err(|error| Error::Protocol(format!("invalid error envelope: {error}")))
        }
        _ => Err(Error::Protocol(
            "expected ok plus exactly one result or error".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Raw decimal spellings captured from native worker responses.  The
    // fixture writes them directly so its own JSON parser cannot change them.
    #[test]
    #[ignore = "transport fixture run only as a child process"]
    fn native_float_wire_fixture() {
        for line in io::stdin().lock().lines() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            println!(
                "{{\"version\":1,\"id\":{},\"ok\":true,\"result\":{{\"values\":[-99.99999999999999,99.99999999999999,4.04121180963557e-14,-0.0]}}}}",
                request["id"]
            );
            io::stdout().flush().unwrap();
        }
    }

    #[test]
    fn native_wire_float_bits_and_signed_zero_survive_json_roundtrip() {
        let mut config = WorkerConfig::new(std::env::current_exe().unwrap(), "--quiet");
        config.args = [
            "--ignored",
            "--exact",
            "tests::native_float_wire_fixture",
            "--nocapture",
        ]
        .map(OsString::from)
        .to_vec();
        config.response_timeout = Duration::from_secs(2);
        let mut worker = WorkerSession::spawn(&config).unwrap();
        let response = worker.call("native_values", Map::new()).unwrap();
        let expected = [
            0xc058_ffff_ffff_ffff_u64,
            0x4058_ffff_ffff_ffff_u64,
            0x3d26_c000_0000_0000_u64,
            0x8000_0000_0000_0000_u64,
        ];
        let bits = |v: &Value| {
            v["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap().to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&response), expected);
        let serialized = serde_json::to_vec(&response).unwrap();
        let reparsed: Value = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(bits(&reparsed), expected);
        worker.shutdown();
    }

    fn session() -> WorkerSession {
        let mut config = WorkerConfig::new(std::env::current_exe().unwrap(), "--quiet");
        config.args = [
            "--ignored",
            "--exact",
            "tests::worker_fixture",
            "--nocapture",
        ]
        .map(OsString::from)
        .to_vec();
        config.response_timeout = Duration::from_secs(2);
        WorkerSession::spawn(&config).unwrap()
    }

    // Run only as a child process. This is a transport fixture, never geometry.
    #[test]
    #[ignore]
    fn worker_fixture() {
        println!("FreeCAD startup diagnostic");
        io::stdout().flush().unwrap();
        let mut count = 0;
        let mut sequence = 0;
        let mut fixture_revision = 0;
        let mut shutdown_config = Value::Null;
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        while let Some(line) = lines.next() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            count += 1;
            let mut response = json!({"version":1,"id":request["id"],"ok":true,"result":{"count":count,"request":request}});
            match request["op"].as_str().unwrap() {
                "journal_roundtrip"
                | "journal_bad_id"
                | "journal_bad_sequence"
                | "journal_bad_version"
                | "journal_delay"
                | "journal_stalled_replay" => {
                    for (action, extra) in [
                        ("begin", json!({})),
                        (
                            "stage_accept",
                            json!({"group":{"edits":[{"op":"datum","value":20}]}}),
                        ),
                        ("resolve", json!({"accepted":true})),
                        ("replay", json!({})),
                        ("end", json!({})),
                    ] {
                        sequence += 1;
                        let mut frame = json!({"kind":sketch_journal::CHANNEL,"version":1,
                            "id":request["id"],"sequence":sequence,"action":action,"document":"Fixture"});
                        frame
                            .as_object_mut()
                            .unwrap()
                            .extend(extra.as_object().unwrap().clone());
                        if request["op"] == "journal_stalled_replay" && action == "stage_accept" {
                            frame["group"]["edits"][0]["value"] = json!("x".repeat(512 * 1024));
                        }
                        match request["op"].as_str().unwrap() {
                            "journal_bad_id" => frame["id"] = json!(9999),
                            "journal_bad_sequence" => frame["sequence"] = json!(0),
                            "journal_bad_version" => frame["version"] = json!(2),
                            _ => (),
                        }
                        println!("{frame}");
                        io::stdout().flush().unwrap();
                        if request["op"] == "journal_stalled_replay" && action == "replay" {
                            std::thread::sleep(Duration::from_secs(4));
                        }
                        let Some(reply) = lines.next() else { return };
                        let reply: Value = serde_json::from_str(&reply.unwrap()).unwrap();
                        assert_eq!(reply["ok"], true);
                        if action == "replay" {
                            response["result"]["journal"] = reply["result"].clone();
                        }
                        if request["op"] == "journal_delay" {
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
                "lease_fact_delay" => {
                    for fact in [None, Some(0)] {
                        sequence += 1;
                        let mut frame = json!({"kind":gesture_leases::CHANNEL,"version":1,
                            "id":request["id"],"sequence":sequence,"action":"preview_ready",
                            "document":"Fixture","request":{"op":"preview","token":"p"}});
                        if let Some(redo) = fact {
                            frame["redo"] = json!(redo);
                        }
                        println!("{frame}");
                        io::stdout().flush().unwrap();
                        let Some(reply) = lines.next() else { return };
                        let reply: Value = serde_json::from_str(&reply.unwrap()).unwrap();
                        assert_eq!(reply["ok"], true);
                        assert_eq!(reply["result"]["needs_redo"], fact.is_none());
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                "revision_roundtrip"
                | "revision_bad_id"
                | "revision_bad_sequence"
                | "revision_bad_version"
                | "revision_delay"
                | "revision_unknown_channel"
                | "lease_roundtrip"
                | "lease_bad_id"
                | "lease_bad_sequence"
                | "lease_bad_version"
                | "lease_delay" => {
                    fixture_revision += 1;
                    let mut replies = Vec::new();
                    for (channel, action, extra) in [
                        (sketch_journal::CHANNEL, "begin", json!({})),
                        (revision_registry::CHANNEL, "lookup", json!({})),
                        (revision_registry::CHANNEL, "peek", json!({})),
                        (revision_registry::CHANNEL, "bump", json!({})),
                        (
                            revision_registry::CHANNEL,
                            "check",
                            json!({"expected_revision":fixture_revision}),
                        ),
                        (gesture_leases::CHANNEL, "metadata", json!({})),
                        (sketch_journal::CHANNEL, "end", json!({})),
                        (revision_registry::CHANNEL, "close", json!({})),
                    ] {
                        sequence += 1;
                        let mut frame = json!({"kind":channel,"version":1,"id":request["id"],
                            "sequence":sequence,"action":action,"document":"Fixture"});
                        frame
                            .as_object_mut()
                            .unwrap()
                            .extend(extra.as_object().unwrap().clone());
                        // Reach the revision channel before injecting transport faults.
                        if (channel == revision_registry::CHANNEL
                            && !request["op"].as_str().unwrap().starts_with("lease_"))
                            || (channel == gesture_leases::CHANNEL
                                && request["op"].as_str().unwrap().starts_with("lease_"))
                        {
                            match request["op"].as_str().unwrap() {
                                "revision_bad_id" | "lease_bad_id" => frame["id"] = json!(9999),
                                "revision_bad_sequence" | "lease_bad_sequence" => {
                                    frame["sequence"] = json!(0)
                                }
                                "revision_bad_version" | "lease_bad_version" => {
                                    frame["version"] = json!(2)
                                }
                                "revision_unknown_channel" => {
                                    frame["kind"] = json!("unknown-callback")
                                }
                                _ => (),
                            }
                        }
                        println!("{frame}");
                        io::stdout().flush().unwrap();
                        let Some(reply) = lines.next() else { return };
                        let reply: Value = serde_json::from_str(&reply.unwrap()).unwrap();
                        assert_eq!(reply["ok"], true);
                        replies.push(reply["result"].clone());
                        if (channel == revision_registry::CHANNEL
                            && request["op"] == "revision_delay")
                            || (channel == gesture_leases::CHANNEL
                                && request["op"] == "lease_delay")
                        {
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                    response["result"]["callbacks"] = json!(replies);
                }
                "shutdown_config" => shutdown_config = request.clone(),
                "crash" => std::process::exit(17),
                #[cfg(target_os = "linux")]
                // This fixture intentionally leaves a live descendant for the
                // host's private-process-group termination test.
                #[allow(clippy::zombie_processes)]
                "spawn_descendant" => {
                    let descendant = Command::new("sleep")
                        .arg("30")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .spawn()
                        .unwrap();
                    response["result"]["descendant"] = json!(descendant.id());
                }
                "eof" => return,
                "malformed" => {
                    println!("{{not valid json}}");
                    io::stdout().flush().unwrap();
                    continue;
                }
                "wrong_version" => response["version"] = json!(2),
                "wrong_id" => response["id"] = json!(9999),
                "scalar" => response = Value::Null,
                "wrong_error" => {
                    response = json!({"version":1,"id":request["id"],"ok":false,"error":{"message":"missing code"}})
                }
                "remote_error" => {
                    response = json!({"version":1,"id":request["id"],"ok":false,"error":{"code":"invalid_shape","message":"transaction aborted"}})
                }
                "delay" => std::thread::sleep(Duration::from_secs(1)),
                _ => (),
            }
            println!("{response}");
            io::stdout().flush().unwrap();
            if request["op"] == "stall_stdin" {
                std::thread::sleep(Duration::from_secs(4));
            }
        }
        if let Some(marker) = shutdown_config["marker"].as_str() {
            std::fs::write(marker, b"stdin EOF observed").unwrap();
        }
        for _ in 0..shutdown_config["lines"].as_u64().unwrap_or(0) {
            println!("worker shutdown diagnostic");
        }
        io::stdout().flush().unwrap();
        if let Some(ms) = shutdown_config["delay_ms"].as_u64() {
            std::thread::sleep(Duration::from_millis(ms));
        }
        if let Some(code) = shutdown_config["exit_code"].as_i64() {
            std::process::exit(i32::try_from(code).unwrap());
        }
    }

    #[cfg(not(unix))]
    #[test]
    fn graceful_shutdown_waits_for_eof_and_reports_child_exit() {
        let mut worker = session();
        worker
            .call(
                "shutdown_config",
                json!({"lines": 2048}).as_object().unwrap().clone(),
            )
            .unwrap();
        assert!(
            worker
                .shutdown_gracefully(Duration::from_secs(2))
                .unwrap()
                .success()
        );
        assert!(worker.child.try_wait().unwrap().unwrap().success());
        assert!(matches!(
            worker.call("inspect", Map::new()),
            Err(Error::Closed)
        ));
    }

    #[cfg(not(unix))]
    #[test]
    fn graceful_shutdown_preserves_nonzero_exit_status() {
        let mut worker = session();
        worker
            .call(
                "shutdown_config",
                json!({"exit_code": 7}).as_object().unwrap().clone(),
            )
            .unwrap();
        assert_eq!(
            worker
                .shutdown_gracefully(Duration::from_secs(2))
                .unwrap()
                .code(),
            Some(7)
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn graceful_shutdown_timeout_forces_child_without_claiming_success() {
        let mut worker = session();
        worker
            .call(
                "shutdown_config",
                json!({"delay_ms": 4000}).as_object().unwrap().clone(),
            )
            .unwrap();
        assert!(matches!(
            worker.shutdown_gracefully(Duration::from_millis(50)),
            Err(Error::Timeout(_))
        ));
        assert!(worker.child.try_wait().unwrap().is_some());
        assert!(matches!(
            worker.shutdown_gracefully(Duration::from_secs(1)),
            Err(Error::Closed)
        ));
    }

    #[cfg(not(unix))]
    #[test]
    fn graceful_shutdown_invalid_deadline_leaves_transport_usable() {
        let mut worker = session();
        assert!(matches!(
            worker.shutdown_gracefully(Duration::ZERO),
            Err(Error::InvalidRequest(_))
        ));
        assert!(matches!(
            worker.shutdown_gracefully(Duration::MAX),
            Err(Error::InvalidRequest(_))
        ));
        assert!(worker.call("inspect", Map::new()).is_ok());
        worker.shutdown();
        assert!(matches!(
            worker.shutdown_gracefully(Duration::from_secs(1)),
            Err(Error::Closed)
        ));
    }

    #[cfg(not(unix))]
    #[test]
    fn healthy_drop_delivers_stdin_eof() {
        let marker =
            std::env::temp_dir().join(format!("cad-worker-eof-{}.txt", std::process::id()));
        assert!(!marker.exists());
        let mut worker = session();
        worker
            .call(
                "shutdown_config",
                json!({"marker": marker}).as_object().unwrap().clone(),
            )
            .unwrap();
        drop(worker);
        assert_eq!(std::fs::read(&marker).unwrap(), b"stdin EOF observed");
        std::fs::remove_file(marker).unwrap();
    }

    #[cfg(not(unix))]
    #[test]
    #[ignore = "requires the pinned native runtime and private profile"]
    fn native_graceful_shutdown_observes_real_eof_and_exit() {
        let runtime: Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("CAD_GRACEFUL_RUNTIME").unwrap()).unwrap(),
        )
        .unwrap();
        let mut config = WorkerConfig::new(
            runtime["executable"].as_str().unwrap(),
            runtime["script"].as_str().unwrap(),
        );
        config.args = runtime["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| OsString::from(v.as_str().unwrap()))
            .collect();
        let mut worker = WorkerSession::spawn(&config).unwrap();
        let state = worker
            .call(
                "new",
                json!({"name":"GracefulUnsaved"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
        assert_eq!(state["document"], "GracefulUnsaved");
        let status = worker.shutdown_gracefully(Duration::from_secs(10)).unwrap();
        assert_eq!(status.code(), Some(0));
        assert!(worker.child.try_wait().unwrap().unwrap().success());
        assert!(matches!(
            worker.output.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn unix_graceful_refusal_preserves_the_live_transport() {
        let mut worker = session();
        assert!(matches!(
            worker.shutdown_gracefully(Duration::from_secs(1)),
            Err(Error::InvalidRequest(_))
        ));
        assert!(worker.call("inspect", Map::new()).is_ok());
    }

    #[test]
    fn persistent_calls_preserve_parameters_and_caller_ids() {
        let mut session = session();
        assert_eq!(session.capabilities()["count"], 1);
        let response = session.request(json!({"id":-7,"op":"transaction","document":"Part","edits":[{"kind":"circle","radius":3.25}]})).unwrap();
        assert_eq!(response["id"], -7);
        assert_eq!(response["result"]["count"], 2);
        assert_eq!(response["result"]["request"]["edits"][0]["radius"], 3.25);
        assert_eq!(session.call("inspect", Map::new()).unwrap()["count"], 3);
    }

    #[test]
    fn remote_errors_and_invalid_requests_leave_session_usable() {
        let mut session = session();
        assert!(
            matches!(session.call("remote_error", Map::new()), Err(Error::Remote { code, .. }) if code == "invalid_shape")
        );
        assert!(matches!(
            session.request(json!({"op":"inspect","version":2})),
            Err(Error::InvalidRequest(_))
        ));
        assert!(matches!(
            session.request(json!({"op":"inspect","id":1.5})),
            Err(Error::InvalidRequest(_))
        ));
        assert_eq!(session.call("inspect", Map::new()).unwrap()["count"], 3);
    }

    #[test]
    fn protocol_failures_close_the_session() {
        for op in [
            "malformed",
            "wrong_version",
            "wrong_id",
            "wrong_error",
            "scalar",
        ] {
            let mut session = session();
            assert!(
                matches!(session.call(op, Map::new()), Err(Error::Protocol(_))),
                "{op}"
            );
            assert!(matches!(
                session.call("inspect", Map::new()),
                Err(Error::Closed)
            ));
        }
    }

    #[test]
    fn journal_callbacks_are_private_correlated_and_keep_the_outer_response() {
        let mut session = session();
        for _ in 0..2 {
            let response = session
                .request(json!({"id":-91,"op":"journal_roundtrip"}))
                .unwrap();
            assert_eq!(response["id"], -91);
            assert_eq!(
                response["result"]["journal"],
                json!([{"edits":[{"op":"datum","value":20}]}])
            );
        }
        assert!(session.call("inspect", Map::new()).is_ok());
    }

    #[test]
    fn journal_protocol_errors_and_callback_timeout_close_the_session() {
        for op in [
            "journal_bad_id",
            "journal_bad_version",
            "journal_bad_sequence",
        ] {
            let mut session = session();
            assert!(
                matches!(session.call(op, Map::new()), Err(Error::Protocol(_))),
                "{op}"
            );
            assert!(matches!(
                session.call("inspect", Map::new()),
                Err(Error::Closed)
            ));
            assert!(session.child.try_wait().unwrap().is_some());
        }
        let mut session = session();
        session.timeout = Duration::from_millis(30);
        assert!(matches!(
            session.call("journal_delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert!(session.child.try_wait().unwrap().is_some());
    }

    #[test]
    fn shared_callback_sequence_routes_journal_and_revision_state() {
        let mut session = session();
        session.call("journal_roundtrip", Map::new()).unwrap();
        for revision in 1..=2 {
            let result = session.call("revision_roundtrip", Map::new()).unwrap();
            assert_eq!(
                result["callbacks"],
                json!([null, null, 0, revision, revision, {"preview":null,"sketch_edit":null,"available":true}, null, null])
            );
        }
        session.call("journal_roundtrip", Map::new()).unwrap();
        assert!(session.call("inspect", Map::new()).is_ok());
    }

    #[test]
    fn revision_callback_correlation_and_deadline_fail_closed() {
        for op in [
            "revision_bad_id",
            "revision_bad_version",
            "revision_bad_sequence",
            "revision_unknown_channel",
            "lease_bad_id",
            "lease_bad_version",
            "lease_bad_sequence",
        ] {
            let mut session = session();
            assert!(
                matches!(session.call(op, Map::new()), Err(Error::Protocol(_))),
                "{op}"
            );
            assert!(matches!(
                session.call("inspect", Map::new()),
                Err(Error::Closed)
            ));
            assert!(session.child.try_wait().unwrap().is_some());
        }
        let mut session = session();
        session.timeout = Duration::from_millis(30);
        assert!(matches!(
            session.call("revision_delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert!(session.child.try_wait().unwrap().is_some());
    }

    #[test]
    fn lease_callback_keeps_outer_integer_id_and_original_deadline() {
        for id in [json!(i64::MIN), json!(u64::MAX)] {
            let mut session = session();
            let response = session
                .request(json!({"id":id,"op":"lease_roundtrip"}))
                .unwrap();
            assert_eq!(response["id"], id);
            assert_eq!(session.callback_sequence, 8);
        }
        let mut session = session();
        session.timeout = Duration::from_millis(30);
        assert!(matches!(
            session.call("lease_delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert!(session.child.try_wait().unwrap().is_some());
        assert!(matches!(
            session.call("inspect", Map::new()),
            Err(Error::Closed)
        ));
    }

    #[test]
    fn lazy_preview_fact_handshake_keeps_the_original_request_deadline() {
        let mut session = session();
        session.timeout = Duration::from_millis(150);
        assert!(matches!(
            session.call("lease_fact_delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert_eq!(
            session.callback_sequence, 2,
            "Both demand and fact replies used the same request"
        );
        assert!(session.child.try_wait().unwrap().is_some());
        assert!(matches!(
            session.call("inspect", Map::new()),
            Err(Error::Closed)
        ));
    }

    #[test]
    fn worker_crash_and_clean_eof_are_errors() {
        for op in ["crash", "eof"] {
            let mut session = session();
            assert!(
                matches!(session.call(op, Map::new()), Err(Error::WorkerExited(_))),
                "{op}"
            );
            assert!(matches!(
                session.call("inspect", Map::new()),
                Err(Error::Closed)
            ));
        }
    }

    #[test]
    fn stalled_large_replay_write_times_out_and_reaps_the_worker() {
        let mut session = session();
        session.timeout = Duration::from_millis(300);
        let start = Instant::now();
        assert!(matches!(
            session.call("journal_stalled_replay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert_eq!(
            session.callback_sequence, 4,
            "The replay callback reached its blocked write"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(session.child.try_wait().unwrap().is_some());
        assert!(matches!(
            session.call("inspect", Map::new()),
            Err(Error::Closed)
        ));
    }

    #[test]
    fn stalled_large_outer_request_write_uses_the_same_deadline() {
        let mut session = session();
        session.call("stall_stdin", Map::new()).unwrap();
        session.timeout = Duration::from_millis(300);
        let start = Instant::now();
        assert!(matches!(
            session.request(json!({"op":"inspect", "payload":"x".repeat(512 * 1024)})),
            Err(Error::Timeout(_))
        ));
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(session.child.try_wait().unwrap().is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn timeout_terminates_launcher_descendants_in_its_private_group() {
        let mut session = session();
        let pid = session.call("spawn_descendant", Map::new()).unwrap()["descendant"]
            .as_u64()
            .unwrap();
        session.timeout = Duration::from_millis(30);
        assert!(matches!(
            session.call("delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        let path = format!("/proc/{pid}/stat");
        let stopped = || {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|stat| {
                    stat.split_once(") ")
                        .map(|(_, rest)| rest.starts_with('Z') || rest.starts_with('X'))
                })
                .unwrap_or(true)
        };
        let start = Instant::now();
        while !stopped() && start.elapsed() < Duration::from_millis(500) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            stopped(),
            "The native descendant must not outlive a timed-out launcher"
        );
    }

    #[test]
    fn timeout_kills_worker_and_reports_uncertain_outcome() {
        let mut session = session();
        session.timeout = Duration::from_millis(30);
        assert!(matches!(
            session.call("delay", Map::new()),
            Err(Error::Timeout(_))
        ));
        assert!(session.child.try_wait().unwrap().is_some());
        assert!(matches!(
            session.call("inspect", Map::new()),
            Err(Error::Closed)
        ));
    }
}
