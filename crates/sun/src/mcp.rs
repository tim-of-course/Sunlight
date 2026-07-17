//! Persistent local MCP transport over the shared in-process command engine.
//!
//! This module owns protocol framing and typed argument validation, but no
//! repository semantics. Every tool calls the same explicit, repository-rooted
//! engine boundary as the CLI.

use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::{execute_engine, EngineCommandInput, EngineContext, EngineOutputFormat, EngineRequest};

const PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;

pub(crate) fn serve_from_args(args: &[String]) -> Result<(), String> {
    if args == ["mcp", "--help"] || args == ["mcp", "serve", "--help"] {
        println!(
            "sun mcp serve\n\nUsage:\n  sun mcp serve --repo <initialized-repo>\n\nRuns newline-delimited MCP JSON-RPC 2.0 on stdio. Protocol messages use stdout; diagnostics use stderr."
        );
        return Ok(());
    }
    if args.len() != 4 || args[0] != "mcp" || args[1] != "serve" || args[2] != "--repo" {
        return Err("usage: sun mcp serve --repo <initialized-repo>".to_string());
    }
    let requested = PathBuf::from(&args[3]);
    let repo = fs::canonicalize(&requested).map_err(|error| {
        format!(
            "cannot canonicalize repository `{}`: {error}",
            requested.display()
        )
    })?;
    if !repo.is_dir() {
        return Err(format!(
            "repository `{}` is not a directory",
            repo.display()
        ));
    }
    let engine = EngineContext::new(&repo)?;
    let temp = PrivateTemp::new(&repo)?;
    serve(repo, engine, temp)
}

struct PrivateTemp {
    root: PathBuf,
    next: AtomicU64,
}

impl PrivateTemp {
    fn new(repo: &Path) -> Result<Arc<Self>, String> {
        let parent = repo.join(".sunlight/local/mcp");
        fs::create_dir_all(&parent)
            .map_err(|error| format!("cannot create MCP temp parent: {error}"))?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("cannot create MCP temp nonce: {error}"))?
            .as_nanos();
        let root = parent.join(format!("server-{}-{nonce}", std::process::id()));
        fs::create_dir(&root)
            .map_err(|error| format!("cannot create exclusive MCP temp directory: {error}"))?;
        Ok(Arc::new(Self {
            root,
            next: AtomicU64::new(1),
        }))
    }

    fn stage(&self, kind: &str, content: &str) -> Result<StagedFile, ToolFailure> {
        if content.len() > MAX_CONTENT_BYTES {
            return Err(ToolFailure::new(
                "mcp_content_too_large",
                format!("{kind} exceeds the {MAX_CONTENT_BYTES}-byte limit"),
            ));
        }
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let path = self.root.join(format!("{kind}-{number}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&path).map_err(|error| {
            ToolFailure::new("mcp_temp_io", format!("cannot stage {kind}: {error}"))
        })?;
        file.write_all(content.as_bytes()).map_err(|error| {
            ToolFailure::new("mcp_temp_io", format!("cannot stage {kind}: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            ToolFailure::new(
                "mcp_temp_io",
                format!("cannot flush staged {kind}: {error}"),
            )
        })?;
        Ok(StagedFile { path })
    }
}

impl Drop for PrivateTemp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct StagedFile {
    path: PathBuf,
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

enum Inbound {
    Message(Value),
    Malformed(String),
    TooLarge,
    Eof,
}

struct ActiveCall {
    id: Value,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Value>,
    join: Option<thread::JoinHandle<()>>,
}

impl Drop for ActiveCall {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn serve(repo: PathBuf, engine: EngineContext, temp: Arc<PrivateTemp>) -> Result<(), String> {
    let (input_tx, input_rx) = mpsc::channel();
    let reader = thread::spawn(move || read_input(io::stdin().lock(), input_tx));
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut pending = VecDeque::new();
    let mut active: Option<ActiveCall> = None;
    let mut initialized = false;
    let mut input_closed = false;

