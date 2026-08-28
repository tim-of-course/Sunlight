use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

#[cfg(windows)]
const PYTHON: &str = "python";
#[cfg(not(windows))]
const PYTHON: &str = "python3";

#[test]
fn stdio_mcp_topic_create_matches_cli_durable_intent_metadata() {
    let temp = TempDir::new("sun-mcp-topic-metadata");
    let cli_repo = temp.path().join("cli");
    let mcp_repo = temp.path().join("mcp");
    for repo in [&cli_repo, &mcp_repo] {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "--quiet"]);
        git(repo, &["config", "user.name", "Sun MCP Test"]);
        git(repo, &["config", "user.email", "sun-mcp@example.invalid"]);
        fs::write(repo.join("README.md"), "# Topic metadata\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(repo, &["commit", "--quiet", "-m", "base"]);
    }

    let _ = sun_json(&cli_repo, &["init"]);
    let cli = sun_json(
        &cli_repo,
        &[
            "topic",
            "create",
            "typed-intent",
            "--display-name",
            "Typed intent",
            "--owner",
            "intent-agent",
            "--visibility",
            "private",
            "--acceptance-criterion",
            "first criterion",
            "--acceptance-criterion",
            "second criterion",
        ],
    );

    let mut mcp = Mcp::start(&mcp_repo);
    let _ = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-topic-contract-test","version":"1"}
        }),
    );
    mcp.notify("notifications/initialized", json!({}));
    let _ = mcp.call(2, "repository_init", json!({}));
    let mcp_created = mcp.call(
        3,
        "topic_create",
        json!({
            "slug":"typed-intent",
            "display_name":"Typed intent",
            "owner":"intent-agent",
            "visibility":"private",
            "acceptance_criteria":["first criterion", "second criterion"]
        }),
    );
    assert_eq!(
        normalize_repository_identity(cli["data"]["topic"].clone()),
        normalize_repository_identity(mcp_created["data"]["topic"].clone())
    );
    let inspected = mcp.call(4, "inspect", json!({"selector":"topic:typed-intent"}));
    assert_eq!(inspected["data"]["topic"], mcp_created["data"]["topic"]);
    mcp.shutdown();
}

