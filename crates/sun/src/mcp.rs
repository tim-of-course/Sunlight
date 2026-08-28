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
const REPOSITORY_MUTATION_QUEUE_TIMEOUT: Duration = Duration::from_secs(10);
const REPOSITORY_MUTATION_QUEUE_POLL: Duration = Duration::from_millis(10);
const ARTIFACT_CLASSIFICATIONS: &[&str] = &["source", "generated"];
const PROMOTION_CLASSIFICATIONS: &[&str] = &["source_like_delta", "generated_artifact"];
const STATUS_SCOPE_FLAGS: &[(&str, Option<&str>)] = &[
    ("repository", None),
    ("topic", Some("--topic")),
    ("session", Some("--session")),
    ("view", Some("--view")),
    ("projection", Some("--projection")),
    ("execution", Some("--execution")),
    ("checkpoint", Some("--checkpoint")),
    ("export", Some("--export")),
    ("compat_import", Some("--compat-import")),
];
const INSPECT_SELECTOR_PREFIXES: &[&str] = &[
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
];

pub(crate) fn serve_from_args(args: &[String]) -> Result<(), String> {
    if args == ["mcp", "--help"] || args == ["mcp", "serve", "--help"] {
        println!(
            "sun mcp serve\n\nUsage:\n  sun mcp serve --repo <repository-directory>\n\nRuns newline-delimited MCP JSON-RPC 2.0 on stdio, bound to one canonical directory. The directory may be uninitialized so repository_init can perform first ingest. Protocol messages use stdout; diagnostics use stderr."
        );
        return Ok(());
    }
    if args.len() != 4 || args[0] != "mcp" || args[1] != "serve" || args[2] != "--repo" {
        return Err("usage: sun mcp serve --repo <repository-directory>".to_string());
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
    let mut pending: VecDeque<(Value, Instant)> = VecDeque::new();
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
            if let Some((message, queued_at)) = pending.pop_front() {
                handle_message(
                    message,
                    queued_at,
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
                        pending.retain(|(queued, _)| queued.get("id") != Some(&requested));
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
                    pending.push_back((message, Instant::now()));
                } else {
                    handle_message(
                        message,
                        Instant::now(),
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
    queued_at: Instant,
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
                        "instructions": "All tools are bound to the canonical repository supplied when this server started. Author with repository_status, topic_create, session_start, scoped artifact reads and mutations, and topic_complete. Integrate with topic_wait and view_resolve using exact selected revisions; omitting include is discovery-only and resolves moving current heads. Validate the exact combined view with execution_run. Promote each intentional output with execution_promote_output using its returned candidate classification and a live topic-owned session over the validated view; resolve the resulting exact revision and create a checkpoint from matching passing evidence. For a requested Git handoff, call policy_check_export with the exact checkpoint and target ref, then git_export; completion is a returned export_map_id for that checkpoint and ref. artifact_read/list/search accept exactly one scope: session for the authoring frontier or view for session-free read-only access to an exact resolved view. topic_complete and completed topic status return a structured handoff with summary, operations, changed paths, and hashes; use topic_wait instead of polling when another agent owns the topic. Transient repository writer and state-sequence races are retried automatically for safe native and read commands; commands that can replay external side effects are never automatically retried. Use exact IDs and sha256 hashes returned by tools. artifact_write uses expect_hash \"new\" only when the path must be absent. Sessions have fixed topic scope: session_refresh advances only non-write topics already in that session frontier and does not discover newly created topics. execution_run returns bounded stdout/stderr text in that response, phase timings, and only source-like or explicitly generated promotion candidates. No fixture tools or arbitrary host paths are available."
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
                let client_queue_ms = queued_at.elapsed().as_millis();
                let call_started = Instant::now();
                let mut repository_queue_time = Duration::ZERO;
                let mut result = if name == "topic_wait" {
                    execute_topic_wait(&engine, &arguments, &worker_cancel)
                } else {
                    match build_invocation(&name, &arguments, &repo, &temp) {
                        Ok(invocation) if tool_uses_repository_mutation_queue(&name) => {
                            let queue_started = Instant::now();
                            let guard = acquire_repository_mutation_queue(&repo, &worker_cancel);
                            repository_queue_time = queue_started.elapsed();
                            match guard {
                                Ok(_guard) => {
                                    execute_invocation(&engine, invocation, &worker_cancel)
                                }
                                Err(error) => tool_failure_result(error),
                            }
                        }
                        Ok(invocation) => execute_invocation(&engine, invocation, &worker_cancel),
                        Err(error) => tool_failure_result(error),
                    }
                };
                let concurrency_retries = result
                    .as_object_mut()
                    .and_then(|object| object.remove("_automatic_concurrency_retries"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                attach_transport_metrics(
                    &mut result,
                    client_queue_ms + repository_queue_time.as_millis(),
                    call_started
                        .elapsed()
                        .saturating_sub(repository_queue_time)
                        .as_millis(),
                    concurrency_retries,
                );
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

#[derive(Debug)]
struct RepositoryMutationQueueGuard {
    _file: fs::File,
}

fn tool_uses_repository_mutation_queue(name: &str) -> bool {
    matches!(
        name,
        "repository_init"
            | "topic_create"
            | "topic_complete"
            | "session_start"
            | "session_refresh"
            | "artifact_patch"
            | "artifact_write"
            | "artifact_move"
            | "artifact_delete"
            | "artifact_metadata_set"
            | "view_resolve"
            | "execution_promote_output"
            | "checkpoint_create"
            | "policy_check_export"
            | "git_export"
    )
}

fn acquire_repository_mutation_queue(
    repo: &Path,
    cancel: &AtomicBool,
) -> Result<RepositoryMutationQueueGuard, ToolFailure> {
    let lock_path = repo.join(".sunlight/local/mcp-mutation-queue.lock");
    let parent = lock_path.parent().expect("queue lock has a parent");
    fs::create_dir_all(parent).map_err(|error| {
        ToolFailure::new(
            "repository_queue_io",
            format!("cannot create the repository mutation queue directory: {error}"),
        )
        .detail("lock", lock_path.display().to_string())
    })?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .map_err(|error| {
            ToolFailure::new(
                "repository_queue_io",
                format!("cannot open the repository mutation queue: {error}"),
            )
            .detail("lock", lock_path.display().to_string())
        })?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(ToolFailure::new(
                "request_cancelled",
                "request was cancelled while waiting for the repository mutation queue",
            )
            .detail("lock", lock_path.display().to_string()));
        }
        match file.try_lock() {
            Ok(()) => return Ok(RepositoryMutationQueueGuard { _file: file }),
            Err(fs::TryLockError::WouldBlock) => {
                if started.elapsed() >= REPOSITORY_MUTATION_QUEUE_TIMEOUT {
                    return Err(ToolFailure::new(
                        "repository_writer_busy",
                        "timed out waiting for another MCP writer in this repository",
                    )
                    .detail("lock", lock_path.display().to_string())
                    .detail(
                        "timeout_ms",
                        REPOSITORY_MUTATION_QUEUE_TIMEOUT.as_millis() as u64,
                    ));
                }
                thread::sleep(REPOSITORY_MUTATION_QUEUE_POLL);
            }
            Err(fs::TryLockError::Error(error)) => {
                return Err(ToolFailure::new(
                    "repository_queue_io",
                    format!("cannot lock the repository mutation queue: {error}"),
                )
                .detail("lock", lock_path.display().to_string()));
            }
        }
    }
}

fn tool_failure_result(error: ToolFailure) -> Value {
    let envelope = json!({"ok":false,"error":{"code":error.code,"message":error.message,"details":error.details,"next_action":super::next_action_for_error_code(error.code)}});
    json!({"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope,"isError":true})
}

fn attach_transport_metrics(
    result: &mut Value,
    queue_ms: u128,
    worker_ms: u128,
    automatic_concurrency_retries: u64,
) {
    let Some(mut envelope) = result.get("structuredContent").cloned() else {
        return;
    };
    let transport = json!({
        "queue_ms": queue_ms,
        "worker_ms": worker_ms,
        "automatic_concurrency_retries": automatic_concurrency_retries,
    });
    if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        envelope["data"]["transport"] = transport;
    } else {
        envelope["error"]["details"]["transport"] = transport;
    }
    result["content"] = json!([{"type":"text","text":envelope.to_string()}]);
    result["structuredContent"] = envelope;
}

fn with_concurrency_retries(mut result: Value, count: usize) -> Value {
    result["_automatic_concurrency_retries"] = json!(count);
    result
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
        return with_concurrency_retries(
            tool_failure_result(
                ToolFailure::new(
                    "mcp_stdout_too_large",
                    "engine response exceeded the MCP response limit",
                )
                .detail("max_bytes", MAX_STDOUT_BYTES as u64),
            ),
            response.concurrency_retry_count,
        );
    }
    if response.stderr_overflowed {
        return with_concurrency_retries(
            tool_failure_result(
                ToolFailure::new(
                    "mcp_stderr_too_large",
                    "engine diagnostics exceeded the MCP diagnostic limit",
                )
                .detail("max_bytes", MAX_STDERR_BYTES as u64),
            ),
            response.concurrency_retry_count,
        );
    }
    let parsed: Value = match serde_json::from_str(&response.stdout) {
        Ok(value) => value,
        Err(error) => {
            return with_concurrency_retries(
                tool_failure_result(
                    ToolFailure::new(
                        "mcp_invalid_engine_contract",
                        "command engine did not return one valid JSON contract",
                    )
                    .detail("source", error.to_string())
                    .detail("stderr", response.stderr),
                ),
                response.concurrency_retry_count,
            );
        }
    };
    let is_error = !response.success || parsed.get("ok").and_then(Value::as_bool) == Some(false);
    with_concurrency_retries(
        json!({
            "content":[{"type":"text","text":parsed.to_string()}],
            "structuredContent":parsed,
            "isError":is_error
        }),
        response.concurrency_retry_count,
    )
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
            promotion_classification(args)?,
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
    if value.is_empty() || value.contains('\0') {
        return Err(ToolFailure::new(
            "invalid_request",
            format!("`{key}` must be nonempty and contain no NUL bytes"),
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
    enumeration(args, "classification", ARTIFACT_CLASSIFICATIONS)
}
fn promotion_classification(args: &Map<String, Value>) -> Result<String, ToolFailure> {
    enumeration(args, "classification", PROMOTION_CLASSIFICATIONS)
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
        let flag = STATUS_SCOPE_FLAGS
            .iter()
            .find_map(|(name, flag)| (*name == scope).then_some(*flag))
            .ok_or_else(|| ToolFailure::new("invalid_request", "unknown status scope"))?;
        if flag.is_none() {
            if args.contains_key("id") {
                return Err(ToolFailure::new(
                    "invalid_request",
                    "repository status does not accept `id`",
                ));
            }
            return Ok(v);
        }
        let id = identifier(args, "id")?;
        v.extend([
            flag.expect("non-repository status scope has a flag").into(),
            id,
        ]);
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
        || (INSPECT_SELECTOR_PREFIXES.iter().any(|p| v.starts_with(p))
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
    v.extend([
        "--session-generation".into(),
        identifier(args, "session_generation")?,
    ]);
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
    Ok(vec![
        "policy".into(),
        "check-export".into(),
        "--checkpoint".into(),
        identifier(args, "checkpoint")?,
        "--branch".into(),
        identifier(args, "branch")?,
    ])
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

struct ToolContract {
    name: &'static str,
    allowed: &'static [&'static str],
    required: &'static [&'static str],
}

const TOOL_CONTRACTS: &[ToolContract] = &[
    ToolContract {
        name: "repository_init",
        allowed: &[],
        required: &[],
    },
    ToolContract {
        name: "repository_status",
        allowed: &["scope", "id"],
        required: &[],
    },
    ToolContract {
        name: "topic_create",
        allowed: &[
            "slug",
            "display_name",
            "owner",
            "visibility",
            "acceptance_criteria",
        ],
        required: &["slug", "display_name"],
    },
    ToolContract {
        name: "topic_complete",
        allowed: &["topic", "revision", "session", "summary"],
        required: &["topic", "revision", "session"],
    },
    ToolContract {
        name: "topic_wait",
        allowed: &["topic", "timeout_ms", "poll_interval_ms"],
        required: &["topic"],
    },
    ToolContract {
        name: "session_start",
        allowed: &["topic", "view", "actor"],
        required: &["topic", "view", "actor"],
    },
    ToolContract {
        name: "session_refresh",
        allowed: &["session", "policy"],
        required: &["session", "policy"],
    },
    ToolContract {
        name: "artifact_read",
        allowed: &["path", "session", "view"],
        required: &["path"],
    },
    ToolContract {
        name: "artifact_list",
        allowed: &["prefix", "session", "view"],
        required: &[],
    },
    ToolContract {
        name: "artifact_search",
        allowed: &["query", "session", "view"],
        required: &["query"],
    },
    ToolContract {
        name: "artifact_patch",
        allowed: &["path", "session", "expect_hash", "patch"],
        required: &["path", "session", "expect_hash", "patch"],
    },
    ToolContract {
        name: "artifact_write",
        allowed: &[
            "path",
            "session",
            "expect_hash",
            "content",
            "classification",
        ],
        required: &[
            "path",
            "session",
            "expect_hash",
            "content",
            "classification",
        ],
    },
    ToolContract {
        name: "artifact_move",
        allowed: &["from", "to", "session", "expect_hash"],
        required: &["from", "to", "session", "expect_hash"],
    },
    ToolContract {
        name: "artifact_delete",
        allowed: &["path", "session", "expect_hash"],
        required: &["path", "session", "expect_hash"],
    },
    ToolContract {
        name: "artifact_metadata_set",
        allowed: &["path", "session", "expect_hash", "classification"],
        required: &["path", "session", "expect_hash", "classification"],
    },
    ToolContract {
        name: "view_resolve",
        allowed: &["base", "include"],
        required: &["base"],
    },
    ToolContract {
        name: "project_materialize",
        allowed: &["view", "purpose", "strategy", "require_strategy"],
        required: &["view", "purpose"],
    },
    ToolContract {
        name: "compat_project",
        allowed: &["session"],
        required: &["session"],
    },
    ToolContract {
        name: "compat_diff",
        allowed: &["projection"],
        required: &["projection"],
    },
    ToolContract {
        name: "compat_import",
        allowed: &["projection", "candidates", "session_generation"],
        required: &["projection", "candidates", "session_generation"],
    },
    ToolContract {
        name: "execution_run",
        allowed: &["view", "program", "args", "cwd", "network"],
        required: &["view", "program"],
    },
    ToolContract {
        name: "execution_promote_output",
        allowed: &["execution", "path", "session", "classification"],
        required: &["execution", "path", "session", "classification"],
    },
    ToolContract {
        name: "checkpoint_create",
        allowed: &["view", "execution"],
        required: &["view"],
    },
    ToolContract {
        name: "policy_check_export",
        allowed: &["checkpoint", "branch"],
        required: &["checkpoint", "branch"],
    },
    ToolContract {
        name: "policy_check_commit",
        allowed: &["paths"],
        required: &[],
    },
    ToolContract {
        name: "policy_explain",
        allowed: &["validation_report"],
        required: &["validation_report"],
    },
    ToolContract {
        name: "git_export",
        allowed: &["checkpoint", "branch", "mode"],
        required: &["checkpoint", "branch", "mode"],
    },
    ToolContract {
        name: "inspect",
        allowed: &["selector", "session"],
        required: &["selector"],
    },
];

fn tool_contract(name: &str) -> Option<&'static ToolContract> {
    TOOL_CONTRACTS.iter().find(|contract| contract.name == name)
}

fn allowed_fields(name: &str) -> &'static [&'static str] {
    tool_contract(name)
        .map(|contract| contract.allowed)
        .unwrap_or_default()
}

fn tool_names() -> Vec<&'static str> {
    TOOL_CONTRACTS
        .iter()
        .map(|contract| contract.name)
        .collect()
}

fn tools() -> Vec<Value> {
    vec![
        tool(
            "repository_init",
            "Initialize and ingest the bound repository. Git-tracked files and non-ignored untracked files are source; human-owned repository-root .sunignore explicitly excludes additional paths, including tracked paths, and remains visible. Sunlight does not scan or hide secret-like content. .git and .sunlight are intrinsic exclusions. Call again after a human changes .sunignore: a clean state refreshes automatically, while authored history fails closed with preservation guidance.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "repository_status",
            "Read persisted repository or object status. Omit id for repository scope; every other scope requires the exact matching object id. Completed topic status includes its structured handoff.",
            json!({"scope":status_scope_schema(),"id":id_schema("Required exact object id whenever scope is not repository; omit for repository scope.")}),
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
            "Start a topic-bound authoring session over an exact base, current, or checkpointed resolved view. The session writes only to the supplied topic; its initial read frontier is copied from the supplied view.",
            json!({"topic":id_schema("Exact topic_id returned by topic_create or inspect, or its unique topic slug."),"view":id_schema("Exact resolved_view_id returned by repository_status, view_resolve, or checkpoint_create."),"actor":id_schema("Stable caller-chosen actor identifier used for provenance.")}),
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
            "Set artifact classification with CAS. source checkpoints and exports normally. generated checkpoints normally but exports only with reachable execution-output promotion provenance; relabeling an artifact as generated does not create that provenance.",
            json!({"path":path_schema(),"session":id_schema("Exact authoring session_id."),"expect_hash":existing_hash_schema(),"classification":class_schema()}),
            &["path", "session", "expect_hash", "classification"],
            true,
        ),
        tool(
            "view_resolve",
            "Resolve one exact revision per topic over the base checkpoint. Supply include for durable integration. Omitting include is discovery-only and resolves moving current heads. Every supplied selection is echoed as requested_frontier and normalized_frontier; dependencies, conflicts, and staleness are returned as facts rather than merged implicitly.",
            json!({"base":id_schema("Exact base checkpoint_id."),"include":{"type":"array","maxItems":128,"description":"Exact topic revision selections for durable integration. Omit only to discover the current heads. A topic may appear at most once.","items":{"type":"object","additionalProperties":false,"properties":{"topic":id_schema("Exact topic_id, not a slug."),"revision":id_schema("Exact topic_revision_id belonging to topic.")},"required":["topic","revision"]}}}),
            &["base"],
            true,
        ),
        tool(
            "project_materialize",
            "Materialize a managed projection inside the bound repository policy root. On Windows, automatic read-only inspection uses a verified-cache hardlink strategy; writable purposes prefer copy-on-write and safely fall back to private copies.",
            json!({"view":id_schema("Exact conflict-free resolved_view_id to materialize."),"purpose":{"type":"string","description":"Consumer-specific projection policy and cache namespace.","enum":["execution","compatibility","inspection","export"]},"strategy":{"type":"string","description":"Optional required or preferred filesystem strategy.","enum":["copy","reflink","hardlink_readonly","overlay_copyup"]},"require_strategy":{"type":"boolean","description":"When true, fail instead of using a safe fallback strategy.","default":false}}),
            &["view", "purpose"],
            true,
        ),
        tool(
            "compat_project",
            "Create a fresh compatibility projection from a session's current generation. Filesystem changes remain adapter-local until compat_diff and compat_import create a native operation.",
            json!({"session":id_schema("Exact session_id whose current generation becomes the compatibility baseline.")}),
            &["session"],
            true,
        ),
        tool(
            "compat_diff",
            "Diff a managed compatibility projection against its persisted baseline. Re-echoes the baseline session_generation_id required by compat_import.",
            json!({"projection":id_schema("Exact compatibility projection_id returned by compat_project.")}),
            &["projection"],
            false,
        ),
        tool(
            "compat_import",
            "Import selected compatibility candidates as one native transaction.",
            json!({"projection":id_schema("Exact compatibility projection_id returned by compat_project."),"candidates":{"type":"array","description":"Exact candidate_delta_id values returned by compat_diff.","minItems":1,"maxItems":128,"items":id_schema("One exact candidate_delta_id.")},"session_generation":id_schema("Exact session_generation_id that owns the compatibility baseline.")}),
            &["projection", "candidates", "session_generation"],
            true,
        ),
        tool(
            "execution_run",
            "Run one bare non-shell program with structured arguments against an exact view. Returns bounded stdout/stderr text in output_text for immediate diagnosis, capture digests, phase_timings_ms, classified file deltas, and only actionable source/generated promotion candidates. Known build/cache paths are classified without per-file subprocesses; remaining Git ignore checks are batched and bounded.",
            json!({"view":id_schema("Exact conflict-free resolved_view_id to execute."),"program":{"type":"string","description":"Bare executable name; shells and host paths are rejected."},"args":{"type":"array","description":"Structured argv entries passed without a shell.","maxItems":256,"items":{"type":"string","maxLength":16384}},"cwd":{"type":"string","description":"Repository-relative projection cwd.","default":"."},"network":{"type":"string","enum":["disabled","not_enforced"],"description":"Optional per-run network policy override."}}),
            &["view", "program"],
            true,
        ),
        tool(
            "execution_promote_output",
            "Promote one classified regular-file execution output into a session-owned operation. Pass the candidate classification returned by execution_run verbatim: source_like_delta or generated_artifact. These are execution provenance classes, not artifact classes. Ignored, log, cache, and outputs larger than 2 MiB remain local-only and fail closed with recovery facts. Sunlight does not classify content as secret.",
            json!({"execution":id_schema("Exact execution_id that produced the candidate."),"path":path_schema(),"session":id_schema("Exact authoring session_id that will own the promoted operation."),"classification":promotion_class_schema()}),
            &["execution", "path", "session", "classification"],
            true,
        ),
        tool(
            "checkpoint_create",
            "Freeze an exact resolved view with optional validated passing execution evidence. source and generated artifacts are both checkpointed; export later requires reachable promotion provenance for each generated artifact.",
            json!({"view":id_schema("Exact conflict-free resolved_view_id to freeze."),"execution":id_schema("Optional passing execution_id whose view and tree exactly match.")}),
            &["view"],
            true,
        ),
        tool(
            "policy_check_export",
            "Validate and persist export policy for an exact checkpoint and target Git ref. This is the source-artifact safety gate; a passing result returns the validation_report_id accepted by policy_explain and Git export.",
            json!({"checkpoint":id_schema("Exact checkpoint_id to validate."),"branch":{"type":"string","description":"Required target Git branch or fully qualified ref for this validation.","minLength":1,"maxLength":16384}}),
            &["checkpoint", "branch"],
            false,
        ),
        tool(
            "policy_check_commit",
            "Validate Sunlight's own commit metadata, not application source. Omit paths to check the managed .gitignore block, or supply only .sunlight/** metadata candidates. For exact source safety, create a checkpoint and call policy_check_export with its target ref. This check returns an inline report and does not persist a validation_report_id.",
            json!({"paths":{"type":"array","maxItems":256,"description":"Optional .sunlight/** metadata paths. Application-source paths are outside this tool's scope.","items":sunlight_metadata_path_schema()}}),
            &[],
            false,
        ),
        tool(
            "policy_explain",
            "Explain a persisted export-policy validation report.",
            json!({"validation_report":id_schema("Exact persisted validation_report_id returned by policy_check_export or git_export.")}),
            &["validation_report"],
            false,
        ),
        tool(
            "git_export",
            "Plan or execute Git export of a checkpoint to a branch in the bound repository. A completed handoff returns an export_map_id that maps the exact checkpoint and target ref to the resulting commit.",
            json!({"checkpoint":id_schema("Exact checkpoint_id to export."),"branch":{"type":"string","description":"Target Git branch or ref."},"mode":{"type":"string","description":"Plan without Git mutation or execute the validated local export.","enum":["plan","execute"]}}),
            &["checkpoint", "branch", "mode"],
            true,
        ),
        tool(
            "inspect",
            "Inspect persisted repository objects with a typed selector.",
            json!({"selector":inspect_selector_schema(),"session":id_schema("Optional exact session_id used to disambiguate session-relative artifact inspection.")}),
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
    let contract = tool_contract(name).expect("every advertised tool has one contract row");
    debug_assert_eq!(required, contract.required);
    json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":contract.required},"outputSchema":output_schema(name),"annotations":{"readOnlyHint":!mutating,"destructiveHint":matches!(name,"artifact_delete"|"git_export"),"idempotentHint":matches!(name,"repository_init"|"repository_status"|"topic_complete"|"topic_wait"|"artifact_read"|"artifact_list"|"artifact_search"|"compat_diff"|"policy_check_export"|"policy_check_commit"|"policy_explain"|"inspect")}})
}

fn output_schema(name: &str) -> Value {
    let mut data = serde_json::Map::new();
    data.insert(
        "command".to_string(),
        json!({"type":"string","description":"Stable Sunlight command name that produced this envelope."}),
    );
    data.insert(
        "repository_id".to_string(),
        json!({"type":"string","description":"Canonical repository identity when the command is repository-backed."}),
    );
    data.insert(
        "transport".to_string(),
        json!({
            "type":"object",
            "description":"Per-call MCP observability for interactive latency and contention measurement.",
            "required":["queue_ms","worker_ms","automatic_concurrency_retries"],
            "properties":{
                "queue_ms":{"type":"integer","minimum":0,"description":"Time spent in the local server queue plus any cross-process repository mutation queue, excluding worker execution."},
                "worker_ms":{"type":"integer","minimum":0,"description":"Total time spent validating and executing this tool in its worker."},
                "automatic_concurrency_retries":{"type":"integer","minimum":0,"description":"Safe engine retries caused by writer-lock or state-sequence contention."}
            },
            "additionalProperties":false
        }),
    );

    let ids = output_ids(name);
    if !ids.is_empty() {
        let properties = ids
            .iter()
            .map(|id| {
                (
                    (*id).to_string(),
                    json!({"type":["string","null"],"description":format!("Exact {id} returned by this operation when applicable.")}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        data.insert(
            "ids".to_string(),
            json!({"type":"object","description":"Exact native identities for chaining subsequent tools.","properties":properties,"additionalProperties":true}),
        );
    }

    for (field, description) in output_payloads(name) {
        data.insert(field.to_string(), json!({"description":description}));
    }

    json!({
        "type":"object",
        "description":"One Sunlight envelope: ok=true with tool-specific data and warnings, or ok=false with a stable structured error.",
        "required":["ok"],
        "properties":{
            "ok":{"type":"boolean"},
            "data":{"type":"object","description":format!("Successful {name} result. Use exact returned IDs and hashes as later inputs."),"properties":data,"additionalProperties":true},
            "error":{"type":"object","description":"Stable native error with code, message, inspectable details, and one concrete recovery action.","required":["code","message","details","next_action"],"properties":{"code":{"type":"string"},"message":{"type":"string"},"details":{"type":"object"},"next_action":{"type":"string","description":"The safest normal next action derived from this error code. Inspect returned details and exact IDs before acting."}},"additionalProperties":true},
            "warnings":{"type":["array","object"],"description":"Advisory facts that do not replace hard error states."}
        },
        "additionalProperties":false
    })
}

fn output_ids(name: &str) -> &'static [&'static str] {
    match name {
        "repository_init" => &["repository_id", "checkpoint_id", "resolved_view_id"],
        "repository_status" => &["repository_id"],
        "topic_create" => &["topic_id", "topic_revision_id"],
        "topic_complete" | "topic_wait" => &["topic_id", "topic_revision_id", "session_id"],
        "session_start" | "session_refresh" => &[
            "topic_id",
            "session_id",
            "session_generation_id",
            "resolved_view_id",
        ],
        "artifact_read" | "artifact_list" | "artifact_search" => {
            &["session_id", "session_generation_id", "resolved_view_id"]
        }
        "artifact_patch"
        | "artifact_write"
        | "artifact_move"
        | "artifact_delete"
        | "artifact_metadata_set"
        | "compat_import" => &[
            "operation_transaction_id",
            "topic_revision_id",
            "session_generation_id",
            "resolved_view_id",
        ],
        "view_resolve" => &["resolved_view_id"],
        "project_materialize" | "compat_project" => &["projection_id", "resolved_view_id"],
        "compat_diff" => &["projection_id", "resolved_view_id", "session_generation_id"],
        "execution_run" => &["execution_id", "projection_id", "resolved_view_id"],
        "execution_promote_output" => &["execution_id", "operation_transaction_id"],
        "checkpoint_create" => &["checkpoint_id", "resolved_view_id", "execution_id"],
        "policy_check_export" | "policy_explain" => &["validation_report_id"],
        "policy_check_commit" => &["repository_id"],
        "git_export" => &["checkpoint_id", "export_map_id", "validation_report_id"],
        "inspect" => &[],
        _ => &[],
    }
}

fn output_payloads(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "repository_init" | "repository_status" => &[(
            "repository",
            "Repository lifecycle, policy, and health facts.",
        )],
        "topic_create" => &[("topic", "Created durable topic record.")],
        "topic_complete" => &[
            ("topic", "Completed topic record."),
            ("handoff", "Immutable factual completion handoff."),
        ],
        "topic_wait" => &[
            ("topic", "Observed topic status."),
            (
                "handoff",
                "Immutable factual completion handoff when available.",
            ),
            ("wait", "Wait outcome and timing facts."),
        ],
        "session_start" | "session_refresh" => &[
            ("session", "Exact authoring session and frontier facts."),
            ("view", "Exact session-visible resolved view."),
        ],
        "artifact_read" => &[
            (
                "artifact",
                "Persisted artifact identity, hash, classification, and content.",
            ),
            ("content", "UTF-8 artifact content when readable."),
        ],
        "artifact_list" => &[("artifacts", "Ordered persisted artifact summaries.")],
        "artifact_search" => &[("matches", "Bounded persisted-content search matches.")],
        "artifact_patch"
        | "artifact_write"
        | "artifact_move"
        | "artifact_delete"
        | "artifact_metadata_set" => &[
            ("operation", "Atomic topic-owned operation transaction."),
            ("artifact", "Before and after artifact facts."),
            ("view", "Exact post-operation session view."),
        ],
        "view_resolve" => &[
            (
                "resolved_view",
                "Exact normalized frontier and tree identity.",
            ),
            (
                "conflicts",
                "Inspectable conflict records that block downstream use.",
            ),
            (
                "staleness",
                "Inspectable dependency staleness records that block downstream use.",
            ),
        ],
        "project_materialize" | "compat_project" => &[
            (
                "projection",
                "Managed projection identity, strategy, root handle, and policy.",
            ),
            (
                "metrics",
                "Materialization cost, cache, and amplification measurements.",
            ),
        ],
        "compat_diff" => &[(
            "candidates",
            "Explicit compatibility deltas available for import.",
        )],
        "compat_import" => &[
            ("operation", "Atomic native import transaction."),
            (
                "candidates",
                "Selected compatibility candidates and outcomes.",
            ),
        ],
        "execution_run" => &[
            (
                "execution",
                "Persisted execution result and environment evidence.",
            ),
            (
                "output_text",
                "Bounded stdout and stderr text for diagnosis.",
            ),
            ("phase_timings_ms", "Measured execution phase timings."),
            ("file_deltas", "Classified projection file changes."),
            (
                "promotion_candidates",
                "Only actionable source-like or generated outputs.",
            ),
        ],
        "execution_promote_output" => &[
            ("promotion", "Execution provenance for the promoted output."),
            ("operation", "Topic-owned operation created by promotion."),
        ],
        "checkpoint_create" => &[(
            "checkpoint",
            "Frozen exact view, tree, and selected evidence.",
        )],
        "policy_check_export" | "policy_explain" => &[(
            "validation_report",
            "Persisted export-policy checks, warnings, and blocking failures.",
        )],
        "policy_check_commit" => &[(
            "validation_report",
            "Inline Sunlight-metadata commit checks; this report is not persisted.",
        )],
        "git_export" => &[
            (
                "validation_report",
                "Persisted export-policy report used by the handoff when returned.",
            ),
            (
                "export_map",
                "Planned or persisted checkpoint-to-Git mapping when returned.",
            ),
            ("planned_commit", "Planned Git commit for plan mode."),
            ("ref_update", "Planned Git ref update for plan mode."),
            (
                "created_commit_id",
                "Created Git commit identity for completed execute mode.",
            ),
        ],
        "inspect" => &[(
            "object",
            "Typed persisted object or provenance response selected by the caller.",
        )],
        _ => &[],
    }
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
    json!({
        "type":"string",
        "enum":ARTIFACT_CLASSIFICATIONS,
        "description":"Artifact lifecycle class. source is checkpointed and exportable. generated is checkpointed but exportable only with reachable execution-output promotion provenance."
    })
}
fn promotion_class_schema() -> Value {
    json!({"type":"string","enum":PROMOTION_CLASSIFICATIONS,"description":"Exact classification returned for this promotion candidate by execution_run."})
}
fn status_scope_schema() -> Value {
    let scopes = STATUS_SCOPE_FLAGS
        .iter()
        .map(|(scope, _)| *scope)
        .collect::<Vec<_>>();
    json!({
        "type":"string",
        "description":"Select repository for the whole repository, or a persisted native object type paired with id.",
        "enum":scopes,
        "default":"repository"
    })
}
fn inspect_selector_schema() -> Value {
    json!({
        "type":"string",
        "description":format!(
            "repository or a persisted native object selector beginning with {}",
            INSPECT_SELECTOR_PREFIXES.join(", ")
        )
    })
}
fn sunlight_metadata_path_schema() -> Value {
    json!({
        "type":"string",
        "pattern":r"^\.sunlight(?:/|$)",
        "description":"Repository-relative .sunlight metadata path; application artifacts are not accepted."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let number = TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("sun-mcp-unit-{}-{number}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn private_temp() -> (TestRoot, Arc<PrivateTemp>) {
        let root = TestRoot::new();
        let temp = PrivateTemp::new(&root.0).unwrap();
        (root, temp)
    }

    #[test]
    fn advertised_tools_have_no_fixture_vocabulary() {
        let encoded = serde_json::to_string(&tools()).unwrap();
        assert!(!encoded.contains("fixture"));
        assert_eq!(tools().len(), tool_names().len());
    }
    #[test]
    fn run_rejects_shells_and_host_paths() {
        let (root, temp) = private_temp();
        let shell = build_invocation(
            "execution_run",
            &json!({"view":"v","program":"powershell","args":[]}),
            &root.0,
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
    fn oversized_patch_is_rejected_before_any_private_staging_write() {
        let (root, temp) = private_temp();
        let oversized = "x".repeat(MAX_CONTENT_BYTES + 1);
        let invocation = build_invocation(
            "artifact_patch",
            &json!({
                "path":"README.md",
                "session":"session_a",
                "expect_hash":format!("sha256:{}", "0".repeat(64)),
                "patch":oversized
            }),
            &root.0,
            &temp,
        );

        assert!(matches!(
            invocation,
            Err(ToolFailure {
                code: "mcp_content_too_large",
                ..
            })
        ));
        assert_eq!(fs::read_dir(&temp.root).unwrap().count(), 0);
    }

    #[test]
    fn repository_mutation_queue_covers_short_state_writers_only() {
        for name in [
            "repository_init",
            "topic_create",
            "topic_complete",
            "session_start",
            "session_refresh",
            "artifact_patch",
            "artifact_write",
            "artifact_move",
            "artifact_delete",
            "artifact_metadata_set",
            "view_resolve",
            "execution_promote_output",
            "checkpoint_create",
            "policy_check_export",
            "git_export",
        ] {
            assert!(tool_uses_repository_mutation_queue(name), "{name}");
        }
        for name in [
            "repository_status",
            "topic_wait",
            "artifact_read",
            "artifact_list",
            "artifact_search",
            "project_materialize",
            "compat_project",
            "compat_diff",
            "compat_import",
            "execution_run",
            "policy_check_commit",
            "policy_explain",
            "inspect",
        ] {
            assert!(!tool_uses_repository_mutation_queue(name), "{name}");
        }
    }

    #[test]
    fn repository_mutation_queue_serializes_independent_file_handles() {
        let root = TestRoot::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let first = acquire_repository_mutation_queue(&root.0, &cancel).unwrap();
        let second_root = root.0.clone();
        let second_cancel = Arc::clone(&cancel);
        let started = Instant::now();
        let waiter = thread::spawn(move || {
            let guard = acquire_repository_mutation_queue(&second_root, &second_cancel).unwrap();
            let waited = started.elapsed();
            drop(guard);
            waited
        });
        thread::sleep(Duration::from_millis(40));
        assert!(!waiter.is_finished());
        drop(first);
        assert!(waiter.join().unwrap() >= Duration::from_millis(30));
    }

    #[test]
    fn repository_mutation_queue_wait_is_cancellable() {
        let root = TestRoot::new();
        let first_cancel = AtomicBool::new(false);
        let _first = acquire_repository_mutation_queue(&root.0, &first_cancel).unwrap();
        let cancelled = AtomicBool::new(true);
        let error = acquire_repository_mutation_queue(&root.0, &cancelled).unwrap_err();
        assert_eq!(error.code, "request_cancelled");
    }

    #[test]
    fn agent_contract_exposes_creation_patch_and_completion_facts() {
        let advertised = tools();
        let find = |name: &str| advertised.iter().find(|tool| tool["name"] == name).unwrap();

        for tool in &advertised {
            let name = tool["name"].as_str().unwrap();
            let contract = tool_contract(name).unwrap();
            assert_eq!(tool["inputSchema"]["required"], json!(contract.required));
            let advertised_fields = tool["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            let contract_fields = contract
                .allowed
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(advertised_fields, contract_fields, "{name}");
            let error = &tool["outputSchema"]["properties"]["error"];
            assert!(error["required"]
                .as_array()
                .unwrap()
                .contains(&json!("next_action")));
            assert_eq!(error["properties"]["next_action"]["type"], "string");
            let transport = &tool["outputSchema"]["properties"]["data"]["properties"]["transport"];
            assert_eq!(transport["properties"]["queue_ms"]["type"], "integer");
            assert_eq!(
                transport["properties"]["automatic_concurrency_retries"]["type"],
                "integer"
            );
        }

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
        assert!(wait["outputSchema"]["properties"]["data"]["properties"]
            .get("handoff")
            .is_some());
        assert!(wait["outputSchema"]["properties"]["data"]["properties"]
            .get("wait")
            .is_some());

        let promote = find("execution_promote_output");
        assert_eq!(
            promote["inputSchema"]["properties"]["classification"]["enum"],
            json!(["source_like_delta", "generated_artifact"])
        );
        assert!(promote["description"]
            .as_str()
            .unwrap()
            .contains("returned by execution_run verbatim"));

        let write = find("artifact_write");
        assert_eq!(
            write["inputSchema"]["properties"]["classification"]["enum"],
            json!(["source", "generated"])
        );
        assert!(
            write["inputSchema"]["properties"]["classification"]["description"]
                .as_str()
                .unwrap()
                .contains("promotion provenance")
        );

        let status = find("repository_status");
        assert!(!status["inputSchema"]["properties"]["scope"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("git")));
        let inspect = find("inspect");
        assert!(
            !inspect["inputSchema"]["properties"]["selector"]["description"]
                .as_str()
                .unwrap()
                .contains("git:")
        );

        let export_policy = find("policy_check_export");
        assert_eq!(
            export_policy["inputSchema"]["required"],
            json!(["checkpoint", "branch"])
        );
        let commit_policy = find("policy_check_commit");
        assert_eq!(
            commit_policy["outputSchema"]["properties"]["data"]["properties"]["ids"]["properties"]
                .get("validation_report_id"),
            None
        );
        assert!(commit_policy["description"]
            .as_str()
            .unwrap()
            .contains("does not persist a validation_report_id"));

        let compat_diff = find("compat_diff");
        assert!(
            compat_diff["outputSchema"]["properties"]["data"]["properties"]["ids"]["properties"]
                .get("session_generation_id")
                .is_some()
        );
        assert_eq!(
            find("compat_import")["inputSchema"]["required"],
            json!(["projection", "candidates", "session_generation"])
        );
    }

    #[test]
    fn fixture_only_git_lookup_is_not_an_mcp_contract() {
        let root = TestRoot::new();
        let temp = PrivateTemp::new(&root.0).unwrap();

        for (name, arguments) in [
            ("repository_status", json!({"scope":"git","id":"HEAD"})),
            ("inspect", json!({"selector":"git:HEAD"})),
        ] {
            let error = match build_invocation(name, &arguments, &root.0, &temp) {
                Ok(_) => panic!("{name} must reject fixture-only Git lookup"),
                Err(error) => error,
            };
            assert_eq!(error.code, "invalid_request");
        }
    }

    #[test]
    fn promotion_invocation_accepts_execution_candidate_classes_only() {
        let root = TestRoot::new();
        let temp = PrivateTemp::new(&root.0).unwrap();
        let invocation = build_invocation(
            "execution_promote_output",
            &json!({
                "execution":"exec_native_0001",
                "path":"src/generated.rs",
                "session":"session_agent_a",
                "classification":"source_like_delta"
            }),
            &root.0,
            &temp,
        )
        .unwrap();
        assert_eq!(
            invocation.argv,
            vec![
                "execution",
                "promote-output",
                "exec_native_0001",
                "--path",
                "src/generated.rs",
                "--session",
                "session_agent_a",
                "--classification",
                "source_like_delta"
            ]
        );

        let error = match build_invocation(
            "execution_promote_output",
            &json!({
                "execution":"exec_native_0001",
                "path":"src/generated.rs",
                "session":"session_agent_a",
                "classification":"source"
            }),
            &root.0,
            &temp,
        ) {
            Ok(_) => panic!("artifact classification must not be accepted for promotion"),
            Err(error) => error,
        };
        assert_eq!(error.code, "invalid_request");
        assert!(error
            .message
            .contains("source_like_delta, generated_artifact"));
    }

    #[test]
    fn artifact_invocation_accepts_only_lifecycle_classes() {
        let root = TestRoot::new();
        let temp = PrivateTemp::new(&root.0).unwrap();
        let error = match build_invocation(
            "artifact_metadata_set",
            &json!({
                "path":"Cargo.lock",
                "session":"session_agent_a",
                "expect_hash":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "classification":"lockfile"
            }),
            &root.0,
            &temp,
        ) {
            Ok(_) => panic!("non-lifecycle artifact class must be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.code, "invalid_request");
        assert!(error.message.contains("source, generated"));
    }

    #[test]
    fn structured_errors_include_specific_recovery_actions() {
        let mut tool_error = tool_failure_result(ToolFailure::new(
            "artifact_read_scope_invalid",
            "choose one read scope",
        ));
        attach_transport_metrics(&mut tool_error, 7, 11, 2);
        assert_eq!(
            tool_error["structuredContent"]["error"]["next_action"],
            "Supply exactly one scope: a session for topic-bound authoring context or an exact resolved view for read-only access."
        );
        assert_eq!(
            tool_error["structuredContent"]["error"]["details"]["transport"],
            json!({"queue_ms":7,"worker_ms":11,"automatic_concurrency_retries":2})
        );

        let native_error = super::super::CliError::new(
            "precondition_failed",
            "the artifact changed after it was read",
        )
        .with_detail("actual", "sha256:current");
        let envelope: Value =
            serde_json::from_str(&super::super::failure_envelope(&native_error)).unwrap();
        assert_eq!(envelope["error"]["details"]["actual"], "sha256:current");
        assert!(envelope["error"]["next_action"]
            .as_str()
            .unwrap()
            .contains("returned exact hash and IDs"));
    }

    #[test]
    fn local_mcp_documentation_names_every_advertised_tool() {
        let documentation = include_str!("../../../docs/local_mcp.md");
        for name in tool_names() {
            assert!(
                documentation.contains(&format!("`{name}`")),
                "local MCP documentation omits {name}"
            );
        }
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

        let (root, temp) = private_temp();
        let invocation = build_invocation(
            "artifact_read",
            &json!({"path":"README.md","view":"view_exact"}),
            &root.0,
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
            &root.0,
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
        let (root, temp) = private_temp();
        let argv = build_invocation(
            "view_resolve",
            &json!({
                "base":"checkpoint_base_0001",
                "include":[
                    {"topic":"topic_a","revision":"rev_a_0001"},
                    {"topic":"topic_b","revision":"rev_b_0001"}
                ]
            }),
            &root.0,
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