    loop {
        if let Some(call) = active.as_ref() {
            match call.result.try_recv() {
                Ok(result) => {
                    let mut call = active.take().expect("active call exists");
                    write_message(&mut stdout, rpc_result(call.id.clone(), result))?;
                    if let Some(join) = call.join.take() {
                        let _ = join.join();
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let mut call = active.take().expect("active call exists");
                    write_message(
                        &mut stdout,
                        rpc_error(call.id.clone(), -32603, "tool worker terminated", None),
                    )?;
                    if let Some(join) = call.join.take() {
                        let _ = join.join();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if active.is_none() {
            if let Some(message) = pending.pop_front() {
                handle_message(
                    message,
                    &repo,
                    &engine,
                    &temp,
                    &mut initialized,
                    &mut active,
                    &mut stdout,
                )?;
                continue;
            }
            if input_closed {
                break;
            }
        }

        match input_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Inbound::Message(message)) => {
                if is_cancel_notification(&message) {
                    let requested = cancelled_id(&message).cloned();
                    if let Some(call) = active.as_ref() {
                        if requested.as_ref() == Some(&call.id) {
                            call.cancel.store(true, Ordering::Release);
                        }
                    }
                    if let Some(requested) = requested {
                        let before = pending.len();
                        pending.retain(|queued| queued.get("id") != Some(&requested));
                        if pending.len() != before {
                            write_message(
                                &mut stdout,
                                rpc_error(requested, -32800, "request cancelled", None),
                            )?;
                        }
                    }
                } else if active.is_some()
                    && message.get("method").and_then(Value::as_str) == Some("tools/call")
                {
                    pending.push_back(message);
                } else {
                    handle_message(
                        message,
                        &repo,
                        &engine,
                        &temp,
                        &mut initialized,
                        &mut active,
                        &mut stdout,
                    )?;
                }
            }
            Ok(Inbound::Malformed(message)) => write_message(
                &mut stdout,
                rpc_error(
                    Value::Null,
                    -32700,
                    "parse error",
                    Some(json!({"detail": message})),
                ),
            )?,
            Ok(Inbound::TooLarge) => write_message(
                &mut stdout,
                rpc_error(
                    Value::Null,
                    -32600,
                    "request exceeds size limit",
                    Some(json!({"max_bytes": MAX_REQUEST_BYTES})),
                ),
            )?,
            Ok(Inbound::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                input_closed = true;
                pending.clear();
                if let Some(call) = active.as_ref() {
                    call.cancel.store(true, Ordering::Release);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }

    if let Some(call) = active {
        call.cancel.store(true, Ordering::Release);
    }
    let _ = reader.join();
    Ok(())
}

fn handle_message(
    message: Value,
    repo: &Path,
    engine: &EngineContext,
    temp: &Arc<PrivateTemp>,
    initialized: &mut bool,
    active: &mut Option<ActiveCall>,
    stdout: &mut impl Write,
) -> Result<(), String> {
    let Some(object) = message.as_object() else {
        return write_message(
            stdout,
            rpc_error(Value::Null, -32600, "invalid request", None),
        );
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return write_message(
            stdout,
            rpc_error(request_id(object), -32600, "invalid JSON-RPC version", None),
        );
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return write_message(
            stdout,
            rpc_error(
                request_id(object),
                -32600,
                "request method must be a string",
                None,
            ),
        );
    };
    let id = object.get("id").cloned();
    if let Some(id) = id.as_ref() {
        if !(id.is_string() || id.is_i64() || id.is_u64()) {
            return write_message(
                stdout,
                rpc_error(
                    Value::Null,
                    -32600,
                    "request id must be a string or integer",
                    None,
                ),
            );
        }
    }

    match method {
        "notifications/initialized" => {
            *initialized = true;
            Ok(())
        }
        "notifications/cancelled" => Ok(()),
        "initialize" => {
            let Some(id) = id else {
                return Ok(());
            };
            let requested = object
                .get("params")
                .and_then(|value| value.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            let negotiated = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
                requested
            } else {
                PROTOCOL_VERSION
            };
            write_message(
                stdout,
                rpc_result(
                    id,
                    json!({
                        "protocolVersion": negotiated,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {
                            "name": "sunlight-local",
                            "version": env!("CARGO_PKG_VERSION"),
                            "description": "Repository-confined Sunlight v0.3 authoring and operation tools"
                        },
                        "instructions": "All tools are bound to the canonical repository supplied when this server started. Typical authoring lifecycle: repository_status, topic_create, session_start, artifact_read/search, artifact_patch/write, topic_complete, then topic_wait or view_resolve, execution_run, and checkpoint_create. artifact_read/list/search accept exactly one scope: session for the authoring frontier or view for session-free read-only access to an exact resolved view. topic_complete and completed topic status return a structured handoff with summary, operations, changed paths, and hashes; use topic_wait instead of polling when another agent owns the topic. Transient repository writer and state-sequence races are retried automatically for safe native and read commands; commands that can replay external side effects are never automatically retried. Use exact IDs and sha256 hashes returned by tools. artifact_write uses expect_hash \"new\" only when the path must be absent. Sessions have fixed topic scope: session_refresh advances only non-write topics already in that session frontier and does not discover newly created topics. execution_run returns bounded stdout/stderr text in that response, phase timings, and only source-like or explicitly generated promotion candidates. No fixture tools or arbitrary host paths are available."
                    }),
                ),
            )
        }
        "ping" => {
            if let Some(id) = id {
                write_message(stdout, rpc_result(id, json!({})))
            } else {
                Ok(())
            }
        }
        "tools/list" => {
            let Some(id) = id else {
                return Ok(());
            };
            write_message(stdout, rpc_result(id, json!({"tools": tools()})))
        }
        "tools/call" => {
            let Some(id) = id else {
                return Ok(());
            };
            if !*initialized {
                return write_message(
                    stdout,
                    rpc_error(id, -32002, "server is not initialized", None),
                );
            }
            let params = object.get("params").and_then(Value::as_object);
            let Some(name) = params
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
            else {
                return write_message(
                    stdout,
                    rpc_error(id, -32602, "tools/call requires a tool name", None),
                );
            };
            if !tool_names().contains(&name) {
                return write_message(
                    stdout,
                    rpc_error(id, -32602, "unknown tool", Some(json!({"name": name}))),
                );
            }
            let arguments = params
                .and_then(|params| params.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return write_message(
                    stdout,
                    rpc_result(
                        id,
                        tool_failure_result(ToolFailure::new(
                            "invalid_request",
                            "tool arguments must be an object",
                        )),
                    ),
                );
            }
            let repo = repo.to_path_buf();
            let engine = engine.clone();
            let name = name.to_string();
            let temp = Arc::clone(temp);
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            let (tx, rx) = mpsc::channel();
            let join = thread::spawn(move || {
                let result = if name == "topic_wait" {
                    execute_topic_wait(&engine, &arguments, &worker_cancel)
                } else {
                    match build_invocation(&name, &arguments, &repo, &temp) {
                        Ok(invocation) => execute_invocation(&engine, invocation, &worker_cancel),
                        Err(error) => tool_failure_result(error),
                    }
                };
                let _ = tx.send(result);
            });
            *active = Some(ActiveCall {
                id,
                cancel,
                result: rx,
                join: Some(join),
            });
            Ok(())
        }
        _ => {
            if let Some(id) = id {
                write_message(
                    stdout,
                    rpc_error(
                        id,
                        -32601,
                        "method not found",
                        Some(json!({"method": method})),
                    ),
                )
            } else {
                Ok(())
            }
        }
    }
}

fn read_input(stdin: impl BufRead, tx: mpsc::Sender<Inbound>) {
    let mut reader = stdin;
    loop {
        let line = match read_line_bounded(&mut reader, MAX_REQUEST_BYTES) {
            Ok(Some(line)) => line,
            Ok(None) => {
                let _ = tx.send(Inbound::Eof);
                return;
            }
            Err(ReadFrameError::TooLarge) => {
                if tx.send(Inbound::TooLarge).is_err() {
                    return;
                }
                continue;
            }
            Err(ReadFrameError::Io(error)) => {
                let _ = tx.send(Inbound::Malformed(error.to_string()));
                let _ = tx.send(Inbound::Eof);
                return;
            }
        };
        let mut trimmed = line.as_slice();
        while trimmed.ends_with(b"\n") || trimmed.ends_with(b"\r") {
            trimmed = &trimmed[..trimmed.len() - 1];
        }
        if trimmed.is_empty() {
            continue;
        }
        let header_text = String::from_utf8_lossy(trimmed);
        let bytes = if header_text
            .to_ascii_lowercase()
            .starts_with("content-length:")
        {
            let Some(length) = header_text
                .split_once(':')
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            else {
                if tx
                    .send(Inbound::Malformed(
                        "invalid Content-Length header".to_string(),
                    ))
                    .is_err()
                {
                    return;
                }
                continue;
            };
            loop {
                match read_line_bounded(&mut reader, 8192) {
                    Ok(Some(header)) if header == b"\n" || header == b"\r\n" => break,
                    Ok(Some(_)) => continue,
                    _ => {
                        let _ = tx.send(Inbound::Eof);
                        return;
                    }
                }
            }
            if length > MAX_REQUEST_BYTES {
                let mut limited = (&mut reader).take(length as u64);
                let _ = io::copy(&mut limited, &mut io::sink());
                if tx.send(Inbound::TooLarge).is_err() {
                    return;
                }
                continue;
            }
            let mut body = vec![0; length];
            if let Err(error) = reader.read_exact(&mut body) {
                let _ = tx.send(Inbound::Malformed(error.to_string()));
                let _ = tx.send(Inbound::Eof);
                return;
            }
            body
        } else {
            trimmed.to_vec()
        };
        match serde_json::from_slice(&bytes) {
            Ok(value) => {
                if tx.send(Inbound::Message(value)).is_err() {
                    return;
                }
            }
            Err(error) => {
                if tx.send(Inbound::Malformed(error.to_string())).is_err() {
                    return;
                }
            }
        }
    }
}

enum ReadFrameError {
    TooLarge,
    Io(io::Error),
}

fn read_line_bounded(
    reader: &mut impl BufRead,
    limit: usize,
) -> Result<Option<Vec<u8>>, ReadFrameError> {
    let mut output = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(ReadFrameError::Io)?;
        if available.is_empty() {
            return Ok((!output.is_empty()).then_some(output));
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let ends_line = available[..take].ends_with(b"\n");
        if output.len().saturating_add(take) > limit {
            reader.consume(take);
            while !ends_line {
                let chunk = reader.fill_buf().map_err(ReadFrameError::Io)?;
                if chunk.is_empty() {
                    break;
                }
                let amount = chunk
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(chunk.len(), |index| index + 1);
                let done = chunk[..amount].ends_with(b"\n");
                reader.consume(amount);
                if done {
                    break;
                }
            }
            return Err(ReadFrameError::TooLarge);
        }
        output.extend_from_slice(&available[..take]);
        reader.consume(take);
        if ends_line {
            return Ok(Some(output));
        }
    }
}

fn write_message(writer: &mut impl Write, value: Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, &value).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code":code,"message":message});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc":"2.0","id":id,"error":error})
}
fn request_id(object: &Map<String, Value>) -> Value {
    object.get("id").cloned().unwrap_or(Value::Null)
}
fn is_cancel_notification(value: &Value) -> bool {
    value.get("method").and_then(Value::as_str) == Some("notifications/cancelled")
}
fn cancelled_id(value: &Value) -> Option<&Value> {
    value.get("params")?.get("requestId")
}

struct Invocation {
    argv: Vec<String>,
    staged: Vec<StagedFile>,
}

#[derive(Debug)]
struct ToolFailure {
    code: &'static str,
    message: String,
    details: Value,
}

impl ToolFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: json!({}),
        }
    }
    fn detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.details[key] = value.into();
        self
    }
}

fn tool_failure_result(error: ToolFailure) -> Value {
    let envelope = json!({"ok":false,"error":{"code":error.code,"message":error.message,"details":error.details}});
    json!({"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope,"isError":true})
}

fn execute_invocation(
    engine: &EngineContext,
    invocation: Invocation,
    cancel: &Arc<AtomicBool>,
) -> Value {
    let Invocation {
        argv,
        staged: _staged,
    } = invocation;
    if cancel.load(Ordering::Acquire) {
        return tool_failure_result(ToolFailure::new(
            "request_cancelled",
            "tool call was cancelled",
        ));
    }
    let response = execute_engine(
        &engine.clone().with_cancellation(Arc::clone(cancel)),
        EngineRequest {
            command: EngineCommandInput::Arguments(argv),
            output_format: EngineOutputFormat::Json,
            max_stdout_bytes: Some(MAX_STDOUT_BYTES),
            max_stderr_bytes: Some(MAX_STDERR_BYTES),
        },
    );
    if response.stdout_overflowed {
        return tool_failure_result(
            ToolFailure::new(
                "mcp_stdout_too_large",
                "engine response exceeded the MCP response limit",
            )
            .detail("max_bytes", MAX_STDOUT_BYTES as u64),
        );
    }
    if response.stderr_overflowed {
        return tool_failure_result(
            ToolFailure::new(
                "mcp_stderr_too_large",
                "engine diagnostics exceeded the MCP diagnostic limit",
            )
            .detail("max_bytes", MAX_STDERR_BYTES as u64),
        );
    }
    let parsed: Value = match serde_json::from_str(&response.stdout) {
        Ok(value) => value,
        Err(error) => {
            return tool_failure_result(
                ToolFailure::new(
                    "mcp_invalid_engine_contract",
                    "command engine did not return one valid JSON contract",
                )
                .detail("source", error.to_string())
                .detail("stderr", response.stderr),
            );
        }
    };
    let is_error = !response.success || parsed.get("ok").and_then(Value::as_bool) == Some(false);
    json!({
        "content":[{"type":"text","text":parsed.to_string()}],
        "structuredContent":parsed,
        "isError":is_error
    })
}

fn tool_result_with_wait(mut result: Value, outcome: &str, elapsed_ms: u128) -> Value {
    let Some(mut envelope) = result.get("structuredContent").cloned() else {
        return result;
    };
    envelope["data"]["wait"] = json!({"outcome":outcome,"elapsed_ms":elapsed_ms});
    result["content"] = json!([{"type":"text","text":envelope.to_string()}]);
    result["structuredContent"] = envelope;
    result
}

fn execute_topic_wait(engine: &EngineContext, value: &Value, cancel: &Arc<AtomicBool>) -> Value {
    let Some(args) = value.as_object() else {
        return tool_failure_result(ToolFailure::new(
            "invalid_request",
            "tool arguments must be an object",
        ));
    };
    if let Err(error) = reject_unknown(args, allowed_fields("topic_wait")) {
        return tool_failure_result(error);
    }
    let topic = match identifier(args, "topic") {
        Ok(topic) => topic,
        Err(error) => return tool_failure_result(error),
    };
    let bounded_number = |name: &str, default: u64, min: u64, max: u64| {
        args.get(name)
            .map(|value| {
                value
                    .as_u64()
                    .filter(|value| (*value >= min) && (*value <= max))
                    .ok_or_else(|| {
                        ToolFailure::new(
                            "invalid_request",
                            format!("`{name}` must be an integer from {min} through {max}"),
                        )
                    })
            })
            .unwrap_or(Ok(default))
    };
    let timeout_ms = match bounded_number("timeout_ms", 300_000, 0, 900_000) {
        Ok(value) => value,
        Err(error) => return tool_failure_result(error),
    };
    let poll_interval_ms = match bounded_number("poll_interval_ms", 250, 50, 5_000) {
        Ok(value) => value,
        Err(error) => return tool_failure_result(error),
    };
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return tool_failure_result(ToolFailure::new(
                "request_cancelled",
                "tool call was cancelled",
            ));
        }
        let result = execute_invocation(
            engine,
            Invocation {
                argv: vec!["status".into(), "--topic".into(), topic.clone()],
                staged: Vec::new(),
            },
            cancel,
        );
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return result;
        }
        let completed = result
            .get("structuredContent")
            .and_then(|value| value.get("data"))
            .and_then(|value| value.get("topic"))
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            == Some("completed");
        if completed {
            return tool_result_with_wait(result, "completed", started.elapsed().as_millis());
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            return tool_result_with_wait(result, "timeout", started.elapsed().as_millis());
        }
        thread::sleep(
            Duration::from_millis(poll_interval_ms)
                .min(Duration::from_millis(timeout_ms).saturating_sub(started.elapsed())),
        );
    }
}