#[test]
fn stdio_mcp_bounds_large_engine_output_and_remains_usable() {
    let temp = TempDir::new("sun-mcp-large-output");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("large.txt"), vec![b'x'; 8 * 1024 * 1024 + 1024]).unwrap();

    let mut mcp = Mcp::start(&repo);
    let initialized = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-output-bound-test","version":"1"}
        }),
    );
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    mcp.notify("notifications/initialized", json!({}));

    assert_eq!(mcp.call(2, "repository_init", json!({}))["ok"], true);
    mcp.call(
        3,
        "topic_create",
        json!({"slug":"large-output","display_name":"Large output"}),
    );
    let session = mcp.call(
        4,
        "session_start",
        json!({"topic":"large-output","view":"view_base_0001","actor":"large-agent"}),
    );
    let session_id = session["data"]["ids"]["session_id"].as_str().unwrap();
    let error = mcp.call_error(
        5,
        "artifact_read",
        json!({"path":"large.txt","session":session_id}),
    );
    assert_eq!(error["error"]["code"], "mcp_stdout_too_large");
    assert_eq!(
        error["error"]["message"],
        "engine response exceeded the MCP response limit"
    );
    assert_eq!(error["error"]["details"]["max_bytes"], 8 * 1024 * 1024);

    let status = mcp.call(6, "repository_status", json!({}));
    assert_eq!(status["data"]["command"], "status.repository");
    mcp.shutdown();
}

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
    assert_eq!(tools.len(), 28);
    let advertised = serde_json::to_string(tools).unwrap();
    assert!(!advertised.to_ascii_lowercase().contains("fixture"));
    for required in [
        "repository_init",
        "repository_status",
        "topic_create",
        "topic_wait",
        "session_start",
        "session_refresh",
        "artifact_read",
        "artifact_write",
        "topic_complete",
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
    let revision = write["data"]["ids"]["topic_revision_id"]
        .as_str()
        .expect("write response should identify its exact revision")
        .to_string();
    let direct_read = mcp.call(
        90,
        "artifact_read",
        json!({"path":"mcp/note.txt","view":view}),
    );
    assert_eq!(direct_read["data"]["access_mode"], "read_only_view");
    assert_eq!(direct_read["data"]["ids"]["resolved_view_id"], view);
    assert_eq!(
        direct_read["data"]["content"]["bytes"],
        "written through typed MCP content\n"
    );
    let direct_list = mcp.call(91, "artifact_list", json!({"prefix":"mcp","view":view}));
    assert_eq!(direct_list["data"]["artifacts"][0]["path"], "mcp/note.txt");
    let direct_search = mcp.call(
        92,
        "artifact_search",
        json!({"query":"typed MCP","view":view}),
    );
    assert_eq!(direct_search["data"]["matches"][0]["path"], "mcp/note.txt");
    let invalid_scope = mcp.call_error(
        93,
        "artifact_read",
        json!({"path":"mcp/note.txt","session":session_id,"view":view}),
    );
    assert_eq!(
        invalid_scope["error"]["code"],
        "artifact_read_scope_invalid"
    );
    let completed = mcp.call(
        94,
        "topic_complete",
        json!({
            "topic":"topic_mcp_authoring",
            "revision":revision,
            "session":session_id,
            "summary":"MCP authoring change finished"
        }),
    );
    assert_eq!(completed["data"]["command"], "topic.complete");

    let repeated_completion = mcp.call(
        95,
        "topic_complete",
        json!({
            "topic":"topic_mcp_authoring",
            "revision":revision,
            "session":session_id,
            "summary":"same immutable completion"
        }),
    );
    assert_eq!(repeated_completion["data"]["changed"], false);
    let completed_write = mcp.call_error(
        96,
        "artifact_write",
        json!({
            "path":"mcp/after-completion.txt",
            "session":session_id,
            "expect_hash":"new",
            "content":"must not be authored\n",
            "classification":"source"
        }),
    );
    assert_eq!(completed_write["error"]["code"], "topic_completed");

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
        json!({"view":view,"program":"git","args":["--version"],"cwd":".","network":"not_enforced"}),
    );
    assert_eq!(execution["data"]["command"], "execution.run");
    assert_eq!(
        execution["data"]["runtime_policy"]["network"]["requested"],
        "not_enforced"
    );
    assert_eq!(
        execution["data"]["environment_summary"]["command_runner_version"],
        "bounded_local_process_v3"
    );
    assert_eq!(
        execution["data"]["environment_summary"]["tool_hints"][0]["name"],
        "git"
    );
    assert!(execution["data"]["environment_summary"]["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    let execution_id = execution["data"]["execution_id"]
        .as_str()
        .expect("execution response should identify execution");
    let checkpoint = mcp.call(
        14,
        "checkpoint_create",
        json!({"view":view,"execution":execution_id}),
    );
    assert_eq!(checkpoint["data"]["command"], "checkpoint.create");
    assert_eq!(
        checkpoint["data"]["checkpoint"]["evidence_refs"][0]["execution_id"],
        execution_id
    );
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
    let recovered = mcp.call(17, "repository_status", json!({}));
    assert_eq!(recovered["data"]["command"], "status.repository");
    println!(
        "OA-02 evidence {}",
        json!({
            "topic_id": "topic_mcp_authoring",
            "session_id": session_id,
            "revision_id": revision,
            "resolved_view_id": view,
            "execution_id": execution_id,
            "checkpoint_id": checkpoint_id,
            "planned_export_map_id": export["data"]["ids"]["export_map_id"],
            "malformed_json_recovered": true,
            "final_repository_status": "readable"
        })
    );
    let sent = mcp.sent.join("\n");
    assert!(!sent.contains("--fixture"));
    assert!(!sent.contains("\"fixture\""));
    mcp.shutdown();
}

#[test]
fn two_live_mcp_agents_author_independent_topics_into_one_exact_view() {
    let temp = TempDir::new("sun-mcp-two-agents");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Sun MCP Test"]);
    git(&repo, &["config", "user.email", "sun-mcp@example.invalid"]);
    fs::write(repo.join("README.md"), "# shared repository\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    let mut agent_a = Mcp::start(&repo);
    let mut agent_b = Mcp::start(&repo);
    for (agent, name) in [
        (&mut agent_a, "agent-a-client"),
        (&mut agent_b, "agent-b-client"),
    ] {
        let initialized = agent.request(
            1,
            "initialize",
            json!({
                "protocolVersion":"2025-11-25",
                "capabilities":{},
                "clientInfo":{"name":name,"version":"1"}
            }),
        );
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        agent.notify("notifications/initialized", json!({}));
    }

    assert_eq!(agent_a.call(2, "repository_init", json!({}))["ok"], true);
    assert_eq!(
        agent_b.call(2, "repository_status", json!({}))["data"]["command"],
        "status.repository"
    );

    agent_a.call(
        3,
        "topic_create",
        json!({"slug":"agent-a-change","display_name":"Agent A change"}),
    );
    agent_b.call(
        3,
        "topic_create",
        json!({"slug":"agent-b-change","display_name":"Agent B change"}),
    );
    let session_a = agent_a.call(
        4,
        "session_start",
        json!({
            "topic":"agent-a-change",
            "view":"view_base_0001",
            "actor":"agent-a"
        }),
    );
    let session_b = agent_b.call(
        4,
        "session_start",
        json!({
            "topic":"agent-b-change",
            "view":"view_base_0001",
            "actor":"agent-b"
        }),
    );
    let session_a_id = session_a["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session_b_id = session_b["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    agent_a.start_call(
        5,
        "artifact_write",
        json!({
            "path":"agents/a.txt",
            "session":session_a_id,
            "expect_hash":"new",
            "content":"authored by agent A\n",
            "classification":"source"
        }),
    );
    agent_b.start_call(
        5,
        "artifact_write",
        json!({
            "path":"agents/b.txt",
            "session":session_b_id,
            "expect_hash":"new",
            "content":"authored by agent B\n",
            "classification":"source"
        }),
    );
    let mut write_a = agent_a.finish_call(5);
    let mut write_b = agent_b.finish_call(5);
    for round in 1..=6 {
        let request_id = 100 + round;
        agent_a.start_call(
            request_id,
            "artifact_write",
            json!({
                "path":format!("agents/a-{round}.txt"),
                "session":session_a_id,
                "expect_hash":"new",
                "content":format!("agent A round {round}\n"),
                "classification":"source"
            }),
        );
        agent_b.start_call(
            request_id,
            "artifact_write",
            json!({
                "path":format!("agents/b-{round}.txt"),
                "session":session_b_id,
                "expect_hash":"new",
                "content":format!("agent B round {round}\n"),
                "classification":"source"
            }),
        );
        write_a = agent_a.finish_call(request_id);
        write_b = agent_b.finish_call(request_id);
    }
    let revision_a = write_a["data"]["ids"]["topic_revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revision_b = write_b["data"]["ids"]["topic_revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    let combined = agent_a.call(
        6,
        "view_resolve",
        json!({
            "base":"checkpoint_base_0001",
            "include":[
                {"topic":"topic_agent_a_change","revision":revision_a},
                {"topic":"topic_agent_b_change","revision":revision_b}
            ]
        }),
    );
    assert_eq!(combined["data"]["conflict_ids"], json!([]));
    assert_eq!(combined["data"]["staleness_ids"], json!([]));
    assert_eq!(
        combined["data"]["normalized_frontier"]["topic_agent_a_change"],
        revision_a
    );
    assert_eq!(
        combined["data"]["normalized_frontier"]["topic_agent_b_change"],
        revision_b
    );
    let combined_view = combined["data"]["ids"]["resolved_view_id"]
        .as_str()
        .unwrap()
        .to_string();

    let a_reads_b = agent_a.call(
        7,
        "artifact_read",
        json!({"path":"agents/b.txt","view":combined_view}),
    );
    let b_reads_a = agent_b.call(
        7,
        "artifact_read",
        json!({"path":"agents/a.txt","view":combined_view}),
    );
    assert_eq!(
        a_reads_b["data"]["content"]["bytes"],
        "authored by agent B\n"
    );
    assert_eq!(
        b_reads_a["data"]["content"]["bytes"],
        "authored by agent A\n"
    );
    assert_eq!(a_reads_b["data"]["access_mode"], "read_only_view");
    assert_eq!(b_reads_a["data"]["access_mode"], "read_only_view");

    agent_b.start_call(
        8,
        "topic_wait",
        json!({"topic":"topic_agent_a_change","timeout_ms":5000}),
    );
    let completed_a = agent_a.call(
        8,
        "topic_complete",
        json!({
            "topic":"topic_agent_a_change",
            "revision":revision_a,
            "session":session_a_id,
            "summary":"Agent A authored the agents/a files."
        }),
    );
    assert_eq!(completed_a["data"]["command"], "topic.complete");
    let waited_a = agent_b.finish_call(8);
    assert_eq!(waited_a["data"]["wait"]["outcome"], "completed");
    assert_eq!(waited_a["data"]["topic"]["status"], "completed");
    assert_eq!(
        waited_a["data"]["handoff"]["summary"],
        "Agent A authored the agents/a files."
    );
    assert_eq!(waited_a["data"]["handoff"]["operation_count"], 7);
    assert!(waited_a["data"]["handoff"]["changed_paths"]
        .as_array()
        .unwrap()
        .contains(&json!("agents/a.txt")));
    assert_eq!(
        agent_b.call(
            9,
            "topic_complete",
            json!({
                "topic":"topic_agent_b_change",
                "revision":revision_b,
                "session":session_b_id,
                "summary":"Agent B authored the agents/b files."
            }),
        )["data"]["command"],
        "topic.complete"
    );
    let final_status = agent_b.call(10, "repository_status", json!({}));
    assert_eq!(
        final_status["data"]["operational_summary"]["topics"]["count"],
        2
    );
    assert!(fs::read_dir(repo.join(".sunlight/projections"))
        .unwrap()
        .next()
        .is_none());

    agent_a.shutdown();
    agent_b.shutdown();
}

#[test]
fn open_alpha_oa04_mcp_termination_boundaries_recover_from_durable_facts() {
    let temp = TempDir::new("sun-mcp-oa04-recovery");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Sun OA-04 Test"]);
    git(&repo, &["config", "user.email", "sun-oa04@example.invalid"]);
    fs::write(repo.join("README.md"), "# OA-04 recovery\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    let mut mcp = initialized_mcp(&repo);
    mcp.call(2, "repository_init", json!({}));
    mcp.call(
        3,
        "topic_create",
        json!({"slug":"oa04-recovery","display_name":"OA-04 recovery"}),
    );
    let session = mcp.call(
        4,
        "session_start",
        json!({"topic":"oa04-recovery","view":"view_base_0001","actor":"oa04-agent"}),
    );
    let session_id = session["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Boundary 1: an acknowledged mutation must survive abrupt server death exactly once.
    let write = mcp.call(
        5,
        "artifact_write",
        json!({
            "path":"src/recovered.txt",
            "session":session_id,
            "expect_hash":"new",
            "content":"acknowledged before termination\n",
            "classification":"source"
        }),
    );
    let view_id = write["data"]["view"]["resolved_view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revision_id = write["data"]["ids"]["topic_revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    mcp.terminate();

    let mut mcp = initialized_mcp(&repo);
    let recovered = mcp.call(
        2,
        "artifact_read",
        json!({"path":"src/recovered.txt","view":view_id}),
    );
    assert_eq!(
        recovered["data"]["content"]["bytes"],
        "acknowledged before termination\n"
    );
    assert_eq!(recovered["data"]["artifacts"].as_array().unwrap().len(), 1);

    // Boundary 3: completion is immutable and remains an integration-ready exact revision.
    mcp.call(
        3,
        "topic_complete",
        json!({
            "topic":"topic_oa04_recovery",
            "revision":revision_id,
            "session":session_id,
            "summary":"OA-04 recovered authoring"
        }),
    );
    mcp.terminate();
    let mut mcp = initialized_mcp(&repo);
    let completed = mcp.call(2, "inspect", json!({"selector":"topic:oa04-recovery"}));
    assert_eq!(completed["data"]["topic"]["status"], "completed");
    assert_eq!(
        completed["data"]["topic"]["completed_revision_id"],
        revision_id
    );

    // Boundary 2: the running record is durable before the child process can outlive the server.
    mcp.start_call(
        3,
        "execution_run",
        json!({
            "view":view_id,
            "program":PYTHON,
            "args":["-c","import time; time.sleep(30)"],
            "cwd":".",
            "network":"not_enforced"
        }),
    );
    let execution_id = wait_for_running_execution(&repo);
    mcp.terminate();
    let mut mcp = initialized_mcp(&repo);
    let interrupted = mcp.call(
        2,
        "repository_status",
        json!({"scope":"execution","id":execution_id}),
    );
    assert_eq!(interrupted["data"]["result"]["status"], "interrupted");
    assert_eq!(
        interrupted["data"]["result"]["termination_reason"],
        "runner_process_terminated"
    );

    // Boundary 4: an OS-owned writer lease disappears with its owning process; no lock deletion is
    // required. Terminate a request while a separate process-scoped lease blocks publication.
    let writer_lock = TestWriterLock::acquire(&repo);
    mcp.start_call(3, "repository_status", json!({}));
    std::thread::sleep(std::time::Duration::from_millis(40));
    mcp.terminate();
    drop(writer_lock);
    let mut mcp = initialized_mcp(&repo);
    assert_eq!(
        mcp.call(2, "repository_status", json!({}))["data"]["command"],
        "status.repository"
    );

    // Boundary 5: facts needed for checkpoint creation survive a death immediately before it.
    mcp.terminate();
    let mut mcp = initialized_mcp(&repo);
    let checkpoint = mcp.call(2, "checkpoint_create", json!({"view":view_id}));
    assert_eq!(checkpoint["data"]["command"], "checkpoint.create");
    assert_eq!(
        checkpoint["data"]["checkpoint"]["resolved_view_id"],
        view_id
    );
    let final_read = mcp.call(
        3,
        "artifact_read",
        json!({"path":"src/recovered.txt","view":view_id}),
    );
    assert_eq!(
        final_read["data"]["content"]["bytes"],
        "acknowledged before termination\n"
    );
    println!(
        "OA-04 evidence {}",
        json!({
            "topic_id": "topic_oa04_recovery",
            "session_id": session_id,
            "revision_id": revision_id,
            "view_id": view_id,
            "execution_id": execution_id,
            "checkpoint_id": checkpoint["data"]["checkpoint_id"],
            "recovered_status": "interrupted"
        })
    );
    mcp.shutdown();
}

#[test]
fn stdio_mcp_domain_calls_do_not_respawn_the_server_executable() {
    let temp = TempDir::new("sun-mcp-no-respawn");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# no respawn\n").unwrap();

    let server_executable = temp.path().join(if cfg!(windows) {
        "sun-mcp-server.exe"
    } else {
        "sun-mcp-server"
    });
    fs::copy(env!("CARGO_BIN_EXE_sun"), &server_executable).unwrap();
    let mut mcp = Mcp::start_with_executable(&server_executable, &repo);
    let initialized = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-no-respawn-test","version":"1"}
        }),
    );
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    mcp.notify("notifications/initialized", json!({}));

    let unavailable = server_executable.with_extension("unavailable");
    fs::rename(&server_executable, &unavailable)
        .expect("the running test server executable should become unavailable for respawn");

    let initialized_repo = mcp.call(2, "repository_init", json!({}));
    assert_eq!(initialized_repo["data"]["command"], "repository.init");
    let status = mcp.call(3, "repository_status", json!({}));
    assert_eq!(status["data"]["command"], "status.repository");
    mcp.shutdown();
}

#[test]
fn stdio_mcp_no_fixture_contract_omits_git_lookup_and_states_policy_scope() {
    let temp = TempDir::new("sun-mcp-public-contract");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    let mut mcp = initialized_mcp(&repo);

    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let find = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap();

    let status_scopes = find("repository_status")["inputSchema"]["properties"]["scope"]["enum"]
        .as_array()
        .unwrap();
    assert!(!status_scopes.contains(&json!("git")));
    assert!(
        !find("inspect")["inputSchema"]["properties"]["selector"]["description"]
            .as_str()
            .unwrap()
            .contains("git:")
    );

    assert_eq!(
        find("policy_check_export")["inputSchema"]["required"],
        json!(["checkpoint", "branch"])
    );
    assert!(find("policy_check_commit")["description"]
        .as_str()
        .unwrap()
        .contains(".sunlight"));
    assert!(
        find("policy_check_commit")["outputSchema"]["properties"]["data"]["properties"]["ids"]
            ["properties"]
            .get("validation_report_id")
            .is_none()
    );

    let status_error = mcp.call_error(3, "repository_status", json!({"scope":"git","id":"HEAD"}));
    assert_eq!(status_error["error"]["code"], "invalid_request");
    let inspect_error = mcp.call_error(4, "inspect", json!({"selector":"git:HEAD"}));
    assert_eq!(inspect_error["error"]["code"], "invalid_request");
    let policy_error = mcp.call_error(
        5,
        "policy_check_export",
        json!({"checkpoint":"checkpoint_missing"}),
    );
    assert_eq!(policy_error["error"]["code"], "invalid_request");

    mcp.shutdown();
}

#[test]
fn cli_and_mcp_share_read_mutation_and_error_contracts() {
    let temp = TempDir::new("sun-engine-contract-equivalence");
    let cli_repo = temp.path().join("cli-repository");
    let mcp_repo = temp.path().join("mcp-repository");
    for repo in [&cli_repo, &mcp_repo] {
        fs::create_dir_all(repo).unwrap();
        fs::write(repo.join("README.md"), "# equivalent repository\n").unwrap();
    }

    let _ = sun_json(&cli_repo, &["init"]);
    let _ = sun_json(
        &cli_repo,
        &[
            "topic",
            "create",
            "equivalence",
            "--display-name",
            "Equivalence",
        ],
    );
    let cli_session = sun_json(
        &cli_repo,
        &[
            "session",
            "start",
            "--topic",
            "equivalence",
            "--view",
            "view_base_0001",
            "--actor",
            "contract-agent",
        ],
    );
    let cli_session_id = cli_session["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cli_read = sun_json(
        &cli_repo,
        &["read", "README.md", "--session", &cli_session_id],
    );
    fs::write(cli_repo.join("content.tmp"), "shared engine mutation\n").unwrap();
    let cli_write = sun_json(
        &cli_repo,
        &[
            "write",
            "notes/equivalent.txt",
            "--session",
            &cli_session_id,
            "--expect-hash",
            "new",
            "--content-file",
            "content.tmp",
            "--classification",
            "source",
        ],
    );
    let cli_error = sun_json_error(
        &cli_repo,
        &[
            "write",
            "notes/equivalent.txt",
            "--session",
            &cli_session_id,
            "--expect-hash",
            "new",
            "--content-file",
            "content.tmp",
            "--classification",
            "source",
        ],
    );

    let mut mcp = Mcp::start(&mcp_repo);
    let _ = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-contract-test","version":"1"}
        }),
    );
    mcp.notify("notifications/initialized", json!({}));
    let _ = mcp.call(2, "repository_init", json!({}));
    let _ = mcp.call(
        3,
        "topic_create",
        json!({"slug":"equivalence","display_name":"Equivalence"}),
    );
    let mcp_session = mcp.call(
        4,
        "session_start",
        json!({"topic":"equivalence","view":"view_base_0001","actor":"contract-agent"}),
    );
    let mcp_session_id = mcp_session["data"]["ids"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mcp_read = mcp.call(
        5,
        "artifact_read",
        json!({"path":"README.md","session":mcp_session_id}),
    );
    let mcp_write = mcp.call(
        6,
        "artifact_write",
        json!({
            "path":"notes/equivalent.txt",
            "session":mcp_session_id,
            "expect_hash":"new",
            "content":"shared engine mutation\n",
            "classification":"source"
        }),
    );
    let mcp_error = mcp.call_error(
        7,
        "artifact_write",
        json!({
            "path":"notes/equivalent.txt",
            "session":mcp_session_id,
            "expect_hash":"new",
            "content":"shared engine mutation\n",
            "classification":"source"
        }),
    );
    mcp.shutdown();

    assert_eq!(
        normalize_repository_identity(cli_read),
        normalize_repository_identity(mcp_read)
    );
    assert_eq!(
        normalize_repository_identity(cli_write),
        normalize_repository_identity(mcp_write)
    );
    assert_eq!(
        normalize_repository_identity(cli_error),
        normalize_repository_identity(mcp_error)
    );
}

struct Mcp {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    sent: Vec<String>,
}

impl Mcp {
    fn start(repo: &Path) -> Self {
        Self::start_with_executable(Path::new(env!("CARGO_BIN_EXE_sun")), repo)
    }

    fn start_with_executable(executable: &Path, repo: &Path) -> Self {
        let mut child = Command::new(executable)
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
        self.start_call(id, name, arguments);
        self.finish_call(id)
    }
    fn start_call(&mut self, id: u64, name: &str, arguments: Value) {
        self.send(json!({
            "jsonrpc":"2.0",
            "id":id,
            "method":"tools/call",
            "params":{"name":name,"arguments":arguments}
        }));
    }
    fn finish_call(&mut self, id: u64) -> Value {
        let response = self.read();
        assert_eq!(response["id"], id);
        assert!(
            response.get("error").is_none(),
            "protocol error: {response}"
        );
        let result = &response["result"];
        assert_ne!(result["isError"], true, "tool error: {result}");
        result["structuredContent"].clone()
    }
    fn call_error(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        let response = self.request(id, "tools/call", json!({"name":name,"arguments":arguments}));
        assert!(
            response.get("error").is_none(),
            "protocol error: {response}"
        );
        let result = &response["result"];
        assert_eq!(result["isError"], true, "expected tool error: {result}");
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
    fn terminate(&mut self) {
        self.stdin.take();
        let child = self.child.as_mut().unwrap();
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
        }
        child.wait().unwrap();
    }
}

fn initialized_mcp(repo: &Path) -> Mcp {
    let mut mcp = Mcp::start(repo);
    let initialized = mcp.request(
        1,
        "initialize",
        json!({
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"sun-oa04-recovery-test","version":"1"}
        }),
    );
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    mcp.notify("notifications/initialized", json!({}));
    mcp
}

fn wait_for_running_execution(repo: &Path) -> String {
    let root = repo.join(".sunlight/executions");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let body = fs::read_to_string(entry.path()).unwrap();
                if body.contains("\"status\":\"running\"") {
                    return entry
                        .path()
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                }
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "running execution record was not published before timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

struct TestWriterLock(fs::File);

impl TestWriterLock {
    #[cfg(windows)]
    fn acquire(repo: &Path) -> Self {
        use std::os::windows::fs::OpenOptionsExt;

        let path = repo.join(".sunlight/local/command-transaction.lock");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(0)
            .open(path)
            .unwrap();
        Self(file)
    }

    #[cfg(unix)]
    fn acquire(repo: &Path) -> Self {
        use std::os::fd::AsRawFd;

        const LOCK_EX: i32 = 2;
        let path = repo.join(".sunlight/local/command-transaction.lock");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        assert_eq!(unsafe { flock(file.as_raw_fd(), LOCK_EX) }, 0);
        Self(file)
    }
}

impl Drop for TestWriterLock {
    fn drop(&mut self) {
        let _ = &self.0;
    }
}

fn sun_json(repo: &Path, args: &[&str]) -> Value {
    sun_json_result(repo, args, true)
}

fn sun_json_error(repo: &Path, args: &[&str]) -> Value {
    sun_json_result(repo, args, false)
}

fn sun_json_result(repo: &Path, args: &[&str], success: bool) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_sun"))
        .args(args)
        .arg("--json")
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "sun {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn normalize_repository_identity(mut value: Value) -> Value {
    fn visit(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("transport");
                for (key, value) in object {
                    if key == "repository_id" {
                        *value = Value::String("<repository>".to_string());
                    } else if key == "resolved_view_id" || key == "authored_context_id" {
                        *value = Value::String("<view>".to_string());
                    } else {
                        visit(value);
                    }
                }
            }
            Value::Array(values) => values.iter_mut().for_each(visit),
            _ => {}
        }
    }
    visit(&mut value);
    value
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
