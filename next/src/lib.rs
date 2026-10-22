//! Persistent local modeling session. Native documents stay inside the worker.

pub mod body_commands;
#[cfg(feature = "desktop")]
pub mod editor;
pub mod feature_commands;
pub mod history_commands;
pub mod packages;
pub mod property_commands;
pub mod sketch_commands;
pub mod sketch_curve_commands;
pub mod sketch_operation_commands;
pub mod transform_commands;
#[cfg(feature = "desktop")]
pub mod viewport;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
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

/// One worker, one authoritative native document session, one request at a time.
/// A failed transaction is a normal remote error and leaves the session usable.
/// Transport/protocol failures terminate the child because mutation outcome may
/// be unknown. Callers must reopen the last saved native document explicitly.
pub struct WorkerSession {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<io::Result<String>>,
    timeout: Duration,
    next_id: u64,
    closed: bool,
    capabilities: Value,
}

impl WorkerSession {
    pub fn spawn(config: &WorkerConfig) -> Result<Self, Error> {
        if config.response_timeout.is_zero() {
            return Err(Error::InvalidRequest(
                "response timeout must be positive".into(),
            ));
        }
        let mut child = Command::new(&config.executable)
            .args(&config.args)
            .arg(&config.script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().expect("piped worker stdin");
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
            input: Some(input),
            output: receiver,
            timeout: config.response_timeout,
            next_id: 1,
            closed: false,
            capabilities: Value::Null,
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

    fn exchange(&mut self, request: &Value, id: &Value) -> Result<Value, Error> {
        let input = self.input.as_mut().ok_or(Error::Closed)?;
        serde_json::to_writer(&mut *input, request).map_err(io::Error::other)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let start = Instant::now();
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
            validate_response(&response, id)?;
            return Ok(response);
        }
    }

    /// Stop the child. Does not save native documents implicitly.
    pub fn shutdown(&mut self) {
        self.closed = true;
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        self.shutdown();
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
        for line in io::stdin().lock().lines() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            count += 1;
            let mut response = json!({"version":1,"id":request["id"],"ok":true,"result":{"count":count,"request":request}});
            match request["op"].as_str().unwrap() {
                "crash" => std::process::exit(17),
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
        }
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
