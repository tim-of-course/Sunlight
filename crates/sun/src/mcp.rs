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
use std::time::Duration;

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
                        "instructions": "All tools are bound to the canonical repository supplied when this server started. Use returned native IDs and hashes as preconditions; no fixture tools or arbitrary host paths are available."
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
                let result = match build_invocation(&name, &arguments, &repo, &temp) {
                    Ok(invocation) => execute_invocation(&engine, invocation, &worker_cancel),
                    Err(error) => tool_failure_result(error),
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
        "topic_create" => vec![
            "topic".into(),
            "create".into(),
            identifier(args, "slug")?,
            "--display-name".into(),
            text(args, "display_name")?,
        ],
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
        "artifact_read" => vec![
            "read".into(),
            artifact_path(args, "path", false)?,
            "--session".into(),
            identifier(args, "session")?,
        ],
        "artifact_list" => {
            let mut v = vec!["list".into()];
            if let Some(path) = optional_artifact_path(args, "prefix", true)? {
                v.push(path)
            }
            v.extend(["--session".into(), identifier(args, "session")?]);
            v
        }
        "artifact_search" => vec![
            "search".into(),
            text(args, "query")?,
            "--session".into(),
            identifier(args, "session")?,
        ],
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
        "checkpoint_create" => vec![
            "checkpoint".into(),
            "create".into(),
            "--view".into(),
            identifier(args, "view")?,
        ],
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
        "topic_create" => &["slug", "display_name"],
        "session_start" => &["topic", "view", "actor"],
        "session_refresh" => &["session", "policy"],
        "artifact_read" => &["path", "session"],
        "artifact_list" => &["prefix", "session"],
        "artifact_search" => &["query", "session"],
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
        "checkpoint_create" => &["view"],
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
            "Read persisted repository or object status.",
            json!({"scope":{"type":"string","enum":["repository","topic","session","view","projection","execution","checkpoint","export","git","compat_import"],"default":"repository"},"id":{"type":"string"}}),
            &[],
            false,
        ),
        tool(
            "topic_create",
            "Create a durable authoring topic.",
            json!({"slug":s(),"display_name":s()}),
            &["slug", "display_name"],
            true,
        ),
        tool(
            "session_start",
            "Start a topic-bound authoring session over an exact view.",
            json!({"topic":s(),"view":s(),"actor":s()}),
            &["topic", "view", "actor"],
            true,
        ),
        tool(
            "session_refresh",
            "Refresh a session using an explicit frontier policy.",
            json!({"session":s(),"policy":{"type":"string","enum":["manual","follow","none"]}}),
            &["session", "policy"],
            true,
        ),
        tool(
            "artifact_read",
            "Read persisted artifact content and identity from a session.",
            json!({"path":path_schema(),"session":s()}),
            &["path", "session"],
            false,
        ),
        tool(
            "artifact_list",
            "List persisted artifacts under an optional repository-relative prefix.",
            json!({"prefix":path_schema(),"session":s()}),
            &["session"],
            false,
        ),
        tool(
            "artifact_search",
            "Search persisted artifact content in a session.",
            json!({"query":s(),"session":s()}),
            &["query", "session"],
            false,
        ),
        tool(
            "artifact_patch",
            "Apply a JSON string unified patch with an expected content hash.",
            json!({"path":path_schema(),"session":s(),"expect_hash":s(),"patch":{"type":"string","maxLength":MAX_CONTENT_BYTES}}),
            &["path", "session", "expect_hash", "patch"],
            true,
        ),
        tool(
            "artifact_write",
            "Create or replace an artifact from JSON string content with CAS and classification.",
            json!({"path":path_schema(),"session":s(),"expect_hash":s(),"content":{"type":"string","maxLength":MAX_CONTENT_BYTES},"classification":class_schema()}),
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
            json!({"from":path_schema(),"to":path_schema(),"session":s(),"expect_hash":s()}),
            &["from", "to", "session", "expect_hash"],
            true,
        ),
        tool(
            "artifact_delete",
            "Tombstone an artifact with CAS.",
            json!({"path":path_schema(),"session":s(),"expect_hash":s()}),
            &["path", "session", "expect_hash"],
            true,
        ),
        tool(
            "artifact_metadata_set",
            "Set artifact classification with CAS.",
            json!({"path":path_schema(),"session":s(),"expect_hash":s(),"classification":class_schema()}),
            &["path", "session", "expect_hash", "classification"],
            true,
        ),
        tool(
            "view_resolve",
            "Resolve an exact base and topic revision selection.",
            json!({"base":s(),"include":{"type":"array","maxItems":128,"items":{"type":"object","additionalProperties":false,"properties":{"topic":s(),"revision":s()},"required":["topic","revision"]}}}),
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
            "Run one bare non-shell program with structured arguments against an exact view.",
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
            "Freeze an exact resolved view and eligible evidence.",
            json!({"view":s()}),
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
fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    mutating: bool,
) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":required},"annotations":{"readOnlyHint":!mutating,"destructiveHint":matches!(name,"artifact_delete"|"git_export"),"idempotentHint":matches!(name,"repository_init"|"repository_status"|"artifact_read"|"artifact_list"|"artifact_search"|"compat_diff"|"policy_check_export"|"policy_check_commit"|"policy_explain"|"inspect")}})
}
fn s() -> Value {
    json!({"type":"string","minLength":1,"maxLength":16384})
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
}
