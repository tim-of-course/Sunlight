use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

#[test]
fn stdio_mcp_real_repository_journey_and_recovery() {
    let temp = TempDir::new("sun-mcp");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Sun MCP Test"]);
    git(&repo, &["config", "user.email", "sun-mcp@example.invalid"]);
    fs::write(repo.join("README.md"), "# MCP repository\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    let mut mcp = Mcp::start(&repo);
    let initialized = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-integration-test","version":"1"}
        }),
    );
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(
        initialized["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    mcp.notify("notifications/initialized", json!({}));

    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 26);
    let advertised = serde_json::to_string(tools).unwrap();
    assert!(!advertised.to_ascii_lowercase().contains("fixture"));
    for required in [
        "repository_init",
        "repository_status",
        "topic_create",
        "session_start",
        "session_refresh",
        "artifact_read",
        "artifact_write",
        "execution_run",
        "checkpoint_create",
        "git_export",
        "inspect",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == required),
            "missing {required}"
        );
    }

    let reinit = mcp.call(3, "repository_init", json!({}));
    assert_eq!(reinit["ok"], true);
    assert_eq!(reinit["data"]["command"], "repository.init");
    let status = mcp.call(4, "repository_status", json!({}));
    assert_eq!(status["data"]["command"], "status.repository");
    let repository = mcp.call(5, "inspect", json!({"selector":"repository"}));
    assert_eq!(repository["data"]["command"], "inspect.repository");

    let topic = mcp.call(
        6,
        "topic_create",
        json!({"slug":"mcp-authoring","display_name":"MCP authoring"}),
    );
    assert_eq!(topic["data"]["command"], "topic.create");
    let session = mcp.call(
        7,
        "session_start",
        json!({"topic":"mcp-authoring","view":"view_base_0001","actor":"mcp-agent"}),
    );
    assert_eq!(session["data"]["command"], "session.start");
    let session_id = session["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = mcp.call(
        8,
        "artifact_read",
        json!({"path":"README.md","session":session_id}),
    );
    assert_eq!(read["data"]["command"], "artifact.read");
    let read_hash = read["data"]["artifacts"][0]["content_hash"]
        .as_str()
        .unwrap();
    assert!(read_hash.starts_with("sha256:"));
    let write = mcp.call(
        9,
        "artifact_write",
        json!({
            "path":"mcp/note.txt",
            "session":session_id,
            "expect_hash":"new",
            "content":"written through typed MCP content\n",
            "classification":"source"
        }),
    );
    assert_eq!(write["data"]["command"], "artifact.write");
    let view = write["data"]["view"]["resolved_view_id"]
        .as_str()
        .expect("write response should identify its after view")
        .to_string();
    let refreshed = mcp.call(
        10,
        "session_refresh",
        json!({"session":session_id,"policy":"none"}),
    );
    assert_eq!(refreshed["data"]["command"], "session.refresh");
    let session_status = mcp.call(
        11,
        "repository_status",
        json!({"scope":"session","id":session_id}),
    );
    assert_eq!(session_status["data"]["command"], "status.session");
    let session_inspect = mcp.call(
        12,
        "inspect",
        json!({"selector":format!("session:{session_id}")}),
    );
    assert_eq!(session_inspect["data"]["command"], "inspect.session");

    let execution = mcp.call(
        13,
        "execution_run",
        json!({"view":view,"program":"git","args":["--version"],"cwd":"."}),
    );
    assert_eq!(execution["data"]["command"], "execution.run");
    let checkpoint = mcp.call(14, "checkpoint_create", json!({"view":view}));
    assert_eq!(checkpoint["data"]["command"], "checkpoint.create");
    let checkpoint_id = checkpoint["data"]["checkpoint"]["id"]
        .as_str()
        .or_else(|| checkpoint["data"]["checkpoint_id"].as_str())
        .expect("checkpoint response should identify checkpoint")
        .to_string();
    let export = mcp.call(
        15,
        "git_export",
        json!({
            "checkpoint":checkpoint_id,
            "branch":"refs/heads/sunlight/mcp-test",
            "mode":"plan"
        }),
    );
    assert!(export["data"]["command"]
        .as_str()
        .unwrap()
        .starts_with("git.export"));

    mcp.raw("{malformed");
    let parse_error = mcp.read();
    assert_eq!(parse_error["error"]["code"], -32700);
    let pong = mcp.request(16, "ping", json!({}));
    assert_eq!(pong["result"], json!({}));
    let sent = mcp.sent.join("\n");
    assert!(!sent.contains("--fixture"));
    assert!(!sent.contains("\"fixture\""));
    mcp.shutdown();
}

struct Mcp {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    sent: Vec<String>,
}

impl Mcp {
    fn start(repo: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sun"))
            .args(["mcp", "serve", "--repo"])
            .arg(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout,
            sent: Vec::new(),
        }
    }
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let response = self.read();
        assert_eq!(response["id"], id);
        response
    }
    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}));
    }
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        let response = self.request(id, "tools/call", json!({"name":name,"arguments":arguments}));
        assert!(
            response.get("error").is_none(),
            "protocol error: {response}"
        );
        let result = &response["result"];
        assert_ne!(result["isError"], true, "tool error: {result}");
        result["structuredContent"].clone()
    }
    fn send(&mut self, value: Value) {
        self.raw(&value.to_string());
    }
    fn raw(&mut self, value: &str) {
        self.sent.push(value.to_string());
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{value}").unwrap();
        stdin.flush().unwrap();
    }
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "MCP server closed stdout");
        serde_json::from_str(&line).unwrap()
    }
    fn shutdown(&mut self) {
        self.stdin.take();
        let status = self.child.as_mut().unwrap().wait().unwrap();
        assert!(status.success());
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        self.stdin.take();
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