fn build_invocation(
    name: &str,
    value: &Value,
    repo: &Path,
    temp: &PrivateTemp,
) -> Result<Invocation, ToolFailure> {
    let args = value.as_object().expect("validated object");
    reject_unknown(args, allowed_fields(name))?;
    let mut staged = Vec::new();
    let mut argv = match name {
        "repository_init" => vec!["init".into(), "--repo".into(), repo.display().to_string()],
        "repository_status" => status_argv(args)?,
        "topic_create" => {
            let mut v = vec![
                "topic".into(),
                "create".into(),
                identifier(args, "slug")?,
                "--display-name".into(),
                text(args, "display_name")?,
            ];
            if args.contains_key("owner") {
                v.extend(["--owner".into(), identifier(args, "owner")?]);
            }
            if args.contains_key("visibility") {
                v.extend([
                    "--visibility".into(),
                    enumeration(args, "visibility", &["local", "private"])?,
                ]);
            }
            if let Some(criteria) = args.get("acceptance_criteria") {
                let criteria = criteria.as_array().ok_or_else(|| {
                    ToolFailure::new(
                        "invalid_request",
                        "`acceptance_criteria` must be an array of strings",
                    )
                })?;
                for criterion in criteria {
                    let criterion = criterion.as_str().ok_or_else(|| {
                        ToolFailure::new(
                            "invalid_request",
                            "`acceptance_criteria` entries must be strings",
                        )
                    })?;
                    v.extend(["--acceptance-criterion".into(), criterion.to_string()]);
                }
            }
            v
        }
        "topic_complete" => {
            let mut v = vec![
                "topic".into(),
                "complete".into(),
                "--topic".into(),
                identifier(args, "topic")?,
                "--revision".into(),
                identifier(args, "revision")?,
                "--session".into(),
                identifier(args, "session")?,
            ];
            if args.contains_key("summary") {
                v.extend(["--summary".into(), text(args, "summary")?]);
            }
            v
        }
        "session_start" => vec![
            "session".into(),
            "start".into(),
            "--topic".into(),
            identifier(args, "topic")?,
            "--view".into(),
            identifier(args, "view")?,
            "--actor".into(),
            identifier(args, "actor")?,
        ],
        "session_refresh" => vec![
            "session".into(),
            "refresh".into(),
            identifier(args, "session")?,
            "--policy".into(),
            enumeration(args, "policy", &["manual", "follow", "none"])?,
        ],
        "artifact_read" => {
            let mut v = vec!["read".into(), artifact_path(args, "path", false)?];
            v.extend(artifact_read_scope_argv(args)?);
            v
        }
        "artifact_list" => {
            let mut v = vec!["list".into()];
            if let Some(path) = optional_artifact_path(args, "prefix", true)? {
                v.push(path)
            }
            v.extend(artifact_read_scope_argv(args)?);
            v
        }
        "artifact_search" => {
            let mut v = vec!["search".into(), text(args, "query")?];
            v.extend(artifact_read_scope_argv(args)?);
            v
        }
        "artifact_patch" => {
            let file = temp.stage("patch", required_string(args, "patch")?)?;
            let path = file.path.display().to_string();
            staged.push(file);
            vec![
                "patch".into(),
                artifact_path(args, "path", false)?,
                "--session".into(),
                identifier(args, "session")?,
                "--expect-hash".into(),
                expected_hash(args)?,
                "--patch-file".into(),
                path,
            ]
        }
        "artifact_write" => {
            let file = temp.stage("content", required_string(args, "content")?)?;
            let path = file.path.display().to_string();
            staged.push(file);
            vec![
                "write".into(),
                artifact_path(args, "path", false)?,
                "--session".into(),
                identifier(args, "session")?,
                "--expect-hash".into(),
                expected_hash(args)?,
                "--content-file".into(),
                path,
                "--classification".into(),
                classification(args)?,
            ]
        }
        "artifact_move" => vec![
            "move".into(),
            artifact_path(args, "from", false)?,
            artifact_path(args, "to", false)?,
            "--session".into(),
            identifier(args, "session")?,
            "--expect-hash".into(),
            expected_hash(args)?,
        ],
        "artifact_delete" => vec![
            "delete".into(),
            artifact_path(args, "path", false)?,
            "--session".into(),
            identifier(args, "session")?,
            "--expect-hash".into(),
            expected_hash(args)?,
        ],
        "artifact_metadata_set" => vec![
            "metadata".into(),
            "set".into(),
            artifact_path(args, "path", false)?,
            "--session".into(),
            identifier(args, "session")?,
            "--expect-hash".into(),
            expected_hash(args)?,
            "--classification".into(),
            classification(args)?,
        ],
        "view_resolve" => view_resolve_argv(args)?,
        "project_materialize" => project_argv(args)?,
        "compat_project" => vec![
            "compat".into(),
            "project".into(),
            "--session".into(),
            identifier(args, "session")?,
        ],
        "compat_diff" => vec![
            "compat".into(),
            "diff".into(),
            "--projection".into(),
            identifier(args, "projection")?,
        ],
        "compat_import" => compat_import_argv(args)?,
        "execution_run" => run_argv(args)?,
        "execution_promote_output" => vec![
            "execution".into(),
            "promote-output".into(),
            identifier(args, "execution")?,
            "--path".into(),
            artifact_path(args, "path", false)?,
            "--session".into(),
            identifier(args, "session")?,
            "--classification".into(),
            classification(args)?,
        ],
        "checkpoint_create" => {
            let mut argv = vec![
                "checkpoint".into(),
                "create".into(),
                "--view".into(),
                identifier(args, "view")?,
            ];
            if let Some(execution) = optional_identifier(args, "execution")? {
                argv.extend(["--execution".into(), execution]);
            }
            argv
        }
        "policy_check_export" => policy_export_argv(args)?,
        "policy_check_commit" => policy_commit_argv(args)?,
        "policy_explain" => vec![
            "policy".into(),
            "explain".into(),
            identifier(args, "validation_report")?,
        ],
        "git_export" => git_export_argv(args, repo)?,
        "inspect" => {
            let mut v = vec!["inspect".into(), selector(args)?];
            if let Some(session) = optional_identifier(args, "session")? {
                v.extend(["--session".into(), session])
            }
            v
        }
        _ => return Err(ToolFailure::new("unknown_tool", "unknown tool")),
    };
    if argv.first().map(String::as_str) == Some("mcp")
        || argv.iter().any(|argument| argument == "--fixture")
    {
        return Err(ToolFailure::new(
            "mcp_internal_contract",
            "generated command violated the MCP delegation boundary",
        ));
    }
    Ok(Invocation {
        argv: std::mem::take(&mut argv),
        staged,
    })
}

fn reject_unknown(args: &Map<String, Value>, allowed: &[&str]) -> Result<(), ToolFailure> {
    if let Some(key) = args.keys().find(|key| !allowed.contains(&key.as_str())) {
        Err(
            ToolFailure::new("invalid_request", format!("unknown argument `{key}`"))
                .detail("argument", key.clone()),
        )
    } else {
        Ok(())
    }
}
fn required_string<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, ToolFailure> {
    let value = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolFailure::new("invalid_request", format!("`{key}` must be a string")))?;
    if value.is_empty() || value.len() > MAX_CONTENT_BYTES || value.contains('\0') {
        return Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` is empty or exceeds its limit"),
        ));
    }
    Ok(value)
}
fn text(args: &Map<String, Value>, key: &str) -> Result<String, ToolFailure> {
    let v = required_string(args, key)?;
    if v.len() > 16 * 1024 {
        Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` is too long"),
        ))
    } else {
        Ok(v.to_string())
    }
}
fn identifier(args: &Map<String, Value>, key: &str) -> Result<String, ToolFailure> {
    let v = text(args, key)?;
    if v.starts_with('-') || v.chars().any(|c| c.is_control()) {
        Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` is not a safe identifier"),
        ))
    } else {
        Ok(v)
    }
}
fn optional_identifier(
    args: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ToolFailure> {
    match args.get(key) {
        None => Ok(None),
        Some(_) => identifier(args, key).map(Some),
    }
}
fn enumeration(
    args: &Map<String, Value>,
    key: &str,
    values: &[&str],
) -> Result<String, ToolFailure> {
    let v = required_string(args, key)?;
    if values.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` must be one of {}", values.join(", ")),
        ))
    }
}
fn classification(args: &Map<String, Value>) -> Result<String, ToolFailure> {
    enumeration(
        args,
        "classification",
        &[
            "source",
            "generated",
            "cache",
            "secret",
            "local-only",
            "execution-output",
            "lockfile",
            "migration",
            "binary",
            "vendored",
        ],
    )
}
fn expected_hash(args: &Map<String, Value>) -> Result<String, ToolFailure> {
    identifier(args, "expect_hash")
}
fn artifact_path(
    args: &Map<String, Value>,
    key: &str,
    allow_empty: bool,
) -> Result<String, ToolFailure> {
    validate_repo_relative(required_string(args, key)?, key, allow_empty)
}
fn optional_artifact_path(
    args: &Map<String, Value>,
    key: &str,
    allow_empty: bool,
) -> Result<Option<String>, ToolFailure> {
    match args.get(key) {
        None => Ok(None),
        Some(_) => artifact_path(args, key, allow_empty).map(Some),
    }
}
fn validate_repo_relative(
    value: &str,
    key: &str,
    allow_empty: bool,
) -> Result<String, ToolFailure> {
    if (!allow_empty && value.is_empty())
        || value.starts_with('-')
        || value.contains('\\')
        || value.contains(':')
        || Path::new(value).is_absolute()
    {
        return Err(ToolFailure::new(
            "path_outside_repository",
            format!("`{key}` must be a portable repository-relative path"),
        ));
    }
    if Path::new(value).components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ToolFailure::new(
            "path_outside_repository",
            format!("`{key}` may not escape the repository"),
        ));
    }
    Ok(value.to_string())
}
fn string_array(
    args: &Map<String, Value>,
    key: &str,
    max: usize,
) -> Result<Vec<String>, ToolFailure> {
    let values = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| ToolFailure::new("invalid_request", format!("`{key}` must be an array")))?;
    if values.len() > max {
        return Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` has too many entries"),
        ));
    }
    values
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| {
                    ToolFailure::new(
                        "invalid_request",
                        format!("`{key}` entries must be strings"),
                    )
                })
                .and_then(|s| {
                    if s.len() > 16 * 1024 || s.contains('\0') {
                        Err(ToolFailure::new(
                            "invalid_request",
                            format!("`{key}` entry is invalid"),
                        ))
                    } else {
                        Ok(s.to_string())
                    }
                })
        })
        .collect()
}

fn artifact_read_scope_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    match (args.contains_key("session"), args.contains_key("view")) {
        (true, false) => Ok(vec!["--session".into(), identifier(args, "session")?]),
        (false, true) => Ok(vec!["--view".into(), identifier(args, "view")?]),
        _ => Err(ToolFailure::new(
            "artifact_read_scope_invalid",
            "provide exactly one of `session` or `view`",
        )
        .detail("session_supplied", args.contains_key("session"))
        .detail("view_supplied", args.contains_key("view"))),
    }
}

fn status_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec!["status".into()];
    if let Some(scope_value) = args.get("scope") {
        let scope = scope_value
            .as_str()
            .ok_or_else(|| ToolFailure::new("invalid_request", "`scope` must be a string"))?;
        if scope == "repository" {
            if args.contains_key("id") {
                return Err(ToolFailure::new(
                    "invalid_request",
                    "repository status does not accept `id`",
                ));
            }
            return Ok(v);
        }
        let id = identifier(args, "id")?;
        let flag = match scope {
            "topic" => "--topic",
            "session" => "--session",
            "view" => "--view",
            "projection" => "--projection",
            "execution" => "--execution",
            "checkpoint" => "--checkpoint",
            "export" => "--export",
            "git" => "--git",
            "compat_import" => "--compat-import",
            _ => return Err(ToolFailure::new("invalid_request", "unknown status scope")),
        };
        v.extend([flag.into(), id]);
    } else if args.contains_key("id") {
        return Err(ToolFailure::new(
            "invalid_request",
            "`id` requires a non-repository status scope",
        ));
    }
    Ok(v)
}
fn selector(args: &Map<String, Value>) -> Result<String, ToolFailure> {
    let v = text(args, "selector")?;
    if v == "repository"
        || ([
            "topic:",
            "session:",
            "view:",
            "artifact:",
            "operation:",
            "conflict:",
            "projection:",
            "execution:",
            "checkpoint:",
            "export:",
            "git:",
        ]
        .iter()
        .any(|p| v.starts_with(p))
            && !v.contains("..")
            && !v.contains('\\')
            && !v.contains('\0'))
    {
        Ok(v)
    } else {
        Err(ToolFailure::new(
            "invalid_request",
            "selector must be repository or a supported typed selector",
        ))
    }
}
fn view_resolve_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec![
        "view".into(),
        "resolve".into(),
        "--base".into(),
        identifier(args, "base")?,
    ];
    if let Some(includes) = args.get("include") {
        let list = includes
            .as_array()
            .ok_or_else(|| ToolFailure::new("invalid_request", "`include` must be an array"))?;
        if list.len() > 128 {
            return Err(ToolFailure::new(
                "invalid_request",
                "too many topic revisions",
            ));
        }
        for item in list {
            let o = item.as_object().ok_or_else(|| {
                ToolFailure::new("invalid_request", "include entries must be objects")
            })?;
            reject_unknown(o, &["topic", "revision"])?;
            v.extend([
                "--include".into(),
                format!("{}:{}", identifier(o, "topic")?, identifier(o, "revision")?),
            ]);
        }
    }
    Ok(v)
}
fn project_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec![
        "project".into(),
        "materialize".into(),
        "--view".into(),
        identifier(args, "view")?,
        "--purpose".into(),
        enumeration(
            args,
            "purpose",
            &["execution", "compatibility", "inspection", "export"],
        )?,
    ];
    if args.contains_key("strategy") {
        v.extend([
            "--strategy".into(),
            enumeration(
                args,
                "strategy",
                &["copy", "reflink", "hardlink_readonly", "overlay_copyup"],
            )?,
        ]);
    }
    if let Some(required) = args.get("require_strategy") {
        let required = required.as_bool().ok_or_else(|| {
            ToolFailure::new("invalid_request", "`require_strategy` must be a boolean")
        })?;
        if required {
            v.push("--no-copy-fallback".into())
        }
    }
    Ok(v)
}
fn compat_import_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec![
        "compat".into(),
        "import".into(),
        "--projection".into(),
        identifier(args, "projection")?,
    ];
    for c in string_array(args, "candidates", 128)? {
        if c.starts_with('-') {
            return Err(ToolFailure::new(
                "invalid_request",
                "candidate is not a safe identifier",
            ));
        }
        v.extend(["--candidate".into(), c]);
    }
    if let Some(g) = optional_identifier(args, "session_generation")? {
        v.extend(["--session-generation".into(), g]);
    }
    Ok(v)
}
fn run_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let program = identifier(args, "program")?;
    let lower = program.trim_end_matches(".exe").to_ascii_lowercase();
    if [
        "cmd",
        "powershell",
        "pwsh",
        "sh",
        "bash",
        "zsh",
        "fish",
        "wsl",
        "cscript",
        "wscript",
    ]
    .contains(&lower.as_str())
        || program.contains('/')
        || program.contains('\\')
        || program.contains(':')
    {
        return Err(ToolFailure::new(
            "shell_not_allowed",
            "execution program must be a bare non-shell executable name",
        ));
    }
    let cwd = args
        .get("cwd")
        .map(|_| artifact_path(args, "cwd", true))
        .transpose()?
        .unwrap_or_else(|| ".".into());
    let mut v = vec![
        "run".into(),
        "--view".into(),
        identifier(args, "view")?,
        "--cwd".into(),
        cwd,
    ];
    if let Some(network) = optional_identifier(args, "network")? {
        if !["disabled", "not_enforced"].contains(&network.as_str()) {
            return Err(ToolFailure::new(
                "invalid_request",
                "execution network must be disabled or not_enforced",
            ));
        }
        v.extend(["--network".into(), network]);
    }
    v.extend(["--".into(), program]);
    if args.contains_key("args") {
        v.extend(string_array(args, "args", 256)?)
    }
    Ok(v)
}
fn policy_export_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec![
        "policy".into(),
        "check-export".into(),
        "--checkpoint".into(),
        identifier(args, "checkpoint")?,
    ];
    if let Some(branch) = optional_identifier(args, "branch")? {
        v.extend(["--branch".into(), branch]);
    }
    Ok(v)
}
fn policy_commit_argv(args: &Map<String, Value>) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec!["policy".into(), "check-commit".into()];
    if args.contains_key("paths") {
        let paths = string_array(args, "paths", 256)?;
        if !paths.is_empty() {
            v.push("--paths".into());
            for p in paths {
                v.push(validate_repo_relative(&p, "paths", false)?);
            }
        }
    }
    Ok(v)
}
fn git_export_argv(args: &Map<String, Value>, repo: &Path) -> Result<Vec<String>, ToolFailure> {
    let mut v = vec![
        "git".into(),
        "export".into(),
        "--checkpoint".into(),
        identifier(args, "checkpoint")?,
        "--branch".into(),
        identifier(args, "branch")?,
    ];
    match enumeration(args, "mode", &["plan", "execute"])?.as_str() {
        "plan" => v.push("--write-plan".into()),
        "execute" => v.extend([
            "--execute-local".into(),
            "--repo".into(),
            repo.display().to_string(),
        ]),
        _ => unreachable!(),
    }
    Ok(v)
}

fn allowed_fields(name: &str) -> &'static [&'static str] {
    match name {
        "repository_init" => &[],
        "repository_status" => &["scope", "id"],
        "topic_create" => &[
            "slug",
            "display_name",
            "owner",
            "visibility",
            "acceptance_criteria",
        ],
        "topic_complete" => &["topic", "revision", "session", "summary"],
        "topic_wait" => &["topic", "timeout_ms", "poll_interval_ms"],
        "session_start" => &["topic", "view", "actor"],
        "session_refresh" => &["session", "policy"],
        "artifact_read" => &["path", "session", "view"],
        "artifact_list" => &["prefix", "session", "view"],
        "artifact_search" => &["query", "session", "view"],
        "artifact_patch" => &["path", "session", "expect_hash", "patch"],
        "artifact_write" => &[
            "path",
            "session",
            "expect_hash",
            "content",
            "classification",
        ],
        "artifact_move" => &["from", "to", "session", "expect_hash"],
        "artifact_delete" => &["path", "session", "expect_hash"],
        "artifact_metadata_set" => &["path", "session", "expect_hash", "classification"],
        "view_resolve" => &["base", "include"],
        "project_materialize" => &["view", "purpose", "strategy", "require_strategy"],
        "compat_project" => &["session"],
        "compat_diff" => &["projection"],
        "compat_import" => &["projection", "candidates", "session_generation"],
        "execution_run" => &["view", "program", "args", "cwd", "network"],
        "execution_promote_output" => &["execution", "path", "session", "classification"],
        "checkpoint_create" => &["view", "execution"],
        "policy_check_export" => &["checkpoint", "branch"],
        "policy_check_commit" => &["paths"],
        "policy_explain" => &["validation_report"],
        "git_export" => &["checkpoint", "branch", "mode"],
        "inspect" => &["selector", "session"],
        _ => &[],
    }
}

fn tool_names() -> &'static [&'static str] {
    &[
        "repository_init",
        "repository_status",
        "topic_create",
        "topic_complete",
        "topic_wait",
        "session_start",
        "session_refresh",
        "artifact_read",
        "artifact_list",
        "artifact_search",
        "artifact_patch",
        "artifact_write",
        "artifact_move",
        "artifact_delete",
        "artifact_metadata_set",
        "view_resolve",
        "project_materialize",
        "compat_project",
        "compat_diff",
        "compat_import",
        "execution_run",
        "execution_promote_output",
        "checkpoint_create",
        "policy_check_export",
        "policy_check_commit",
        "policy_explain",
        "git_export",
        "inspect",
    ]
}

fn tools() -> Vec<Value> {
    vec![
        tool(
            "repository_init",
            "Initialize and ingest the bound repository. Use once for an uninitialized root.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "repository_status",
            "Read persisted repository or object status. Omit id for repository scope; every other scope requires the exact matching object id. Completed topic status includes its structured handoff.",
            json!({"scope":{"type":"string","description":"Select repository for the whole repository, or an object type paired with id.","enum":["repository","topic","session","view","projection","execution","checkpoint","export","git","compat_import"],"default":"repository"},"id":id_schema("Required exact object id whenever scope is not repository; omit for repository scope.")}),
            &[],
            false,
        ),
        tool(
            "topic_create",
            "Create a durable authoring topic.",
            json!({"slug":s(),"display_name":s(),"owner":s(),"visibility":{"type":"string","enum":["local","private"],"default":"local"},"acceptance_criteria":{"type":"array","maxItems":64,"items":{"type":"string","minLength":1,"maxLength":1024}}}),
            &["slug", "display_name"],
            true,
        ),
        tool(
            "topic_complete",
            "Seal a topic at its exact current head revision. Repeating the same completion is an idempotent no-op; later artifact mutations on the topic are rejected. This is a durable coordination fact, not a review or quality judgment.",
            json!({"topic":id_schema("Exact topic_id owned by session."),"revision":id_schema("Exact current head topic_revision_id to make immutable."),"session":id_schema("Exact authoring session_id for this topic."),"summary":{"type":"string","description":"Optional factual handoff summary.","minLength":1,"maxLength":4096}}),
            &["topic", "revision", "session"],
            true,
        ),
        tool(
            "topic_wait",
            "Wait efficiently until another agent's topic is durably completed, the timeout expires, or the request is cancelled. Returns the same topic status and structured handoff as repository_status plus wait.outcome; this replaces repeated status polling.",
            json!({"topic":id_schema("Exact topic_id to observe."),"timeout_ms":{"type":"integer","minimum":0,"maximum":900000,"default":300000,"description":"Maximum wait in milliseconds. A timeout returns the latest status with wait.outcome=timeout."},"poll_interval_ms":{"type":"integer","minimum":50,"maximum":5000,"default":250,"description":"Internal local status check interval; normally leave at the default."}}),
            &["topic"],
            false,
        ),
        tool(
            "session_start",
            "Start a topic-bound authoring session over an exact resolved view. The session writes only to the supplied topic; its initial read frontier is copied from the supplied view.",
            json!({"topic":id_schema("Exact topic_id returned by topic_create or inspect."),"view":id_schema("Exact resolved_view_id returned by repository_status or view_resolve."),"actor":id_schema("Stable caller-chosen actor identifier used for provenance.")}),
            &["topic", "view", "actor"],
            true,
        ),
        tool(
            "session_refresh",
            "Explicitly refresh heads for non-write topics already present in this session frontier. This never discovers newly created topics. manual and follow both refresh now; follow records continued opt-in intent, while none changes policy without advancing the frontier.",
            json!({"session":id_schema("Exact session_id returned by session_start."),"policy":{"type":"string","enum":["manual","follow","none"],"description":"manual: refresh scoped topic heads now. follow: refresh now and retain follow intent. none: retain the current exact frontier."}}),
            &["session", "policy"],
            true,
        ),
        scoped_read_tool(
            "artifact_read",
            "Read persisted artifact content and identity from either an authoring session or an exact resolved view. View reads are read-only and create no session.",
            json!({"path":path_schema(),"session":id_schema("Exact session_id for session-scoped reading."),"view":id_schema("Exact resolved_view_id for session-free read-only access.")}),
            &["path"],
        ),
        scoped_read_tool(
            "artifact_list",
            "List persisted artifacts under an optional repository-relative prefix in either an authoring session or an exact resolved view.",
            json!({"prefix":path_schema(),"session":id_schema("Exact session_id for session-scoped listing."),"view":id_schema("Exact resolved_view_id for session-free read-only access.")}),
            &[],
        ),
        scoped_read_tool(
            "artifact_search",
            "Search persisted artifact content in either an authoring session or an exact resolved view.",
            json!({"query":s(),"session":id_schema("Exact session_id for session-scoped search."),"view":id_schema("Exact resolved_view_id for session-free read-only access.")}),
            &["query"],
        ),
        tool(
            "artifact_patch",
            "Patch one UTF-8 artifact using compare-and-swap. Standard unified diffs and *** Begin Patch / *** Update File envelopes are accepted. Numeric hunk positions and counts are treated as hints: exact unique context locates the edit. Ambiguous or stale context is rejected, while expect_hash remains the concurrency guard.",
            json!({"path":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":existing_hash_schema(),"patch":patch_schema()}),
            &["path", "session", "expect_hash", "patch"],
            true,
        ),
        tool(
            "artifact_write",
            "Create or replace one artifact using compare-and-swap. Use expect_hash \"new\" only to assert that the path is absent; otherwise pass the exact sha256 content_hash returned by artifact_read.",
            json!({"path":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":write_expect_hash_schema(),"content":{"type":"string","description":"Complete artifact bytes encoded as a JSON UTF-8 string.","maxLength":MAX_CONTENT_BYTES},"classification":class_schema()}),
            &[
                "path",
                "session",
                "expect_hash",
                "content",
                "classification",
            ],
            true,
        ),
        tool(
            "artifact_move",
            "Move an artifact while preserving identity.",
            json!({"from":path_schema(),"to":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":existing_hash_schema()}),
            &["from", "to", "session", "expect_hash"],
            true,
        ),
        tool(
            "artifact_delete",
            "Tombstone an artifact with CAS.",
            json!({"path":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":existing_hash_schema()}),
            &["path", "session", "expect_hash"],
            true,
        ),
        tool(
            "artifact_metadata_set",
            "Set artifact classification with CAS.",
            json!({"path":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":existing_hash_schema(),"classification":class_schema()}),
            &["path", "session", "expect_hash", "classification"],
            true,
        ),
        tool(
            "view_resolve",
            "Resolve one exact revision per topic over the base checkpoint. Every supplied selection is echoed as requested_frontier and normalized_frontier; dependencies, conflicts, and staleness are returned as facts rather than merged implicitly.",
            json!({"base":id_schema("Exact base checkpoint_id."),"include":{"type":"array","maxItems":128,"description":"Exact topic revision selections. Omit to resolve current heads. A topic may appear at most once.","items":{"type":"object","additionalProperties":false,"properties":{"topic":id_schema("Exact topic_id, not a slug."),"revision":id_schema("Exact topic_revision_id belonging to topic.")},"required":["topic","revision"]}}}),
            &["base"],
            true,
        ),
        tool(
            "project_materialize",
            "Materialize a managed projection inside the bound repository policy root.",
            json!({"view":s(),"purpose":{"type":"string","enum":["execution","compatibility","inspection","export"]},"strategy":{"type":"string","enum":["copy","reflink","hardlink_readonly","overlay_copyup"]},"require_strategy":{"type":"boolean","default":false}}),
            &["view", "purpose"],
            true,
        ),
        tool(
            "compat_project",
            "Create a compatibility projection for a session.",
            json!({"session":s()}),
            &["session"],
            true,
        ),
        tool(
            "compat_diff",
            "Diff a managed compatibility projection against its persisted baseline.",
            json!({"projection":s()}),
            &["projection"],
            false,
        ),
        tool(
            "compat_import",
            "Import selected compatibility candidates as one native transaction.",
            json!({"projection":s(),"candidates":{"type":"array","minItems":1,"maxItems":128,"items":s()},"session_generation":s()}),
            &["projection", "candidates"],
            true,
        ),
        tool(
            "execution_run",
            "Run one bare non-shell program with structured arguments against an exact view. Returns bounded stdout/stderr text in output_text for immediate diagnosis, capture digests, phase_timings_ms, classified file deltas, and only actionable source/generated promotion candidates. Known build/cache paths are classified without per-file subprocesses; remaining Git ignore checks are batched and bounded.",
            json!({"view":s(),"program":{"type":"string","description":"Bare executable name; shells and host paths are rejected."},"args":{"type":"array","maxItems":256,"items":{"type":"string","maxLength":16384}},"cwd":{"type":"string","description":"Repository-relative projection cwd.","default":"."},"network":{"type":"string","enum":["disabled","not_enforced"],"description":"Optional per-run network policy override."}}),
            &["view", "program"],
            true,
        ),
        tool(
            "execution_promote_output",
            "Promote one classified execution output into a session-owned operation.",
            json!({"execution":s(),"path":path_schema(),"session":s(),"classification":class_schema()}),
            &["execution", "path", "session", "classification"],
            true,
        ),
        tool(
            "checkpoint_create",
            "Freeze an exact resolved view with optional validated passing execution evidence.",
            json!({"view":s(),"execution":s()}),
            &["view"],
            true,
        ),
        tool(
            "policy_check_export",
            "Validate a checkpoint for export policy.",
            json!({"checkpoint":s(),"branch":s()}),
            &["checkpoint"],
            false,
        ),
        tool(
            "policy_check_commit",
            "Validate repository-relative paths for Sunlight commit policy.",
            json!({"paths":{"type":"array","maxItems":256,"items":path_schema()}}),
            &[],
            false,
        ),
        tool(
            "policy_explain",
            "Explain a persisted policy validation report.",
            json!({"validation_report":s()}),
            &["validation_report"],
            false,
        ),
        tool(
            "git_export",
            "Plan or execute Git export of a checkpoint to a branch in the bound repository.",
            json!({"checkpoint":s(),"branch":s(),"mode":{"type":"string","enum":["plan","execute"]}}),
            &["checkpoint", "branch", "mode"],
            true,
        ),
        tool(
            "inspect",
            "Inspect persisted repository objects with a typed selector.",
            json!({"selector":{"type":"string","description":"repository or topic:/session:/view:/artifact:/operation:/conflict:/projection:/execution:/checkpoint:/export:/git: selector"},"session":s()}),
            &["selector"],
            false,
        ),
    ]
}
fn scoped_read_tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    let mut value = tool(name, description, properties, required, false);
    value["inputSchema"]["oneOf"] = json!([
        {"required":["session"]},
        {"required":["view"]}
    ]);
    value
}

fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    mutating: bool,
) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":required},"outputSchema":{"type":"object","description":"Every tool returns exactly one Sunlight envelope: ok=true with data and warnings, or ok=false with error.","required":["ok"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"error":{"type":"object"},"warnings":{"type":["array","object"]}},"additionalProperties":false},"annotations":{"readOnlyHint":!mutating,"destructiveHint":matches!(name,"artifact_delete"|"git_export"),"idempotentHint":matches!(name,"repository_init"|"repository_status"|"topic_complete"|"topic_wait"|"artifact_read"|"artifact_list"|"artifact_search"|"compat_diff"|"policy_check_export"|"policy_check_commit"|"policy_explain"|"inspect")}})
}
fn s() -> Value {
    json!({"type":"string","minLength":1,"maxLength":16384})
}
fn id_schema(description: &str) -> Value {
    json!({"type":"string","description":description,"minLength":1,"maxLength":16384})
}
fn existing_hash_schema() -> Value {
    json!({
        "type":"string",
        "pattern":r"^sha256:[0-9a-f]{64}$",
        "description":"Exact sha256 content_hash returned by artifact_read for the current session view."
    })
}
fn write_expect_hash_schema() -> Value {
    json!({
        "description":"Use literal new to assert path absence; otherwise use the exact current sha256 content_hash.",
        "oneOf":[
            {"type":"string","const":"new","description":"Create only if the path is absent."},
            {"type":"string","pattern":r"^sha256:[0-9a-f]{64}$","description":"Replace only if the current content hash matches."}
        ]
    })
}
fn patch_schema() -> Value {
    json!({
        "type":"string",
        "description":"Unified patch for this one artifact. Accepted forms: standard hunks starting @@ -old +new @@, or an apply_patch envelope with *** Begin Patch, *** Update File, a bare @@ hunk, and *** End Patch. Use exact unchanged context plus - removals and + additions. Header positions/counts may be approximate when the old context has one unique source match.",
        "examples":["@@ -1,2 +1,2 @@\n first\n-old\n+new\n","*** Begin Patch\n*** Update File: src/example.ts\n@@\n-old\n+new\n*** End Patch\n"],
        "maxLength":MAX_CONTENT_BYTES
    })
}
fn path_schema() -> Value {
    json!({"type":"string","description":"Portable repository-relative artifact path; absolute paths and traversal are rejected.","maxLength":16384})
}
fn class_schema() -> Value {
    json!({"type":"string","enum":["source","generated","cache","secret","local-only","execution-output","lockfile","migration","binary","vendored"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advertised_tools_have_no_fixture_vocabulary() {
        let encoded = serde_json::to_string(&tools()).unwrap();
        assert!(!encoded.contains("fixture"));
        assert_eq!(tools().len(), tool_names().len());
    }
    #[test]
    fn run_rejects_shells_and_host_paths() {
        let temp = PrivateTemp::new(Path::new(".")).unwrap();
        let shell = build_invocation(
            "execution_run",
            &json!({"view":"v","program":"powershell","args":[]}),
            Path::new("."),
            &temp,
        );
        assert!(matches!(
            shell,
            Err(ToolFailure {
                code: "shell_not_allowed",
                ..
            })
        ));
        assert!(validate_repo_relative("../escape", "path", false).is_err());
    }
    #[test]
    fn agent_contract_exposes_creation_patch_and_completion_facts() {
        let advertised = tools();
        let find = |name: &str| advertised.iter().find(|tool| tool["name"] == name).unwrap();

        let write = find("artifact_write");
        let expect = &write["inputSchema"]["properties"]["expect_hash"];
        assert_eq!(expect["oneOf"][0]["const"], "new");
        assert_eq!(expect["oneOf"][1]["pattern"], r"^sha256:[0-9a-f]{64}$");
        assert!(write["description"]
            .as_str()
            .unwrap()
            .contains("Use expect_hash \"new\" only"));

        let patch = find("artifact_patch");
        assert!(patch["description"]
            .as_str()
            .unwrap()
            .contains("Numeric hunk positions and counts are treated as hints"));
        assert!(patch["inputSchema"]["properties"]["patch"]["description"]
            .as_str()
            .unwrap()
            .contains("apply_patch envelope"));

        let complete = find("topic_complete");
        assert_eq!(
            complete["inputSchema"]["required"],
            json!(["topic", "revision", "session"])
        );
        assert_eq!(complete["annotations"]["idempotentHint"], true);
        assert!(complete["description"]
            .as_str()
            .unwrap()
            .contains("durable coordination fact"));

        let wait = find("topic_wait");
        assert_eq!(wait["annotations"]["readOnlyHint"], true);
        assert_eq!(wait["annotations"]["idempotentHint"], true);
        assert_eq!(wait["inputSchema"]["required"], json!(["topic"]));
        assert!(wait.get("outputSchema").is_some());
    }

    #[test]
    fn artifact_reads_require_one_clear_session_or_view_scope() {
        let advertised = tools();
        let read = advertised
            .iter()
            .find(|tool| tool["name"] == "artifact_read")
            .unwrap();
        assert_eq!(
            read["inputSchema"]["oneOf"],
            json!([{"required":["session"]},{"required":["view"]}])
        );
        assert!(read["inputSchema"]["properties"]["view"]["description"]
            .as_str()
            .unwrap()
            .contains("session-free read-only access"));

        let temp = PrivateTemp::new(Path::new(".")).unwrap();
        let invocation = build_invocation(
            "artifact_read",
            &json!({"path":"README.md","view":"view_exact"}),
            Path::new("."),
            &temp,
        )
        .unwrap();
        assert!(invocation
            .argv
            .windows(2)
            .any(|pair| pair == ["--view", "view_exact"]));

        let both = build_invocation(
            "artifact_read",
            &json!({"path":"README.md","session":"session_a","view":"view_exact"}),
            Path::new("."),
            &temp,
        );
        assert!(matches!(
            both,
            Err(ToolFailure {
                code: "artifact_read_scope_invalid",
                ..
            })
        ));
    }

    #[test]
    fn view_resolve_mcp_array_preserves_each_include_argument() {
        let temp = PrivateTemp::new(Path::new(".")).unwrap();
        let argv = build_invocation(
            "view_resolve",
            &json!({
                "base":"checkpoint_base_0001",
                "include":[
                    {"topic":"topic_a","revision":"rev_a_0001"},
                    {"topic":"topic_b","revision":"rev_b_0001"}
                ]
            }),
            Path::new("."),
            &temp,
        )
        .unwrap();
        assert!(argv
            .argv
            .windows(2)
            .any(|pair| pair == ["--include", "topic_a:rev_a_0001"]));
        assert!(argv
            .argv
            .windows(2)
            .any(|pair| pair == ["--include", "topic_b:rev_b_0001"]));
    }
}
