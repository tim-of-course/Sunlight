use std::ffi::OsStr;
use std::fs;
#[cfg(windows)]
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
#[cfg(windows)]
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use sunlight_core::records::parse_json_record;
use sunlight_core::repo_state::RealRepoState;

fn sun() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sun"))
}

#[test]
fn global_and_primary_help_describe_repo_backed_operator_workflow() {
    let output = sun().arg("--help").output().expect("sun --help should run");

    assert_success(&output);
    let help = stdout(&output);
    for expected in [
        "sun init",
        "sun topic create <slug> --display-name <name> [--json]",
        "sun compat import --projection <projection> --candidate <candidate>",
        "sun run --view <view>",
        "sun policy check-export --checkpoint <checkpoint>",
        "sun git export --checkpoint <checkpoint> --branch <ref>",
        "status     Summarize repository health and object lifecycle state",
        "Compatibility/testing:",
    ] {
        assert!(help.contains(expected), "missing help text: {expected}");
    }
    assert!(!help.contains("Create fixture-backed"));
    assert!(!help.contains("Read a fixture artifact"));

    for command in ["status", "inspect", "run"] {
        let output = sun()
            .arg(command)
            .arg("--help")
            .output()
            .expect("primary command help should run");
        assert_success(&output);
        assert!(!stdout(&output).contains("--fixture basic-app"));
    }
}

#[test]
fn no_fixture_repository_status_clean_initial_and_active_human_journey() {
    let repo = TestRepo::new("repository-status-journey");
    init_local_git_repo(&repo);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&init);
    let clean = sun()
        .arg("status")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&clean);
    let clean = stdout(&clean);
    assert_valid_json(&clean);
    assert!(clean.contains("\"command\":\"status.repository\""));
    assert!(clean.contains("\"topics\":{\"count\":0,\"heads\":[]}"));
    assert!(clean.contains("\"executions\":{\"total\":0"));
    assert!(clean.contains("\"checkpoints\":{\"count\":0,\"unexported\":0,\"records\":[]}"));
    assert!(clean.contains(if cfg!(windows) {
        "\"execution_isolation\":{\"enforced\":true"
    } else {
        "\"execution_isolation\":{\"enforced\":false"
    }));
    assert!(!clean.contains("multi_record_publication_non_atomic"));

    let topic = sun()
        .args([
            "topic",
            "create",
            "operator-flow",
            "--display-name",
            "Operator Flow",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&topic);
    let session = sun()
        .args([
            "session",
            "start",
            "--topic",
            "operator-flow",
            "--view",
            "view_base_0001",
            "--actor",
            "operator",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&session);
    let authored = repo.write_file("authored.txt", "authored\n");
    let write = sun()
        .args([
            "write",
            "authored.txt",
            "--session",
            "session_operator",
            "--expect-hash",
            "new",
            "--content-file",
        ])
        .arg(authored)
        .args(["--classification", "source", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&write);

    let active = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&active);
    let active = stdout(&active);
    assert_valid_json(&active);
    assert!(active.contains("\"topic_id\":\"topic_operator_flow\""));
    assert!(active.contains("\"session_id\":\"session_operator\""));
    assert!(active.contains("\"code\":\"checkpoint_missing\""));

    let human = sun()
        .arg("status")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&human);
    let human = stdout(&human);
    assert!(human.contains("Sunlight repo-"));
    assert!(human.contains("topic operator-flow  head rev_operator_flow_0001"));
    assert!(human.contains("session session_operator"));
    assert!(human.contains("checkpoints 0  exports 0"));
    assert!(human.contains("execution isolation:"));
    assert!(!human.contains("multi_record_publication_non_atomic"));
    assert!(human.contains("warning[checkpoint_missing]: create a checkpoint"));
}

#[test]
fn init_json_returns_repository_success_envelope() {
    let repo = TestRepo::new("init-json");

    let output = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"repository.init\""));
    assert!(stdout.contains("\"repository_id\":\"repo-"));
    assert!(stdout.contains("\"ids\":{\"repository_id\":\"repo-"));
    assert!(stdout.contains("\"view\":null"));
    assert!(stdout.contains("\"warnings\":[]"));
    assert!(repo.path().join(".sunlight/config.toml").is_file());
}

#[test]
fn no_fixture_interrupted_state_publication_recovers_and_continues() {
    let repo = TestRepo::new("interrupted-state-recovery");
    init_local_git_repo(&repo);
    let init = sun()
        .args(["init", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&init);

    let canonical = repo
        .path()
        .join(".sunlight")
        .join("records")
        .join("native-state.json");
    let old = fs::read(&canonical).unwrap();
    let process_canonical = PathBuf::from(".")
        .join(".sunlight")
        .join("records")
        .join("native-state.json");
    let interrupted = sun()
        .args([
            "topic",
            "create",
            "recovered",
            "--display-name",
            "Recovered",
            "--json",
        ])
        .env(
            "SUNLIGHT_TEST_FAILPOINT",
            format!("state_after_prepare|{}", process_canonical.display()),
        )
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!interrupted.status.success());
    assert_eq!(fs::read(&canonical).unwrap(), old);
    assert_valid_json(&fs::read_to_string(&canonical).unwrap());
    let recovery_root = repo
        .path()
        .join(".sunlight")
        .join("local")
        .join("recovery")
        .join("native-state");
    assert!(recovery_root.join("journal.json").is_file());
    assert!(recovery_root.join("staged.json").is_file());

    let status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&status);
    let status_json = stdout(&status);
    assert_valid_json(&status_json);
    assert!(status_json.contains("\"topic_id\":\"topic_recovered\""));
    assert!(!recovery_root.join("journal.json").exists());
    assert!(!recovery_root.join("staged.json").exists());
    assert!(!recovery_root.join("backup.json").exists());

    let session = sun()
        .args([
            "session",
            "start",
            "--topic",
            "recovered",
            "--view",
            "view_base_0001",
            "--actor",
            "recovery-agent",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&session);
    let content = repo.write_file("continued.txt", "continued after recovery\n");
    let write = sun()
        .args([
            "write",
            "continued.txt",
            "--session",
            "session_recovery_agent",
            "--expect-hash",
            "new",
            "--content-file",
        ])
        .arg(content)
        .args(["--classification", "source", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&write);
    assert_valid_json(&stdout(&write));
    let final_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&final_status);
    assert!(stdout(&final_status).contains("rev_recovered_0001"));
    assert!(repo
        .path()
        .join(".sunlight/session-generations/gen_recovery_agent_0002.json")
        .is_file());
}

#[test]
fn no_fixture_native_mutation_outbox_recovers_every_declared_record_and_continues() {
    let repo = TestRepo::new("native-mutation-outbox-recovery");
    init_local_git_repo(&repo);
    start_native_session(&repo, "outbox-native");
    let canonical = repo.path().join(".sunlight/records/native-state.json");
    let old = fs::read(&canonical).unwrap();
    let content = repo.write_file("outbox-native.txt", "published through outbox\n");
    let interrupted = sun()
        .args([
            "write",
            "outbox-native.txt",
            "--session",
            "session_agent_a",
            "--expect-hash",
            "new",
            "--content-file",
        ])
        .arg(content)
        .args(["--classification", "source", "--json"])
        .env("SUNLIGHT_TEST_FAILPOINT", "batch_after_canonical_commit")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!interrupted.status.success());
    assert_ne!(fs::read(&canonical).unwrap(), old);
    assert!(!repo
        .path()
        .join(".sunlight/operations/op_native_0001.json")
        .exists());
    assert!(repo
        .path()
        .join(".sunlight/local/publication-outbox")
        .is_dir());

    let recovered = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&recovered);
    let state = RealRepoState::load(repo.path()).unwrap();
    for path in [
        repo.path().join(".sunlight/operations/op_native_0001.json"),
        repo.path()
            .join(".sunlight/topics/rev_outbox_native_0001.json"),
        repo.path()
            .join(".sunlight/session-generations/gen_agent_a_0002.json"),
        repo.path()
            .join(".sunlight/views")
            .join(format!("{}.json", state.resolved_view_id)),
    ] {
        assert!(
            path.is_file(),
            "missing recovered record {}",
            path.display()
        );
        assert_valid_json(&fs::read_to_string(path).unwrap());
    }
    assert!(!repo
        .path()
        .join(".sunlight/local/publication-outbox")
        .exists());

    let continued = repo.write_file("continued-native.txt", "continued\n");
    let write = sun()
        .args([
            "write",
            "continued-native.txt",
            "--session",
            "session_agent_a",
            "--expect-hash",
            "new",
            "--content-file",
        ])
        .arg(continued)
        .args(["--classification", "source", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&write);
    assert!(repo
        .path()
        .join(".sunlight/operations/op_native_0002.json")
        .is_file());
}

#[test]
fn no_fixture_compat_import_outbox_recovers_mid_batch_and_continues() {
    let repo = TestRepo::new("compat-import-outbox-recovery");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(repo.path(), "src/lib.rs", "pub fn answer() -> u32 { 42 }\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "outbox-compat");
    let (projection_id, projection_root, generation_id) = create_real_compat_projection(&repo);
    fs::write(
        projection_root.join("src/lib.rs"),
        b"pub fn answer() -> u32 { 43 }\n",
    )
    .unwrap();
    let diff = real_compat_diff(&repo, &projection_id);
    let candidate_id = candidate_id_for_path(&diff, "src/lib.rs");
    let interrupted = sun()
        .args([
            "compat",
            "import",
            "--projection",
            &projection_id,
            "--candidate",
            &candidate_id,
            "--session-generation",
            &generation_id,
            "--json",
        ])
        .env("SUNLIGHT_TEST_FAILPOINT", "batch_mid_derived_publication")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!interrupted.status.success());
    assert!(repo
        .path()
        .join(".sunlight/local/publication-outbox")
        .is_dir());

    let recovered = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&recovered);
    let state = RealRepoState::load(repo.path()).unwrap();
    for path in [
        repo.path()
            .join(".sunlight/session-generations/gen_agent_a_0002.json"),
        repo.path()
            .join(".sunlight/projections")
            .join(format!("{projection_id}.json")),
        repo.path().join(".sunlight/operations/op_native_0001.json"),
        repo.path()
            .join(".sunlight/compat-imports/op_native_0001.json"),
        repo.path()
            .join(".sunlight/topics/rev_outbox_compat_0001.json"),
        repo.path()
            .join(".sunlight/views")
            .join(format!("{}.json", state.resolved_view_id)),
    ] {
        assert!(
            path.is_file(),
            "missing recovered record {}",
            path.display()
        );
        assert_valid_json(&fs::read_to_string(path).unwrap());
    }
    assert!(!repo
        .path()
        .join(".sunlight/local/publication-outbox")
        .exists());

    let read = sun()
        .args([
            "read",
            "src/lib.rs",
            "--session",
            "session_agent_a",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&read);
    assert!(stdout(&read).contains("43"));
    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg(&state.resolved_view_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&checkpoint);
}

#[test]
fn no_fixture_real_repo_artifact_io_vertical_slice() {
    let repo = TestRepo::new("real-repo-artifact-io");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn answer() -> u32 {\n    42\n}\n",
    );
    repo.write_file("README.md", "# Real repo\n\nneedle\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);
    let native_state = repo.path().join(".sunlight/records/native-state.json");
    assert!(native_state.is_file());
    let native_state_json = fs::read_to_string(native_state).unwrap();
    assert!(native_state_json.contains("\"schema_version\":1"));
    assert!(native_state_json.contains("\"record_type\":\"repo_state\""));

    let topic = sun()
        .arg("topic")
        .arg("create")
        .arg("real-io")
        .arg("--display-name")
        .arg("Real IO")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun topic create should run");
    assert_success(&topic);
    assert!(stdout(&topic).contains("\"topic_id\":\"topic_real_io\""));

    let session = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_real_io")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent-a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");
    assert_success(&session);
    assert!(stdout(&session).contains("\"session_id\":\"session_agent_a\""));

    let read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let read_stdout = stdout(&read);
    assert!(read_stdout.contains("pub fn answer()"));
    let before_hash = json_string_field(&read_stdout, "content_hash");

    let list = sun()
        .arg("list")
        .arg("src")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun list should run");
    assert_success(&list);
    assert!(stdout(&list).contains("\"path\":\"src/lib.rs\""));

    let readme = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read README should run");
    assert_success(&readme);
    let readme_hash = json_string_field(&stdout(&readme), "content_hash");
    let readme_patch = repo.write_file(
        "readme.patch",
        "--- a/README.md\n+++ b/README.md\n@@\n-needle\n+needle patched\n",
    );
    let patch = sun()
        .arg("patch")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&readme_hash)
        .arg("--patch-file")
        .arg(&readme_patch)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun patch should run");
    assert_success(&patch);
    assert!(stdout(&patch).contains("\"command\":\"artifact.patch\""));
    let patched_readme_hash = json_string_field(&stdout(&patch), "after_hash");

    let metadata = sun()
        .arg("metadata")
        .arg("set")
        .arg("README.md")
        .arg("--classification")
        .arg("generated")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&patched_readme_hash)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun metadata set should run");
    assert_success(&metadata);
    assert!(stdout(&metadata).contains("\"command\":\"artifact.metadata_set\""));
    assert!(stdout(&metadata).contains("\"classification\":\"generated\""));

    let content = repo.write_file("new-lib.rs", "pub fn answer() -> u32 {\n    43\n}\n");
    let write = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&before_hash)
        .arg("--content-file")
        .arg(&content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun write should run");
    assert_success(&write);
    let write_stdout = stdout(&write);
    assert!(write_stdout.contains("\"command\":\"artifact.write\""));
    let resolved_view = json_string_field(&write_stdout, "resolved_view_id");

    let reread = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&reread);
    assert!(
        stdout(&reread).contains("43"),
        "reread did not contain accepted bytes: {}",
        stdout(&reread)
    );

    let search = sun()
        .arg("search")
        .arg("needle")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search should run");
    assert_success(&search);
    assert!(stdout(&search).contains("\"path\":\"README.md\""));

    let resolved = sun()
        .arg("view")
        .arg("resolve")
        .arg("--base")
        .arg("checkpoint_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&resolved);
    assert!(stdout(&resolved).contains("\"command\":\"view.resolve\""));
    assert!(stdout(&resolved).contains(&format!("\"resolved_view_id\":\"{resolved_view}\"")));

    let status = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");
    assert_success(&status);
    assert!(stdout(&status).contains("\"command\":\"status.session\""));

    let inspect = sun()
        .arg("inspect")
        .arg("artifact:README.md")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");
    assert_success(&inspect);
    assert!(stdout(&inspect).contains("\"command\":\"inspect.artifact\""));
    assert!(stdout(&inspect).contains("\"classification\":\"generated\""));

    let projection_root = repo.path().join("projection");
    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);
    assert_eq!(
        fs::read_to_string(projection_root.join("src/lib.rs")).unwrap(),
        "pub fn answer() -> u32 {\n    43\n}\n"
    );

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/real-io")
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    assert!(stdout(&export).contains("\"lifecycle_state\":\"exported\""));
    let delivered_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&delivered_status);
    let delivered_status = stdout(&delivered_status);
    assert_valid_json(&delivered_status);
    assert!(
        delivered_status.contains("\"checkpoints\":{\"count\":1,\"unexported\":0,\"records\":[")
    );
    assert!(delivered_status.contains("\"exports\":{\"count\":1,\"maps\":["));
    assert!(delivered_status.contains("\"policy\":{\"reports\":1,\"passed\":1"));
    assert!(!delivered_status.contains("\"code\":\"checkpoint_not_exported\""));
    assert_eq!(
        git(repo.path(), &["show", "sunlight/real-io:src/lib.rs"]),
        "pub fn answer() -> u32 {\n    43\n}\n"
    );
}

#[test]
fn no_fixture_execution_output_promotion_repo_backed_slice() {
    let repo = TestRepo::new("real-execution-output");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# Execution repo\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    let topic = sun()
        .arg("topic")
        .arg("create")
        .arg("generated-output")
        .arg("--display-name")
        .arg("Generated Output")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun topic create should run");
    assert_success(&topic);

    let session = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_generated_output")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent-a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");
    assert_success(&session);

    let escaping_cwd = sun()
        .arg("run")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--cwd")
        .arg("..")
        .arg("--json")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf escaped > SHOULD_NOT_EXIST")
        .current_dir(repo.path())
        .output()
        .expect("sun run with escaping cwd should run");
    assert_failure(&escaping_cwd);
    let escaping_stdout = stdout(&escaping_cwd);
    assert!(escaping_stdout.contains("\"code\":\"invalid_request\""));
    assert!(escaping_stdout.contains("execution cwd must stay inside the projection root"));
    assert!(!repo
        .path()
        .join(".sunlight/executions/exec_native_0001.json")
        .exists());
    assert!(!repo.path().join("SHOULD_NOT_EXIST").exists());

    let run = sun()
        .args(["run", "--view", "view_base_0001", "--json", "--"])
        .args([
            "python",
            "-c",
            "from pathlib import Path; Path('generated').mkdir(exist_ok=True); Path('generated/out.txt').write_bytes(b'promoted needle\\n')",
        ])
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");
    assert_success(&run);
    let run_stdout = stdout(&run);
    assert!(run_stdout.contains("\"command\":\"execution.run\""));
    assert!(run_stdout.contains("\"output_path\":\"generated/out.txt\""));
    assert!(
        run_stdout.contains("\"raw_outputs\":\"local_only\"")
            || run_stdout.contains("\"execution_id\"")
    );
    let execution_id = json_string_field(&run_stdout, "execution_id");

    let status = sun()
        .arg("status")
        .arg("--execution")
        .arg(&execution_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status --execution should run");
    assert_success(&status);
    let status_stdout = stdout(&status);
    assert!(status_stdout.contains("\"command\":\"status.execution\""));
    assert!(status_stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(status_stdout.contains("\"promotion_status\":\"promotion_required\""));
    assert!(status_stdout.contains("\"code\":\"pending_promotion\""));
    let repository_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&repository_status);
    assert!(stdout(&repository_status).contains("\"pending_promotions\":1"));
    assert!(stdout(&repository_status).contains("\"code\":\"pending_promotions\""));

    let inspect = sun()
        .arg("inspect")
        .arg(format!("execution:{execution_id}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect execution should run");
    assert_success(&inspect);
    assert!(stdout(&inspect).contains("\"source_truth\":\"sunlight_persisted_execution\""));

    let bad_promote = sun()
        .arg("execution")
        .arg("promote-output")
        .arg(&execution_id)
        .arg("--path")
        .arg("generated/missing.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--classification")
        .arg("source_like_delta")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun execution promote-output should run");
    assert_failure(&bad_promote);
    assert!(stdout(&bad_promote).contains("\"code\":\"promotion_precondition_failed\""));

    let promote = sun()
        .arg("execution")
        .arg("promote-output")
        .arg(&execution_id)
        .arg("--path")
        .arg("generated/out.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--classification")
        .arg("source_like_delta")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun execution promote-output should run");
    assert_success(&promote);
    let promote_stdout = stdout(&promote);
    assert!(promote_stdout.contains("\"command\":\"execution.promote_output\""));
    assert!(promote_stdout.contains("\"execution_provenance\""));
    let promoted_view = json_string_field(&promote_stdout, "resolved_view_id");
    let promoted_generation = json_string_field(&promote_stdout, "session_generation_id");
    let promoted_generation_record = fs::read_to_string(
        repo.path()
            .join(".sunlight/session-generations")
            .join(format!("{promoted_generation}.json")),
    )
    .unwrap();
    assert!(promoted_generation_record.contains("\"session_id\":\"session_agent_a\""));

    let read = sun()
        .arg("read")
        .arg("generated/out.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read promoted output should run");
    assert_success(&read);
    assert!(stdout(&read).contains("promoted needle"));

    let search = sun()
        .arg("search")
        .arg("promoted needle")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search promoted output should run");
    assert_success(&search);
    assert!(stdout(&search).contains("\"path\":\"generated/out.txt\""));

    let promoted_status = sun()
        .arg("status")
        .arg("--execution")
        .arg(&execution_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status promoted execution should run");
    assert_success(&promoted_status);
    assert!(stdout(&promoted_status).contains("\"promotion_status\":\"promoted\""));

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&promoted_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/generated-output")
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    assert_eq!(
        git(
            repo.path(),
            &["show", "sunlight/generated-output:generated/out.txt"]
        ),
        "promoted needle\n"
    );
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_execution_confines_root_and_descendant_writes_to_private_projection() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let repo = TestRepo::new("windows-filesystem-isolation");
    repo.write_file("source-sentinel.txt", "source unchanged\n");
    let sibling = PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(format!(
        "sunlight-isolation-host-sibling-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&sibling).unwrap();
    let sibling_sentinel = sibling.join("sibling-sentinel.txt");
    fs::write(&sibling_sentinel, "sibling unchanged\n").unwrap();
    let source_sentinel = repo.path().join("source-sentinel.txt");
    let source_created = repo.path().join("SOURCE_CREATED_BY_RUN");
    let sibling_created = sibling.join("SIBLING_CREATED_BY_RUN");
    repo.write_file(
        "child-isolation.cmd",
        "@echo off\r\n(echo escaped>\"%~1\") 2>nul && (echo escaped>descendant-result.txt) || (echo denied>descendant-result.txt)\r\n(echo escaped>\"%~2\") 2>nul && (echo escaped>>descendant-result.txt) || (echo denied>>descendant-result.txt)\r\n",
    );
    repo.write_file(
        "root-isolation.cmd",
        "@echo off\r\necho private write works>private-output.txt\r\n(echo escaped>\"%~1\") 2>nul && (echo escaped>root-result.txt) || (echo denied>root-result.txt)\r\n(echo escaped>\"%~2\") 2>nul && (echo escaped>>root-result.txt) || (echo denied>>root-result.txt)\r\ncmd.exe /d /c child-isolation.cmd \"%~3\" \"%~4\"\r\ncmd.exe /d /c ver>tool-result.txt\r\nexit /b 0\r\n",
    );
    start_native_session(&repo, "windows-isolation");
    let run = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "root-isolation.cmd",
        ])
        .arg(&source_sentinel)
        .arg(&source_created)
        .arg(&sibling_sentinel)
        .arg(&sibling_created)
        .current_dir(repo.path())
        .output()
        .expect("isolated Windows execution should run");
    assert_success(&run);
    let body = stdout(&run);
    assert_valid_json(&body);
    assert!(body.contains("\"status\":\"pass\""), "{body}");
    assert!(body
        .contains("\"network\":{\"requested\":\"not_enforced\",\"effective\":\"not_enforced\"}"));
    assert!(body.contains("\"filesystem_writes_requested\":\"private_projection_isolated\""));
    assert!(body.contains("\"filesystem_writes\":\"windows_low_integrity_private_projection_v1\""));
    assert!(body.contains("\"output_path\":\"private-output.txt\""));
    assert!(body.contains("\"output_path\":\"descendant-result.txt\""));
    assert!(body.contains("\"output_path\":\"tool-result.txt\""));
    assert_eq!(
        fs::read_to_string(&source_sentinel).unwrap(),
        "source unchanged\n"
    );
    assert_eq!(
        fs::read_to_string(&sibling_sentinel).unwrap(),
        "sibling unchanged\n"
    );
    assert!(!source_created.exists());
    assert!(!sibling_created.exists());

    let execution_id = json_string_field(&body, "execution_id");
    for inspected in [
        sun()
            .args(["status", "--execution", &execution_id, "--json"])
            .current_dir(repo.path())
            .output()
            .unwrap(),
        sun()
            .args(["inspect", &format!("execution:{execution_id}"), "--json"])
            .current_dir(repo.path())
            .output()
            .unwrap(),
    ] {
        assert_success(&inspected);
        let inspected = stdout(&inspected);
        assert!(inspected.contains("windows_low_integrity_private_projection_v1"));
        assert!(inspected.contains("private_projection_isolated"));
    }

    let state = RealRepoState::load(repo.path()).unwrap();
    let execution = state
        .executions
        .iter()
        .find(|execution| execution.execution_id == execution_id)
        .unwrap();
    let projection = state
        .projections
        .iter()
        .find(|projection| projection.projection_id == execution.projection_id)
        .unwrap();
    let projection_root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
    assert_eq!(
        fs::read_to_string(projection_root.join("descendant-result.txt")).unwrap(),
        "denied\r\ndenied\r\n"
    );
    assert_eq!(
        fs::read_to_string(projection_root.join("root-result.txt")).unwrap(),
        "denied\r\ndenied\r\n"
    );
    assert!(!projection_root
        .parent()
        .unwrap()
        .join(format!(".{}-private", execution.execution_id))
        .exists());
    let acl = Command::new("icacls.exe")
        .arg(&projection_root)
        .output()
        .unwrap();
    assert!(acl.status.success());
    assert!(
        !String::from_utf8_lossy(&acl.stdout).contains("S-1-15-2-"),
        "ephemeral AppContainer SID remained on the retained projection"
    );
    fs::remove_dir_all(sibling).unwrap();
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_execution_denies_root_and_descendant_loopback_without_host_changes() {
    let _isolation_test_guard = windows_isolation_test_lock();
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let server_accepted = Arc::clone(&accepted);
    let server_stop = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !server_stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    server_accepted.fetch_add(1, Ordering::SeqCst);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = [0_u8; 1024];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("listener failed: {error}"),
            }
        }
    });

    let url = format!("http://127.0.0.1:{port}/");
    let outside = Command::new("curl.exe")
        .args(["--max-time", "2", "--silent", "--fail", &url])
        .output()
        .expect("outside connectivity probe should run");
    assert!(
        outside.status.success(),
        "live endpoint probe failed: {outside:?}"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 1);

    let repo = TestRepo::new("windows-network-isolation");
    repo.write_file(
        "network-child.cmd",
        &format!(
            "@curl.exe --connect-timeout 1 --max-time 2 --silent --fail {url} >nul\r\n@if errorlevel 1 (echo denied>descendant-network.txt& exit /b 0) else (echo connected>descendant-network.txt& exit /b 9)\r\n"
        ),
    );
    repo.write_file(
        "network-root.cmd",
        &format!(
            "@echo off\r\ncurl.exe --connect-timeout 1 --max-time 2 --silent --fail {url} >nul\r\nif errorlevel 1 (echo denied>root-network.txt) else (echo connected>root-network.txt& exit /b 9)\r\ncmd.exe /d /c network-child.cmd\r\n"
        ),
    );
    start_native_session(&repo, "windows-network-isolation");
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path).unwrap().replace(
        "network_policy = \"not_enforced\"",
        "network_policy = \"disabled\"",
    );
    fs::write(&config_path, config).unwrap();
    let run = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "network-root.cmd",
        ])
        .current_dir(repo.path())
        .output()
        .expect("network-isolated Windows execution should run");
    assert_success(&run);
    let body = stdout(&run);
    assert!(body.contains("\"status\":\"pass\""), "{body}");
    assert!(body.contains("\"network\":{\"requested\":\"disabled\",\"effective\":\"windows_appcontainer_no_network_capabilities_v1\"}"));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "an AppContainer process reached the manager listener"
    );
    let state = RealRepoState::load(repo.path()).unwrap();
    let execution = state.executions.last().unwrap();
    assert_eq!(execution.network_policy_requested, "disabled");
    assert_eq!(
        execution.network_policy,
        "windows_appcontainer_no_network_capabilities_v1"
    );
    let projection = state.projections.last().unwrap();
    let root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
    assert_eq!(
        fs::read_to_string(root.join("root-network.txt")).unwrap(),
        "denied\r\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("descendant-network.txt")).unwrap(),
        "denied\r\n"
    );
    for observed in [
        sun()
            .args(["status", "--execution", &execution.execution_id, "--json"])
            .current_dir(repo.path())
            .output()
            .unwrap(),
        sun()
            .args([
                "inspect",
                &format!("execution:{}", execution.execution_id),
                "--json",
            ])
            .current_dir(repo.path())
            .output()
            .unwrap(),
        sun()
            .args(["status", "--json"])
            .current_dir(repo.path())
            .output()
            .unwrap(),
    ] {
        assert_success(&observed);
        assert!(stdout(&observed).contains(
            "\"network\":{\"requested\":\"disabled\",\"effective\":\"windows_appcontainer_no_network_capabilities_v1\"}"
        ));
    }

    let outside_after = Command::new("curl.exe")
        .args(["--max-time", "2", "--silent", "--fail", &url])
        .output()
        .expect("post-run outside connectivity probe should run");
    assert!(outside_after.status.success());
    assert_eq!(accepted.load(Ordering::SeqCst), 2);
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_user_toolchain_incompatibility_is_fail_closed_before_command_code() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let Some(python) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|root| root.join("python.exe"))
            .find(|candidate| candidate.is_file())
    }) else {
        return;
    };
    let windows_root = fs::canonicalize(std::env::var_os("SYSTEMROOT").unwrap()).unwrap();
    if fs::canonicalize(&python)
        .unwrap()
        .starts_with(&windows_root)
    {
        return;
    }
    let repo = TestRepo::new("windows-network-incompatible-toolchain");
    start_native_session(&repo, "windows-network-incompatible-toolchain");
    let signal = repo.path().join("COMMAND_RAN");
    let failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "disabled",
            "--json",
            "--",
            "python",
            "-c",
        ])
        .arg("from pathlib import Path; Path('COMMAND_RAN').write_text('ran')")
        .current_dir(repo.path())
        .output()
        .expect("unsupported toolchain should be rejected");
    assert_failure(&failed);
    let body = stdout(&failed);
    assert!(body.contains("\"code\":\"execution_network_isolation_incompatible_toolchain\""));
    assert!(body.contains("\"command_started\":\"false\""));
    assert!(!signal.exists());
    let state = RealRepoState::load(repo.path()).unwrap();
    assert!(state.executions.is_empty());
    assert!(state.projections.is_empty());
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_concurrent_runs_use_distinct_ephemeral_profiles_and_clean_them() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let profiles_before = sunlight_appcontainer_profile_dirs();
    let first = TestRepo::new("windows-network-concurrent-first");
    let second = TestRepo::new("windows-network-concurrent-second");
    start_native_session(&first, "windows-network-concurrent-first");
    start_native_session(&second, "windows-network-concurrent-second");

    let spawn = |repo: &TestRepo, output: &str| {
        let mut command = sun();
        command
            .args([
                "run",
                "--view",
                "view_base_0001",
                "--network",
                "disabled",
                "--json",
                "--",
                "cmd.exe",
                "/d",
                "/c",
                &format!("echo isolated>{output}"),
            ])
            .current_dir(repo.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("concurrent isolated run should spawn")
    };
    let first_child = spawn(&first, "first.txt");
    let second_child = spawn(&second, "second.txt");
    let first_output = first_child.wait_with_output().unwrap();
    let second_output = second_child.wait_with_output().unwrap();
    assert_success(&first_output);
    assert_success(&second_output);
    for output in [&first_output, &second_output] {
        assert!(stdout(output).contains("windows_appcontainer_no_network_capabilities_v1"));
    }
    assert_eq!(sunlight_appcontainer_profile_dirs(), profiles_before);
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_cleanup_failure_persists_execution_and_recovers_from_journal() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let profiles_before = sunlight_appcontainer_profile_dirs();
    let repo = TestRepo::new("windows-network-cleanup-recovery");
    start_native_session(&repo, "windows-network-cleanup-recovery");
    let test_exe = std::env::current_exe().unwrap();
    let failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "disabled",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "echo command ran>cleanup-evidence.txt",
        ])
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "cleanup_after_command",
        )
        .env(
            "SUNLIGHT_INTERNAL_TEST_PARENT_PID",
            std::process::id().to_string(),
        )
        .env("SUNLIGHT_INTERNAL_TEST_PARENT_EXE", &test_exe)
        .current_dir(repo.path())
        .output()
        .expect("cleanup failure should be reported after command execution");
    assert_failure(&failed);
    let body = stdout(&failed);
    assert!(body.contains("\"code\":\"execution_network_isolation_cleanup_failed\""));
    assert!(body.contains("\"command_started\":\"true\""));
    assert!(body.contains("\"execution_id\":\"exec_native_0001\""));

    let state = RealRepoState::load(repo.path()).unwrap();
    let execution = state.executions.last().unwrap();
    assert!(execution.command_started);
    assert_eq!(execution.status, "policy_blocked");
    assert_eq!(
        execution.termination_reason.as_deref(),
        Some("execution_network_isolation_cleanup_failed")
    );
    let projection = state.projections.last().unwrap();
    assert_eq!(projection.retention_state, "quarantined");
    let durable_execution = fs::read_to_string(
        repo.path()
            .join(".sunlight/executions/exec_native_0001.json"),
    )
    .unwrap();
    assert!(durable_execution.contains("\"command_started\":true"));
    assert!(durable_execution
        .contains("\"termination_reason\":\"execution_network_isolation_cleanup_failed\""));
    let projection_root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
    assert_eq!(
        fs::read_to_string(projection_root.join("cleanup-evidence.txt")).unwrap(),
        "command ran\r\n"
    );
    let journal_root = repo
        .path()
        .join(".sunlight/local/windows-appcontainer-cleanup");
    assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 1);

    let inspected = sun()
        .args(["inspect", "execution:exec_native_0001", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&inspected);
    assert!(stdout(&inspected).contains("\"code\":\"execution_network_isolation_cleanup_failed\""));

    let recovery = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "not_enforced",
            "--json",
            "--",
            "python",
            "-c",
            "pass",
        ])
        .current_dir(repo.path())
        .output()
        .expect("the next run should recover stale isolation cleanup");
    assert_success(&recovery);
    assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
    let acl = Command::new("icacls.exe")
        .arg(&projection_root)
        .output()
        .unwrap();
    assert!(acl.status.success());
    assert!(!String::from_utf8_lossy(&acl.stdout).contains("S-1-15-2-"));
    assert_eq!(sunlight_appcontainer_profile_dirs(), profiles_before);
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_custom_managed_root_cleanup_recovers_only_its_execution_allocation() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let profiles_before = sunlight_appcontainer_profile_dirs();
    let repo = TestRepo::new("windows-custom-managed-cleanup-recovery");
    let external = TestRepo::new("windows-external-managed-cleanup-root");
    let managed_root = external.path().join("managed");
    fs::create_dir_all(&managed_root).unwrap();
    let outside_allocation = managed_root.join("DO_NOT_DELETE.txt");
    fs::write(
        &outside_allocation,
        "outside managed execution allocation\n",
    )
    .unwrap();
    repo.write_file(
        "recovery-order.cmd",
        "@if exist \"%~1\" exit /b 11\r\n@if exist \"%~2\" exit /b 12\r\n@if not exist \"%~3\" exit /b 13\r\n@echo recovered>recovery-order.txt\r\n",
    );
    start_native_session(&repo, "windows-custom-managed-cleanup-recovery");
    set_projection_default_root(&repo, &managed_root.to_string_lossy().replace('\\', "/"));
    let canonical_repo = fs::canonicalize(repo.path()).unwrap();
    let managed_root = fs::canonicalize(&managed_root).unwrap();
    assert!(!managed_root.starts_with(&canonical_repo));
    assert!(!canonical_repo.starts_with(&managed_root));
    let test_exe = std::env::current_exe().unwrap();

    let failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "disabled",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "echo recovered>custom-root-cleanup.txt",
        ])
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "cleanup_after_command",
        )
        .env(
            "SUNLIGHT_INTERNAL_TEST_PARENT_PID",
            std::process::id().to_string(),
        )
        .env("SUNLIGHT_INTERNAL_TEST_PARENT_EXE", &test_exe)
        .current_dir(repo.path())
        .output()
        .expect("custom-root cleanup failure should leave recoverable evidence");
    assert_failure(&failed);
    assert!(stdout(&failed).contains("\"code\":\"execution_network_isolation_cleanup_failed\""));

    let state = RealRepoState::load(repo.path()).unwrap();
    let projection_root = PathBuf::from(
        state
            .projections
            .last()
            .unwrap()
            .materialized_root
            .as_ref()
            .unwrap(),
    );
    assert!(projection_root.starts_with(&managed_root));
    let failed_allocation = projection_root.parent().unwrap().to_path_buf();
    assert_eq!(failed_allocation.parent(), Some(managed_root.as_path()));
    let stale_runtime = projection_root
        .parent()
        .unwrap()
        .join(".exec_native_0001-private");
    assert!(stale_runtime.is_dir());
    let journal_root = repo
        .path()
        .join(".sunlight/local/windows-appcontainer-cleanup");
    assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 1);
    let journal_path = fs::read_dir(&journal_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(journal_path.starts_with(repo.path().join(".sunlight/local")));
    assert!(!journal_path.starts_with(&managed_root));
    let journal = fs::read_to_string(&journal_path).unwrap();
    let profile_name = journal
        .lines()
        .find_map(|line| line.strip_prefix("profile="))
        .unwrap();
    let profiles_during_failure = sunlight_appcontainer_profile_dirs();
    let new_profiles = profiles_during_failure
        .iter()
        .filter(|profile| !profiles_before.contains(profile))
        .collect::<Vec<_>>();
    assert_eq!(
        new_profiles.len(),
        1,
        "unexpected profile delta: {new_profiles:?}"
    );
    assert!(new_profiles[0].eq_ignore_ascii_case(profile_name));

    let recovered = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "not_enforced",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "recovery-order.cmd",
        ])
        .arg(&stale_runtime)
        .arg(&journal_path)
        .arg(&outside_allocation)
        .current_dir(repo.path())
        .output()
        .expect("next custom-root run should recover stale isolation cleanup");
    assert_success(&recovered);
    assert!(!stale_runtime.exists());
    assert!(!journal_path.exists());
    assert_eq!(fs::read_dir(&journal_root).unwrap().count(), 0);
    assert!(failed_allocation.is_dir());
    assert_eq!(
        fs::read_to_string(&outside_allocation).unwrap(),
        "outside managed execution allocation\n"
    );
    let recovered_state = RealRepoState::load(repo.path()).unwrap();
    let recovered_projection_root = PathBuf::from(
        recovered_state
            .projections
            .last()
            .unwrap()
            .materialized_root
            .as_ref()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(recovered_projection_root.join("recovery-order.txt")).unwrap(),
        "recovered\r\n"
    );
    let acl = Command::new("icacls.exe")
        .arg(&failed_allocation)
        .args(["/T", "/C"])
        .output()
        .unwrap();
    assert!(acl.status.success());
    assert!(!String::from_utf8_lossy(&acl.stdout).contains("S-1-15-2-"));
    assert_eq!(sunlight_appcontainer_profile_dirs(), profiles_before);
    fs::remove_dir_all(external.path()).unwrap();
    assert!(!external.path().exists());
}

#[cfg(windows)]
#[test]
fn no_fixture_windows_setup_failure_via_public_run_is_atomic_and_never_launches_command() {
    let _isolation_test_guard = windows_isolation_test_lock();
    let repo = TestRepo::new("windows-isolation-public-setup-failure");
    start_native_session(&repo, "windows-setup-failure");
    let low_signal_root = std::env::temp_dir().join(format!(
        "sunlight-low-signal-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&low_signal_root).unwrap();
    let label = Command::new("icacls.exe")
        .arg(&low_signal_root)
        .args(["/setintegritylevel", "(OI)(CI)L"])
        .output()
        .unwrap();
    assert!(
        label.status.success(),
        "low-integrity signal setup failed: {label:?}"
    );
    let command_signal = low_signal_root.join("COMMAND_RAN");
    let test_exe = std::env::current_exe().unwrap();
    let network_failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "disabled",
            "--json",
            "--",
            "python",
            "-c",
            "import pathlib,sys; pathlib.Path(sys.argv[1]).write_text('ran')",
        ])
        .arg(&command_signal)
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "prepare_appcontainer",
        )
        .env(
            "SUNLIGHT_INTERNAL_TEST_PARENT_PID",
            std::process::id().to_string(),
        )
        .env("SUNLIGHT_INTERNAL_TEST_PARENT_EXE", &test_exe)
        .current_dir(repo.path())
        .output()
        .expect("sun run should report injected setup failure");
    assert_failure(&network_failed);
    let body = stdout(&network_failed);
    assert_valid_json(&body);
    assert!(
        body.contains("\"code\":\"execution_network_isolation_setup_failed\""),
        "{body}"
    );
    assert!(body.contains("\"command_started\":\"false\""), "{body}");
    assert!(!command_signal.exists(), "restricted command code executed");

    let filesystem_failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "not_enforced",
            "--json",
            "--",
            "python",
            "-c",
            "import pathlib,sys; pathlib.Path(sys.argv[1]).write_text('ran')",
        ])
        .arg(&command_signal)
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "prepare_after_runtime_root",
        )
        .env(
            "SUNLIGHT_INTERNAL_TEST_PARENT_PID",
            std::process::id().to_string(),
        )
        .env("SUNLIGHT_INTERNAL_TEST_PARENT_EXE", &test_exe)
        .current_dir(repo.path())
        .output()
        .expect("sun run should report injected filesystem setup failure");
    assert_failure(&filesystem_failed);
    let body = stdout(&filesystem_failed);
    assert_valid_json(&body);
    assert!(
        body.contains("\"code\":\"execution_filesystem_isolation_setup_failed\""),
        "{body}"
    );
    assert!(body.contains("\"command_started\":\"false\""), "{body}");
    assert!(!command_signal.exists(), "restricted command code executed");

    let containment_failed = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--network",
            "not_enforced",
            "--json",
            "--",
            "python",
            "-c",
            "import pathlib,sys; pathlib.Path(sys.argv[1]).write_text('ran')",
        ])
        .arg(&command_signal)
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "job_before_assign",
        )
        .env(
            "SUNLIGHT_INTERNAL_TEST_PARENT_PID",
            std::process::id().to_string(),
        )
        .env("SUNLIGHT_INTERNAL_TEST_PARENT_EXE", &test_exe)
        .current_dir(repo.path())
        .output()
        .expect("sun run should report injected containment setup failure");
    assert_failure(&containment_failed);
    let body = stdout(&containment_failed);
    assert_valid_json(&body);
    assert!(
        body.contains("\"code\":\"execution_containment_setup_failed\""),
        "{body}"
    );
    assert!(body.contains("\"command_started\":\"false\""), "{body}");
    assert!(!command_signal.exists(), "suspended command code executed");

    let state = RealRepoState::load(repo.path()).unwrap();
    assert!(state.executions.is_empty());
    assert!(state.projections.is_empty());
    for records in [
        repo.path().join(".sunlight/executions"),
        repo.path().join(".sunlight/projections"),
    ] {
        assert_eq!(
            fs::read_dir(&records)
                .map(|entries| entries.count())
                .unwrap_or(0),
            0,
            "unpublished record or projection root remained at {}",
            records.display()
        );
    }

    let unscoped = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "cmd.exe",
            "/d",
            "/c",
            "exit 0",
        ])
        .env(
            "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT",
            "job_before_assign",
        )
        .current_dir(repo.path())
        .output()
        .expect("unscoped internal failpoint should be ignored");
    assert_success(&unscoped);
    fs::remove_dir_all(low_signal_root).unwrap();
}

#[test]
fn no_fixture_execution_runtime_policy_is_enforced_and_reported() {
    let repo = TestRepo::new("execution-runtime-policy");
    repo.write_file("README.md", "# runtime policy\n");
    #[cfg(windows)]
    {
        repo.write_file("timeout-root.cmd", "@cmd.exe /d /c timeout-child.cmd\r\n");
        repo.write_file(
            "timeout-child.cmd",
            "@echo off\r\nfor /l %%i in (1,1,50000000) do rem\r\necho late>LATE_MARKER\r\n",
        );
        repo.write_file(
            "large-output.cmd",
            "@echo off\r\nfor /l %%i in (1,1,5000) do <nul set /p \"=A\"\r\nfor /l %%i in (1,1,7000) do <nul set /p \"=B\" 1>&2\r\nexit /b 0\r\n",
        );
        repo.write_file(
            "environment-check.cmd",
            "@if defined SUNLIGHT_TEST_SECRET (exit /b 1) else (exit /b 0)\r\n",
        );
    }
    start_native_session(&repo, "runtime-policy");
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("timeout_ms = 300000", "timeout_ms = 2000")
        .replace("stdout_limit_bytes = 1048576", "stdout_limit_bytes = 1024")
        .replace("stderr_limit_bytes = 1048576", "stderr_limit_bytes = 1024");
    fs::write(&config_path, config).unwrap();

    let mut timeout_command = sun();
    timeout_command.args(["run", "--view", "view_base_0001", "--json", "--"]);
    if cfg!(windows) {
        timeout_command.args(["cmd.exe", "/d", "/c", "timeout-root.cmd"]);
    } else {
        timeout_command.args([
            "python",
            "-c",
            r#"import subprocess,sys,time; from pathlib import Path; p=subprocess.Popen([sys.executable,'-c',"import time; from pathlib import Path; time.sleep(3); Path('LATE_MARKER').write_text('late')"]); time.sleep(5)"#,
        ]);
    }
    let timeout = timeout_command
        .current_dir(repo.path())
        .output()
        .expect("timed execution should run");
    assert_success(&timeout);
    let timeout_stdout = stdout(&timeout);
    assert_valid_json(&timeout_stdout);
    assert!(
        timeout_stdout.contains("\"status\":\"timeout\""),
        "{timeout_stdout}"
    );
    assert!(timeout_stdout.contains("\"timed_out\":true"));
    assert!(timeout_stdout.contains("\"promotion_candidates\":[]"));
    assert!(timeout_stdout.contains("\"timeout_ms\":2000"));
    assert!(timeout_stdout
        .contains("\"network\":{\"requested\":\"not_enforced\",\"effective\":\"not_enforced\"}"));
    assert!(timeout_stdout.contains(if cfg!(windows) {
        "\"filesystem_writes\":\"windows_low_integrity_private_projection_v1\""
    } else {
        "\"filesystem_writes\":\"not_enforced\""
    }));
    let timeout_execution = json_string_field(&timeout_stdout, "execution_id");
    let persisted_timeout = fs::read_to_string(
        repo.path()
            .join(".sunlight/executions")
            .join(format!("{timeout_execution}.json")),
    )
    .unwrap();
    assert_valid_json(&persisted_timeout);
    assert!(persisted_timeout.contains("\"status\":\"timeout\""));
    assert!(persisted_timeout.contains("\"timed_out\":true"));
    assert!(persisted_timeout.contains("\"runtime_policy\":"));
    let timeout_status = sun()
        .args(["status", "--execution", &timeout_execution, "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&timeout_status);
    assert!(stdout(&timeout_status).contains("\"code\":\"execution_timeout\""));
    let repository_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&repository_status);
    assert!(stdout(&repository_status).contains("\"timeouts\":1"));
    assert!(stdout(&repository_status).contains("\"code\":\"executions_need_attention\""));
    let state_after_timeout =
        fs::read_to_string(repo.path().join(".sunlight/records/native-state.json")).unwrap();
    assert!(state_after_timeout.contains("\"operations\":[]"));
    let timeout_projection = json_string_field(&timeout_stdout, "projection_id");
    let late_marker = repo
        .path()
        .join(".sunlight/projections")
        .join(&timeout_projection)
        .join("root/LATE_MARKER");
    std::thread::sleep(Duration::from_millis(3200));
    assert!(
        !late_marker.exists(),
        "timed-out process mutated files after return"
    );
    let config = fs::read_to_string(&config_path).unwrap().replace(
        "timeout_ms = 2000",
        if cfg!(windows) {
            "timeout_ms = 20000"
        } else {
            "timeout_ms = 5000"
        },
    );
    fs::write(&config_path, config).unwrap();

    let mut large_command = sun();
    large_command.args(["run", "--view", "view_base_0001", "--json", "--"]);
    if cfg!(windows) {
        large_command.args(["cmd.exe", "/d", "/c", "large-output.cmd"]);
    } else {
        large_command.args([
            "python",
            "-c",
            "import sys; sys.stdout.buffer.write(b'A'*5000); sys.stderr.buffer.write(b'B'*7000)",
        ]);
    }
    let large = large_command
        .current_dir(repo.path())
        .output()
        .expect("large-output execution should run");
    assert_success(&large);
    let large_stdout = stdout(&large);
    assert_valid_json(&large_stdout);
    assert!(
        large_stdout.contains("\"status\":\"pass\""),
        "{large_stdout}"
    );
    assert!(large_stdout.contains(
        "\"observed_byte_length\":5000,\"captured_byte_length\":1024,\"truncated\":true,\"capture_failed\":false"
    ));
    assert!(large_stdout.contains(
        "\"observed_byte_length\":7000,\"captured_byte_length\":1024,\"truncated\":true,\"capture_failed\":false"
    ));
    let complete_stdout_digest = format!("sha256:{:x}", Sha256::digest(vec![b'A'; 5000]));
    let retained_stdout_prefix_digest = format!("sha256:{:x}", Sha256::digest(vec![b'A'; 1024]));
    assert!(large_stdout.contains(&format!("\"observed_digest\":\"{complete_stdout_digest}\"")));
    assert!(!large_stdout.contains(&retained_stdout_prefix_digest));
    let large_execution = json_string_field(&large_stdout, "execution_id");
    let persisted_large = fs::read_to_string(
        repo.path()
            .join(".sunlight/executions")
            .join(format!("{large_execution}.json")),
    )
    .unwrap();
    assert_valid_json(&persisted_large);
    assert!(persisted_large.contains(&format!("\"observed_digest\":\"{complete_stdout_digest}\"")));
    assert!(!persisted_large.contains(&retained_stdout_prefix_digest));
    assert!(!large_stdout.contains(&"A".repeat(100)));
    assert!(!large_stdout.contains(&"B".repeat(100)));

    let secret = "do-not-inherit-this-test-secret";
    let mut normal_command = sun();
    normal_command.args(["run", "--view", "view_base_0001", "--json", "--"]);
    if cfg!(windows) {
        normal_command.args(["cmd.exe", "/d", "/c", "environment-check.cmd"]);
    } else {
        normal_command.args([
            "python",
            "-c",
            "import os,sys; sys.exit(1 if os.environ.get('SUNLIGHT_TEST_SECRET') else 0)",
        ]);
    }
    let normal = normal_command
        .env("SUNLIGHT_TEST_SECRET", secret)
        .current_dir(repo.path())
        .output()
        .expect("environment-filtered execution should run");
    assert_success(&normal);
    let normal_stdout = stdout(&normal);
    assert!(normal_stdout.contains("\"status\":\"pass\""));
    assert!(normal_stdout.contains("\"inheritance\":\"minimal_os_allowlist\""));
    assert!(normal_stdout.contains("\"values_recorded\":false"));
    assert!(!normal_stdout.contains(secret));
    assert!(!normal_stdout.contains("SUNLIGHT_TEST_SECRET"));
    let execution_id = json_string_field(&normal_stdout, "execution_id");

    for output in [
        sun()
            .arg("status")
            .arg("--execution")
            .arg(&execution_id)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .unwrap(),
        sun()
            .arg("inspect")
            .arg(format!("execution:{execution_id}"))
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .unwrap(),
    ] {
        assert_success(&output);
        let body = stdout(&output);
        assert_valid_json(&body);
        assert!(body.contains("\"runtime_policy\":"));
        assert!(body.contains("\"output_capture\":"));
        assert!(body.contains(
            "\"network\":{\"requested\":\"not_enforced\",\"effective\":\"not_enforced\"}"
        ));
        assert!(body.contains("\"inheritance\":\"minimal_os_allowlist\""));
        assert!(!body.contains(secret));
    }
}

#[cfg(windows)]
#[test]
fn windows_job_object_enforces_cpu_memory_and_active_process_limits() {
    let repo = TestRepo::new("windows-job-resource-limits");
    repo.write_file("README.md", "# Windows Job Object limits\n");
    start_native_session(&repo, "windows-job-limits");
    let config_path = repo.path().join(".sunlight/config.toml");
    let base_config = fs::read_to_string(&config_path).unwrap();

    let normal = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "python",
            "-c",
            "pass",
        ])
        .current_dir(repo.path())
        .output()
        .expect("Windows containment probe should run");
    if !normal.status.success()
        && stdout(&normal).contains("\"code\":\"execution_containment_setup_failed\"")
    {
        eprintln!(
            "skipping Windows Job Object enforcement test: host specifically rejected Job Object setup"
        );
        return;
    }
    assert_success(&normal);
    let normal_body = stdout(&normal);
    assert!(normal_body.contains("\"status\":\"pass\""));
    assert!(normal_body.contains("\"process_tree\":\"windows_job_object_kill_on_close\""));
    assert!(normal_body.contains("\"cpu\":\"windows_job_object_cpu_time\""));
    assert!(normal_body.contains("\"memory\":\"windows_job_object_process_and_job_memory\""));

    let ordinary_failure = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "python",
            "-c",
            "raise SystemExit(7)",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&ordinary_failure);
    let ordinary_failure = stdout(&ordinary_failure);
    assert!(ordinary_failure.contains("\"status\":\"fail\""));
    assert!(ordinary_failure.contains("\"exit_code\":7"));
    assert!(ordinary_failure.contains("\"termination_reason\":\"command_exit\""));

    let cases = [
        (
            base_config
                .replace(
                    "process_memory_limit_bytes = 2147483648",
                    "process_memory_limit_bytes = 67108864",
                )
                .replace(
                    "job_memory_limit_bytes = 4294967296",
                    "job_memory_limit_bytes = 134217728",
                ),
            "x=bytearray(512*1024*1024); import time; time.sleep(5)",
            "process_memory_limit",
        ),
        (
            base_config
                .replace(
                    "process_memory_limit_bytes = 2147483648",
                    "process_memory_limit_bytes = 268435456",
                )
                .replace(
                    "job_memory_limit_bytes = 4294967296",
                    "job_memory_limit_bytes = 335544320",
                ),
            "import subprocess,sys,time; children=[subprocess.Popen([sys.executable,'-c','import time; x=bytearray(200*1024*1024); time.sleep(5)']) for _ in range(2)]; [child.wait() for child in children]; time.sleep(5)",
            "job_memory_limit",
        ),
        (
            base_config.replace(
                "cpu_time_limit_ms = 300000",
                "cpu_time_limit_ms = 100",
            ),
            "while True: pass",
            "cpu_time_limit",
        ),
        (
            base_config.replace(
                "active_process_limit = 32",
                "active_process_limit = 1",
            ),
            "import subprocess,sys,time; subprocess.run([sys.executable,'-c','import time; time.sleep(5)']); time.sleep(5)",
            "active_process_limit",
        ),
    ];
    for (config, script, expected_reason) in cases {
        fs::write(&config_path, config).unwrap();
        let output = sun()
            .args([
                "run",
                "--view",
                "view_base_0001",
                "--json",
                "--",
                "python",
                "-c",
                script,
            ])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert_success(&output);
        let body = stdout(&output);
        assert_valid_json(&body);
        assert!(
            body.contains("\"status\":\"policy_blocked\""),
            "resource case did not persist policy_blocked: {body}"
        );
        assert!(
            body.contains(&format!("\"termination_reason\":\"{expected_reason}\"")),
            "resource case did not persist {expected_reason}: {body}"
        );
        let execution_id = json_string_field(&body, "execution_id");
        for inspected in [
            sun()
                .args(["status", "--execution", &execution_id, "--json"])
                .current_dir(repo.path())
                .output()
                .unwrap(),
            sun()
                .args(["inspect", &format!("execution:{execution_id}"), "--json"])
                .current_dir(repo.path())
                .output()
                .unwrap(),
        ] {
            assert_success(&inspected);
            let inspected = stdout(&inspected);
            assert!(inspected.contains("\"code\":\"execution_resource_policy_blocked\""));
            assert!(inspected.contains(expected_reason));
            assert!(inspected.contains("\"limits\":"));
        }
    }
}

#[test]
fn no_fixture_invalid_execution_policy_precedes_projection_process_and_state_mutation() {
    let repo = TestRepo::new("invalid-execution-policy");
    repo.write_file("README.md", "# invalid runtime policy\n");
    start_native_session(&repo, "invalid-runtime-policy");
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let state_before = fs::read(&state_path).unwrap();
    let projections_root = repo.path().join(".sunlight/projections");
    let projections_before = fs::read_dir(&projections_root).unwrap().count();
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("timeout_ms = 300000", "timeout_ms = 0");
    fs::write(&config_path, config).unwrap();

    let run = sun()
        .arg("run")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--json")
        .arg("--")
        .arg("python")
        .arg("-c")
        .arg("from pathlib import Path; Path('PROCESS_MUST_NOT_RUN').write_text('bad')")
        .current_dir(repo.path())
        .output()
        .expect("invalid-policy run should return an error");
    assert_failure(&run);
    let body = stdout(&run);
    assert!(body.contains("\"code\":\"invalid_repository_config\""));
    assert!(body.contains("execution_policy.timeout_ms"));
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    assert_eq!(
        fs::read_dir(&projections_root).unwrap().count(),
        projections_before
    );
    assert!(!repo.path().join("PROCESS_MUST_NOT_RUN").exists());
    assert!(!repo
        .path()
        .join(".sunlight/executions/exec_native_0001.json")
        .exists());
}

#[test]
fn no_fixture_custom_managed_root_drives_compat_and_execution_projections() {
    let repo = TestRepo::new("custom-managed-projections");
    repo.write_file("README.md", "# managed roots\n");
    start_native_session(&repo, "managed-roots");
    set_projection_default_root(&repo, ".sunlight/custom-managed");

    let project = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--purpose")
        .arg("inspection")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&project);
    let project_stdout = stdout(&project);
    let inspection_projection_id = json_string_field(&project_stdout, "projection_id");
    assert!(repo
        .path()
        .join(".sunlight/custom-managed/inspection")
        .join(&inspection_projection_id)
        .join("root/README.md")
        .is_file());
    assert!(project_stdout.contains("custom-managed"));

    let (projection_id, compatibility_root, _) =
        create_real_compat_projection_at(&repo, ".sunlight/custom-managed");
    assert!(compatibility_root.join("README.md").is_file());
    let compat_status = sun()
        .args(["status", "--projection", &projection_id, "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&compat_status);
    assert!(stdout(&compat_status).contains("\"materialization\":{"));
    let diff = real_compat_diff(&repo, &projection_id);
    assert!(diff.contains("\"candidate_counts\":"));

    let run = sun()
        .args([
            "run",
            "--view",
            "view_base_0001",
            "--json",
            "--",
            "python",
            "-c",
            "pass",
        ])
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");
    assert_success(&run);
    let run_stdout = stdout(&run);
    assert!(run_stdout.contains("\"materialization\":{"));
    assert!(run_stdout.contains("\"cache_key\":\"projection-cache:"));
    assert!(run_stdout.contains("\"cache_hit\":false"));
    let execution_id = json_string_field(&run_stdout, "execution_id");
    let execution_projection_id = json_string_field(&run_stdout, "projection_id");
    let execution_root = repo
        .path()
        .join(".sunlight/custom-managed")
        .join(&execution_projection_id)
        .join("root");
    assert!(execution_root.join("README.md").is_file());
    let projection_record = fs::read_to_string(
        repo.path()
            .join(".sunlight/projections")
            .join(format!("{execution_projection_id}.json")),
    )
    .unwrap();
    assert!(projection_record.contains("custom-managed"));
    assert!(!repo
        .path()
        .join(".sunlight/projections")
        .join(&execution_projection_id)
        .exists());

    for (command, selector) in [
        ("status", format!("--projection={execution_projection_id}")),
        ("inspect", format!("projection:{execution_projection_id}")),
    ] {
        let output = if command == "status" {
            sun()
                .arg("status")
                .arg("--projection")
                .arg(&execution_projection_id)
                .arg("--json")
                .current_dir(repo.path())
                .output()
                .expect("sun status projection should run")
        } else {
            sun()
                .arg("inspect")
                .arg(&selector)
                .arg("--json")
                .current_dir(repo.path())
                .output()
                .expect("sun inspect projection should run")
        };
        assert_success(&output);
        assert!(stdout(&output).contains("custom-managed"));
    }

    let execution_status = sun()
        .arg("status")
        .arg("--execution")
        .arg(&execution_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status execution should run");
    assert_success(&execution_status);
    assert_valid_json(&stdout(&execution_status));
    assert!(stdout(&execution_status).contains("custom-managed"));
    assert!(stdout(&execution_status).contains("\"cache_key\":\"projection-cache:"));
    assert!(stdout(&execution_status).contains("\"materialization\":{"));
    let execution_inspect = sun()
        .arg("inspect")
        .arg(format!("execution:{execution_id}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect execution should run");
    assert_success(&execution_inspect);
    assert_valid_json(&stdout(&execution_inspect));
    assert!(stdout(&execution_inspect).contains("custom-managed"));
    assert!(stdout(&execution_inspect).contains("\"cache_key\":\"projection-cache:"));
    assert!(stdout(&execution_inspect).contains("\"materialization\":{"));
}

#[test]
fn no_fixture_forced_copy_reports_truthful_metrics_and_isolates_store_mutation() {
    let repo = TestRepo::new("projection-forced-copy-metrics");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-copy");
    let root = repo.path().join("forced-copy-root");
    let output = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--strategy",
            "copy",
            "--no-copy-fallback",
            "--projection-root",
        ])
        .arg(&root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&output);
    let body = stdout(&output);
    assert_valid_json(&body);
    assert!(body.contains("\"selected_strategy\":\"copy\""));
    assert!(body.contains("\"logical_bytes\":5"));
    assert!(body.contains("\"physically_materialized_bytes\":10"));
    assert!(body.contains("\"physical_allocation_bytes\":null"));
    assert!(body.contains("\"file_count\":1"));
    assert!(body.contains("\"cache_hit\":false"));
    assert!(body.contains("\"reuse\":\"created\""));
    assert!(body.contains("\"integrity_revalidated\":true"));
    assert!(body.contains("\"storage_amplification\":2.000000"));
    assert!(body.contains("\"cache_key\":\"projection-cache:"));

    fs::write(root.join("base.txt"), "projection mutation\n").unwrap();
    let digest = format!("{:x}", Sha256::digest(b"base\n"));
    assert_eq!(
        fs::read(
            repo.path()
                .join(".sunlight/objects/blobs/sha256")
                .join(digest)
        )
        .unwrap(),
        b"base\n"
    );

    let projection_id = json_string_field(&body, "projection_id");
    for output in [
        sun()
            .args(["status", "--projection", &projection_id, "--json"])
            .current_dir(repo.path())
            .output()
            .unwrap(),
        sun()
            .arg("inspect")
            .arg(format!("projection:{projection_id}"))
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .unwrap(),
    ] {
        assert_success(&output);
        let persisted = stdout(&output);
        assert!(persisted.contains("\"strategy\":\"copy\""));
        assert!(persisted.contains("\"logical_bytes\":5"));
        assert!(persisted.contains("\"cache_key\":\"projection-cache:"));
    }
}

#[test]
fn no_fixture_repeated_exact_view_reuses_one_durable_projection_cache_entry() {
    let repo = TestRepo::new("projection-cache-reuse");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-cache-reuse");

    let first = materialize_real_projection_copy(
        &repo,
        "view_base_0001",
        "inspection",
        &repo.path().join("projection-cache-first"),
    );
    assert_success(&first);
    let first_body = stdout(&first);
    assert!(first_body.contains("\"cache_hit\":false"));
    assert!(first_body.contains("\"reuse\":\"created\""));
    assert!(first_body.contains("\"physically_materialized_bytes\":10"));

    let second = materialize_real_projection_copy(
        &repo,
        "view_base_0001",
        "inspection",
        &repo.path().join("projection-cache-second"),
    );
    assert_success(&second);
    let second_body = stdout(&second);
    assert!(second_body.contains("\"cache_hit\":true"));
    assert!(second_body.contains("\"reuse\":\"reused\""));
    assert!(second_body.contains("\"physically_materialized_bytes\":5"));
    assert!(second_body.contains("\"storage_amplification\":1.000000"));
    assert_eq!(
        json_string_field(&first_body, "cache_key"),
        json_string_field(&second_body, "cache_key")
    );
    assert_eq!(projection_cache_entry_roots(&repo).len(), 1);
    let cached_manifest = projection_cache_entry_roots(&repo)[0]
        .join("manifest.json")
        .strip_prefix(repo.path())
        .unwrap()
        .to_path_buf();
    let cache_git_status = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["check-ignore", "-q"])
        .arg(&cached_manifest)
        .output()
        .unwrap();
    assert!(cache_git_status.status.success());

    let export = materialize_real_projection_copy(
        &repo,
        "view_base_0001",
        "export",
        &repo.path().join("projection-cache-export"),
    );
    assert_success(&export);
    let export_body = stdout(&export);
    assert!(export_body.contains("\"cache_hit\":false"));
    assert_ne!(
        json_string_field(&first_body, "cache_key"),
        json_string_field(&export_body, "cache_key")
    );
    assert_eq!(projection_cache_entry_roots(&repo).len(), 2);
}

#[test]
fn no_fixture_corrupt_cache_is_quarantined_rebuilt_without_source_truth_damage() {
    let repo = TestRepo::new("projection-cache-corruption");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-cache-corruption");
    let first_root = repo.path().join("corruption-first");
    let first =
        materialize_real_projection_copy(&repo, "view_base_0001", "inspection", &first_root);
    assert_success(&first);
    let cache_entry = projection_cache_entry_roots(&repo).pop().unwrap();
    let cache_file = cache_entry.join("root/base.txt");
    make_test_file_writable(&cache_file);
    fs::write(&cache_file, b"evil!\n").unwrap();

    let second_root = repo.path().join("corruption-second");
    let second =
        materialize_real_projection_copy(&repo, "view_base_0001", "inspection", &second_root);
    assert_success(&second);
    let body = stdout(&second);
    assert!(body.contains("\"cache_hit\":false"));
    assert!(body.contains("\"reuse\":\"rebuilt_after_quarantine\""));
    assert_eq!(fs::read(second_root.join("base.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(first_root.join("base.txt")).unwrap(), b"base\n");
    let digest = format!("{:x}", Sha256::digest(b"base\n"));
    assert_eq!(
        fs::read(
            repo.path()
                .join(".sunlight/objects/blobs/sha256")
                .join(digest)
        )
        .unwrap(),
        b"base\n"
    );
    let quarantine = repo.path().join(".sunlight/quarantine/projection-cache");
    assert!(fs::read_dir(&quarantine).unwrap().count() >= 2);
    assert_eq!(projection_cache_entry_roots(&repo).len(), 1);
}

#[test]
fn no_fixture_writable_compat_projection_never_aliases_cached_or_peer_bytes() {
    let repo = TestRepo::new("projection-cache-writable-isolation");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-cache-writable");
    let (_, first_root, _) = create_real_compat_projection(&repo);
    let (_, second_root, _) = create_real_compat_projection(&repo);
    let cache_entry = projection_cache_entry_roots(&repo).pop().unwrap();
    let cache_file = cache_entry.join("root/base.txt");
    assert_eq!(fs::read(&cache_file).unwrap(), b"base\n");

    fs::write(first_root.join("base.txt"), b"private mutation\n").unwrap();
    assert_eq!(
        fs::read(first_root.join("base.txt")).unwrap(),
        b"private mutation\n"
    );
    assert_eq!(fs::read(second_root.join("base.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(&cache_file).unwrap(), b"base\n");
    assert_eq!(projection_cache_entry_roots(&repo).len(), 1);
}

#[test]
fn no_fixture_external_managed_roots_reuse_repository_local_projection_cache() {
    let repo = TestRepo::new("projection-cache-external-repo");
    let external = TestRepo::new("projection-cache-external-managed");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-cache-external");
    set_projection_default_root(&repo, &external.path().to_string_lossy().replace('\\', "/"));

    let first = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--strategy",
            "copy",
            "--no-copy-fallback",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&first);
    assert!(stdout(&first).contains("\"cache_hit\":false"));
    let second = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--strategy",
            "copy",
            "--no-copy-fallback",
            "--json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&second);
    assert!(stdout(&second).contains("\"cache_hit\":true"));
    assert_eq!(projection_cache_entry_roots(&repo).len(), 1);
    assert!(!external.path().join(".sunlight/cache").exists());
    assert!(
        fs::read_dir(external.path().join("inspection"))
            .unwrap()
            .count()
            >= 2
    );
}

#[cfg(windows)]
#[test]
fn no_fixture_cache_reparse_point_is_quarantined_before_reuse() {
    let repo = TestRepo::new("projection-cache-reparse");
    let outside = TestRepo::new("projection-cache-reparse-outside");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-cache-reparse");
    let first = materialize_real_projection_copy(
        &repo,
        "view_base_0001",
        "inspection",
        &repo.path().join("reparse-first"),
    );
    assert_success(&first);
    let cache_entry = projection_cache_entry_roots(&repo).pop().unwrap();
    let content_root = cache_entry.join("root");
    fs::rename(&content_root, cache_entry.join("displaced-root")).unwrap();
    outside.write_file("base.txt", "malicious outside bytes\n");
    let junction_command = format!(
        "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
        content_root.display(),
        outside.path().display()
    );
    let junction = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(junction_command)
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "stdout={} stderr={}",
        stdout(&junction),
        String::from_utf8_lossy(&junction.stderr)
    );

    let second_root = repo.path().join("reparse-second");
    let second =
        materialize_real_projection_copy(&repo, "view_base_0001", "inspection", &second_root);
    assert_success(&second);
    let body = stdout(&second);
    assert!(body.contains("\"reuse\":\"rebuilt_after_quarantine\""));
    assert_eq!(fs::read(second_root.join("base.txt")).unwrap(), b"base\n");
    assert_eq!(
        fs::read(outside.path().join("base.txt")).unwrap(),
        b"malicious outside bytes\n"
    );
}

#[test]
fn no_fixture_unsupported_strategy_falls_back_or_fails_atomically_when_required() {
    let repo = TestRepo::new("projection-strategy-fallback-atomic");
    init_local_git_repo(&repo);
    start_native_session(&repo, "projection-fallback");

    let fallback_root = repo.path().join("fallback-root");
    let fallback = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--strategy",
            "hardlink_readonly",
            "--projection-root",
        ])
        .arg(&fallback_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&fallback);
    assert!(stdout(&fallback).contains("\"selected_strategy\":\"copy\""));

    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let state_before = fs::read(&state_path).unwrap();
    let required_root = repo.path().join("required-unsupported-root");
    let required = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--strategy",
            "hardlink_readonly",
            "--no-copy-fallback",
            "--projection-root",
        ])
        .arg(&required_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_failure(&required);
    let error = stdout(&required);
    assert_valid_json(&error);
    assert!(
        error.contains("\"code\":\"projection_materialization_unsupported_filesystem_strategy\"")
    );
    assert!(error.contains("\"strategy\":\"hardlink_readonly\""));
    assert!(!required_root.exists());
    assert_eq!(fs::read(state_path).unwrap(), state_before);
    assert!(!repo
        .path()
        .join(".sunlight/projections/projection_inspection_native_0002.json")
        .exists());
}

#[test]
fn no_fixture_automatic_strategy_reports_real_volume_result_without_assuming_cow() {
    let repo = TestRepo::new("projection-auto-capability");
    init_local_git_repo(&repo);
    repo.write_file("aligned.bin", &"x".repeat(8192));
    git(repo.path(), &["add", "aligned.bin"]);
    git(repo.path(), &["commit", "-m", "aligned extent"]);
    start_native_session(&repo, "projection-auto");
    let root = repo.path().join("automatic-root");
    let output = sun()
        .args([
            "project",
            "materialize",
            "--view",
            "view_base_0001",
            "--purpose",
            "inspection",
            "--projection-root",
        ])
        .arg(&root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&output);
    let body = stdout(&output);
    assert_valid_json(&body);
    let strategy = json_string_field(&body, "selected_strategy");
    assert!(
        strategy == "copy" || strategy == "reflink",
        "unexpected strategy: {strategy}"
    );
    if strategy == "reflink" {
        assert!(!body.contains("\"physically_materialized_bytes\":16394"));
    } else {
        assert!(body.contains("\"physically_materialized_bytes\":16394"));
    }
    fs::write(root.join("aligned.bin"), "private write").unwrap();
    assert_eq!(
        fs::read_to_string(repo.path().join("aligned.bin")).unwrap(),
        "x".repeat(8192)
    );
}

#[test]
fn no_fixture_projection_policy_rejection_precedes_files_state_and_process() {
    let repo = TestRepo::new("projection-policy-rejection");
    repo.write_file("README.md", "# policy rejection\n");
    start_native_session(&repo, "policy-rejection");
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let state_before = fs::read(&state_path).unwrap();
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    fs::write(
        &config_path,
        config.replace("case_sensitive = true", "case_sensitive = false"),
    )
    .unwrap();
    let process_marker = repo.path().join("PROCESS_MUST_NOT_RUN");

    let run = sun()
        .arg("run")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--json")
        .arg("--")
        .arg("python")
        .arg("-c")
        .arg("from pathlib import Path; import sys; Path(sys.argv[1]).write_text('ran')")
        .arg(&process_marker)
        .current_dir(repo.path())
        .output()
        .expect("sun run should reject unsupported config");
    assert_failure(&run);
    assert!(stdout(&run).contains("\"code\":\"invalid_repository_config\""));
    assert!(!process_marker.exists());
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    assert!(!repo
        .path()
        .join(".sunlight/projections/projection_execution_native_0001")
        .exists());

    fs::write(&config_path, config).unwrap();
    set_projection_default_root(&repo, "src/projections");
    let compat = sun()
        .arg("compat")
        .arg("project")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat project should reject unsafe config");
    assert_failure(&compat);
    assert!(stdout(&compat).contains("source tree"));
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    assert!(!repo.path().join("src/projections").exists());
}

#[test]
fn no_fixture_compat_root_tampering_is_rejected_within_custom_managed_root() {
    let repo = TestRepo::new("compat-root-tampering");
    repo.write_file("README.md", "# root tampering\n");
    start_native_session(&repo, "root-tampering");
    set_projection_default_root(&repo, ".sunlight/custom-managed");
    let (projection_id, projection_root, generation) =
        create_real_compat_projection_at(&repo, ".sunlight/custom-managed");
    let tampered_root = repo.path().join(".sunlight/custom-managed/tampered/root");
    fs::create_dir_all(&tampered_root).unwrap();
    fs::copy(
        projection_root.join("README.md"),
        tampered_root.join("README.md"),
    )
    .unwrap();
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let state = fs::read_to_string(&state_path).unwrap();
    let original_json_path = json_string_field(&state, "materialized_root");
    let tampered_json_path = fs::canonicalize(&tampered_root)
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let tampered_state = state.replacen(&original_json_path, &tampered_json_path, 1);
    assert_ne!(tampered_state, state);
    fs::write(&state_path, &tampered_state).unwrap();

    let diff = sun()
        .arg("compat")
        .arg("diff")
        .arg("--projection")
        .arg(&projection_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat diff should reject root tampering");
    assert_failure(&diff);
    assert!(stdout(&diff).contains("\"code\":\"compat_projection_invalid\""));
    assert!(stdout(&diff).contains("configured managed subtree"));

    let import = real_compat_import(
        &repo,
        &projection_id,
        "compat_delta_tampered",
        Some(&generation),
    );
    assert_failure(&import);
    assert!(stdout(&import).contains("\"code\":\"compat_projection_invalid\""));
    assert_eq!(fs::read_to_string(&state_path).unwrap(), tampered_state);
}

#[test]
fn no_fixture_topics_and_sessions_are_distinct_repo_backed_records() {
    let repo = TestRepo::new("real-multi-topic-session");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# Multi\n\nalpha\n");
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn value() -> u32 {\n    1\n}\n",
    );
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    for (slug, display) in [("alpha-topic", "Alpha Topic"), ("beta-topic", "Beta Topic")] {
        let topic = sun()
            .arg("topic")
            .arg("create")
            .arg(slug)
            .arg("--display-name")
            .arg(display)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun topic create should run");
        assert_success(&topic);
        assert!(stdout(&topic).contains(&format!("\"slug\":\"{slug}\"")));
    }

    let duplicate = sun()
        .arg("topic")
        .arg("create")
        .arg("alpha-topic")
        .arg("--display-name")
        .arg("Duplicate Alpha")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("duplicate topic create should run");
    assert_failure(&duplicate);
    assert!(stdout(&duplicate).contains("\"code\":\"topic_conflict\""));

    let alpha_session = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("alpha-topic")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent-a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alpha session start should run");
    assert_success(&alpha_session);
    let alpha_stdout = stdout(&alpha_session);
    assert!(alpha_stdout.contains("\"session_id\":\"session_agent_a\""));
    assert!(alpha_stdout.contains("\"write_topic_id\":\"topic_alpha_topic\""));

    let beta_session = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_beta_topic")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent-b")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("beta session start should run");
    assert_success(&beta_session);
    let beta_stdout = stdout(&beta_session);
    assert!(beta_stdout.contains("\"session_id\":\"session_agent_b\""));
    assert!(beta_stdout.contains("\"write_topic_id\":\"topic_beta_topic\""));

    let alpha_read = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alpha read should run");
    assert_success(&alpha_read);
    let readme_hash = json_string_field(&stdout(&alpha_read), "content_hash");
    let alpha_content = repo.write_file("alpha-readme.md", "# Multi\n\nalpha updated\n");
    let alpha_write = sun()
        .arg("write")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&readme_hash)
        .arg("--content-file")
        .arg(&alpha_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alpha write should run");
    assert_success(&alpha_write);
    let alpha_write_stdout = stdout(&alpha_write);
    assert!(alpha_write_stdout.contains("\"topic_id\":\"topic_alpha_topic\""));
    assert!(alpha_write_stdout.contains("\"topic_revision_id\":\"rev_alpha_topic_0001\""));
    assert!(alpha_write_stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));

    let beta_read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_b")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("beta read should run");
    assert_success(&beta_read);
    let lib_hash = json_string_field(&stdout(&beta_read), "content_hash");
    let beta_content = repo.write_file("beta-lib.rs", "pub fn value() -> u32 {\n    2\n}\n");
    let beta_write = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_b")
        .arg("--expect-hash")
        .arg(&lib_hash)
        .arg("--content-file")
        .arg(&beta_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("beta write should run");
    assert_success(&beta_write);
    let beta_write_stdout = stdout(&beta_write);
    assert!(beta_write_stdout.contains("\"topic_id\":\"topic_beta_topic\""));
    assert!(beta_write_stdout.contains("\"topic_revision_id\":\"rev_beta_topic_0001\""));
    assert!(beta_write_stdout.contains("\"session_generation_id\":\"gen_agent_b_0002\""));
    let beta_session_view = json_string_field(&beta_write_stdout, "resolved_view_id");

    let beta_projection_root = repo.path().join("beta-session-projection");
    let beta_materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&beta_session_view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&beta_projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("beta session materialize should run");
    assert_success(&beta_materialize);
    assert_eq!(
        fs::read_to_string(beta_projection_root.join("README.md")).unwrap(),
        "# Multi\n\nalpha\n"
    );
    assert_eq!(
        fs::read_to_string(beta_projection_root.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u32 {\n    2\n}\n"
    );

    let alpha_status = sun()
        .arg("status")
        .arg("--topic")
        .arg("alpha-topic")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alpha status should run");
    assert_success(&alpha_status);
    let alpha_status_stdout = stdout(&alpha_status);
    assert!(alpha_status_stdout.contains("\"topic_id\":\"topic_alpha_topic\""));
    assert!(alpha_status_stdout.contains("\"head_revision_id\":\"rev_alpha_topic_0001\""));
    assert!(!alpha_status_stdout.contains("\"topic_id\":\"topic_beta_topic\""));

    let beta_status = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_b")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("beta session status should run");
    assert_success(&beta_status);
    let beta_status_stdout = stdout(&beta_status);
    assert!(beta_status_stdout.contains("\"session_id\":\"session_agent_b\""));
    assert!(beta_status_stdout.contains("\"topic_id\":\"topic_beta_topic\""));
    assert!(beta_status_stdout.contains("\"session_generation_id\":\"gen_agent_b_0002\""));

    let state_json = fs::read_to_string(repo.path().join(".sunlight/records/native-state.json"))
        .expect("native state should exist");
    assert!(state_json.contains("\"topics\":["));
    assert!(state_json.contains("\"sessions\":["));
    assert!(state_json.contains("\"topic_id\":\"topic_alpha_topic\""));
    assert!(state_json.contains("\"topic_id\":\"topic_beta_topic\""));
    assert!(state_json.contains("\"session_id\":\"session_agent_a\""));
    assert!(state_json.contains("\"session_id\":\"session_agent_b\""));
}

#[test]
fn no_fixture_repo_backed_resolver_merges_independent_topics_and_reports_conflicts() {
    let repo = TestRepo::new("real-resolver-topics");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# Resolver\n\nbase\n");
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn value() -> u32 {\n    1\n}\n",
    );
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    for (slug, display) in [
        ("docs-topic", "Docs Topic"),
        ("code-topic", "Code Topic"),
        ("alt-code-topic", "Alt Code Topic"),
    ] {
        let topic = sun()
            .arg("topic")
            .arg("create")
            .arg(slug)
            .arg("--display-name")
            .arg(display)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun topic create should run");
        assert_success(&topic);
    }

    for (topic, actor) in [
        ("docs-topic", "agent-docs"),
        ("code-topic", "agent-code"),
        ("alt-code-topic", "agent-alt"),
    ] {
        let session = sun()
            .arg("session")
            .arg("start")
            .arg("--topic")
            .arg(topic)
            .arg("--view")
            .arg("view_base_0001")
            .arg("--actor")
            .arg(actor)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun session start should run");
        assert_success(&session);
    }

    let docs_read = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_docs")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("docs read should run");
    assert_success(&docs_read);
    let readme_hash = json_string_field(&stdout(&docs_read), "content_hash");
    let docs_content = repo.write_file("docs-readme.md", "# Resolver\n\nbase\ndocs\n");
    let docs_write = sun()
        .arg("write")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_docs")
        .arg("--expect-hash")
        .arg(&readme_hash)
        .arg("--content-file")
        .arg(&docs_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("docs write should run");
    assert_success(&docs_write);

    let code_read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code read should run");
    assert_success(&code_read);
    let lib_hash = json_string_field(&stdout(&code_read), "content_hash");
    assert!(stdout(&code_read).contains("    1"));
    let code_content = repo.write_file("code-lib.rs", "pub fn value() -> u32 {\n    2\n}\n");
    let code_write = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--expect-hash")
        .arg(&lib_hash)
        .arg("--content-file")
        .arg(&code_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code write should run");
    assert_success(&code_write);

    let resolved = sun()
        .arg("view")
        .arg("resolve")
        .arg("--base")
        .arg("checkpoint_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&resolved);
    let resolved_stdout = stdout(&resolved);
    assert!(resolved_stdout.contains("\"conflict_ids\":[]"));
    assert!(resolved_stdout.contains("\"topic_docs_topic\":\"rev_docs_topic_0001\""));
    assert!(resolved_stdout.contains("\"topic_code_topic\":\"rev_code_topic_0001\""));
    let resolved_view = json_string_field(&resolved_stdout, "resolved_view_id");

    let projection_root = repo.path().join("resolved-projection");
    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);
    assert_eq!(
        fs::read_to_string(projection_root.join("README.md")).unwrap(),
        "# Resolver\n\nbase\ndocs\n"
    );
    assert_eq!(
        fs::read_to_string(projection_root.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u32 {\n    2\n}\n"
    );

    let docs_search = sun()
        .arg("search")
        .arg("docs")
        .arg("--session")
        .arg("session_agent_docs")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("docs search should run");
    assert_success(&docs_search);
    assert!(stdout(&docs_search).contains("\"path\":\"README.md\""));

    let alt_read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_alt")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alt read should run");
    assert_success(&alt_read);
    let alt_hash = json_string_field(&stdout(&alt_read), "content_hash");
    assert_eq!(alt_hash, lib_hash);
    let alt_content = repo.write_file("alt-lib.rs", "pub fn value() -> u32 {\n    3\n}\n");
    let alt_write = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_alt")
        .arg("--expect-hash")
        .arg(&alt_hash)
        .arg("--content-file")
        .arg(&alt_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("alt write should run");
    assert_success(&alt_write);

    let conflicted = sun()
        .arg("view")
        .arg("resolve")
        .arg("--base")
        .arg("checkpoint_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&conflicted);
    let conflicted_stdout = stdout(&conflicted);
    assert!(conflicted_stdout.contains("\"tree_identity\":null"));
    assert!(conflicted_stdout.contains("\"conflict_ids\":[\"conflict_src_lib_rs_0001\"]"));
    assert!(conflicted_stdout.contains("\"kind\":\"same_artifact_conflict\""));
    assert!(
        conflicted_stdout.contains("\"operation_ids\":[\"op_native_0002\",\"op_native_0003\"]")
            || conflicted_stdout
                .contains("\"operation_ids\":[\"op_native_0003\",\"op_native_0002\"]")
    );

    let repository_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&repository_status);
    let repository_status = stdout(&repository_status);
    assert_valid_json(&repository_status);
    assert!(repository_status.contains("\"resolution\":{\"conflicts\":1"));
    assert!(repository_status.contains("\"code\":\"resolver_conflicts\""));

    let base_projection_root = repo.path().join("base-projection-after-conflict");
    let base_materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&base_projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize base should run");
    assert_success(&base_materialize);
    assert_eq!(
        fs::read_to_string(base_projection_root.join("README.md")).unwrap(),
        "# Resolver\n\nbase\n"
    );
    assert_eq!(
        fs::read_to_string(base_projection_root.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u32 {\n    1\n}\n"
    );

    let unknown_revision = sun()
        .arg("view")
        .arg("resolve")
        .arg("--base")
        .arg("checkpoint_base_0001")
        .arg("--include")
        .arg("topic_code_topic:rev_code_topic_9999")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_failure(&unknown_revision);
    let unknown_revision_stdout = stdout(&unknown_revision);
    assert!(unknown_revision_stdout.contains("\"code\":\"object_not_found\""));
    assert!(unknown_revision_stdout.contains("\"object_type\":\"topic_revision\""));
    assert!(unknown_revision_stdout.contains("\"selector\":\"rev_code_topic_9999\""));

    let inspect = sun()
        .arg("inspect")
        .arg("conflict:conflict_src_lib_rs_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect conflict should run");
    assert_success(&inspect);
    let inspect_stdout = stdout(&inspect);
    assert!(inspect_stdout.contains("\"command\":\"inspect.conflict\""));
    assert!(inspect_stdout.contains("\"conflict_id\":\"conflict_src_lib_rs_0001\""));
    assert!(inspect_stdout.contains("\"path\":\"src/lib.rs\""));

    let conflict_record = repo
        .path()
        .join(".sunlight/conflicts/conflict_src_lib_rs_0001.json");
    assert!(conflict_record.is_file());
}

#[test]
fn no_fixture_checkpoint_export_uses_persisted_snapshot_after_head_moves() {
    let repo = TestRepo::new("real-checkpoint-export-snapshot");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# Snapshot\n\nbase\n");
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn value() -> u32 {\n    1\n}\n",
    );
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    for (slug, display, actor) in [
        ("snapshot-docs", "Snapshot Docs", "agent-docs"),
        ("snapshot-code", "Snapshot Code", "agent-code"),
    ] {
        let topic = sun()
            .arg("topic")
            .arg("create")
            .arg(slug)
            .arg("--display-name")
            .arg(display)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun topic create should run");
        assert_success(&topic);
        let session = sun()
            .arg("session")
            .arg("start")
            .arg("--topic")
            .arg(slug)
            .arg("--view")
            .arg("view_base_0001")
            .arg("--actor")
            .arg(actor)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun session start should run");
        assert_success(&session);
    }

    let readme = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_docs")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("docs read should run");
    assert_success(&readme);
    let readme_hash = json_string_field(&stdout(&readme), "content_hash");
    let docs_content = repo.write_file("docs-snapshot.md", "# Snapshot\n\nbase\ndocs v1\n");
    let docs_write = sun()
        .arg("write")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_docs")
        .arg("--expect-hash")
        .arg(&readme_hash)
        .arg("--content-file")
        .arg(&docs_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("docs write should run");
    assert_success(&docs_write);

    let lib = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code read should run");
    assert_success(&lib);
    let lib_hash = json_string_field(&stdout(&lib), "content_hash");
    let code_v2 = repo.write_file("code-v2.rs", "pub fn value() -> u32 {\n    2\n}\n");
    let code_write = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--expect-hash")
        .arg(&lib_hash)
        .arg("--content-file")
        .arg(&code_v2)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code write should run");
    assert_success(&code_write);

    let resolved = sun()
        .arg("view")
        .arg("resolve")
        .arg("--base")
        .arg("checkpoint_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&resolved);
    let resolved_stdout = stdout(&resolved);
    assert!(resolved_stdout.contains("\"conflict_ids\":[]"));
    let resolved_view = json_string_field(&resolved_stdout, "resolved_view_id");

    let projection_root = repo.path().join("snapshot-projection");
    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);
    let projection_stdout = stdout(&materialize);
    let projection_id = json_string_field(&projection_stdout, "projection_id");
    assert!(projection_stdout.contains("\"source\":\"resolved_content_tree\""));
    assert_eq!(
        fs::read_to_string(projection_root.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u32 {\n    2\n}\n"
    );

    let projection_status = sun()
        .arg("status")
        .arg("--projection")
        .arg(&projection_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&projection_status);
    assert!(stdout(&projection_status)
        .contains("\"source_truth\":\"sunlight_persisted_resolved_view\""));
    assert!(stdout(&projection_status).contains("\"manifest_digest\":\"sha256:"));

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");

    let checkpoint_inspect = sun()
        .arg("inspect")
        .arg(format!("checkpoint:{checkpoint_id}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect checkpoint should run");
    assert_success(&checkpoint_inspect);
    assert!(
        stdout(&checkpoint_inspect).contains("\"source_truth\":\"sunlight_persisted_checkpoint\"")
    );
    assert!(stdout(&checkpoint_inspect).contains("\"path\":\"src/lib.rs\""));

    let lib_after_checkpoint = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code read after checkpoint should run");
    assert_success(&lib_after_checkpoint);
    let lib_v2_hash = json_string_field(&stdout(&lib_after_checkpoint), "content_hash");
    let code_v3 = repo.write_file("code-v3.rs", "pub fn value() -> u32 {\n    3\n}\n");
    let code_move_head = sun()
        .arg("write")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--expect-hash")
        .arg(&lib_v2_hash)
        .arg("--content-file")
        .arg(&code_v3)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code v3 write should run");
    assert_success(&code_move_head);
    let moved_head_read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_code")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("code moved head read should run");
    assert_success(&moved_head_read);
    assert!(stdout(&moved_head_read).contains("    3"));

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/snapshot-export")
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    let export_map_id = format!("export_map_{checkpoint_id}");
    assert!(stdout(&export).contains("\"lifecycle_state\":\"exported\""));
    assert_eq!(
        git(
            repo.path(),
            &["show", "sunlight/snapshot-export:src/lib.rs"]
        ),
        "pub fn value() -> u32 {\n    2\n}\n"
    );
    assert_ne!(
        git(
            repo.path(),
            &["show", "sunlight/snapshot-export:src/lib.rs"]
        ),
        "pub fn value() -> u32 {\n    3\n}\n"
    );

    let export_status = sun()
        .arg("status")
        .arg("--export")
        .arg(&export_map_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status export should run");
    assert_success(&export_status);
    assert!(stdout(&export_status).contains("\"source_truth\":\"sunlight_persisted_checkpoint\""));
    assert!(stdout(&export_status).contains(&format!("\"checkpoint_id\":\"{checkpoint_id}\"")));
}

#[test]
fn no_fixture_init_respects_git_ignore_policy() {
    let repo = TestRepo::new("real-repo-ignore-policy");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file(".gitignore", "target/\n.cache/\n");
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn kept() -> &'static str {\n    \"normal-source-needle\"\n}\n",
    );
    write_nested_file(
        repo.path(),
        "target/debug/build.log",
        "ignored-build-needle\n",
    );
    write_nested_file(
        repo.path(),
        ".cache/sun/local.txt",
        "ignored-cache-needle\n",
    );
    git(repo.path(), &["add", ".gitignore", "src/lib.rs"]);
    git(repo.path(), &["commit", "-m", "base"]);

    start_native_session(&repo, "ignore-policy");

    let native_state = fs::read_to_string(repo.path().join(".sunlight/records/native-state.json"))
        .expect("native state should exist");
    assert!(native_state.contains("\"path\":\"src/lib.rs\""));
    assert!(!native_state.contains("target/debug/build.log"));
    assert!(!native_state.contains(".cache/sun/local.txt"));
    assert!(!native_state.contains("ignored-build-needle"));
    assert!(!native_state.contains("ignored-cache-needle"));

    let list_src = sun()
        .arg("list")
        .arg("src")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun list src should run");
    assert_success(&list_src);
    assert!(stdout(&list_src).contains("\"path\":\"src/lib.rs\""));

    let list_target = sun()
        .arg("list")
        .arg("target")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun list target should run");
    assert_success(&list_target);
    let target_stdout = stdout(&list_target);
    assert!(!target_stdout.contains("target/debug/build.log"));
    assert!(!target_stdout.contains("ignored-build-needle"));

    let search_source = sun()
        .arg("search")
        .arg("normal-source-needle")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search source should run");
    assert_success(&search_source);
    assert!(stdout(&search_source).contains("\"path\":\"src/lib.rs\""));

    let search_ignored = sun()
        .arg("search")
        .arg("ignored-build-needle")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search ignored should run");
    assert_success(&search_ignored);
    let ignored_stdout = stdout(&search_ignored);
    assert!(!ignored_stdout.contains("target/debug/build.log"));
    assert!(!ignored_stdout.contains("ignored-build-needle"));

    let search_cache = sun()
        .arg("search")
        .arg("ignored-cache-needle")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search ignored cache should run");
    assert_success(&search_cache);
    assert!(!stdout(&search_cache).contains(".cache/sun/local.txt"));
}

#[test]
fn no_fixture_init_quarantines_tracked_secret_without_persisting_or_searching_bytes() {
    let repo = TestRepo::new("real-secret-quarantine");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(repo.path(), "src/lib.rs", "pub fn kept() {}\n");
    repo.write_file(
        ".env",
        "API_KEY=tracked-secret-value-that-must-not-persist\n",
    );
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);

    start_native_session(&repo, "secret-quarantine");

    let native_state = fs::read_to_string(repo.path().join(".sunlight/records/native-state.json"))
        .expect("native state should exist");
    assert!(native_state.contains("\"path\":\".env\""));
    assert!(native_state.contains("\"classification\":\"secret\""));
    assert!(!native_state.contains("tracked-secret-value-that-must-not-persist"));
    let blob_root = repo.path().join(".sunlight/objects/blobs/sha256");
    for blob in fs::read_dir(blob_root).expect("blob directory should exist") {
        let blob = blob.expect("blob entry should be readable");
        let bytes = fs::read(blob.path()).expect("blob should be readable");
        assert!(
            !String::from_utf8_lossy(&bytes).contains("tracked-secret-value-that-must-not-persist")
        );
    }

    let report = fs::read_to_string(repo.path().join(".sunlight/quarantine/ingest-report.json"))
        .expect("quarantine report should exist");
    assert!(report.contains("\"record_type\":\"ingest_quarantine_report\""));
    assert!(report.contains("\"quarantined_count\":1"));
    assert!(report.contains("\"path\":\".env\""));
    assert!(report.contains("\"reason_codes\":[\"secret_path\",\"secret_token\"]"));
    assert!(!report.contains("tracked-secret-value-that-must-not-persist"));

    let status = sun()
        .arg("status")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");
    assert_success(&status);
    let status_stdout = stdout(&status);
    assert!(status_stdout.contains("\"quarantined_secret_count\":1"));
    assert!(status_stdout.contains("\"code\":\"ingest_secrets_quarantined\""));

    let list_root = sun()
        .arg("list")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun list should run");
    assert_success(&list_root);
    let list_stdout = stdout(&list_root);
    assert!(list_stdout.contains("\"path\":\"src/lib.rs\""));
    assert!(!list_stdout.contains("\"path\":\".env\""));

    let read_secret = sun()
        .arg("read")
        .arg(".env")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read secret should run");
    assert_failure(&read_secret);
    let read_stdout = stdout(&read_secret);
    assert!(read_stdout.contains("\"code\":\"path_not_found\""));
    assert!(read_stdout.contains("\"path\":\".env\""));
    assert!(!read_stdout.contains("tracked-secret-value-that-must-not-persist"));

    let search_secret = sun()
        .arg("search")
        .arg("tracked-secret-value-that-must-not-persist")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search secret should run");
    assert_success(&search_secret);
    let search_stdout = stdout(&search_secret);
    assert!(!search_stdout.contains(".env"));
    assert!(!search_stdout.contains("tracked-secret-value-that-must-not-persist"));
}

#[test]
fn no_fixture_git_export_uses_persisted_checkpoint_when_current_head_is_secret_classified() {
    let repo = TestRepo::new("real-secret-export-gate");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# public\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "secret-export");

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");

    let read = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let content_hash = json_string_field(&stdout(&read), "content_hash");

    let metadata = sun()
        .arg("metadata")
        .arg("set")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&content_hash)
        .arg("--classification")
        .arg("secret")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun metadata set should run");
    assert_success(&metadata);

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/secret-export")
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    let export_stdout = stdout(&export);
    assert!(export_stdout.contains("\"lifecycle_state\":\"exported\""));
    assert_eq!(
        git(repo.path(), &["show", "sunlight/secret-export:README.md"]),
        "# public\n"
    );
}

#[test]
fn no_fixture_checkpoint_rejects_local_only_classified_artifact() {
    let repo = TestRepo::new("real-local-only-checkpoint-gate");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "# public\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "local-only-checkpoint");

    let read = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let content_hash = json_string_field(&stdout(&read), "content_hash");

    let metadata = sun()
        .arg("metadata")
        .arg("set")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&content_hash)
        .arg("--classification")
        .arg("local-only")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun metadata set should run");
    assert_success(&metadata);
    let resolved_view = json_string_field(&stdout(&metadata), "resolved_view_id");

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_failure(&checkpoint);
    let checkpoint_stdout = stdout(&checkpoint);
    assert!(checkpoint_stdout.contains("\"code\":\"export_policy_failed\""));
    assert!(checkpoint_stdout.contains("\"blocked_paths\":\"README.md\""));
    assert!(checkpoint_stdout.contains("\"blocked_classifications\":\"local-only\""));
}

#[test]
fn no_fixture_move_after_sort_uses_moved_artifact_for_response() {
    let repo = TestRepo::new("real-move-sort-order");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("b.txt", "b\n");
    repo.write_file("c.txt", "c\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "move-sort");

    let read = sun()
        .arg("read")
        .arg("c.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let content_hash = json_string_field(&stdout(&read), "content_hash");

    let moved = sun()
        .arg("move")
        .arg("c.txt")
        .arg("a.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&content_hash)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun move should run");
    assert_success(&moved);
    let moved_stdout = stdout(&moved);
    assert!(moved_stdout.contains("\"command\":\"artifact.move\""));
    assert!(moved_stdout.contains("\"artifact_id\":\"artifact_c_txt\""));
    assert!(moved_stdout.contains("\"path\":\"a.txt\""));
    assert!(moved_stdout.contains("\"path_state\":\"active\""));
    assert!(!moved_stdout.contains("\"artifact_id\":\"artifact_b_txt\",\"path\":\"b.txt\""));
}

#[test]
fn no_fixture_delete_is_removed_from_local_git_export() {
    let repo = TestRepo::new("real-delete-export");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("keep.txt", "keep\n");
    repo.write_file("remove.txt", "remove\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "delete-export");

    let read = sun()
        .arg("read")
        .arg("remove.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let content_hash = json_string_field(&stdout(&read), "content_hash");

    let deleted = sun()
        .arg("delete")
        .arg("remove.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&content_hash)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun delete should run");
    assert_success(&deleted);
    let resolved_view = json_string_field(&stdout(&deleted), "resolved_view_id");

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&resolved_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/delete-export")
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    assert_eq!(
        git(repo.path(), &["show", "sunlight/delete-export:keep.txt"]),
        "keep\n"
    );
    let missing = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["cat-file", "-e", "sunlight/delete-export:remove.txt"])
        .output()
        .expect("git cat-file should run");
    assert!(
        !missing.status.success(),
        "remove.txt should be absent from exported commit"
    );
}

#[test]
fn no_fixture_compat_project_diff_import_flows_into_native_consumers() {
    let repo = TestRepo::new("real-compat-vertical");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(
        repo.path(),
        "src/lib.rs",
        "pub fn answer() -> u32 {\n    42\n}\n",
    );
    repo.write_file("README.md", "# Compatibility baseline\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "compat-real");

    let (modified_projection, modified_root, modified_generation) =
        create_real_compat_projection(&repo);
    fs::write(
        modified_root.join("src/lib.rs"),
        b"pub fn answer() -> u32 {\n    43\n}\n",
    )
    .unwrap();
    repo.write_file("main-worktree-only.txt", "not compatibility truth\n");
    let modified_diff = real_compat_diff(&repo, &modified_projection);
    assert!(modified_diff.contains("\"kind\":\"modified_source\""));
    assert!(!modified_diff.contains("main-worktree-only.txt"));
    let projection_status = sun()
        .arg("status")
        .arg("--projection")
        .arg(&modified_projection)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&projection_status);
    assert!(stdout(&projection_status).contains("\"lifecycle_state\":\"dirty\""));
    assert!(stdout(&projection_status).contains("\"dirty_candidate_summary\""));
    assert!(stdout(&projection_status).contains("\"code\":\"dirty_compatibility_projection\""));
    let projection_inspect = sun()
        .arg("inspect")
        .arg(format!("projection:{modified_projection}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");
    assert_success(&projection_inspect);
    assert!(stdout(&projection_inspect).contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout(&projection_inspect).contains("\"lifecycle_state\":\"dirty\""));
    let repository_status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&repository_status);
    assert!(stdout(&repository_status).contains("\"lifecycle\":{\"materialized\":0,\"dirty\":1"));
    assert!(stdout(&repository_status).contains("\"code\":\"dirty_compatibility_projections\""));
    let modified_candidate = candidate_id_for_path(&modified_diff, "src/lib.rs");
    let modified_import = real_compat_import(
        &repo,
        &modified_projection,
        &modified_candidate,
        Some(&modified_generation),
    );
    assert_success(&modified_import);
    let modified_stdout = stdout(&modified_import);
    assert!(modified_stdout.contains("\"command\":\"compat.import\""));
    assert!(modified_stdout.contains("\"kind\":\"compat_import\""));
    let modified_operation = json_string_field(&modified_stdout, "operation_transaction_id");
    let modified_artifact = json_string_field(&modified_stdout, "artifact_id");
    let modified_view = json_string_field(&modified_stdout, "resolved_view_id");
    let modified_session_generation = json_string_field(&modified_stdout, "session_generation_id");
    let modified_generation_record = fs::read_to_string(
        repo.path()
            .join(".sunlight/session-generations")
            .join(format!("{modified_session_generation}.json")),
    )
    .unwrap();
    assert!(modified_generation_record.contains("\"session_id\":\"session_agent_a\""));

    let read = sun()
        .arg("read")
        .arg("src/lib.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    assert!(stdout(&read).contains("43"));
    let search = sun()
        .arg("search")
        .arg("43")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search should run");
    assert_success(&search);
    assert!(stdout(&search).contains("\"path\":\"src/lib.rs\""));

    let inspect_operation = sun()
        .arg("inspect")
        .arg(format!("compat-import:{modified_operation}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect compat import should run");
    assert_success(&inspect_operation);
    assert!(stdout(&inspect_operation).contains("\"command\":\"inspect.compat-import\""));
    assert!(stdout(&inspect_operation).contains(&modified_projection));
    let status_import = sun()
        .arg("status")
        .arg("--compat-import")
        .arg(&modified_operation)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status compat import should run");
    assert_success(&status_import);
    assert!(stdout(&status_import).contains("\"command\":\"status.compat-import\""));
    let inspect_artifact = sun()
        .arg("inspect")
        .arg(format!("artifact:{modified_artifact}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect imported artifact should run");
    assert_success(&inspect_artifact);
    assert!(stdout(&inspect_artifact).contains("\"compat_import_provenance\":{"));

    let (new_projection, new_root, _) = create_real_compat_projection(&repo);
    write_nested_file(
        &new_root,
        "src/added.rs",
        "pub fn added() -> bool { true }\n",
    );
    let new_diff = real_compat_diff(&repo, &new_projection);
    let new_candidate = candidate_id_for_path(&new_diff, "src/added.rs");
    let new_import = real_compat_import(&repo, &new_projection, &new_candidate, None);
    assert_success(&new_import);
    assert!(stdout(&new_import).contains("\"operation_kind\":\"write\""));

    let (delete_projection, delete_root, _) = create_real_compat_projection(&repo);
    fs::remove_file(delete_root.join("README.md")).unwrap();
    let delete_diff = real_compat_diff(&repo, &delete_projection);
    let delete_candidate = candidate_id_for_path(&delete_diff, "README.md");
    let delete_import = real_compat_import(&repo, &delete_projection, &delete_candidate, None);
    assert_success(&delete_import);
    let delete_stdout = stdout(&delete_import);
    assert!(delete_stdout.contains("\"operation_kind\":\"delete\""));
    let final_view = json_string_field(&delete_stdout, "resolved_view_id");
    assert_ne!(modified_view, final_view);

    let status = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status session should run");
    assert_success(&status);
    assert!(stdout(&status).contains("\"compatibility_projections\":["));
    assert!(stdout(&status).contains("\"last_import_operation_id\":\""));

    let materialized = repo.path().join("compat-result");
    let project = sun()
        .arg("project")
        .arg("materialize")
        .arg(&final_view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&materialized)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&project);
    assert!(fs::read_to_string(materialized.join("src/lib.rs"))
        .unwrap()
        .contains("43"));
    assert!(materialized.join("src/added.rs").is_file());
    assert!(!materialized.join("README.md").exists());

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg(&final_view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");
    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/compat-real")
        .arg("--execute-local")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");
    assert_success(&export);
    assert!(git(repo.path(), &["show", "sunlight/compat-real:src/lib.rs"]).contains("43"));
    assert!(git(repo.path(), &["show", "sunlight/compat-real:src/added.rs"]).contains("added"));
    let missing_readme = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["cat-file", "-e", "sunlight/compat-real:README.md"])
        .output()
        .unwrap();
    assert!(!missing_readme.status.success());
}

#[test]
fn no_fixture_compat_import_rejects_secret_and_stale_generation_without_partial_state() {
    let repo = TestRepo::new("real-compat-atomic");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("README.md", "baseline\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "compat-atomic");

    let (projection, root, generation) = create_real_compat_projection(&repo);
    fs::write(root.join("README.md"), b"safe projection edit\n").unwrap();
    fs::write(root.join(".env"), b"API_KEY=projection-secret\n").unwrap();
    let diff = real_compat_diff(&repo, &projection);
    let safe_candidate = candidate_id_for_path(&diff, "README.md");
    let secret_candidate = candidate_id_for_path(&diff, ".env");
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let before_blocked = fs::read(&state_path).unwrap();
    let blocked = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg(&projection)
        .arg("--candidate")
        .arg(&safe_candidate)
        .arg("--candidate")
        .arg(&secret_candidate)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("blocked sun compat import should run");
    assert_failure(&blocked);
    assert!(stdout(&blocked).contains("\"code\":\"compat_secret_detected\""));
    assert_eq!(before_blocked, fs::read(&state_path).unwrap());
    assert!(!fs::read_to_string(&state_path)
        .unwrap()
        .contains("projection-secret"));

    let read = sun()
        .arg("read")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");
    assert_success(&read);
    let hash = json_string_field(&stdout(&read), "content_hash");
    let native_content = repo.write_file("native-content.txt", "native advance\n");
    let advance = sun()
        .arg("write")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg(&hash)
        .arg("--content-file")
        .arg(native_content)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun write should run");
    assert_success(&advance);
    let before_stale = fs::read(&state_path).unwrap();
    let stale = real_compat_import(&repo, &projection, &safe_candidate, Some(&generation));
    assert_failure(&stale);
    assert!(stdout(&stale).contains("\"code\":\"compat_precondition_failed\""));
    assert_eq!(before_stale, fs::read(&state_path).unwrap());
}

#[test]
fn no_fixture_compat_import_applies_modified_created_and_deleted_as_one_transaction() {
    let repo = TestRepo::new("real-compat-multi");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(repo.path(), "src/lib.rs", "pub fn answer() -> u32 { 42 }\n");
    repo.write_file("README.md", "baseline\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "compat-multi");

    let (projection, root, generation) = create_real_compat_projection(&repo);
    fs::write(root.join("src/lib.rs"), b"pub fn answer() -> u32 { 43 }\n").unwrap();
    write_nested_file(&root, "src/added.rs", "pub fn added() -> bool { true }\n");
    fs::remove_file(root.join("README.md")).unwrap();
    let diff = real_compat_diff(&repo, &projection);
    let candidates = vec![
        candidate_id_for_path(&diff, "src/lib.rs"),
        candidate_id_for_path(&diff, "src/added.rs"),
        candidate_id_for_path(&diff, "README.md"),
        candidate_id_for_path(&diff, "src/lib.rs"),
    ];
    let imported = real_compat_import_many(&repo, &projection, &candidates, Some(&generation));
    assert_success(&imported);
    let output = stdout(&imported);
    assert!(output.contains("\"selected_delta_count\":3"));
    assert_eq!(
        output
            .matches("\"operation_transaction_id\":\"op_native_0001\"")
            .count(),
        3
    );
    assert!(output.contains("\"topic_revision_id\":\"rev_compat_multi_0001\""));
    assert!(output.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    let view = json_string_field(&output, "resolved_view_id");
    let operation = json_string_field(&output, "operation_transaction_id");
    for (command, selector) in [("inspect", format!("compat-import:{operation}"))] {
        let result = sun()
            .arg(command)
            .arg(selector)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert_success(&result);
        let text = stdout(&result);
        assert!(
            text.contains("src/lib.rs")
                && text.contains("src/added.rs")
                && text.contains("README.md")
        );
    }
    let status = sun()
        .arg("status")
        .arg("--compat-import")
        .arg(&operation)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&status);
    assert!(stdout(&status).contains("\"artifact_effects\":["));
    let list = sun()
        .arg("list")
        .arg("src")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&list);
    assert!(stdout(&list).contains("src/added.rs"));
    let search = sun()
        .arg("search")
        .arg("added")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&search);
    assert!(stdout(&search).contains("src/added.rs"));

    let materialized = repo.path().join("compat-multi-result");
    let project = sun()
        .arg("project")
        .arg("materialize")
        .arg(&view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&materialized)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&project);
    assert!(fs::read_to_string(materialized.join("src/lib.rs"))
        .unwrap()
        .contains("43"));
    assert!(materialized.join("src/added.rs").is_file());
    assert!(!materialized.join("README.md").exists());

    let state =
        fs::read_to_string(repo.path().join(".sunlight/records/native-state.json")).unwrap();
    assert_eq!(
        state
            .matches("\"operation_transaction_id\":\"op_native_0001\"")
            .count(),
        1
    );
    assert_eq!(
        state
            .matches("\"topic_revision_id\":\"rev_compat_multi_0001\"")
            .count(),
        1
    );
    assert!(state.contains("\"effects\":[{"));
}

#[test]
fn no_fixture_compat_exact_rename_preserves_identity_through_export() {
    let repo = TestRepo::new("real-compat-rename");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(
        repo.path(),
        "src/old_name.rs",
        "pub fn preserved() -> bool { true }\n",
    );
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "compat-rename");
    let baseline_read = sun()
        .arg("read")
        .arg("src/old_name.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&baseline_read);
    let artifact_id = json_string_field(&stdout(&baseline_read), "artifact_id");

    let (projection, root, generation) = create_real_compat_projection(&repo);
    fs::rename(root.join("src/old_name.rs"), root.join("src/new_name.rs")).unwrap();
    let diff = real_compat_diff(&repo, &projection);
    assert!(diff.contains("\"kind\":\"moved_or_renamed\""));
    assert!(diff.contains("\"operation_kind\":\"move\""));
    assert!(diff.contains("\"source_path\":\"src/old_name.rs\""));
    assert!(!diff.contains("\"kind\":\"deleted_source\""));
    let candidate = candidate_id_for_path(&diff, "src/new_name.rs");
    let imported = real_compat_import(&repo, &projection, &candidate, Some(&generation));
    assert_success(&imported);
    let imported_json = stdout(&imported);
    assert!(imported_json.contains("\"operation_kind\":\"move\""));
    assert!(imported_json.contains(&format!("\"artifact_id\":\"{artifact_id}\"")));
    assert!(imported_json.contains("\"source_path\":\"src/old_name.rs\""));
    let view = json_string_field(&imported_json, "resolved_view_id");

    let new_read = sun()
        .arg("read")
        .arg("src/new_name.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&new_read);
    assert!(stdout(&new_read).contains(&format!("\"artifact_id\":\"{artifact_id}\"")));
    let old_read = sun()
        .arg("read")
        .arg("src/old_name.rs")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_failure(&old_read);
    let list = sun()
        .arg("list")
        .arg("src")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&list);
    let list_json = stdout(&list);
    assert!(list_json.contains("src/new_name.rs"));
    assert!(!list_json.contains("src/old_name.rs"));
    let inspect = sun()
        .arg("inspect")
        .arg(format!("artifact:{artifact_id}"))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&inspect);
    let inspect_json = stdout(&inspect);
    assert!(inspect_json.contains("\"path_history\":["));
    assert!(inspect_json.contains("\"path\":\"src/old_name.rs\",\"state\":\"tombstone\""));
    assert!(inspect_json.contains("\"path\":\"src/new_name.rs\",\"state\":\"active\""));

    let materialized = repo.path().join("compat-rename-result");
    let project = sun()
        .arg("project")
        .arg("materialize")
        .arg(&view)
        .arg("--purpose")
        .arg("inspection")
        .arg("--projection-root")
        .arg(&materialized)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&project);
    assert!(materialized.join("src/new_name.rs").is_file());
    assert!(!materialized.join("src/old_name.rs").exists());

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg(&view)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");
    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("sunlight/compat-rename")
        .arg("--execute-local")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&export);
    assert!(git(
        repo.path(),
        &["show", "sunlight/compat-rename:src/new_name.rs"]
    )
    .contains("preserved"));
    let missing_old = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["cat-file", "-e", "sunlight/compat-rename:src/old_name.rs"])
        .output()
        .unwrap();
    assert!(!missing_old.status.success());
}

#[test]
fn no_fixture_compat_ambiguous_exact_rename_is_atomic() {
    let repo = TestRepo::new("real-compat-rename-ambiguous");
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    write_nested_file(repo.path(), "src/original.rs", "pub fn same() {}\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "base"]);
    start_native_session(&repo, "compat-rename-ambiguous");

    let (projection, root, generation) = create_real_compat_projection(&repo);
    let bytes = fs::read(root.join("src/original.rs")).unwrap();
    fs::remove_file(root.join("src/original.rs")).unwrap();
    fs::write(root.join("src/copy_a.rs"), &bytes).unwrap();
    fs::write(root.join("src/copy_b.rs"), &bytes).unwrap();
    let diff = real_compat_diff(&repo, &projection);
    assert_eq!(diff.matches("\"kind\":\"moved_or_renamed\"").count(), 2);
    assert!(diff.contains("\"source_path\":null"));
    assert!(!diff.contains("\"kind\":\"deleted_source\""));
    let candidate = candidate_id_for_path(&diff, "src/copy_a.rs");
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let before = fs::read(&state_path).unwrap();
    let imported = real_compat_import(&repo, &projection, &candidate, Some(&generation));
    assert_failure(&imported);
    assert!(stdout(&imported).contains("\"code\":\"compat_ambiguous_rename\""));
    assert_eq!(before, fs::read(&state_path).unwrap());
}

#[test]
fn policy_check_commit_json_after_init_returns_success_envelope() {
    let repo = TestRepo::new("policy-check-commit-success");

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    let output = sun()
        .arg("policy")
        .arg("check-commit")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-commit should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"policy.check-commit\""));
    assert!(stdout.contains("\"repository_id\":\"repo-"));
    assert!(stdout.contains("\"validation_report\":{\"ok\":true"));
    assert!(stdout.contains("\"managed_ignore_blocks_checked\":1"));
    assert!(stdout.contains("\"candidate_paths_checked\":0"));
    assert!(stdout.contains("\"blocked\":0"));
    assert!(stdout.contains("\"failures\":[]"));
}

#[test]
fn policy_check_commit_json_paths_rejects_blocked_local_path() {
    let repo = TestRepo::new("policy-check-commit-blocked-path");

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    let output = sun()
        .arg("policy")
        .arg("check-commit")
        .arg("--paths")
        .arg(".sunlight/local/lease.json")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-commit should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"commit_policy_failed\""));
    assert!(stdout.contains("\"validation_report\":{\"ok\":false"));
    assert!(stdout.contains("\"candidate_paths_checked\":1"));
    assert!(stdout.contains("\"blocked\":1"));
    assert!(stdout.contains("\"check\":\"policy_class\""));
    assert!(stdout.contains("\"code\":\"blocked_local_path\""));
    assert!(stdout.contains("\"path\":\".sunlight/local/lease.json\""));
}

#[test]
fn policy_check_commit_json_missing_gitignore_reports_managed_block_failure() {
    let repo = TestRepo::new("policy-check-commit-missing-gitignore");

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);
    fs::remove_file(repo.path().join(".sunlight/.gitignore")).unwrap();

    let output = sun()
        .arg("policy")
        .arg("check-commit")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-commit should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"commit_policy_failed\""));
    assert!(stdout.contains("\"check\":\"ignore_policy\""));
    assert!(stdout.contains("\"code\":\"managed_ignore_block_missing\""));
    assert!(stdout.contains("\"path\":\".gitignore\""));
}

#[test]
fn policy_check_commit_json_rejects_missing_paths_values() {
    let repo = TestRepo::new("policy-check-commit-missing-paths");

    let output = sun()
        .arg("policy")
        .arg("check-commit")
        .arg("--paths")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-commit should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"invalid_request\""));
    assert!(stdout.contains("\"missing\":\"paths\""));
}

#[test]
fn global_json_unknown_command_returns_failure_envelope() {
    let repo = TestRepo::new("unknown-json");

    let output = sun()
        .arg("--json")
        .arg("nope")
        .current_dir(repo.path())
        .output()
        .expect("sun unknown command should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"invalid_request\""));
    assert!(stdout.contains("\"message\":\"unknown command `nope`\""));
    assert!(stdout.contains("\"details\":{\"command\":\"nope\"}"));
}

#[test]
fn status_json_without_repository_returns_not_initialized() {
    let repo = TestRepo::new("status-not-initialized");

    let output = sun()
        .arg("status")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"not_initialized\""));
    assert!(stdout.contains("\"message\":\"Sunlight repository is not initialized\""));
    assert!(stdout.contains("\"details\":{}"));
}

#[test]
fn inspect_json_in_initialized_repository_returns_object_not_found() {
    let repo = TestRepo::new("inspect-object-not-found");

    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    let output = sun()
        .arg("inspect")
        .arg("topic:missing")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"message\":\"Sunlight object was not found\""));
    assert!(stdout.contains("\"details\":{}"));
}

#[test]
fn topic_create_json_fixture_basic_app_returns_lifecycle_envelope() {
    let repo = TestRepo::new("topic-create-fixture");

    let output = sun()
        .arg("topic")
        .arg("create")
        .arg("auth-nullability")
        .arg("--display-name")
        .arg("Auth Nullability")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun topic create should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"topic.create\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"ids\":{\"topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"slug\":\"auth-nullability\""));
    assert!(stdout.contains("\"display_name\":\"Auth Nullability\""));
    assert!(stdout.contains("\"status\":\"open\""));
    assert!(stdout.contains("\"lifecycle\":\"open\""));
    assert!(stdout.contains("\"head_revision_id\":null"));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn session_start_json_fixture_basic_app_returns_pinned_lifecycle_envelope() {
    let repo = TestRepo::new("session-start-fixture");

    let output = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_auth_nullability")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"session.start\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"session_id\":\"session_agent_a\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"write_topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"actor_id\":\"agent_a\""));
    assert!(stdout.contains("\"refresh_policy\":\"pinned_except_own_topic\""));
    assert!(stdout.contains(
        "\"capabilities\":[\"read\",\"list\",\"search\",\"inspect\",\"patch\",\"write\",\"move\",\"delete\",\"metadata\"]"
    ));
    assert!(stdout.contains(
        "\"topic_frontier\":[{\"topic_id\":\"topic_auth_nullability\",\"revision_id\":null,\"mode\":\"write\"}]"
    ));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn topic_create_json_unknown_fixture_returns_invalid_request() {
    let repo = TestRepo::new("topic-create-unknown-fixture");

    let output = sun()
        .arg("topic")
        .arg("create")
        .arg("auth-nullability")
        .arg("--display-name")
        .arg("Auth Nullability")
        .arg("--fixture")
        .arg("missing")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun topic create should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"invalid_request\""));
    assert!(stdout.contains("\"message\":\"unknown fixture `missing`\""));
    assert!(stdout.contains("\"details\":{\"fixture\":\"missing\"}"));
}

#[test]
fn session_start_json_missing_topic_returns_topic_not_found() {
    let repo = TestRepo::new("session-start-missing-topic");

    let output = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_missing")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"topic_not_found\""));
    assert!(stdout.contains("\"message\":\"topic `topic_missing` was not found\""));
    assert!(stdout.contains("\"details\":{\"topic\":\"topic_missing\"}"));
}

#[test]
fn session_start_json_missing_view_returns_object_not_found() {
    let repo = TestRepo::new("session-start-missing-view");

    let output = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg("topic_auth_nullability")
        .arg("--view")
        .arg("view_missing")
        .arg("--actor")
        .arg("agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"message\":\"Sunlight object was not found\""));
    assert!(stdout.contains("\"selector\":\"view_missing\""));
    assert!(stdout.contains("\"object_type\":\"view\""));
}

#[test]
fn read_json_fixture_basic_app_returns_artifact_and_content() {
    let repo = TestRepo::new("read-fixture");

    let output = sun()
        .arg("read")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"artifact.read\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"ids\":{\"session_id\":\"session_agent_a\"}"));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"content_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"byte_length\":78"));
    assert!(stdout.contains(
        "\"bytes\":\"export function login(email: string) {\\n  return email.trim().toLowerCase();\\n}\\n\""
    ));
}

#[test]
fn list_json_fixture_basic_app_orders_prefix_matches_by_path() {
    let repo = TestRepo::new("list-fixture");

    let output = sun()
        .arg("list")
        .arg("src")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun list should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.list\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"artifacts\":["));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_profile_ts\""));
    assert!(
        stdout.find("\"path\":\"src/auth.ts\"").unwrap()
            < stdout.find("\"path\":\"src/profile.ts\"").unwrap(),
        "expected src/auth.ts before src/profile.ts in stdout:\n{stdout}"
    );
    assert!(!stdout.contains("\"path\":\"README.md\""));
}

#[test]
fn search_json_fixture_basic_app_returns_match_shape() {
    let repo = TestRepo::new("search-fixture");

    let output = sun()
        .arg("search")
        .arg("User.email")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun search should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.search\""));
    assert!(stdout.contains("\"matches\":["));
    assert!(stdout.contains(
        "{\"artifact_id\":\"artifact_readme_md\",\"path\":\"README.md\",\"content_hash\":\"sha256:readme_base\",\"line\":3,\"snippet\":\"Uses User.email for login.\"}"
    ));
    assert!(stdout.contains(
        "{\"artifact_id\":\"artifact_docs_guide_md\",\"path\":\"docs/guide.md\",\"content_hash\":\"sha256:guide_base\",\"line\":1,\"snippet\":\"Search token: User.email\"}"
    ));
    assert!(stdout.contains(
        "{\"artifact_id\":\"artifact_src_profile_ts\",\"path\":\"src/profile.ts\",\"content_hash\":\"sha256:profile_base\",\"line\":1,\"snippet\":\"export const profileLabel = \\\"User.email\\\";\"}"
    ));
}

#[test]
fn read_json_fixture_basic_app_missing_path_returns_path_not_found() {
    let repo = TestRepo::new("read-missing-fixture");

    let output = sun()
        .arg("read")
        .arg("src/missing.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun read should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"path_not_found\""));
    assert!(stdout.contains("\"message\":\"path `src/missing.ts` was not found\""));
    assert!(stdout.contains("\"path\":\"src/missing.ts\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
}

#[test]
fn patch_json_fixture_basic_app_returns_mutation_success_envelope() {
    let repo = TestRepo::new("patch-fixture");
    let patch_file = repo.write_file("auth.patch", auth_trim_guard_patch());

    let output = sun()
        .arg("patch")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--patch-file")
        .arg(&patch_file)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun patch should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.patch\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_auth_trim_guard_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_patch_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout.contains("\"tree_hash\":\"tree_after_auth_patch_0001\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_trim_guard\""));
    assert!(stdout.contains("\"operation\":{"));
    assert!(stdout.contains("\"topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"mutation\":\"patch\""));
    assert!(stdout.contains("\"expected_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"patch\""));
    assert!(stdout.contains("\"topic_revision\":{"));
    assert!(stdout.contains("\"session_generation\":{"));
    assert!(stdout
        .contains("\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\"}"));
}

#[test]
fn write_json_fixture_basic_app_new_file_returns_mutation_success_envelope() {
    let repo = TestRepo::new("write-fixture");
    let content_file = repo.write_file(
        "session.ts",
        "export const sessionLabel = \"SessionStore\";\n",
    );

    let output = sun()
        .arg("write")
        .arg("src/session.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("new")
        .arg("--content-file")
        .arg(&content_file)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun write should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.write\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_write_session_ts_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_write_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_session_ts\""));
    assert!(stdout.contains("\"path\":\"src/session.ts\""));
    assert!(stdout.contains("\"before_hash\":null"));
    assert!(stdout.contains("\"after_hash\":\"sha256:session_new\""));
    assert!(stdout.contains("\"mutation\":\"write\""));
    assert!(stdout.contains("\"expected_hash\":\"new\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"write\",\"write_mode\":\"create\""));
    assert!(stdout
        .contains("\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\"}"));
}

#[test]
fn move_json_fixture_basic_app_returns_structural_mutation_envelope() {
    let repo = TestRepo::new("move-fixture");

    let output = sun()
        .arg("move")
        .arg("src/auth.ts")
        .arg("src/auth.renamed.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun move should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.move\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_auth_move_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_move_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.renamed.ts\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"mutation\":\"move\""));
    assert!(stdout.contains("\"expected_path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"expected_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"move\""));
    assert!(stdout.contains("\"source_path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"target_path\":\"src/auth.renamed.ts\""));
    assert!(stdout
        .contains("\"path_binding_removal\":{\"path\":\"src/auth.ts\",\"state\":\"tombstone\"}"));
    assert!(stdout.contains(
        "{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"tombstone\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn delete_json_fixture_basic_app_returns_tombstone_mutation_envelope() {
    let repo = TestRepo::new("delete-fixture");

    let output = sun()
        .arg("delete")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun delete should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.delete\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_auth_delete_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_delete_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"mutation\":\"delete\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"delete\""));
    assert!(stdout
        .contains("\"path_binding_removal\":{\"path\":\"src/auth.ts\",\"state\":\"tombstone\"}"));
    assert!(stdout.contains("\"tombstone\":true"));
    assert!(stdout.contains("\"path_state\":\"tombstone\""));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn metadata_set_json_fixture_basic_app_returns_metadata_mutation_envelope() {
    let repo = TestRepo::new("metadata-fixture");

    let output = sun()
        .arg("metadata")
        .arg("set")
        .arg("src/auth.ts")
        .arg("--classification")
        .arg("generated")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun metadata set should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"artifact.metadata_set\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_auth_metadata_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_metadata_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"classification\":\"generated\""));
    assert!(stdout.contains("\"mutation\":\"metadata_set\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"metadata_set\""));
    assert!(stdout.contains("\"classification_before\":\"source\""));
    assert!(stdout.contains("\"classification_after\":\"generated\""));
    assert!(stdout.contains("\"content_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn structural_mutation_provenance_round_trips_through_operation_inspect() {
    let repo = TestRepo::new("structural-mutation-inspect-roundtrip");

    let move_output = sun()
        .arg("move")
        .arg("src/auth.ts")
        .arg("src/auth.renamed.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun move should run");
    assert_success(&move_output);

    let move_inspect = inspect_fixture_operation(repo.path(), "op_auth_move_0001");
    assert!(move_inspect.contains("\"command\":\"inspect.operation\""));
    assert!(move_inspect.contains("\"operation_transaction_id\":\"op_auth_move_0001\""));
    assert!(move_inspect.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(move_inspect.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(move_inspect.contains("\"mutation\":\"move\""));
    assert!(move_inspect.contains("\"expected_path\":\"src/auth.ts\""));
    assert!(move_inspect.contains("\"expected_hash\":\"sha256:auth_base\""));
    assert!(move_inspect.contains("\"payload\":{\"kind\":\"move\""));
    assert!(move_inspect
        .contains("\"path_binding_removal\":{\"path\":\"src/auth.ts\",\"state\":\"tombstone\"}"));
    assert!(move_inspect.contains(
        "\"path_binding_addition\":{\"path\":\"src/auth.renamed.ts\",\"state\":\"active\"}"
    ));
    assert!(move_inspect.contains(
        "\"created_revision\":{\"topic_revision_id\":\"rev_auth_nullability_0001\",\"topic_id\":\"topic_auth_nullability\",\"revision_number\":1,\"parent_revision_id\":null,\"operation_transaction_id\":\"op_auth_move_0001\""
    ));
    assert!(move_inspect.contains(
        "\"session_generation\":{\"session_generation_id\":\"gen_agent_a_0002\",\"session_id\":\"session_agent_a\",\"write_topic_id\":\"topic_auth_nullability\""
    ));
    assert!(move_inspect.contains("\"created_by_operation_id\":\"op_auth_move_0001\""));

    let delete_output = sun()
        .arg("delete")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun delete should run");
    assert_success(&delete_output);

    let delete_inspect = inspect_fixture_operation(repo.path(), "op_auth_delete_0001");
    assert!(delete_inspect.contains("\"operation_transaction_id\":\"op_auth_delete_0001\""));
    assert!(delete_inspect.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(delete_inspect.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(delete_inspect.contains("\"mutation\":\"delete\""));
    assert!(delete_inspect.contains("\"payload\":{\"kind\":\"delete\""));
    assert!(delete_inspect
        .contains("\"path_binding_removal\":{\"path\":\"src/auth.ts\",\"state\":\"tombstone\"}"));
    assert!(delete_inspect.contains("\"tombstone\":true"));
    assert!(delete_inspect.contains("\"path_state\":\"tombstone\""));
    assert!(delete_inspect.contains("\"created_by_operation_id\":\"op_auth_delete_0001\""));

    let metadata_output = sun()
        .arg("metadata")
        .arg("set")
        .arg("src/auth.ts")
        .arg("--classification")
        .arg("generated")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun metadata set should run");
    assert_success(&metadata_output);

    let metadata_inspect = inspect_fixture_operation(repo.path(), "op_auth_metadata_0001");
    assert!(metadata_inspect.contains("\"operation_transaction_id\":\"op_auth_metadata_0001\""));
    assert!(metadata_inspect.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(metadata_inspect.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(metadata_inspect.contains("\"mutation\":\"metadata_set\""));
    assert!(metadata_inspect.contains("\"classification\":\"generated\""));
    assert!(metadata_inspect.contains("\"payload\":{\"kind\":\"metadata_set\""));
    assert!(metadata_inspect.contains("\"classification_before\":\"source\""));
    assert!(metadata_inspect.contains("\"classification_after\":\"generated\""));
    assert!(metadata_inspect.contains("\"created_by_operation_id\":\"op_auth_metadata_0001\""));
}

#[test]
fn patch_json_fixture_basic_app_stale_hash_returns_precondition_failure() {
    let repo = TestRepo::new("patch-stale-fixture");
    let patch_file = repo.write_file("auth.patch", auth_trim_guard_patch());

    let output = sun()
        .arg("patch")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:stale_auth")
        .arg("--patch-file")
        .arg(&patch_file)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun patch should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"precondition_failed\""));
    assert!(stdout.contains("\"failed_precondition\":\"expected_hash\""));
    assert!(stdout.contains("\"expected\":\"sha256:stale_auth\""));
    assert!(stdout.contains("\"actual\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(!stdout.contains("operation_transaction_id"));
    assert!(!stdout.contains("topic_revision_id"));
}

#[test]
fn move_json_fixture_basic_app_stale_hash_returns_precondition_failure() {
    let repo = TestRepo::new("move-stale-fixture");

    let output = sun()
        .arg("move")
        .arg("src/auth.ts")
        .arg("src/auth.renamed.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:stale_auth")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun move should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"precondition_failed\""));
    assert!(stdout.contains("\"failed_precondition\":\"expected_hash\""));
    assert!(stdout.contains("\"expected\":\"sha256:stale_auth\""));
    assert!(stdout.contains("\"actual\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(!stdout.contains("operation_transaction_id"));
    assert!(!stdout.contains("topic_revision_id"));
}

#[test]
fn patch_json_fixture_basic_app_bad_hunk_returns_patch_apply_failure() {
    let repo = TestRepo::new("patch-bad-hunk-fixture");
    let patch_file = repo.write_file("bad-auth.patch", bad_auth_patch());

    let output = sun()
        .arg("patch")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("sha256:auth_base")
        .arg("--patch-file")
        .arg(&patch_file)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun patch should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"patch_apply_failed\""));
    assert!(
        stdout.contains("\"message\":\"patch did not apply to expected content at `src/auth.ts`\"")
    );
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"content_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"failed_hunk\":\"1\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(!stdout.contains("operation_transaction_id"));
    assert!(!stdout.contains("topic_revision_id"));
}

#[test]
fn write_json_fixture_basic_app_existing_with_new_returns_precondition_failure() {
    let repo = TestRepo::new("write-existing-new-fixture");
    let content_file = repo.write_file("replacement.ts", "export const replacement = true;\n");

    let output = sun()
        .arg("write")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--expect-hash")
        .arg("new")
        .arg("--content-file")
        .arg(&content_file)
        .arg("--classification")
        .arg("source")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun write should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"precondition_failed\""));
    assert!(stdout.contains("\"failed_precondition\":\"expected_hash\""));
    assert!(stdout.contains("\"expected\":\"new\""));
    assert!(stdout.contains("\"actual\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(!stdout.contains("operation_transaction_id"));
    assert!(!stdout.contains("topic_revision_id"));
}

#[test]
fn view_resolve_json_reversed_frontier_order_produces_same_tree_identity() {
    let repo = TestRepo::new("view-resolve-reversed");

    let left = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg(
            "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
        )
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&left);

    let right = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg(
            "topic_profile_ui:rev_profile_ui_0001,topic_auth_nullability:rev_auth_nullability_0001",
        )
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");
    assert_success(&right);

    let left_stdout = stdout(&left);
    let right_stdout = stdout(&right);
    assert_eq!(tree_hash(&left_stdout), tree_hash(&right_stdout));
    assert!(left_stdout.contains("\"command\":\"view.resolve\""));
    assert!(left_stdout.contains("\"conflict_ids\":[]"));
    assert!(left_stdout.contains("\"staleness_ids\":[]"));
    assert!(left_stdout.contains("\"resolver_order\":{\"operation_ids\":[\"op_auth_trim_guard_0001\",\"op_profile_write_0001\"]}"));
    assert!(left_stdout.contains("\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\",\"topic_profile_ui\":\"rev_profile_ui_0001\"}"));
    assert_eq!(
        resolved_view_id(&left_stdout),
        resolved_view_id(&right_stdout)
    );
}

#[test]
fn view_resolve_json_independent_files_returns_conflict_free_tree() {
    let repo = TestRepo::new("view-resolve-independent");

    let output = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg(
            "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
        )
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"ids\":{\"resolved_view_id\":\"view_fixture_"));
    assert!(stdout.contains("\"base_checkpoint_ids\":[\"checkpoint_base_0001\"]"));
    assert!(stdout.contains("\"dependency_closure\":{\"revision_ids\":[\"rev_auth_nullability_0001\",\"rev_profile_ui_0001\"]}"));
    assert!(stdout.contains("\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_"));
    assert!(stdout.contains("\"conflicts\":[]"));
    assert!(stdout.contains("\"staleness\":[]"));
}

#[test]
fn view_resolve_json_overlapping_same_artifact_returns_conflict_summary() {
    let repo = TestRepo::new("view-resolve-conflict");

    let output = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg("topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"view.resolve\""));
    assert!(stdout.contains("\"view\":null"));
    assert!(stdout.contains("\"tree_identity\":null"));
    assert!(stdout.contains("\"conflict_ids\":[\"conflict_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"staleness_ids\":[]"));
    assert!(stdout.contains("\"record_type\":\"conflict\""));
    assert!(stdout.contains("\"kind\":\"same_artifact_conflict\""));
    assert!(stdout.contains("\"artifact_ids\":[\"artifact_src_auth_ts\"]"));
    assert!(stdout.contains(
        "\"operation_ids\":[\"op_auth_trim_guard_0001\",\"op_profile_auth_null_guard_0001\"]"
    ));
    assert!(stdout.contains("\"path_refs\":[{\"path\":\"src/auth.ts\",\"path_state\":\"active\"}]"));
    assert!(stdout.contains(
        "\"policy_reason\":\"same artifact operations are not proven commutative under file_ops_v1\""
    ));
}

#[test]
fn view_conflict_visibility_json_round_trips_conflict_id_through_status_and_inspect() {
    let repo = TestRepo::new("view-conflict-visibility");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001",
    );
    let conflict_id = "conflict_src_auth_ts_0001";

    let status = sun()
        .arg("status")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status --view should run");
    assert_success(&status);
    let status_stdout = stdout(&status);
    assert!(status_stdout.contains("\"command\":\"status.view\""));
    assert!(status_stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(status_stdout.contains("\"lifecycle_state\":\"conflicted\""));
    assert!(status_stdout.contains("\"base_checkpoint_ids\":[\"checkpoint_base_0001\"]"));
    assert!(status_stdout.contains("\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\",\"topic_profile_ui\":\"rev_profile_auth_overlap_0001\"}"));
    assert!(status_stdout.contains("\"dependency_closure\":{\"revision_ids\":[\"rev_auth_nullability_0001\",\"rev_profile_auth_overlap_0001\"]}"));
    assert!(status_stdout.contains("\"resolver_order\":{\"operation_ids\":[]}"));
    assert!(status_stdout.contains("\"conflict_count\":1"));
    assert!(status_stdout.contains(&format!("\"conflict_ids\":[\"{conflict_id}\"]")));
    assert!(status_stdout.contains("\"staleness_count\":0"));
    assert!(status_stdout.contains("\"tree_identity\":null"));
    assert!(status_stdout.contains("\"missing_tree_reason\":\"blocked_by_conflict\""));

    let inspect_view = sun()
        .arg("inspect")
        .arg(format!("view:{view_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect view should run");
    assert_success(&inspect_view);
    let inspect_view_stdout = stdout(&inspect_view);
    assert!(inspect_view_stdout.contains("\"command\":\"inspect.view\""));
    assert!(inspect_view_stdout.contains("\"record_type\":\"resolved_view\""));
    assert!(inspect_view_stdout.contains(&format!("\"conflict_ids\":[\"{conflict_id}\"]")));
    assert!(inspect_view_stdout.contains(&format!(
        "\"conflict_refs\":[{{\"id\":\"{conflict_id}\",\"kind\":\"same_artifact_conflict\"}}]"
    )));
    assert!(inspect_view_stdout.contains("\"tree_identity\":null"));

    let inspect_conflict = sun()
        .arg("inspect")
        .arg(format!("conflict:{conflict_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect conflict should run");
    assert_success(&inspect_conflict);
    let inspect_conflict_stdout = stdout(&inspect_conflict);
    assert!(inspect_conflict_stdout.contains("\"command\":\"inspect.conflict\""));
    assert!(inspect_conflict_stdout.contains(&format!("\"conflict_id\":\"{conflict_id}\"")));
    assert!(inspect_conflict_stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(inspect_conflict_stdout.contains("\"kind\":\"same_artifact_conflict\""));
    assert!(inspect_conflict_stdout.contains(
        "\"competing_operation_ids\":[\"op_auth_trim_guard_0001\",\"op_profile_auth_null_guard_0001\"]"
    ));
    assert!(inspect_conflict_stdout
        .contains("\"path_refs\":[{\"path\":\"src/auth.ts\",\"path_state\":\"active\"}]"));
    assert!(inspect_conflict_stdout.contains("\"artifact_ids\":[\"artifact_src_auth_ts\"]"));
    assert!(inspect_conflict_stdout.contains(
        "\"policy_reason\":\"same artifact operations are not proven commutative under file_ops_v1\""
    ));

    let inspect_operation = sun()
        .arg("inspect")
        .arg("operation:op_auth_trim_guard_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect operation should run");
    assert_success(&inspect_operation);
    let inspect_operation_stdout = stdout(&inspect_operation);
    assert!(inspect_operation_stdout.contains("\"command\":\"inspect.operation\""));
    assert!(inspect_operation_stdout
        .contains("\"operation_transaction_id\":\"op_auth_trim_guard_0001\""));
    assert!(inspect_operation_stdout.contains(&format!("\"conflict_ids\":[\"{conflict_id}\"]")));
    assert!(inspect_operation_stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
}

#[test]
fn view_conflict_visibility_json_missing_view_selector_returns_object_not_found() {
    let repo = TestRepo::new("view-conflict-missing");

    let output = sun()
        .arg("status")
        .arg("--view")
        .arg("view_missing_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status --view should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"view_missing_0001\""));
    assert!(stdout.contains("\"object_type\":\"resolved_view\""));
}

#[test]
fn view_resolve_json_missing_dependency_returns_staleness_summary() {
    let repo = TestRepo::new("view-resolve-staleness");

    let output = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg("topic_profile_ui:rev_profile_ui_0002")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun view resolve should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"view.resolve\""));
    assert!(stdout.contains("\"view\":null"));
    assert!(stdout.contains("\"tree_identity\":null"));
    assert!(stdout.contains("\"conflict_ids\":[]"));
    assert!(stdout
        .contains("\"staleness_ids\":[\"stale_missing_dependency_rev_auth_nullability_0001\"]"));
    assert!(stdout.contains("\"record_type\":\"staleness\""));
    assert!(stdout.contains("\"kind\":\"missing_dependency\""));
    assert!(stdout.contains("\"candidate_refs\":{\"dependent_revision_ids\":[\"rev_profile_ui_0002\"],\"required_revision_ids\":[\"rev_auth_nullability_0001\"]}"));
    assert!(stdout.contains("\"dependency_closure\":{\"revision_ids\":[\"rev_auth_nullability_0001\",\"rev_profile_ui_0002\"]}"));
}

#[test]
fn run_json_fixture_ready_view_returns_execution_projection_envelope() {
    let repo = TestRepo::new("run-fixture-ready");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"execution.run\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"execution_id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"result\":{\"status\":\"pass\",\"exit_code\":0,\"timed_out\":false}"));
    assert!(stdout.contains("\"output_summary_counts\":{\"total\":1,\"stdout_summary\":1,\"stderr_summary\":0,\"file_delta\":0,\"source_like_delta\":0}"));
    assert!(stdout.contains("\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_"));
    assert!(stdout.contains("\"promotion_candidates\":[{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"output_path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"classification\":\"source_like_delta\""));
}

#[test]
fn run_json_fixture_scan_missing_blob_integrity_rejects_before_execution_record() {
    let repo = TestRepo::new("run-fixture-integrity-scan-missing-blob");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("scan-missing-blob")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains(
        "\"message\":\"projection store integrity verification failed for fixture scan-missing-blob\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"execution_id\":null"));
    assert!(stdout.contains("\"integrity_fixture\":\"scan-missing-blob\""));
    assert!(stdout.contains("\"integrity_status\":\"failed\""));
    assert!(stdout.contains("\"quarantine_reason\":\"store_integrity_mismatch\""));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains(
        "\"quarantine_refs\":{\"projection\":\"projection:projection_exec_auth_profile_0001\""
    ));
    assert!(stdout.contains("\"cache\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains(
        "\"native_error\":\"native-error:execution_store_integrity_failed:projection_exec_auth_profile_0001\""
    ));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\""));
    assert!(stdout.contains("\"local_quarantine\":{\"privacy_class\":\"local_only\""));
    assert!(!stdout.contains("\"exec_auth_profile_tests_0001\""));
}

#[test]
fn run_json_fixture_store_mismatch_integrity_rejects_before_execution_record() {
    let repo = TestRepo::new("run-fixture-integrity-store-mismatch");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains(
        "\"message\":\"projection store integrity verification failed for fixture store-mismatch\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"execution_id\":null"));
    assert!(stdout.contains("\"integrity_fixture\":\"store-mismatch\""));
    assert!(stdout.contains("\"integrity_status\":\"failed\""));
    assert!(stdout.contains("\"quarantine_reason\":\"store_integrity_mismatch\""));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains(
        "\"quarantine_refs\":{\"projection\":\"projection:projection_exec_auth_profile_0001\""
    ));
    assert!(stdout.contains("\"cache\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\""));
    assert!(stdout.contains("\"local_quarantine\":{\"privacy_class\":\"local_only\""));
    assert!(!stdout.contains("\"exec_auth_profile_tests_0001\""));
}

#[test]
fn run_json_fixture_verified_integrity_still_returns_execution_projection_envelope() {
    let repo = TestRepo::new("run-fixture-integrity-verified");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("verified")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"execution.run\""));
    assert!(stdout.contains("\"execution_id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"result\":{\"status\":\"pass\",\"exit_code\":0,\"timed_out\":false}"));
    assert!(stdout.contains("\"promotion_candidates\":[{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
}

#[test]
fn run_json_fixture_failure_result_still_returns_execution_record() {
    let repo = TestRepo::new("run-fixture-fail");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .arg("--fixture-fail")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"execution_id\":\"exec_auth_profile_tests_fail_0001\""));
    assert!(
        stdout.contains("\"result\":{\"status\":\"fail\",\"exit_code\":101,\"timed_out\":false}")
    );
    assert!(stdout.contains("\"output_summary_counts\":{\"total\":2,\"stdout_summary\":1,\"stderr_summary\":1,\"file_delta\":0,\"source_like_delta\":0}"));
    assert!(stdout.contains("\"promotion_candidates\":[]"));
}

#[test]
fn execution_promote_output_json_fixture_returns_mutation_with_execution_provenance() {
    let repo = TestRepo::new("execution-promote-output");

    let output = sun()
        .arg("execution")
        .arg("promote-output")
        .arg("exec_auth_profile_tests_0001")
        .arg("--path")
        .arg("src/generated/auth.generated.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--classification")
        .arg("source_like_delta")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun execution promote-output should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"execution.promote_output\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"execution_id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_promote_generated_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_promotion_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_promotion_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_promotion_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_generated_auth_generated_ts\""));
    assert!(stdout.contains("\"path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"before_hash\":null"));
    assert!(stdout.contains("\"after_hash\":\"sha256:generated_auth_after\""));
    assert!(stdout.contains("\"mutation\":\"write\""));
    assert!(stdout.contains("\"expected_hash\":\"new\""));
    assert!(stdout.contains("\"authored_context_id\":\"execution:exec_auth_profile_tests_0001:src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"promotion_source\":{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\",\"output_path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"target_topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"classification\":\"source_like_delta\""));
    assert!(stdout.contains("\"execution_provenance\":{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\",\"output_path\":\"src/generated/auth.generated.ts\",\"classification\":\"source_like_delta\"}"));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_promotion_0001\"}"
    ));
}

#[test]
fn execution_promote_output_json_unknown_execution_returns_stable_failure() {
    let repo = TestRepo::new("execution-promote-output-unknown");

    let output = sun()
        .arg("execution")
        .arg("promote-output")
        .arg("exec_stale_0001")
        .arg("--path")
        .arg("src/generated/auth.generated.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--classification")
        .arg("source_like_delta")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun execution promote-output should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"promotion_precondition_failed\""));
    assert!(stdout.contains("\"execution_id\":\"exec_stale_0001\""));
    assert!(stdout.contains("\"path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"session_id\":\"session_agent_a\""));
    assert!(stdout.contains("\"classification\":\"source_like_delta\""));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(!stdout.contains("\"ok\":true"));
    assert!(!stdout.contains("\"command\":\"execution.promote_output\""));
}

#[test]
fn status_execution_json_fixture_reports_unpromoted_candidate_by_default() {
    let repo = TestRepo::new("status-execution-unpromoted");

    let output = sun()
        .arg("status")
        .arg("--execution")
        .arg("exec_auth_profile_tests_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status --execution should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.execution\""));
    assert!(stdout.contains("\"execution_id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"promotion_status\":\"promotion_required\""));
    assert!(stdout.contains("\"promotion_candidates\":[{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"output_path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"classification\":\"source_like_delta\""));
    assert!(stdout.contains("\"before_hash\":null"));
    assert!(stdout.contains("\"after_hash\":\"sha256:generated_auth_after\""));
    assert!(stdout.contains("\"promotions\":[]"));
    assert!(stdout.contains("\"promotion_record\":\"not_persisted\""));
    assert!(stdout.contains("\"durability\":\"fixture_only_not_persisted\""));
}

#[test]
fn status_execution_json_fixture_promoted_flag_exposes_promotion_record() {
    let repo = TestRepo::new("status-execution-promoted");

    let output = sun()
        .arg("status")
        .arg("--execution")
        .arg("exec_auth_profile_tests_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--promoted")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status --execution --promoted should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.execution\""));
    assert!(stdout.contains("\"promotion_status\":\"promoted\""));
    assert!(stdout.contains("\"promotion_candidates\":[]"));
    assert!(stdout.contains("\"promotions\":[{\"execution_id\":\"exec_auth_profile_tests_0001\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_promote_generated_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_promotion_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_promotion_0001\""));
    assert!(stdout.contains("\"authored_context_id\":\"execution:exec_auth_profile_tests_0001:src/generated/auth.generated.ts\""));
    assert!(stdout.contains(
        "\"provenance_refs\":[{\"kind\":\"execution\",\"id\":\"exec_auth_profile_tests_0001\""
    ));
    assert!(stdout
        .contains("\"kind\":\"operation_transaction\",\"id\":\"op_promote_generated_auth_0001\""));
    assert!(stdout.contains("\"promotion_record\":\"policy_gated\""));
}

#[test]
fn inspect_execution_json_fixture_promoted_flag_exposes_core_promotion_record() {
    let repo = TestRepo::new("inspect-execution-promoted");

    let output = sun()
        .arg("inspect")
        .arg("execution:exec_auth_profile_tests_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--promoted")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect execution should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.execution\""));
    assert!(stdout.contains("\"execution\":{\"schema_version\":1,\"record_type\":\"execution\",\"id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"promotion_status\":\"promoted\""));
    assert!(stdout.contains("\"output_path\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("\"target_topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"classification\":\"source_like_delta\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_promote_generated_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_promotion_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_promotion_0001\""));
    assert!(stdout.contains("\"privacy_semantics\":{\"execution_record\":\"policy_gated\",\"raw_outputs\":\"local_only\",\"promotion_record\":\"policy_gated\",\"durability\":\"fixture_only_not_persisted\"}"));
}

#[test]
fn run_json_fixture_conflicted_view_rejects_before_projection() {
    let repo = TestRepo::new("run-fixture-conflicted");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001",
    );

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"execution_conflicted_view\""));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"conflict_ids\":[\"conflict_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"staleness_ids\":[]"));
    assert!(stdout.contains("\"projection_id\":null"));
    assert!(stdout.contains("\"execution_id\":null"));
}

#[test]
fn run_json_fixture_stale_view_rejects_before_projection() {
    let repo = TestRepo::new("run-fixture-stale");
    let view_id = resolve_fixture_view_id(repo.path(), "topic_profile_ui:rev_profile_ui_0002");

    let output = sun()
        .arg("run")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .arg("--")
        .arg("cargo")
        .arg("test")
        .current_dir(repo.path())
        .output()
        .expect("sun run should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"execution_conflicted_view\""));
    assert!(stdout.contains("\"conflict_ids\":[]"));
    assert!(stdout
        .contains("\"staleness_ids\":[\"stale_missing_dependency_rev_auth_nullability_0001\"]"));
    assert!(stdout.contains("\"projection_id\":null"));
    assert!(stdout.contains("\"execution_id\":null"));
}

#[test]
fn project_materialize_json_fixture_ready_view_returns_projection_envelope() {
    let repo = TestRepo::new("projection-fixture-ready");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"projection.materialize\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"purpose\":\"execution\""));
    assert!(stdout.contains("\"selected_strategy\":\"copy\""));
    assert!(stdout.contains("\"strategy\":\"copy\""));
    assert!(stdout.contains("\"source\":\"resolved_content_tree\""));
    assert!(stdout.contains("\"local_materialization\":{\"privacy_class\":\"local_only\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"root_ref\":{\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"}"));
    assert!(stdout.contains("\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_"));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains(":execution:copy:read_only_source_private_outputs\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"policy\":{\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\",\"operation_semantics_version\":\"file_ops_v1\",\"writable_policy\":\"read_only_source_private_outputs\",\"store_integrity_policy\":\"verify_before_reuse\",\"privacy_class\":\"local_only\"}"));
    assert!(stdout.contains("\"record_type\":\"projection\""));
}

#[test]
fn project_materialize_json_projection_root_writes_basic_app_copy() {
    let repo = TestRepo::new("projection-fixture-copy-write");
    let projection_root = repo.path().join("projection-root");
    let view_id = "view_base_0001";

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"projection.materialize\""));
    assert!(stdout.contains("\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\""));
    assert!(stdout.contains("\"files_written\":5"));
    assert!(stdout.contains("\"bytes_written\":222"));
    assert!(stdout.contains("\"executable_files\":1"));
    assert!(stdout.contains("\"local_projection_manifest\":{"));
    assert!(stdout.contains("\"id\":\"projection_manifest_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"summary\":{\"directories\":3,\"files\":5,\"bytes\":222"));
    assert!(stdout.contains("\"content_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"cleanup\":{\"projection_root\":{"));
    assert!(stdout.contains("\"exists\":true,\"local_only\":true"));
    assert_eq!(
        fs::read_to_string(projection_root.join("README.md")).unwrap(),
        "# Fixture Basic App\n\nUses User.email for login.\n"
    );
    assert_eq!(
        fs::read_to_string(projection_root.join("src/auth.ts")).unwrap(),
        "export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n"
    );
    assert!(projection_root.join("docs/guide.md").is_file());
    assert!(projection_root.join("scripts/build.sh").is_file());
    let local_record_path = projection_root
        .join(".sunlight/projections/execution/projection_exec_auth_profile_0001")
        .join("projection-manifest-local.json");
    let local_record = fs::read_to_string(local_record_path).unwrap();
    assert!(local_record.contains("\"manifest\":{"));
    assert!(local_record.contains("\"root_binding\":{"));
    assert!(local_record.contains("\"normalization\":\"local_uri_relative_v1\""));
    assert!(local_record.contains(
        "\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\""
    ));
    assert!(!local_record.contains(&projection_root.display().to_string()));
}

#[test]
fn status_json_fixture_projection_reports_local_root_verification() {
    let repo = TestRepo::new("projection-status-root");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"lifecycle_state\":\"materialized\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"integrity_status\":\"not_checked\""));
    assert!(stdout.contains("\"local_projection_manifest\":{"));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"verified\""));
    assert!(stdout.contains("\"dirty_local\":false"));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"missing_files\":0"));
    assert!(stdout.contains("\"extra_files\":0"));
    assert!(stdout.contains("\"metadata_mismatches\":0"));
    assert!(stdout.contains("\"verification_errors\":[]"));
    assert!(stdout.contains("\"files\":5"));
    assert!(stdout.contains("\"bytes\":222"));
    #[cfg(unix)]
    assert!(stdout.contains("\"executable_files\":1"));
    #[cfg(not(unix))]
    assert!(stdout.contains("\"executable_files\":0"));
    assert!(stdout.contains("\"sample_paths\":[\"README.md\",\"docs/guide.md\""));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[test]
fn status_json_fixture_projection_store_mismatch_reports_quarantine() {
    let repo = TestRepo::new("projection-status-store-mismatch");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"lifecycle_state\":\"quarantined\""));
    assert!(stdout.contains("\"retention_state\":\"quarantined\""));
    assert!(stdout.contains("\"integrity_status\":\"failed\""));
    assert!(stdout.contains("\"reason\":\"store_integrity_mismatch\""));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains(
        "\"quarantine_refs\":{\"projection\":\"projection:projection_exec_auth_profile_0001\""
    ));
    assert!(stdout.contains("\"source_truth\":\"immutable_store_manifest\""));
    assert!(stdout.contains("\"local_filesystem_source_truth\":false"));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"native_errors\":[{\"code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"local_root_verification\":null"));
    assert!(!stdout.contains("\"content_verification\":\"verified\""));
}

#[test]
fn status_json_fixture_projection_store_mismatch_writes_quarantine_record_with_root() {
    let repo = TestRepo::new("projection-status-store-mismatch-quarantine-record");
    let projection_root = repo.path().join("projection-root");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));

    let record = fs::read_to_string(quarantine_record_path(&projection_root))
        .expect("quarantine record should be written");
    assert_projection_quarantine_record_json(&record);
}

#[test]
fn projection_quarantine_cleanup_json_removes_persisted_record() {
    let repo = TestRepo::new("projection-quarantine-cleanup-remove");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);
    assert!(quarantine_record_path(&projection_root).is_file());

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");

    assert_success(&cleanup);
    let stdout = stdout(&cleanup);
    assert!(stdout.contains("\"command\":\"projection.quarantine_cleanup\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"existed\":true"));
    assert!(stdout.contains("\"local_only\":true"));
    assert!(stdout.contains("\"retention_state_after\":\"removed\""));
    assert!(stdout.contains("\"removed_records\":[\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\"]"));
    assert!(!quarantine_record_path(&projection_root).exists());
    assert!(!projection_root
        .join(".sunlight/quarantine/projections/projection_exec_auth_profile_0001")
        .exists());
}

#[test]
fn projection_quarantine_cleanup_does_not_clear_later_mismatch_invalidation() {
    let repo = TestRepo::new("projection-quarantine-cleanup-revalidates-mismatch");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);
    assert!(quarantine_record_path(&projection_root).is_file());

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");
    assert_success(&cleanup);
    assert!(!quarantine_record_path(&projection_root).exists());

    let recheck = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&recheck);
    let stdout = stdout(&recheck);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"lifecycle_state\":\"quarantined\""));
    assert!(stdout.contains("\"retention_state\":\"quarantined\""));
    assert!(stdout.contains("\"integrity_status\":\"failed\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"native_errors\":[{\"code\":\"execution_store_integrity_failed\""));
    assert!(quarantine_record_path(&projection_root).is_file());
}

#[test]
fn projection_quarantine_cleanup_preserves_other_sunlight_content_and_projection_dirs() {
    let repo = TestRepo::new("projection-quarantine-cleanup-preserve");
    let projection_root = repo.path().join("projection-root");

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);

    write_nested_file(
        &projection_root,
        ".sunlight/other/local.txt",
        "not projection metadata\n",
    );
    let other_projection_record = projection_root
        .join(".sunlight/quarantine/projections/projection_inspect_auth_profile_0001")
        .join("execution_store_integrity_failed.json");
    fs::create_dir_all(other_projection_record.parent().unwrap()).unwrap();
    fs::write(&other_projection_record, "{}\n").unwrap();

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");

    assert_success(&cleanup);
    assert!(!quarantine_record_path(&projection_root).exists());
    assert!(projection_root.join(".sunlight/other/local.txt").is_file());
    assert!(other_projection_record.is_file());
}

#[test]
fn projection_quarantine_cleanup_reports_only_target_projection_quarantine() {
    let repo = TestRepo::new("projection-quarantine-cleanup-scoped-report");
    let projection_root = repo.path().join("projection-root");

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);

    let sibling_record = projection_root
        .join(".sunlight/quarantine/projections/projection_inspect_auth_profile_0001")
        .join("execution_store_integrity_failed.json");
    fs::create_dir_all(sibling_record.parent().unwrap()).unwrap();
    fs::write(
        &sibling_record,
        "{\"projection_id\":\"projection_inspect_auth_profile_0001\"}\n",
    )
    .unwrap();

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");

    assert_success(&cleanup);
    let stdout = stdout(&cleanup);
    assert!(stdout.contains("\"command\":\"projection.quarantine_cleanup\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"retention_state_after\":\"removed\""));
    assert!(stdout.contains(
        "local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json"
    ));
    assert!(!stdout.contains("projection_inspect_auth_profile_0001"));
    assert!(!quarantine_record_path(&projection_root).exists());
    assert!(sibling_record.is_file());
}

#[test]
fn projection_quarantine_cleanup_preserves_compat_projection_local_metadata() {
    let repo = TestRepo::new("projection-quarantine-cleanup-preserve-compat");
    let projection_root = repo.path().join("projection-root");

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);

    write_nested_file(
        &projection_root,
        ".sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json",
        "{}\n",
    );
    write_nested_file(
        &projection_root,
        ".sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_env_secret_0001",
        "{}\n",
    );
    write_nested_file(
        &projection_root,
        ".sunlight/quarantine/compat/projection_compat_agent_a_0001/env.json",
        "{}\n",
    );

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");

    assert_success(&cleanup);
    assert!(!quarantine_record_path(&projection_root).exists());
    assert!(projection_root
        .join(
            ".sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json"
        )
        .is_file());
    assert!(projection_root
        .join(
            ".sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_env_secret_0001"
        )
        .is_file());
    assert!(projection_root
        .join(".sunlight/quarantine/compat/projection_compat_agent_a_0001/env.json")
        .is_file());
}

#[test]
fn projection_quarantine_cleanup_json_is_idempotent_when_absent() {
    let repo = TestRepo::new("projection-quarantine-cleanup-absent");
    let projection_root = repo.path().join("projection-root");

    let cleanup = sun()
        .arg("projection")
        .arg("quarantine-cleanup")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun projection quarantine-cleanup should run");

    assert_success(&cleanup);
    let stdout = stdout(&cleanup);
    assert!(stdout.contains("\"command\":\"projection.quarantine_cleanup\""));
    assert!(stdout.contains("\"existed\":false"));
    assert!(stdout.contains("\"removed_records\":[]"));
    assert!(stdout.contains("\"removed_dirs\":[]"));
    assert!(stdout.contains("\"retention_state_after\":\"absent\""));
}

#[test]
fn status_json_fixture_projection_ignores_persisted_quarantine_record_in_root_scan() {
    let repo = TestRepo::new("projection-status-quarantine-record-root-scan");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let quarantine = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&quarantine);
    assert!(quarantine_record_path(&projection_root).is_file());

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"content_verification\":\"verified\""));
    assert!(stdout.contains("\"dirty_local\":false"));
    assert!(stdout.contains("\"extra_files\":0"));
    assert!(!stdout.contains(".sunlight/quarantine"));
}

#[test]
fn status_json_fixture_projection_reports_arbitrary_sunlight_other_as_extra_file() {
    let repo = TestRepo::new("projection-status-sunlight-other-extra");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    write_nested_file(
        &projection_root,
        ".sunlight/other/local.txt",
        "not projection metadata\n",
    );

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"content_verification\":\"dirty\""));
    assert!(stdout.contains("\"dirty_local\":true"));
    assert!(stdout.contains("\"extra_files\":1"));
    assert!(stdout.contains(".sunlight/other/local.txt"));
}

#[test]
fn status_json_fixture_projection_scan_missing_blob_reports_quarantine() {
    let repo = TestRepo::new("projection-status-scan-missing-blob");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("scan-missing-blob")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"lifecycle_state\":\"quarantined\""));
    assert!(stdout.contains("\"retention_state\":\"quarantined\""));
    assert!(stdout.contains("\"integrity_status\":\"failed\""));
    assert!(stdout.contains("\"reason\":\"store_integrity_mismatch\""));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains(
        "\"quarantine_refs\":{\"projection\":\"projection:projection_exec_auth_profile_0001\""
    ));
    assert!(stdout.contains("\"source_truth\":\"immutable_store_manifest\""));
    assert!(stdout.contains("\"local_filesystem_source_truth\":false"));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"native_errors\":[{\"code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains(
        "\"message\":\"projection store integrity verification failed for fixture scan-missing-blob\""
    ));
    assert!(stdout.contains("\"local_root_verification\":null"));
    assert!(!stdout.contains("\"content_verification\":\"verified\""));
}

#[test]
fn status_json_fixture_projection_store_verified_reports_manifest_integrity() {
    let repo = TestRepo::new("projection-status-store-verified");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("verified")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"lifecycle_state\":\"materialized\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\",\"integrity_status\":\"verified\""));
    assert!(stdout.contains("\"source_truth\":\"immutable_store_manifest\""));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"root_ref\":{\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"local_filesystem_source_truth\":false"));
    assert!(stdout.contains("\"quarantine\":null"));
    assert!(stdout.contains("\"native_errors\":[]"));
}

#[test]
fn status_json_fixture_projection_store_verified_rejects_non_execution_projection() {
    let repo = TestRepo::new("projection-status-store-verified-non-exec");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("store-verified")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"invalid_request\""));
    assert!(stdout.contains(
        "\"message\":\"store integrity fixture applies only to the basic-app execution projection\""
    ));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"integrity_fixture\":\"verified\""));
}

#[test]
fn status_json_fixture_projection_reports_dirty_content_from_manifest() {
    let repo = TestRepo::new("projection-status-root-dirty-content");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    fs::write(
        projection_root.join("src/auth.ts"),
        "export function login(email: string) {\n  return email;\n}\n",
    )
    .unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let status_stdout = stdout(&output);
    assert!(status_stdout.contains("\"command\":\"status.projection\""));
    assert!(status_stdout.contains("\"dirty_local\":true"));
    assert!(status_stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(status_stdout.contains("\"verification_state\":\"present\""));
    assert!(status_stdout.contains("\"content_verification\":\"dirty\""));
    assert!(status_stdout.contains("\"mismatched_files\":1"));
    assert!(status_stdout.contains("\"missing_files\":0"));
    assert!(status_stdout.contains("\"extra_files\":0"));
    assert!(status_stdout.contains("\"metadata_mismatches\":0"));
    assert!(status_stdout.contains("\"verification_errors\":[]"));
    assert!(status_stdout.contains("\"files\":5"));
    assert!(status_stdout.contains("\"sample_paths\":[\"README.md\",\"docs/guide.md\""));
    assert!(!status_stdout.contains("\"operation_transaction_id\""));
    assert!(!status_stdout.contains("\"checkpoint_id\""));

    let session_status = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status session should run");
    assert_success(&session_status);
    let session_stdout = stdout(&session_status);
    assert!(session_stdout.contains("\"command\":\"status.session\""));
    assert!(session_stdout
        .contains("\"ids\":{\"session_id\":\"session_agent_a\",\"write_topic_id\":\"topic_auth_nullability\"}"));
    assert!(session_stdout.contains("\"resolved_view_id\":\"view_agent_a_after_patch_0001\""));
    assert!(session_stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(session_stdout.contains("\"last_operation_id\":\"op_auth_trim_guard_0001\""));
    assert!(!session_stdout.contains("\"checkpoint_id\""));
    assert!(!session_stdout.contains("return email;"));
}

#[test]
fn status_json_fixture_projection_reports_root_mismatch_from_persisted_binding() {
    let repo = TestRepo::new("projection-status-root-mismatch");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    fs::write(
        projection_root.join("src/auth.ts"),
        "export function login(email: string) {\n  return email;\n}\n",
    )
    .unwrap();
    let local_record_path = projection_root
        .join(".sunlight/projections/execution/projection_exec_auth_profile_0001")
        .join("projection-manifest-local.json");
    let local_record = fs::read_to_string(&local_record_path).unwrap();
    let root_binding_start = local_record
        .find("\"root_binding\":")
        .expect("local manifest record should include root binding");
    let rebound_record = format!(
        "{}{}",
        &local_record[..root_binding_start],
        local_record[root_binding_start..].replace(
            "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
            "local://.sunlight/projections/compatibility/projection_compat_agent_a_0001",
        )
    );
    fs::write(&local_record_path, rebound_record).unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"root_mismatch\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"missing_files\":0"));
    assert!(stdout.contains("\"extra_files\":0"));
    assert!(stdout.contains("\"metadata_mismatches\":0"));
    assert!(stdout.contains("\"verification_errors\":[]"));
}

#[test]
fn status_json_fixture_projection_does_not_synthesize_root_mismatch_without_persisted_binding() {
    let repo = TestRepo::new("projection-status-root-no-synthetic-mismatch");
    let projection_root = repo.path().join("projection-root");
    let other_root = repo.path().join("other-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);
    fs::create_dir_all(&other_root).unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&other_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"dirty\""));
    assert!(stdout.contains("\"dirty_local\":true"));
    assert!(stdout.contains("\"missing_files\":5"));
    assert!(!stdout.contains("\"content_verification\":\"root_mismatch\""));
}

#[test]
fn status_json_fixture_projection_reports_invalid_persisted_manifest_not_root_mismatch() {
    let repo = TestRepo::new("projection-status-root-invalid-manifest");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let local_record_path = projection_root
        .join(".sunlight/projections/execution/projection_exec_auth_profile_0001")
        .join("projection-manifest-local.json");
    fs::write(&local_record_path, "{\"manifest\":null").unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"manifest_invalid\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_manifest_local_invalid\"]"));
    assert!(!stdout.contains("\"content_verification\":\"root_mismatch\""));
}

#[test]
fn status_json_fixture_projection_reports_stale_persisted_manifest_projection_id_invalid() {
    let repo = TestRepo::new("projection-status-root-stale-projection-id");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    fs::write(
        projection_root.join("src/auth.ts"),
        "export function login(email: string) {\n  return email;\n}\n",
    )
    .unwrap();
    let local_record_path = projection_root
        .join(".sunlight/projections/execution/projection_exec_auth_profile_0001")
        .join("projection-manifest-local.json");
    let local_record = fs::read_to_string(&local_record_path).unwrap();
    fs::write(
        &local_record_path,
        local_record.replace(
            "\"projection_id\":\"projection_exec_auth_profile_0001\"",
            "\"projection_id\":\"projection_compat_agent_a_0001\"",
        ),
    )
    .unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"manifest_invalid\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_manifest_local_invalid\"]"));
    assert!(!stdout.contains("\"content_verification\":\"dirty\""));
    assert!(!stdout.contains("\"content_verification\":\"root_mismatch\""));
}

#[cfg(unix)]
#[test]
fn status_json_fixture_projection_reports_dirty_executable_bit_from_manifest() {
    use std::os::unix::fs::PermissionsExt;

    let repo = TestRepo::new("projection-status-root-dirty-executable");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let build_script = projection_root.join("scripts/build.sh");
    let mut permissions = fs::metadata(&build_script).unwrap().permissions();
    permissions.set_mode(permissions.mode() & !0o111);
    fs::set_permissions(&build_script, permissions).unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"dirty\""));
    assert!(stdout.contains("\"dirty_local\":true"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"missing_files\":0"));
    assert!(stdout.contains("\"extra_files\":0"));
    assert!(stdout.contains("\"metadata_mismatches\":1"));
    assert!(stdout.contains("\"verification_errors\":[]"));
}

#[test]
fn status_json_fixture_projection_reports_missing_and_extra_local_files_from_manifest() {
    let repo = TestRepo::new("projection-status-root-extra-missing");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    fs::remove_file(projection_root.join("docs/guide.md")).unwrap();
    write_nested_file(&projection_root, "local-only.txt", "not in manifest\n");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"dirty_local\":true"));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"dirty\""));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"missing_files\":1"));
    assert!(stdout.contains("\"extra_files\":1"));
    assert!(stdout.contains("\"metadata_mismatches\":0"));
    assert!(stdout.contains("\"verification_errors\":[]"));
    assert!(stdout.contains("\"files\":5"));
    assert!(stdout.contains("\"sample_paths\":[\"README.md\",\"local-only.txt\""));
    assert!(stdout.contains("\"scan_error\":null"));
    assert!(!stdout.contains("\"content_verification\":\"verified\""));
}

#[test]
fn inspect_json_fixture_projection_verifies_unchanged_materialized_root() {
    let repo = TestRepo::new("projection-inspect-root");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"local_projection_manifest\":{"));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"content_verification\":\"verified\""));
    assert!(stdout.contains("\"dirty_local\":false"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"missing_files\":0"));
    assert!(stdout.contains("\"extra_files\":0"));
    assert!(stdout.contains("\"metadata_mismatches\":0"));
    assert!(stdout.contains("\"verification_errors\":[]"));
}

#[test]
fn inspect_json_fixture_projection_reports_stale_persisted_manifest_digest_invalid() {
    let repo = TestRepo::new("projection-inspect-root-stale-digest");
    let projection_root = repo.path().join("projection-root");

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    fs::write(
        projection_root.join("src/auth.ts"),
        "export function login(email: string) {\n  return email;\n}\n",
    )
    .unwrap();
    let local_record_path = projection_root
        .join(".sunlight/projections/execution/projection_exec_auth_profile_0001")
        .join("projection-manifest-local.json");
    let local_record = fs::read_to_string(&local_record_path).unwrap();
    let digest_start = local_record
        .find("\"manifest_digest\":\"sha256:")
        .expect("local manifest record should include manifest digest");
    let digest_value_start = digest_start + "\"manifest_digest\":\"".len();
    let digest_value_end = digest_value_start + "sha256:".len() + 64;
    let stale_record = format!(
        "{}sha256:{}{}",
        &local_record[..digest_value_start],
        "0".repeat(64),
        &local_record[digest_value_end..]
    );
    fs::write(&local_record_path, stale_record).unwrap();

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"manifest_invalid\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"mismatched_files\":0"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_manifest_local_invalid\"]"));
    assert!(!stdout.contains("\"content_verification\":\"dirty\""));
    assert!(!stdout.contains("\"content_verification\":\"root_mismatch\""));
}

#[test]
fn status_json_fixture_projection_reports_sorted_nested_local_root_sample() {
    let repo = TestRepo::new("projection-status-root-nested-sample");
    let projection_root = repo.path().join("projection-root");
    write_nested_file(&projection_root, "z-last.txt", "x\n");
    write_nested_file(&projection_root, "alpha/deep/02.txt", "x\n");
    write_nested_file(&projection_root, "00-root.txt", "x\n");
    write_nested_file(&projection_root, "gamma/nested/file.txt", "x\n");
    write_nested_file(&projection_root, "theta.txt", "x\n");
    write_nested_file(&projection_root, "alpha/00-first.txt", "x\n");
    write_nested_file(&projection_root, "zz-extra.txt", "x\n");
    write_nested_file(&projection_root, "beta.txt", "x\n");
    write_nested_file(&projection_root, "omega.txt", "x\n");
    write_nested_file(&projection_root, "alpha/deep/01.txt", "x\n");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"lifecycle_state\":\"materialized\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"content_verification\":\"dirty\""));
    assert!(stdout.contains("\"dirty_local\":true"));
    assert!(stdout.contains("\"directories\":5"));
    assert!(stdout.contains("\"files\":10"));
    assert!(stdout.contains("\"bytes\":20"));
    assert!(stdout.contains(concat!(
        "\"sample_paths\":[",
        "\"00-root.txt\",",
        "\"alpha/00-first.txt\",",
        "\"alpha/deep/01.txt\",",
        "\"alpha/deep/02.txt\",",
        "\"beta.txt\",",
        "\"gamma/nested/file.txt\",",
        "\"omega.txt\",",
        "\"theta.txt\"",
        "]"
    )));
    assert!(!stdout.contains("z-last.txt"));
    assert!(!stdout.contains("zz-extra.txt"));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[test]
fn status_json_fixture_projection_reports_missing_local_root() {
    let repo = TestRepo::new("projection-status-root-missing");
    let projection_root = repo.path().join("missing-projection-root");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"lifecycle_state\":\"removed\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"missing\""));
    assert!(stdout.contains("\"content_verification\":\"verification_error\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_root_missing\"]"));
    assert!(stdout.contains("\"exists\":false"));
    assert!(stdout.contains("\"is_dir\":false"));
    assert!(stdout.contains("\"files\":0"));
    assert!(stdout.contains("\"bytes\":0"));
    assert!(stdout.contains("\"sample_paths\":[]"));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[test]
fn status_json_fixture_projection_reports_file_local_root() {
    let repo = TestRepo::new("projection-status-root-file");
    let projection_root = repo.write_file("projection-root-file", "not a directory\n");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"lifecycle_state\":\"materialized\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"not_directory\""));
    assert!(stdout.contains("\"content_verification\":\"verification_error\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_root_not_directory\"]"));
    assert!(stdout.contains("\"exists\":true"));
    assert!(stdout.contains("\"is_dir\":false"));
    assert!(stdout.contains("\"directories\":0"));
    assert!(stdout.contains("\"files\":0"));
    assert!(stdout.contains("\"bytes\":0"));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[cfg(unix)]
#[test]
fn status_json_fixture_projection_does_not_follow_symlink_local_root() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::new("projection-status-root-symlink");
    let target_root = repo.path().join("target-root");
    write_nested_file(&target_root, "target-only.txt", "do not scan\n");
    let projection_root = repo.path().join("projection-root-link");
    symlink(&target_root, &projection_root).unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"not_directory\""));
    assert!(stdout.contains("\"content_verification\":\"verification_error\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_root_not_directory\"]"));
    assert!(stdout.contains("\"exists\":true"));
    assert!(stdout.contains("\"is_dir\":false"));
    assert!(stdout.contains("\"directories\":0"));
    assert!(stdout.contains("\"files\":0"));
    assert!(stdout.contains("\"bytes\":0"));
    assert!(stdout.contains("\"sample_paths\":[]"));
    assert!(stdout.contains("\"scan_error\":null"));
    assert!(!stdout.contains("target-only.txt"));
}

#[cfg(unix)]
#[test]
fn status_json_fixture_projection_scan_skips_symlink_entries_inside_local_root() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::new("projection-status-root-nested-symlink");
    let projection_root = repo.path().join("projection-root");
    write_nested_file(&projection_root, "actual.txt", "ok\n");
    let outside_root = repo.path().join("outside-root");
    write_nested_file(&outside_root, "outside.txt", "do not scan\n");
    symlink(&outside_root, projection_root.join("linked-dir")).unwrap();
    symlink(
        outside_root.join("outside.txt"),
        projection_root.join("linked-file.txt"),
    )
    .unwrap();

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"directories\":1"));
    assert!(stdout.contains("\"files\":1"));
    assert!(stdout.contains("\"bytes\":3"));
    assert!(stdout.contains("\"sample_paths\":[\"actual.txt\"]"));
    assert!(stdout.contains("\"scan_error\":null"));
    assert!(!stdout.contains("linked-dir"));
    assert!(!stdout.contains("linked-file.txt"));
    assert!(!stdout.contains("outside.txt"));
}

#[test]
fn project_materialize_json_projection_root_requires_empty_directory() {
    let repo = TestRepo::new("projection-fixture-copy-nonempty");
    let projection_root = repo.path().join("projection-root");
    fs::create_dir_all(&projection_root).unwrap();
    fs::write(projection_root.join("sentinel.txt"), "keep\n").unwrap();
    let view_id = "view_base_0001";

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_materialization_projection_root_unavailable\""));
    assert!(stdout.contains(
        "\"message\":\"projection root must be an empty directory or a creatable path\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert_eq!(
        fs::read_to_string(projection_root.join("sentinel.txt")).unwrap(),
        "keep\n"
    );
    assert!(!projection_root.join("README.md").exists());
}

#[test]
fn project_materialize_json_projection_root_conflicted_view_rejects_before_writing() {
    let repo = TestRepo::new("projection-fixture-copy-conflicted");
    let projection_root = repo.path().join("projection-root");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_conflicted_view\""));
    assert!(stdout.contains("\"conflict_ids\":[\"conflict_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"projection_id\":null"));
    assert!(!projection_root.exists());
}

#[test]
fn project_materialize_json_projection_root_stale_view_rejects_before_writing() {
    let repo = TestRepo::new("projection-fixture-copy-stale");
    let projection_root = repo.path().join("projection-root");
    let view_id = resolve_fixture_view_id(repo.path(), "topic_profile_ui:rev_profile_ui_0002");

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_stale_view\""));
    assert!(stdout
        .contains("\"staleness_ids\":[\"stale_missing_dependency_rev_auth_nullability_0001\"]"));
    assert!(stdout.contains("\"projection_id\":null"));
    assert!(!projection_root.exists());
}

#[test]
fn project_materialize_json_fixture_reflink_strategy_succeeds() {
    let repo = TestRepo::new("projection-fixture-reflink");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--strategy")
        .arg("reflink")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"projection.materialize\""));
    assert!(stdout.contains("\"selected_strategy\":\"reflink\""));
    assert!(stdout.contains("\"strategy\":\"reflink\""));
    assert!(stdout.contains(":execution:reflink:read_only_source_private_outputs\""));
    assert!(stdout.contains("\"local_materialization\":{\"privacy_class\":\"local_only\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"source\":\"resolved_content_tree\""));
}

#[test]
fn project_materialize_json_fixture_copy_fallback_for_ineligible_strategy() {
    let repo = TestRepo::new("projection-fixture-copy-fallback");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--strategy")
        .arg("hardlink_readonly")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"selected_strategy\":\"copy\""));
    assert!(stdout.contains("\"strategy\":\"copy\""));
    assert!(stdout.contains(":execution:copy:read_only_source_private_outputs\""));
    assert!(stdout.contains("\"local_materialization\":{\"privacy_class\":\"local_only\",\"projection_id\":\"projection_exec_auth_profile_0001\""));
}

#[test]
fn project_materialize_json_fixture_required_unsupported_strategy_fails() {
    let repo = TestRepo::new("projection-fixture-required-unsupported");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--strategy")
        .arg("hardlink_readonly")
        .arg("--no-copy-fallback")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains(
        "\"code\":\"projection_materialization_hardlink_readonly_requires_read_only_policy\""
    ));
    assert!(stdout.contains(
        "\"message\":\"read-only hardlink materialization requires a read-only projection policy\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"strategy\":\"hardlink_readonly\""));
    assert!(stdout.contains("\"projection_id\":null"));
}

#[test]
fn project_materialize_json_fixture_required_overlay_copyup_fails_when_unavailable() {
    let repo = TestRepo::new("projection-fixture-required-overlay-unavailable");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--strategy")
        .arg("overlay_copyup")
        .arg("--no-copy-fallback")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_materialization_overlay_copyup_unsupported\""));
    assert!(stdout.contains(
        "\"message\":\"overlay copy-up materialization is unsupported for this fixture\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"strategy\":\"overlay_copyup\""));
    assert!(stdout.contains("\"projection_id\":null"));
}

#[test]
fn project_materialize_json_fixture_required_hardlink_inspection_fails_without_store_proof() {
    let repo = TestRepo::new("projection-fixture-required-hardlink-unprotected");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("inspection")
        .arg("--strategy")
        .arg("hardlink_readonly")
        .arg("--no-copy-fallback")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(
        stdout.contains("\"code\":\"projection_materialization_hardlink_readonly_unsupported\"")
    );
    assert!(stdout.contains(
        "\"message\":\"read-only hardlink materialization is unsupported for this fixture\""
    ));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"strategy\":\"hardlink_readonly\""));
    assert!(stdout.contains("\"projection_id\":null"));
}

#[test]
fn project_materialize_json_fixture_compatibility_records_import_policy() {
    let repo = TestRepo::new("projection-fixture-compat");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("compatibility")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"purpose\":\"compatibility\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains(
        "\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/"
    ));
    assert!(stdout.contains("\"writable_policy\":\"writable_with_explicit_import\""));
    assert!(stdout.contains("\"store_integrity_policy\":\"verify_on_import\""));
}

#[test]
fn compat_project_json_fixture_basic_app_returns_projection_surface() {
    let repo = TestRepo::new("compat-project-fixture");

    let output = sun()
        .arg("compat")
        .arg("project")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat project should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.project\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"session_id\":\"session_agent_a\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"purpose\":\"compatibility\""));
    assert!(stdout.contains(
        "\"root_ref\":{\"value\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"}"
    ));
    assert!(stdout.contains("\"strategy\":\"copy\""));
    assert!(stdout.contains(
        "\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/view_base_0001\""
    ));
    assert!(stdout.contains("\"baseline_manifest_digest\":\"sha256:compat_baseline\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"privacy_class\":\"local_only\""));
    assert!(stdout
        .contains("\"path_policy\":{\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\""));
}

#[test]
fn compat_project_json_fixture_missing_session_returns_invalid_request() {
    let repo = TestRepo::new("compat-project-missing-session");

    let output = sun()
        .arg("compat")
        .arg("project")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat project should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"invalid_request\""));
    assert!(stdout.contains(
        "\"message\":\"usage: sun compat project --session <session-id> --fixture basic-app\""
    ));
}

#[test]
fn compat_diff_json_fixture_basic_app_returns_candidate_surface() {
    let repo = TestRepo::new("compat-diff-fixture");

    let output = sun()
        .arg("compat")
        .arg("diff")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat diff should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.diff\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(stdout.contains(
        "\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_base_0001\"}"
    ));
    assert!(stdout.contains("\"candidate_counts\":{\"total\":13"));
    assert!(stdout.contains(
        "\"by_classification\":{\"cache\":1,\"generated\":1,\"ignored\":1,\"policy\":1,\"secret\":1,\"source\":8}"
    ));
    assert!(stdout.contains(
        "\"by_kind\":{\"cache_or_build_output\":1,\"conflicted_delta\":1,\"created_source\":1,\"deleted_source\":1,\"generated_source\":1,\"ignored_path\":1,\"metadata_changed\":1,\"modified_source\":1,\"moved_or_renamed\":3,\"path_policy_blocked\":1,\"secret_like\":1}"
    ));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"selected_safe_default_candidate\":{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains(
        "\"quarantine_refs\":[\"quarantine://compat/projection_compat_agent_a_0001/env\"]"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_dist_bundle_0001\""));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_ignored_editor_swap_0001\""));
    assert!(stdout.contains("\"kind\":\"ignored_path\""));
    assert!(stdout.contains("\"path\":\"tmp/auth.ts.swp\""));
    assert!(stdout.contains("\"classification\":\"ignored\""));
    assert!(stdout.contains("\"privacy_class\":\"local_only\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"tmp/auth.ts.swp\",\"reason\":\"ignored_path\"}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_env_secret_0001\""));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_conflict_0001\""));
    assert!(stdout.contains("\"kind\":\"conflicted_delta\""));
    assert!(stdout.contains("\"path\":\"src/auth.conflicted.ts\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth.conflicted.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_delete_0001\""));
    assert!(stdout.contains("\"kind\":\"deleted_source\""));
    assert!(stdout.contains("\"operation_kind\":\"delete\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":null"));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_metadata_0001\""));
    assert!(stdout.contains("\"kind\":\"metadata_changed\""));
    assert!(stdout.contains("\"operation_kind\":\"metadata\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"source_path\":null"));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"executable\":false"));
    assert!(stdout.contains("\"media_type\":\"text/typescript; charset=utf-8\""));
    assert!(stdout.contains("\"classification\":\"source\""));
    assert!(stdout.contains("\"privacy_class\":\"policy_gated\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_auth_rename_ambiguous_0001\""));
    assert!(stdout.contains("\"kind\":\"moved_or_renamed\""));
    assert!(stdout.contains("\"operation_kind\":\"move\""));
    assert!(stdout.contains("\"path\":\"src/auth-renamed.ts\""));
    assert!(stdout.contains("\"source_path\":null"));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth-renamed.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_rename_0001\""));
    assert!(stdout.contains("\"path\":\"src/auth.renamed.ts\""));
    assert!(stdout.contains("\"source_path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth.renamed.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_rename_edit_0001\""));
    assert!(stdout.contains("\"path\":\"src/auth.renamed-edited.ts\""));
    assert!(stdout.contains("\"source_path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_rename_edit_projection_after\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/auth.renamed-edited.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_generated_schema_0001\""));
    assert!(stdout.contains("\"kind\":\"generated_source\""));
    assert!(stdout.contains("\"path\":\"src/generated/schema.ts\""));
    assert!(stdout.contains("\"classification\":\"generated\""));
    assert!(stdout.contains("\"privacy_class\":\"policy_gated\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":true,\"normalized_path\":\"src/generated/schema.ts\",\"reason\":null}"
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_reserved_sunlight_0001\""));
    assert!(stdout.contains("\"kind\":\"path_policy_blocked\""));
    assert!(stdout.contains("\"path\":\".sunlight/config.toml\""));
    assert!(stdout.contains(
        "\"path_policy_result\":{\"allowed\":false,\"normalized_path\":null,\"reason\":\"reserved_path\"}"
    ));
    assert!(stdout.contains("\"native_operation_ids\":[]"));
    assert!(stdout.contains("\"native_revision_ids\":[]"));
    assert!(!stdout.contains("op_compat_import_auth_0001"));
    assert!(!stdout.contains("rev_auth_nullability_compat_0001"));
}

#[test]
fn compat_working_tree_unrelated_files_do_not_appear_in_diff_fixture() {
    let repo = TestRepo::new("compat-working-tree-diff-fixture");
    init_local_git_repo(&repo);
    write_nested_file(
        repo.path(),
        "src/untracked-main-worktree.ts",
        "export const mainWorktreeOnly = true;\n",
    );
    write_nested_file(
        repo.path(),
        ".sunlight/local/noise.json",
        "{\"main_worktree_only\":true}\n",
    );

    let output = sun()
        .arg("compat")
        .arg("diff")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat diff should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.diff\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_counts\":{\"total\":13"));
    assert!(stdout.contains(
        "\"by_classification\":{\"cache\":1,\"generated\":1,\"ignored\":1,\"policy\":1,\"secret\":1,\"source\":8}"
    ));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""));
    assert!(!stdout.contains("src/untracked-main-worktree.ts"));
    assert!(!stdout.contains(".sunlight/local/noise.json"));
}

#[test]
fn compat_diff_json_fixture_invalid_projection_returns_not_found() {
    let repo = TestRepo::new("compat-diff-invalid-projection");

    let output = sun()
        .arg("compat")
        .arg("diff")
        .arg("--projection")
        .arg("projection_missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat diff should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"projection_missing\""));
    assert!(stdout.contains("\"object_type\":\"projection\""));
}

#[test]
fn compat_import_json_fixture_candidate_returns_operation_plan() {
    let repo = TestRepo::new("compat-import-fixture");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"tree_hash\":\"tree_after_compat_import_0001\""));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"operation_kind\":\"patch\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_projection_after\""));
    assert!(stdout.contains("\"mutation\":\"compat_import\""));
    assert!(stdout.contains("\"projection_purpose\":\"compatibility\""));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"baseline_manifest_digest\":\"sha256:compat_baseline\""));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_compat_0001\"}"
    ));
}

#[test]
fn compat_working_tree_unrelated_files_do_not_appear_in_import_fixture() {
    let repo = TestRepo::new("compat-working-tree-import-fixture");
    init_local_git_repo(&repo);
    write_nested_file(
        repo.path(),
        "src/untracked-main-worktree.ts",
        "export const mainWorktreeOnly = true;\n",
    );
    write_nested_file(
        repo.path(),
        ".sunlight/local/noise.json",
        "{\"main_worktree_only\":true}\n",
    );

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"tree_hash\":\"tree_after_compat_import_0001\""));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(!stdout.contains("src/untracked-main-worktree.ts"));
    assert!(!stdout.contains(".sunlight/local/noise.json"));
}

#[test]
fn compat_import_rename_preserves_artifact_id_json_fixture_returns_operation_plan() {
    let repo = TestRepo::new("compat-import-rename-preserves-artifact-id");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_rename_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import rename should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(
        stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_rename_0001\"]")
    );
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_rename_0001\",\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed.ts\",\"operation_kind\":\"move\",\"before_hash\":\"sha256:auth_base\",\"after_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_rename_0001\",\"operation_kind\":\"move\",\"path\":\"src/auth.renamed.ts\",\"patch_digest\":null,\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"write_set\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed.ts\",\"mutation\":\"write\"}]"
    ));
    assert!(stdout.contains(
        "\"before_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"after_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
}

#[test]
fn compat_import_rename_plus_edit_records_both_json_fixture_returns_operation_plan() {
    let repo = TestRepo::new("compat-import-rename-plus-edit-records-both");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_rename_edit_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import rename plus edit should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout
        .contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_rename_edit_0001\"]"));
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_rename_edit_0001\",\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed-edited.ts\",\"operation_kind\":\"move\",\"before_hash\":\"sha256:auth_base\",\"after_hash\":\"sha256:auth_rename_edit_projection_after\""
    ));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_rename_edit_0001\",\"operation_kind\":\"move\",\"path\":\"src/auth.renamed-edited.ts\",\"patch_digest\":null,\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":\"sha256:auth_rename_edit_projection_after\""
    ));
    assert!(stdout.contains(
        "\"operations\":[{\"operation_kind\":\"move\",\"source_path\":\"src/auth.ts\",\"target_path\":\"src/auth.renamed-edited.ts\",\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":\"sha256:auth_base\",\"patch_digest\":null},{\"operation_kind\":\"patch\",\"source_path\":\"src/auth.renamed-edited.ts\",\"target_path\":\"src/auth.renamed-edited.ts\",\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":\"sha256:auth_rename_edit_projection_after\",\"patch_digest\":\"sha256:compat_delta_src_auth_rename_edit_0001_patch\"}]"
    ));
    assert!(stdout.contains(
        "\"write_set\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed-edited.ts\",\"mutation\":\"write\"}]"
    ));
    assert!(stdout.contains(
        "\"before_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"after_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.renamed-edited.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_rename_edit_projection_after\""
    ));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
}

#[test]
fn compat_import_metadata_preserves_artifact_id_json_fixture_returns_operation_plan() {
    let repo = TestRepo::new("compat-import-metadata-preserves-artifact-id");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_metadata_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import metadata should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout
        .contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_metadata_0001\"]"));
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_metadata_0001\",\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"operation_kind\":\"metadata\",\"before_hash\":\"sha256:auth_base\",\"after_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_metadata_0001\",\"operation_kind\":\"metadata\",\"path\":\"src/auth.ts\",\"patch_digest\":null,\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"write_set\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"mutation\":\"write\"}]"
    ));
    assert!(stdout.contains(
        "\"before_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"after_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
}

#[test]
fn compat_import_delete_tombstones_path_json_fixture_returns_operation_plan() {
    let repo = TestRepo::new("compat-import-delete-tombstones-path");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_delete_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"selected_delta_count\":1"));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_delete_0001\"]"));
    assert!(
        stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_delete_0001\"]")
    );
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_delete_0001\",\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"operation_kind\":\"delete\",\"before_hash\":\"sha256:auth_base\",\"after_hash\":null"
    ));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_delete_0001\",\"operation_kind\":\"delete\",\"path\":\"src/auth.ts\",\"patch_digest\":null,\"base_content_hash\":\"sha256:auth_base\",\"result_content_hash\":null"
    ));
    assert!(stdout.contains(
        "\"write_set\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"mutation\":\"write\"}]"
    ));
    assert!(stdout.contains(
        "\"before_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"active\",\"content_hash\":\"sha256:auth_base\""
    ));
    assert!(stdout.contains(
        "\"after_refs\":{\"artifacts\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"path_state\":\"tombstone\",\"content_hash\":null"
    ));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_compat_0001\"}"
    ));
}

#[test]
fn compat_import_json_fixture_multiple_candidates_returns_one_operation_plan() {
    let repo = TestRepo::new("compat-import-fixture-multiple-candidates");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--candidate")
        .arg("compat_delta_src_session_ts_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"compat.import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"selected_delta_count\":2"));
    assert!(stdout.contains(
        "\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\",\"compat_delta_src_session_ts_0001\"]"
    ));
    assert!(stdout.contains(
        "\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\",\"compat_delta_src_session_ts_0001\"]"
    ));
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains("\"candidate_delta_id\":\"compat_delta_src_session_ts_0001\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_session_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"path\":\"src/session.ts\""));
    assert!(stdout.contains(
        "\"write_set\":[{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"mutation\":\"patch\"},{\"artifact_id\":\"artifact_src_session_ts\",\"path\":\"src/session.ts\",\"mutation\":\"write\"}]"
    ));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_compat_0001\"}"
    ));
}

#[test]
fn status_json_fixture_compat_import_returns_lifecycle_snapshot() {
    let repo = TestRepo::new("status-fixture-compat-import");

    let output = sun()
        .arg("status")
        .arg("--compat-import")
        .arg("op_compat_import_auth_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.compat_import\""));
    assert!(stdout.contains("\"compat_import_operation_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"lifecycle_state\":\"imported\""));
    assert!(stdout.contains("\"imported_artifact_count\":1"));
    assert!(stdout.contains("\"selected_delta_count\":1"));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"operation_plan\":{\"operation_transaction_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_compat_0001\"}"
    ));
}

#[test]
fn inspect_json_fixture_compat_import_selector_returns_import_detail() {
    let repo = TestRepo::new("inspect-fixture-compat-import");

    let output = sun()
        .arg("inspect")
        .arg("compat_import:op_compat_import_auth_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect compat import should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.compat_import\""));
    assert!(stdout
        .contains("\"import_provenance\":{\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"imported_artifacts\":[{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains(
        "\"selected_deltas\":[{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains(
        "\"operation_plan\":{\"operation_transaction_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"payload\":{\"kind\":\"compat_import\""));
    assert!(stdout.contains(
        "\"topic_revision\":{\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""
    ));
    assert!(stdout
        .contains("\"session_generation\":{\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn inspect_json_fixture_operation_selector_returns_compat_import_payload() {
    let repo = TestRepo::new("inspect-fixture-compat-import-operation");

    let output = sun()
        .arg("inspect")
        .arg("operation:op_compat_import_auth_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect compat import operation should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.operation\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout
        .contains("\"operation\":{\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"mutation\":\"compat_import\""));
    assert!(stdout.contains("\"payload\":{\"kind\":\"compat_import\""));
    assert!(stdout.contains("\"baseline_manifest_digest\":\"sha256:compat_baseline\""));
    assert!(stdout.contains(
        "\"projection_provenance\":{\"projection_id\":\"projection_compat_agent_a_0001\""
    ));
    assert!(stdout.contains(
        "\"created_revision\":{\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""
    ));
    assert!(stdout
        .contains("\"session_generation\":{\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_json_fixture_no_candidate_returns_no_selected_changes() {
    let repo = TestRepo::new("compat-import-no-candidate");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_no_selected_changes\""));
    assert!(stdout.contains("\"message\":\"no compatibility import candidates selected\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
}

#[test]
fn compat_import_json_fixture_missing_candidate_returns_diff_failed() {
    let repo = TestRepo::new("compat-import-missing-candidate");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_missing_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"compat_diff_failed\""));
    assert!(stdout.contains("\"message\":\"selected compatibility candidate was not found\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_missing_0001\"]"));
    assert!(stdout.contains(
        "\"reason\":\"selected candidate delta was not present in fixture diff output\""
    ));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
}

#[test]
fn compat_import_stale_session_generation_json_fixture_returns_precondition_failed() {
    let repo = TestRepo::new("compat-import-stale-session-generation");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--session-generation")
        .arg("gen_agent_a_stale_0000")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_precondition_failed\""));
    assert!(stdout.contains("\"message\":\"compatibility import precondition failed\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"reason\":\"session generation `gen_agent_a_stale_0000` does not match current generation `gen_agent_a_0001`\""
    ));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(stdout.contains("\"session_generation_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_stale_projection_baseline_json_fixture_returns_projection_stale() {
    let repo = TestRepo::new("compat-import-stale-projection-baseline");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_stale_baseline_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_projection_stale\""));
    assert!(stdout.contains("\"message\":\"compatibility projection is stale\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_stale_baseline_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[]"));
    assert!(stdout
        .contains("\"reason\":\"projection baseline does not match the supplied current view\""));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(stdout.contains("\"session_generation_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_json_fixture_secret_candidate_is_policy_blocked() {
    let repo = TestRepo::new("compat-import-secret-candidate");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_env_secret_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"compat_secret_detected\""));
    assert!(stdout.contains("\"message\":\"selected compatibility candidate contains secrets\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_env_secret_0001\"]"));
    assert!(stdout.contains("\"reason\":\"secret-like candidate cannot be imported as source\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
}

#[test]
fn compat_import_atomic_failure_json_fixture_mixed_selected_candidate_is_policy_blocked() {
    let repo = TestRepo::new("compat-import-atomic-failure");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_ts_0001")
        .arg("--candidate")
        .arg("compat_delta_env_secret_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_secret_detected\""));
    assert!(stdout.contains("\"message\":\"selected compatibility candidate contains secrets\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_env_secret_0001\"]"));
    assert!(stdout.contains("\"reason\":\"secret-like candidate cannot be imported as source\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_json_fixture_cache_candidate_is_policy_blocked() {
    let repo = TestRepo::new("compat-import-cache-candidate");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_dist_bundle_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"compat_cache_blocked\""));
    assert!(stdout.contains(
        "\"message\":\"selected compatibility candidate is cache, build, or ignored path\""
    ));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_dist_bundle_0001\"]"));
    assert!(stdout
        .contains("\"reason\":\"cache, build, and ignored candidates are blocked by default\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
}

#[test]
fn compat_import_json_fixture_ignored_path_candidate_is_policy_blocked() {
    let repo = TestRepo::new("compat-import-ignored-path-candidate");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_ignored_editor_swap_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"compat_cache_blocked\""));
    assert!(stdout.contains(
        "\"message\":\"selected compatibility candidate is cache, build, or ignored path\""
    ));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_ignored_editor_swap_0001\"]"));
    assert!(stdout
        .contains("\"reason\":\"cache, build, and ignored candidates are blocked by default\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
}

#[test]
fn compat_import_generated_policy_json_fixture_generated_candidate_is_atomic_failure() {
    let repo = TestRepo::new("compat-import-generated-policy");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_generated_schema_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_policy_failed\""));
    assert!(
        stdout.contains("\"message\":\"selected compatibility candidate failed import policy\"")
    );
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_generated_schema_0001\"]"));
    assert!(stdout.contains(
        "\"reason\":\"generated or binary candidates require an explicit policy conversion\""
    ));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(stdout.contains("\"session_generation_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_conflicted_delta_json_fixture_is_atomic_failure() {
    let repo = TestRepo::new("compat-import-conflicted-delta");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_src_auth_conflict_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_conflicted_delta\""));
    assert!(stdout.contains("\"message\":\"selected compatibility candidate is conflicted\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_conflict_0001\"]"));
    assert!(stdout.contains("\"reason\":\"conflicted candidate cannot be imported\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(stdout.contains("\"session_generation_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_ambiguous_rename_json_fixture_is_atomic_failure() {
    let repo = TestRepo::new("compat-import-ambiguous-rename");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_auth_rename_ambiguous_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_ambiguous_rename\""));
    assert!(stdout.contains(
        "\"message\":\"selected compatibility candidate has ambiguous rename identity\""
    ));
    assert!(
        stdout.contains("\"candidate_delta_ids\":[\"compat_delta_auth_rename_ambiguous_0001\"]")
    );
    assert!(stdout.contains("\"reason\":\"fixture foundation does not resolve rename identity\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(stdout.contains("\"topic_revision_id\":null"));
    assert!(stdout.contains("\"session_generation_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn compat_import_path_policy_json_fixture_reserved_sunlight_candidate_is_rejected() {
    let repo = TestRepo::new("compat-import-path-policy-reserved-sunlight");

    let output = sun()
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--candidate")
        .arg("compat_delta_reserved_sunlight_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"compat_path_policy_failed\""));
    assert!(stdout.contains("\"message\":\"selected compatibility candidate failed path policy\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_reserved_sunlight_0001\"]"));
    assert!(stdout.contains("\"reason\":\"reserved_path\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
    assert!(!stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(!stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(!stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
}

#[test]
fn project_materialize_json_fixture_conflicted_view_returns_projection_error() {
    let repo = TestRepo::new("projection-fixture-conflicted");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001",
    );

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_conflicted_view\""));
    assert!(stdout.contains("\"message\":\"resolved view has conflicts and cannot be projected\""));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"conflict_ids\":[\"conflict_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"staleness_ids\":[]"));
    assert!(stdout.contains("\"projection_id\":null"));
}

#[test]
fn project_materialize_json_fixture_stale_view_returns_projection_error() {
    let repo = TestRepo::new("projection-fixture-stale");
    let view_id = resolve_fixture_view_id(repo.path(), "topic_profile_ui:rev_profile_ui_0002");

    let output = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"projection_stale_view\""));
    assert!(stdout.contains("\"message\":\"resolved view has staleness and cannot be projected\""));
    assert!(stdout.contains("\"conflict_ids\":[]"));
    assert!(stdout
        .contains("\"staleness_ids\":[\"stale_missing_dependency_rev_auth_nullability_0001\"]"));
    assert!(stdout.contains("\"projection_id\":null"));
}

#[test]
fn checkpoint_create_json_fixture_ready_view_returns_checkpoint_envelope() {
    let repo = TestRepo::new("checkpoint-fixture-ready");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );

    let output = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"checkpoint.create\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains(
        "\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_"
    ));
    assert!(stdout.contains(
        "\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\",\"topic_profile_ui\":\"rev_profile_ui_0001\"}"
    ));
    assert!(stdout.contains(
        "\"evidence_refs\":[{\"kind\":\"execution\",\"execution_id\":\"exec_auth_profile_tests_0001\",\"result\":\"pass\""
    ));
    assert!(stdout.contains("\"export_refs\":[]"));
    assert!(stdout.contains("\"export_ready\":true"));
    assert!(stdout.contains("\"conflict_free\":true"));
}

#[test]
fn checkpoint_create_json_fixture_conflicted_view_returns_stable_error() {
    let repo = TestRepo::new("checkpoint-fixture-conflicted");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001",
    );

    let output = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"checkpoint_conflicted_view\""));
    assert!(
        stdout.contains("\"message\":\"resolved view has conflicts and cannot be checkpointed\"")
    );
    assert!(stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(stdout.contains("\"conflict_ids\":[\"conflict_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"staleness_ids\":[]"));
    assert!(stdout.contains("\"checkpoint_id\":null"));
}

#[test]
fn checkpoint_create_json_fixture_stale_view_returns_stable_error() {
    let repo = TestRepo::new("checkpoint-fixture-stale");
    let view_id = resolve_fixture_view_id(repo.path(), "topic_profile_ui:rev_profile_ui_0002");

    let output = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"checkpoint_stale_view\""));
    assert!(
        stdout.contains("\"message\":\"resolved view has staleness and cannot be checkpointed\"")
    );
    assert!(stdout.contains("\"conflict_ids\":[]"));
    assert!(stdout
        .contains("\"staleness_ids\":[\"stale_missing_dependency_rev_auth_nullability_0001\"]"));
    assert!(stdout.contains("\"checkpoint_id\":null"));
}

#[test]
fn policy_check_export_json_fixture_checkpoint_returns_validation_envelope() {
    let repo = TestRepo::new("policy-check-export-fixture-ready");

    let output = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-export should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"policy.check-export\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"summary\":{\"records_checked\":4,\"payloads_checked\":0"));
    assert!(stdout.contains("\"failures\":[]"));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn policy_check_export_json_without_initialized_repository_returns_not_initialized() {
    let repo = TestRepo::new("policy-check-export-missing-fixture");

    let output = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy check-export should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"not_initialized\""));
}

#[test]
fn no_fixture_policy_check_and_git_export_share_persisted_validation() {
    let repo = TestRepo::new("real-policy-check-export-success");
    init_local_git_repo(&repo);
    start_native_session(&repo, "policy-export-success");
    let checkpoint_id = create_real_base_checkpoint(&repo);
    let branch = "refs/heads/sunlight/policy-export-success";

    let check = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg(branch)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real policy check-export should run");
    assert_success(&check);
    let check_stdout = stdout(&check);
    assert_valid_json(&check_stdout);
    let report_id = json_string_field(&check_stdout, "validation_report_id");
    assert!(check_stdout
        .contains("\"policy_id\":\"git_interop.sunlight_commit_policy.conservative.v1\""));
    assert!(check_stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(check_stdout.contains("\"tree_identity\":{"));
    assert!(check_stdout.contains("\"payloads_checked\":1"));
    assert!(!check_stdout.contains("fixture"));
    let report_path = repo
        .path()
        .join(".sunlight/records/validation-reports")
        .join(format!("{report_id}.json"));
    let report_bytes = fs::read(&report_path).unwrap();
    parse_json_record(&report_bytes).expect("persisted report should be valid JSON");

    let explain = sun()
        .arg("policy")
        .arg("explain")
        .arg(&report_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real policy explain should run");
    assert_success(&explain);
    let explain_stdout = stdout(&explain);
    assert_valid_json(&explain_stdout);
    assert!(explain_stdout.contains("\"command\":\"policy.explain\""));
    assert!(explain_stdout.contains(&format!("\"validation_report_id\":\"{report_id}\"")));
    assert!(explain_stdout.contains("\"checkpoint_id\":"));
    assert!(explain_stdout.contains("\"policy_id\":"));
    assert!(explain_stdout.contains("\"tree_identity\":"));

    let repeated_check = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg(branch)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("repeated real policy check-export should run");
    assert_success(&repeated_check);
    assert_eq!(
        json_string_field(&stdout(&repeated_check), "validation_report_id"),
        report_id
    );
    assert_eq!(
        fs::read_dir(repo.path().join(".sunlight/records/validation-reports"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(fs::read(&report_path).unwrap(), report_bytes);

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg(branch)
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real git export should run");
    assert_success(&export);
    let export_stdout = stdout(&export);
    assert_valid_json(&export_stdout);
    assert!(export_stdout.contains(&format!("\"validation_report_id\":\"{report_id}\"")));
    assert!(export_stdout.contains("\"lifecycle_state\":\"exported\""));
    assert!(!export_stdout.contains("fixture"));
    assert!(git_ref_exists(repo.path(), branch));
    let export_record = fs::read_to_string(
        repo.path()
            .join(".sunlight/export-map")
            .join(format!("export_map_{checkpoint_id}.json")),
    )
    .unwrap();
    assert!(export_record.contains(&format!("\"validation_report_id\":\"{report_id}\"")));
    assert_eq!(fs::read(&report_path).unwrap(), report_bytes);
}

#[test]
fn no_fixture_generated_policy_failure_does_not_mutate_git_or_export_maps() {
    let repo = TestRepo::new("real-policy-generated-block");
    init_local_git_repo(&repo);
    start_native_session(&repo, "generated-block");
    let generated = repo.write_file("generated.txt", "generated output\n");
    let write = sun()
        .arg("write")
        .arg("src/generated.txt")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--expect-hash")
        .arg("new")
        .arg("--content-file")
        .arg(generated)
        .arg("--classification")
        .arg("generated")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("generated write should run");
    assert_success(&write);
    let view_id = json_string_field(&stdout(&write), "resolved_view_id");
    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(view_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("generated checkpoint should run");
    assert_success(&checkpoint);
    let checkpoint_id = json_string_field(&stdout(&checkpoint), "checkpoint_id");
    let branch = "refs/heads/sunlight/generated-block";
    let state_before = fs::read(repo.path().join(".sunlight/records/native-state.json")).unwrap();
    let commit_count_before = git(repo.path(), &["rev-list", "--all", "--count"]);

    let check = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg(branch)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("generated policy check should run");
    assert_failure(&check);
    let check_stdout = stdout(&check);
    assert!(check_stdout.contains("\"code\":\"export_policy_failed\""));
    assert!(check_stdout.contains("\"check\":\"generated_policy\""));
    assert!(check_stdout.contains("\"code\":\"generated_output_requires_promotion\""));
    let report_id = json_string_field(&check_stdout, "id");
    let report_path = repo
        .path()
        .join(".sunlight/records/validation-reports")
        .join(format!("{report_id}.json"));
    assert!(report_path.is_file());
    let explain = sun()
        .arg("policy")
        .arg("explain")
        .arg(&report_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("blocked real policy explain should run");
    assert_success(&explain);
    let explain_stdout = stdout(&explain);
    assert!(explain_stdout.contains("\"ok\":false"));
    assert!(explain_stdout.contains("\"check\":\"generated_policy\""));
    assert!(explain_stdout.contains("\"value\":\"src/generated.txt\""));

    let export = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg(branch)
        .arg("--execute-local")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("blocked generated export should run");
    assert_failure(&export);
    let export_stdout = stdout(&export);
    assert_eq!(json_string_field(&export_stdout, "id"), report_id);
    assert!(export_stdout.contains("\"check\":\"generated_policy\""));
    assert!(export_stdout.contains("\"commit_created\":false"));
    assert!(export_stdout.contains("\"ref_updated\":false"));
    assert!(export_stdout.contains("\"export_map_written\":false"));
    assert!(!git_ref_exists(repo.path(), branch));
    assert_eq!(
        git(repo.path(), &["rev-list", "--all", "--count"]),
        commit_count_before
    );
    assert_eq!(
        fs::read(repo.path().join(".sunlight/records/native-state.json")).unwrap(),
        state_before
    );
    assert!(!repo
        .path()
        .join(".sunlight/export-map")
        .join(format!("export_map_{checkpoint_id}.json"))
        .exists());
}

#[test]
fn no_fixture_policy_explain_reports_missing_and_tampered_records() {
    let repo = TestRepo::new("real-policy-explain-integrity");
    init_local_git_repo(&repo);
    start_native_session(&repo, "policy-explain-integrity");

    let missing = sun()
        .arg("policy")
        .arg("explain")
        .arg(format!("validation_sha256_{}", "0".repeat(64)))
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("missing real policy explain should run");
    assert_failure(&missing);
    let missing_stdout = stdout(&missing);
    assert!(missing_stdout.contains("\"code\":\"object_not_found\""));
    assert!(missing_stdout.contains("\"object_type\":\"validation_report\""));

    let checkpoint_id = create_real_base_checkpoint(&repo);
    let check = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg(checkpoint_id)
        .arg("--branch")
        .arg("refs/heads/sunlight/policy-explain-integrity")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real policy check should run");
    assert_success(&check);
    let report_id = json_string_field(&stdout(&check), "validation_report_id");
    let report_path = repo
        .path()
        .join(".sunlight/records/validation-reports")
        .join(format!("{report_id}.json"));
    let tampered = fs::read_to_string(&report_path)
        .unwrap()
        .replace("\"ok\":true", "\"ok\":false");
    fs::write(&report_path, tampered).unwrap();

    let explain = sun()
        .arg("policy")
        .arg("explain")
        .arg(&report_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("tampered real policy explain should run");
    assert_failure(&explain);
    let explain_stdout = stdout(&explain);
    assert!(explain_stdout.contains("\"code\":\"validation_report_integrity_failed\""));
    assert!(explain_stdout.contains(&format!("\"validation_report_id\":\"{report_id}\"")));
    let status = sun()
        .args(["status", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_success(&status);
    let status = stdout(&status);
    assert!(status.contains("\"invalid_or_tampered\":1"));
    assert!(status.contains("\"code\":\"policy_report_integrity\""));
}

#[test]
fn no_fixture_persisted_local_only_checkpoint_entry_is_export_blocked() {
    let repo = TestRepo::new("real-policy-local-only-block");
    init_local_git_repo(&repo);
    start_native_session(&repo, "local-only-export-block");
    let checkpoint_id = create_real_base_checkpoint(&repo);
    let state_path = repo.path().join(".sunlight/records/native-state.json");
    let mut state = fs::read_to_string(&state_path).unwrap();
    let checkpoint_offset = state.find("\"checkpoints\":[{").unwrap();
    let classification_offset = state[checkpoint_offset..]
        .find("\"classification\":\"source\"")
        .unwrap()
        + checkpoint_offset;
    state.replace_range(
        classification_offset..classification_offset + "\"classification\":\"source\"".len(),
        "\"classification\":\"local_only\"",
    );
    fs::write(&state_path, state).unwrap();

    let check = sun()
        .arg("policy")
        .arg("check-export")
        .arg("--checkpoint")
        .arg(&checkpoint_id)
        .arg("--branch")
        .arg("refs/heads/sunlight/local-only-block")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("local-only policy check should run");
    assert_failure(&check);
    let check_stdout = stdout(&check);
    assert!(check_stdout.contains("\"check\":\"policy_class\""));
    assert!(check_stdout.contains("\"code\":\"secret_or_local_only_record\""));
    assert!(check_stdout.contains("\"value\":\"base.txt\""));
}

#[test]
fn no_fixture_unknown_export_policy_is_rejected_without_git_mutation() {
    let repo = TestRepo::new("real-policy-unknown-config");
    init_local_git_repo(&repo);
    start_native_session(&repo, "unknown-policy");
    let checkpoint_id = create_real_base_checkpoint(&repo);
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path).unwrap().replace(
        "sunlight_commit_policy = \"conservative\"",
        "sunlight_commit_policy = \"permissive\"",
    );
    fs::write(&config_path, config).unwrap();
    let branch = "refs/heads/sunlight/unknown-policy";

    for command in ["check", "export"] {
        let mut invocation = sun();
        if command == "check" {
            invocation.arg("policy").arg("check-export");
        } else {
            invocation.arg("git").arg("export");
        }
        let output = invocation
            .arg("--checkpoint")
            .arg(&checkpoint_id)
            .arg("--branch")
            .arg(branch)
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("unknown policy command should run");
        assert_failure(&output);
        let output = stdout(&output);
        assert!(output.contains("\"code\":\"invalid_repository_config\""));
        assert!(output.contains("unsupported git_interop.sunlight_commit_policy `permissive`"));
    }
    assert!(!git_ref_exists(repo.path(), branch));
    assert!(!repo
        .path()
        .join(".sunlight/export-map")
        .join(format!("export_map_{checkpoint_id}.json"))
        .exists());
}

#[test]
fn policy_explain_json_fixture_validation_report_returns_report_envelope() {
    let repo = TestRepo::new("policy-explain-fixture-validation-report");

    let output = sun()
        .arg("policy")
        .arg("explain")
        .arg("validation_export_auth_profile_ready_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy explain should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"policy.explain\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains(
        "\"ids\":{\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"}"
    ));
    assert!(stdout
        .contains("\"validation_report\":{\"id\":\"validation_export_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"summary\":{\"records_checked\":4,\"payloads_checked\":0"));
    assert!(stdout.contains("\"failures\":[]"));
    assert!(stdout.contains("\"warnings\":[]"));
}

#[test]
fn policy_explain_json_missing_validation_report_returns_not_found() {
    let repo = TestRepo::new("policy-explain-missing-validation-report");

    let output = sun()
        .arg("policy")
        .arg("explain")
        .arg("validation_missing_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun policy explain should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"message\":\"Sunlight object was not found\""));
    assert!(stdout.contains("\"selector\":\"validation_missing_0001\""));
    assert!(stdout.contains("\"object_type\":\"validation_report\""));
    assert!(stdout.contains(
        "\"available_fixture_validation_report_id\":\"validation_export_auth_profile_ready_0001\""
    ));
}

#[test]
fn git_export_json_fixture_checkpoint_returns_export_envelope() {
    let repo = TestRepo::new("git-export-fixture-ready");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"failures\":[]"));
    assert!(stdout.contains(
        "\"export_shape\":{\"kind\":\"single_checkpoint_commit\",\"parent_policy\":\"base_checkpoint_git_parent\",\"include_sunlight_metadata\":\"policy_approved_manifest_only\"}"
    ));
    assert!(stdout.contains("\"privacy_class\":\"commit_default\""));
}

#[test]
fn git_export_json_fixture_missing_checkpoint_returns_object_not_found() {
    let repo = TestRepo::new("git-export-missing-checkpoint");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_missing_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"message\":\"Sunlight object was not found\""));
    assert!(stdout.contains("\"selector\":\"checkpoint_missing_0001\""));
    assert!(stdout.contains("\"object_type\":\"checkpoint\""));
}

#[test]
fn git_export_json_fixture_invalid_git_ref_returns_validation_failure() {
    let repo = TestRepo::new("git-export-invalid-ref");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/bad ref")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"export_policy_failed\""));
    assert!(stdout.contains("\"message\":\"checkpoint failed Git export validation\""));
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/bad ref\""));
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"blocked\":1"));
    assert!(stdout.contains("\"check\":\"git_ref\""));
    assert!(stdout.contains("\"code\":\"export_ref_invalid\""));
    assert!(stdout.contains("\"field\":\"git_ref\""));
    assert!(stdout.contains("\"value\":\"refs/heads/bad ref\""));
}

#[test]
fn git_export_json_fixture_unpromoted_generated_output_returns_validation_failure() {
    let repo = TestRepo::new("git-export-unpromoted-generated-output");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/unpromoted-generated-output")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export should run");

    assert_failure(&output);
    assert!(!repo.path().join(".git").exists());
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"export_policy_failed\""));
    assert!(stdout.contains("\"message\":\"checkpoint failed Git export validation\""));
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/unpromoted-generated-output\""));
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"blocked\":1"));
    assert!(stdout.contains("\"check\":\"generated_policy\""));
    assert!(stdout.contains("\"code\":\"generated_output_requires_promotion\""));
    assert!(stdout.contains("\"field\":\"generated_outputs[].path\""));
    assert!(stdout.contains("\"value\":\"src/generated/auth.generated.ts\""));
    assert!(stdout.contains("promotion_operation_id"));
    assert!(stdout.contains(
        "\"git_write\":{\"commit_created\":false,\"ref_updated\":false,\"export_map_written\":false}"
    ));
    assert!(!stdout.contains("\"git_commit_ids\""));
    assert!(!stdout.contains("\"export_map\""));
}

#[test]
fn git_export_write_plan_json_fixture_returns_writer_plan() {
    let repo = TestRepo::new("git-export-write-plan-fixture-ready");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--write-plan")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export write plan should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.write_plan\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"parent_commit\":{\"checkpoint_id\":\"checkpoint_base_0001\",\"commit_id\":\"git_sha1_base_parent_0001\"}")
    );
    assert!(stdout.contains("\"planned_commit\":{"));
    assert!(stdout.contains("\"parent_commit_id\":\"git_sha1_base_parent_0001\""));
    assert!(stdout
        .contains("\"planned_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
    assert!(stdout.contains(
        "\"ref_update\":{\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\",\"expected_old_commit_id\":\"git_sha1_base_parent_0001\",\"new_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"allowed_reason\":\"replace_selected_parent\"}"
    ));
    assert!(stdout.contains("\"export_map\":{"));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
}

#[test]
fn projection_only_local_edits_are_not_checkpoint_or_export_source_truth() {
    let repo = TestRepo::new("projection-only-boundary-fixture-ready");
    let projection_root = repo.path().join("projection-root");
    let view_id = resolve_fixture_view_id(
        repo.path(),
        "topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001",
    );
    let projection_only_filename = "projection-only-unimported.ts";
    let projection_only_content = "projection only local source truth";

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    write_nested_file(
        &projection_root,
        &format!(
            ".sunlight/projections/compatibility/projection_compat_agent_a_0001/\
             candidate-deltas/{projection_only_filename}"
        ),
        projection_only_content,
    );
    write_nested_file(
        &projection_root,
        &format!("src/{projection_only_filename}"),
        projection_only_content,
    );

    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg(&view_id)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun checkpoint create should run");

    assert_success(&checkpoint);
    let checkpoint_stdout = stdout(&checkpoint);
    assert!(checkpoint_stdout.contains("\"command\":\"checkpoint.create\""));
    assert!(checkpoint_stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(checkpoint_stdout.contains(&format!("\"resolved_view_id\":\"{view_id}\"")));
    assert!(checkpoint_stdout.contains("\"export_ready\":true"));
    assert!(!checkpoint_stdout.contains(projection_only_filename));
    assert!(!checkpoint_stdout.contains(projection_only_content));

    let write_plan = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--write-plan")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export write plan should run");

    assert_success(&write_plan);
    let write_plan_stdout = stdout(&write_plan);
    assert!(write_plan_stdout.contains("\"command\":\"git.export.write_plan\""));
    assert!(write_plan_stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(write_plan_stdout
        .contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(write_plan_stdout
        .contains("\"planned_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
    assert!(!write_plan_stdout.contains(projection_only_filename));
    assert!(!write_plan_stdout.contains(projection_only_content));
}

#[test]
fn git_export_execute_fixture_json_returns_execution_success() {
    let repo = TestRepo::new("git-export-execute-fixture-success");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-fixture")
        .arg("success")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute fixture should run");

    assert_success(&output);
    assert!(!repo.path().join(".git").exists());
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.execute_fixture\""));
    assert!(stdout.contains("\"lifecycle_state\":\"exported\""));
    assert!(stdout
        .contains("\"created_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
    assert!(stdout.contains(
        "\"summary\":{\"commit_created\":true,\"ref_updated\":true,\"export_map_written\":true,\"completed_steps\":[\"commit_created\",\"ref_updated\",\"export_map_written\"]}"
    ));
    assert!(stdout.contains("\"error\":null"));
    assert!(stdout.contains("\"export_map\":{"));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
}

#[test]
fn git_export_execute_fixture_json_ref_update_partial_failure() {
    let repo = TestRepo::new("git-export-execute-fixture-ref-update-partial");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-fixture")
        .arg("ref-update-failure")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute fixture should run");

    assert_success(&output);
    assert!(!repo.path().join(".git").exists());
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.execute_fixture\""));
    assert!(stdout.contains("\"lifecycle_state\":\"partial\""));
    assert!(stdout
        .contains("\"created_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
    assert!(stdout.contains(
        "\"summary\":{\"commit_created\":true,\"ref_updated\":false,\"export_map_written\":false,\"completed_steps\":[\"commit_created\"]}"
    ));
    assert!(stdout.contains("\"code\":\"export_ref_update_failed\""));
    assert!(stdout.contains("\"failed_step\":\"ref_updated\""));
    assert!(stdout.contains("\"message\":\"fixture ref update failed\""));
    assert!(stdout.contains("\"export_map\":null"));
}

#[test]
fn git_export_execute_fixture_json_export_map_partial_failure() {
    let repo = TestRepo::new("git-export-execute-fixture-export-map-partial");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-fixture")
        .arg("export-map-failure")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute fixture should run");

    assert_success(&output);
    assert!(!repo.path().join(".git").exists());
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.execute_fixture\""));
    assert!(stdout.contains("\"lifecycle_state\":\"partial\""));
    assert!(stdout
        .contains("\"created_commit_id\":\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
    assert!(stdout.contains(
        "\"summary\":{\"commit_created\":true,\"ref_updated\":true,\"export_map_written\":false,\"completed_steps\":[\"commit_created\",\"ref_updated\"]}"
    ));
    assert!(stdout.contains("\"code\":\"export_map_write_failed\""));
    assert!(stdout.contains("\"failed_step\":\"export_map_written\""));
    assert!(stdout.contains("\"message\":\"fixture export map write failed\""));
    assert!(stdout.contains("\"export_map\":null"));
}

#[test]
fn git_export_execute_local_json_writes_real_commit_and_ref() {
    let repo = TestRepo::new("git-export-execute-local-success");
    let base_commit_id = init_local_git_repo(&repo);

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/local-export")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-local")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute local should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.execute\""));
    assert!(stdout.contains("\"lifecycle_state\":\"exported\""));
    assert!(stdout.contains(
        "\"summary\":{\"commit_created\":true,\"ref_updated\":true,\"export_map_written\":true"
    ));
    assert!(stdout.contains("\"export_map\":{"));

    let commit_id = json_string_field(&stdout, "created_commit_id");
    let ref_id = git(
        repo.path(),
        &["rev-parse", "refs/heads/sunlight/local-export"],
    );
    assert_eq!(ref_id.trim(), commit_id);
    assert_eq!(
        git(repo.path(), &["cat-file", "-t", &commit_id]).trim(),
        "commit"
    );
    assert_eq!(
        git(repo.path(), &["rev-parse", &format!("{commit_id}^")]).trim(),
        base_commit_id
    );
}

#[test]
fn git_export_execute_local_ignores_dirty_worktree_and_index() {
    let repo = TestRepo::new("git-export-execute-local-dirty");
    init_local_git_repo(&repo);
    repo.write_file("dirty-untracked.txt", "untracked\n");
    repo.write_file("dirty-staged.txt", "staged\n");
    git(repo.path(), &["add", "dirty-staged.txt"]);
    repo.write_file("base.txt", "dirty worktree content\n");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/local-dirty")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-local")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute local should run");

    assert_success(&output);
    let stdout = stdout(&output);
    let commit_id = json_string_field(&stdout, "created_commit_id");
    let tree_paths = git(repo.path(), &["ls-tree", "-r", "--name-only", &commit_id]);
    assert!(tree_paths.contains("src/auth.rs"));
    assert!(tree_paths.contains("src/profile.rs"));
    assert!(tree_paths.contains(".sunlight/export-manifest.json"));
    assert!(!tree_paths.contains("dirty-untracked.txt"));
    assert!(!tree_paths.contains("dirty-staged.txt"));
    assert!(!tree_paths.contains("base.txt"));
}

#[test]
fn git_export_execute_local_target_ref_conflict_fails() {
    let repo = TestRepo::new("git-export-execute-local-ref-conflict");
    init_local_git_repo(&repo);
    let unrelated_commit_id = git(
        repo.path(),
        &["commit-tree", "HEAD^{tree}", "-m", "unrelated"],
    );
    let unrelated_commit_id = unrelated_commit_id.trim().to_string();
    git(
        repo.path(),
        &[
            "update-ref",
            "refs/heads/sunlight/local-conflict",
            &unrelated_commit_id,
        ],
    );

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/local-conflict")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-local")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute local should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"code\":\"export_target_ref_conflict\""));
    assert!(stdout.contains("\"target_ref\":\"refs/heads/sunlight/local-conflict\""));
    let ref_id = git(
        repo.path(),
        &["rev-parse", "refs/heads/sunlight/local-conflict"],
    );
    assert_eq!(ref_id.trim(), unrelated_commit_id);
}

#[test]
fn git_export_execute_local_export_map_partial_failure() {
    let repo = TestRepo::new("git-export-execute-local-map-partial");
    init_local_git_repo(&repo);

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/local-map-partial")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--execute-local")
        .arg("--simulate-export-map-write-failure")
        .arg("--repo")
        .arg(repo.path())
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export execute local should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"git.export.execute\""));
    assert!(stdout.contains("\"lifecycle_state\":\"partial\""));
    assert!(stdout.contains("\"code\":\"export_map_write_failed\""));
    assert!(stdout.contains("\"failed_step\":\"export_map_written\""));
    assert!(stdout.contains("\"export_map\":null"));
    let commit_id = json_string_field(&stdout, "created_commit_id");
    let ref_id = git(
        repo.path(),
        &["rev-parse", "refs/heads/sunlight/local-map-partial"],
    );
    assert_eq!(ref_id.trim(), commit_id);
}

#[test]
fn git_export_write_plan_json_fixture_target_ref_conflict_returns_planner_error() {
    let repo = TestRepo::new("git-export-write-plan-ref-conflict");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/ref-conflict")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--write-plan")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export write plan should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"export_target_ref_conflict\""));
    assert!(stdout.contains("existing target ref points at"));
    assert!(stdout.contains("\"target_ref\":\"refs/heads/sunlight/ref-conflict\""));
    assert!(stdout.contains("\"parent_commit_id\":\"git_sha1_base_parent_0001\""));
    assert!(stdout.contains("\"created_commit_id\":null"));
}

#[test]
fn git_export_write_plan_json_fixture_stale_validation_returns_planner_error() {
    let repo = TestRepo::new("git-export-write-plan-stale-validation");

    let output = sun()
        .arg("git")
        .arg("export")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--branch")
        .arg("refs/heads/sunlight/stale-validation")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--write-plan")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun git export write plan should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"export_policy_failed\""));
    assert!(stdout.contains("export validation report must pass and match checkpoint"));
    assert!(stdout.contains("\"target_ref\":\"refs/heads/sunlight/stale-validation\""));
    assert!(stdout.contains("\"parent_commit_id\":null"));
    assert!(stdout.contains("\"created_commit_id\":null"));
}

#[test]
fn status_json_fixture_basic_app_returns_repository_snapshot() {
    let repo = TestRepo::new("status-fixture-repository");

    let output = sun()
        .arg("status")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.repository\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"ids\":{\"base_checkpoint_id\":\"checkpoint_base_0001\"}"));
    assert!(stdout.contains("\"view\":null"));
    assert!(stdout.contains("\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\""));
    assert!(stdout.contains("\"operation_semantics_version\":\"file_ops_v1\""));
    assert!(stdout.contains("\"topic_id\":\"topic_auth_nullability\""));
    assert!(stdout.contains("\"head_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"session_id\":\"session_agent_a\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout.contains("\"native_errors\":[]"));
    assert!(stdout.contains("\"pending_work\":[]"));
}

#[test]
fn repository_inspect_json_fixture_basic_app_returns_repository_record() {
    let repo = TestRepo::new("inspect-fixture-repository");

    let output = sun()
        .arg("inspect")
        .arg("repository:repo_fixture_basic_app")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect repository should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"command\":\"inspect.repository\""));
    assert!(stdout.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(stdout.contains("\"ids\":{\"repository_id\":\"repo_fixture_basic_app\"}"));
    assert!(stdout.contains("\"view\":null"));
    assert!(stdout.contains("\"record_type\":\"repository\""));
    assert!(stdout.contains("\"lifecycle_state\":\"initialized\""));
    assert!(stdout.contains("\"initialized\":true"));
    assert!(stdout.contains("\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\""));
    assert!(stdout.contains("\"projection_policy\":{"));
    assert!(stdout.contains("\"git_interop_policy\":\"default_local_mvp\""));
    assert!(stdout.contains("\"storage_health\":{\"status\":\"ok\""));
    assert!(stdout.contains("\"privacy_export_defaults\":{"));
}

#[test]
fn repository_inspect_json_fixture_missing_repository_returns_object_not_found() {
    let repo = TestRepo::new("inspect-fixture-repository-missing");

    let output = sun()
        .arg("inspect")
        .arg("repository:repo_fixture_missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect missing repository should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"repo_fixture_missing\""));
    assert!(stdout.contains("\"object_type\":\"repository\""));
}

#[test]
fn status_json_fixture_checkpoint_returns_checkpoint_snapshot() {
    let repo = TestRepo::new("status-fixture-checkpoint");

    let output = sun()
        .arg("status")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status checkpoint should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.checkpoint\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"conflict_free\":true"));
    assert!(stdout.contains("\"evidence_ready\":true"));
    assert!(stdout.contains("\"export_ready\":true"));
    assert!(stdout.contains("\"export_refs\":[]"));
}

#[test]
fn status_json_fixture_export_map_returns_git_export_lifecycle_snapshot() {
    let repo = TestRepo::new("status-fixture-export-map");

    let output = sun()
        .arg("status")
        .arg("--export-map")
        .arg("export_map_checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status export map should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.export_map\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains("\"resolved_view_id\":\"view_fixture_"));
    assert!(stdout.contains("\"git_export\":{\"lifecycle_state\":\"exported\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
    assert!(stdout.contains("\"partial_failure_marker\":null"));
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"export_map\":{"));
}

#[test]
fn status_json_fixture_export_alias_returns_git_export_lifecycle_snapshot() {
    let repo = TestRepo::new("status-fixture-export-alias");

    let output = sun()
        .arg("status")
        .arg("--export")
        .arg("export_map_checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status export alias should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.export_map\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains("\"resolved_view_id\":\"view_fixture_"));
    assert!(stdout.contains("\"git_export\":{\"lifecycle_state\":\"exported\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
    assert!(stdout.contains("\"partial_failure_marker\":null"));
    assert!(stdout.contains("\"validation_report\":{"));
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"export_map\":{"));
}

#[test]
fn status_json_fixture_missing_export_map_returns_object_not_found() {
    let repo = TestRepo::new("status-fixture-export-map-missing");

    let output = sun()
        .arg("status")
        .arg("--export-map")
        .arg("missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status export map should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"missing\""));
    assert!(stdout.contains("\"object_type\":\"export_map\""));
}

#[test]
fn status_json_fixture_git_ref_lookup_returns_export_map_snapshot() {
    let repo = TestRepo::new("status-fixture-git-ref");

    let output = sun()
        .arg("status")
        .arg("--git")
        .arg("refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status git ref should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.git\""));
    assert!(stdout.contains("\"ids\":{\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"validation_report_id\":\"validation_export_auth_profile_ready_0001\"")
    );
    assert!(stdout.contains("\"git_export\":{\"lifecycle_state\":\"exported\""));
    assert!(stdout.contains("\"mapping_state\":\"resolved\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
    assert!(stdout.contains("\"export_map\":{"));
    assert!(stdout.contains("\"ok\":true"));
}

#[test]
fn status_json_fixture_git_ref_commit_lookup_returns_export_map_snapshot() {
    let repo = TestRepo::new("status-fixture-git-ref-commit");

    let output = sun()
        .arg("status")
        .arg("--git")
        .arg("git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status git commit should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.git\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
}

#[test]
fn inspect_json_fixture_git_ref_lookup_returns_export_map_mapping() {
    let repo = TestRepo::new("inspect-fixture-git-ref");

    let output = sun()
        .arg("inspect")
        .arg("git:refs/heads/sunlight/auth-profile-ready")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect git ref should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.git\""));
    assert!(stdout.contains("\"ids\":{\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(
        stdout.contains("\"git_mapping\":{\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\"")
    );
    assert!(stdout.contains("\"record_type\":\"git_export_map\""));
    assert!(stdout
        .contains("\"validation_report\":{\"id\":\"validation_export_auth_profile_ready_0001\""));
}

#[test]
fn inspect_json_fixture_git_ref_commit_lookup_returns_export_map_mapping() {
    let repo = TestRepo::new("inspect-fixture-git-ref-commit");

    let output = sun()
        .arg("inspect")
        .arg("git:git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect git commit should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.git\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
}

#[test]
fn status_json_fixture_git_ref_missing_lookup_returns_object_not_found() {
    let repo = TestRepo::new("status-fixture-git-ref-missing");

    let output = sun()
        .arg("status")
        .arg("--git")
        .arg("refs/heads/sunlight/missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status missing git ref should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"refs/heads/sunlight/missing\""));
    assert!(stdout.contains("\"object_type\":\"git\""));
}

#[test]
fn inspect_json_fixture_git_ref_missing_lookup_returns_object_not_found() {
    let repo = TestRepo::new("inspect-fixture-git-ref-missing");

    let output = sun()
        .arg("inspect")
        .arg("git:git_sha1_missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect missing git commit should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"selector\":\"git_sha1_missing\""));
    assert!(stdout.contains("\"object_type\":\"git\""));
}

#[test]
fn status_round_trip_fixture_ids_through_matching_inspect_selectors() {
    let repo = TestRepo::new("status-round-trip-fixture-selectors");
    let projection_root = repo.path().join("projection-root");

    let repository_status = sun()
        .arg("status")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status repository should run");
    assert_success(&repository_status);
    let repository_id = json_string_field(&stdout(&repository_status), "repository_id");
    assert_eq!(repository_id, "repo_fixture_basic_app");
    let repository_inspect = sun()
        .arg("inspect")
        .arg(format!("repository:{repository_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect repository should run");
    assert_success(&repository_inspect);
    assert!(stdout(&repository_inspect).contains(&format!("\"repository_id\":\"{repository_id}\"")));

    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let checkpoint_status = sun()
        .arg("status")
        .arg("--checkpoint")
        .arg("checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status checkpoint should run");
    assert_success(&checkpoint_status);
    let checkpoint_id = json_string_field(&stdout(&checkpoint_status), "checkpoint_id");
    assert_eq!(checkpoint_id, "checkpoint_auth_profile_ready_0001");
    let checkpoint_inspect = sun()
        .arg("inspect")
        .arg(format!("checkpoint:{checkpoint_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect checkpoint should run");
    assert_success(&checkpoint_inspect);
    assert!(stdout(&checkpoint_inspect).contains(&format!("\"checkpoint_id\":\"{checkpoint_id}\"")));

    for (status_flag, inspect_prefix) in [("--export-map", "export_map"), ("--export", "export")] {
        let export_status = sun()
            .arg("status")
            .arg(status_flag)
            .arg("export_map_checkpoint_auth_profile_ready_0001")
            .arg("--fixture")
            .arg("basic-app")
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun status export map should run");
        assert_success(&export_status);
        let export_map_id = json_string_field(&stdout(&export_status), "export_map_id");
        assert_eq!(
            export_map_id,
            "export_map_checkpoint_auth_profile_ready_0001"
        );
        let export_inspect = sun()
            .arg("inspect")
            .arg(format!("{inspect_prefix}:{export_map_id}"))
            .arg("--fixture")
            .arg("basic-app")
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun inspect export map should run");
        assert_success(&export_inspect);
        assert!(stdout(&export_inspect).contains(&format!("\"export_map_id\":\"{export_map_id}\"")));
    }

    for git_selector in [
        "refs/heads/sunlight/auth-profile-ready",
        "git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let git_status = sun()
            .arg("status")
            .arg("--git")
            .arg(git_selector)
            .arg("--fixture")
            .arg("basic-app")
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun status git lookup should run");
        assert_success(&git_status);
        let export_map_id = json_string_field(&stdout(&git_status), "export_map_id");
        let git_inspect = sun()
            .arg("inspect")
            .arg(format!("git:{git_selector}"))
            .arg("--fixture")
            .arg("basic-app")
            .arg("--json")
            .current_dir(repo.path())
            .output()
            .expect("sun inspect git lookup should run");
        assert_success(&git_inspect);
        assert!(stdout(&git_inspect).contains(&format!("\"export_map_id\":\"{export_map_id}\"")));
    }

    let projection_status = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status projection should run");
    assert_success(&projection_status);
    let projection_id = json_string_field(&stdout(&projection_status), "projection_id");
    assert_eq!(projection_id, "projection_exec_auth_profile_0001");
    let projection_inspect = sun()
        .arg("inspect")
        .arg(format!("projection:{projection_id}"))
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");
    assert_success(&projection_inspect);
    assert!(stdout(&projection_inspect).contains(&format!("\"projection_id\":\"{projection_id}\"")));

    let execution_status = sun()
        .arg("status")
        .arg("--execution")
        .arg("exec_auth_profile_tests_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status execution should run");
    assert_success(&execution_status);
    let execution_id = json_string_field(&stdout(&execution_status), "execution_id");
    assert_eq!(execution_id, "exec_auth_profile_tests_0001");
    let execution_inspect = sun()
        .arg("inspect")
        .arg(format!("execution:{execution_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect execution should run");
    assert_success(&execution_inspect);
    assert!(stdout(&execution_inspect).contains(&format!("\"execution_id\":\"{execution_id}\"")));
}

#[test]
fn status_json_fixture_basic_app_returns_session_snapshot() {
    let repo = TestRepo::new("status-fixture-session");

    let output = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.session\""));
    assert!(stdout.contains("\"ids\":{\"session_id\":\"session_agent_a\",\"write_topic_id\":\"topic_auth_nullability\"}"));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_patch_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout
        .contains("\"topic_frontier\":{\"topic_auth_nullability\":\"rev_auth_nullability_0001\"}"));
    assert!(stdout.contains("\"capabilities\":[\"read\",\"list\",\"search\",\"inspect\""));
    assert!(stdout.contains("\"before_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_hash\":\"sha256:auth_trim_guard\""));
    assert!(stdout.contains("\"last_operation_id\":\"op_auth_trim_guard_0001\""));
}

#[test]
fn compat_session_status_import_visibility() {
    let repo = TestRepo::new("compat-session-status-import-visibility");

    let output = sun()
        .arg("status")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.session\""));
    assert!(stdout.contains(
        "\"ids\":{\"session_id\":\"session_agent_a\",\"write_topic_id\":\"topic_auth_nullability\"}"
    ));
    assert!(stdout.contains("\"compatibility_imports\":{"));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"purpose\":\"compatibility\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(stdout.contains("\"tree_hash\":\"tree_fixture_base_0001\""));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"last_import\":{\"compat_import_operation_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
}

#[test]
fn inspect_json_fixture_checkpoint_returns_frozen_record() {
    let repo = TestRepo::new("inspect-fixture-checkpoint");

    let output = sun()
        .arg("inspect")
        .arg("checkpoint:checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect checkpoint should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.checkpoint\""));
    assert!(stdout.contains("\"record_type\":\"checkpoint\""));
    assert!(stdout.contains("\"id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"retention_class\":\"landable\""));
    assert!(stdout.contains("\"privacy_class\":\"commit_default\""));
    assert!(stdout.contains(
        "\"evidence_refs\":[{\"kind\":\"execution\",\"execution_id\":\"exec_auth_profile_tests_0001\""
    ));
}

#[test]
fn inspect_json_fixture_export_map_returns_mapping_record() {
    let repo = TestRepo::new("inspect-fixture-export-map");

    let output = sun()
        .arg("inspect")
        .arg("export_map:export_map_checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect export map should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.export_map\""));
    assert!(stdout
        .contains("\"ids\":{\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"record_type\":\"git_export_map\""));
    assert!(stdout.contains("\"id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"export_shape\":{\"kind\":\"single_checkpoint_commit\""));
    assert!(stdout.contains("\"parent_policy\":\"base_checkpoint_git_parent\""));
    assert!(stdout.contains("\"include_sunlight_metadata\":\"policy_approved_manifest_only\""));
    assert!(stdout.contains("\"exported_at\":\"2026-07-03T00:00:00Z\""));
    assert!(stdout
        .contains("\"validation_report\":{\"id\":\"validation_export_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"privacy_class\":\"commit_default\""));
}

#[test]
fn inspect_json_fixture_export_alias_returns_mapping_record() {
    let repo = TestRepo::new("inspect-fixture-export-alias");

    let output = sun()
        .arg("inspect")
        .arg("export:export_map_checkpoint_auth_profile_ready_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect export alias should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.export_map\""));
    assert!(stdout
        .contains("\"ids\":{\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"record_type\":\"git_export_map\""));
    assert!(stdout.contains("\"id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"export_shape\":{\"kind\":\"single_checkpoint_commit\""));
    assert!(stdout.contains("\"parent_policy\":\"base_checkpoint_git_parent\""));
    assert!(stdout.contains("\"include_sunlight_metadata\":\"policy_approved_manifest_only\""));
    assert!(stdout.contains("\"exported_at\":\"2026-07-03T00:00:00Z\""));
    assert!(stdout
        .contains("\"validation_report\":{\"id\":\"validation_export_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout.contains("\"privacy_class\":\"commit_default\""));
}

#[test]
fn inspect_json_fixture_projection_returns_local_only_metadata() {
    let repo = TestRepo::new("inspect-fixture-projection");
    let projection_root = repo.path().join("projection-root");
    let materialize = sun()
        .arg("project")
        .arg("materialize")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--purpose")
        .arg("execution")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun project materialize should run");
    assert_success(&materialize);

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"root_ref\":{\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"}"));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"privacy_class\":\"local_only\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"present\""));
    assert!(stdout.contains("\"files\":5"));
    assert!(stdout.contains("\"bytes\":222"));
}

#[test]
fn status_json_fixture_compat_projection_reports_dirty_candidates() {
    let repo = TestRepo::new("status-fixture-compat-projection");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status compat projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"status.projection\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"purpose\":\"compatibility\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"candidate_counts\":{\"total\":13"));
    assert!(stdout.contains(
        "\"by_classification\":{\"cache\":1,\"generated\":1,\"ignored\":1,\"policy\":1,\"secret\":1,\"source\":8}"
    ));
    assert!(stdout.contains(
        "\"by_kind\":{\"cache_or_build_output\":1,\"conflicted_delta\":1,\"created_source\":1,\"deleted_source\":1,\"generated_source\":1,\"ignored_path\":1,\"metadata_changed\":1,\"modified_source\":1,\"moved_or_renamed\":3,\"path_policy_blocked\":1,\"secret_like\":1}"
    ));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"quarantine_refs\":[\"quarantine://compat/projection_compat_agent_a_0001/env\"]"
    ));
    assert!(stdout.contains(
        "\"last_import_attempt\":{\"compat_import_operation_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"local_projection_refs\":{\"root_ref\":{\"value\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"privacy_class\":\"local_only\""));
    assert!(stdout.contains("\"candidate_summary_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json\""));
    assert!(stdout.contains("\"candidate_detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas\""));
    assert!(stdout.contains("\"local_projection_refs\":{\"root_ref\":{\"value\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"},\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/view_base_0001\",\"candidate_summary_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json\",\"candidate_detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas\",\"quarantine_refs\":[\"quarantine://compat/projection_compat_agent_a_0001/env\"]}"));
}

#[test]
fn status_json_fixture_compat_projection_last_import_visibility() {
    let repo = TestRepo::new("status-fixture-compat-projection-last-import-visibility");

    let output = sun()
        .arg("status")
        .arg("--projection")
        .arg("projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun status compat projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains(
        "\"last_import_attempt\":{\"compat_import_operation_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
}

#[test]
fn inspect_json_fixture_compat_projection_reports_baseline_policy_and_candidates() {
    let repo = TestRepo::new("inspect-fixture-compat-projection");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect compat projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"purpose\":\"compatibility\""));
    assert!(stdout.contains("\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/view_base_0001\""));
    assert!(stdout.contains(
        "\"compatibility_projection\":{\"baseline\":{\"resolved_view_id\":\"view_base_0001\""
    ));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:compat_baseline\""));
    assert!(stdout
        .contains("\"path_policy\":{\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\""));
    assert!(stdout.contains("\"writable_import_policy\":{\"writable_policy\":\"writable_with_explicit_import\",\"import_required\":true"));
    assert!(stdout.contains("\"candidate_summary\":{\"candidate_counts\":{\"total\":13"));
    assert!(stdout.contains(
        "\"by_classification\":{\"cache\":1,\"generated\":1,\"ignored\":1,\"policy\":1,\"secret\":1,\"source\":8}"
    ));
    assert!(stdout.contains(
        "\"by_kind\":{\"cache_or_build_output\":1,\"conflicted_delta\":1,\"created_source\":1,\"deleted_source\":1,\"generated_source\":1,\"ignored_path\":1,\"metadata_changed\":1,\"modified_source\":1,\"moved_or_renamed\":3,\"path_policy_blocked\":1,\"secret_like\":1}"
    ));
    assert!(stdout.contains("\"selected_candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains(
        "\"candidate_detail_refs\":[{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\""
    ));
    assert!(stdout.contains("\"summary_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json\""));
    assert!(stdout.contains(
        "\"quarantine_refs\":[\"quarantine://compat/projection_compat_agent_a_0001/env\"]"
    ));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_src_auth_delete_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_src_auth_metadata_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_src_auth_conflict_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_auth_rename_ambiguous_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_src_auth_rename_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_src_auth_rename_edit_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_generated_schema_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_ignored_editor_swap_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_env_secret_0001\""));
    assert!(stdout.contains("\"detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas/compat_delta_reserved_sunlight_0001\""));
    assert!(stdout.contains("\"local_projection_refs\":{\"root_ref\":{\"value\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"},\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/view_base_0001\",\"candidate_summary_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/compat-diff-summary.json\",\"candidate_detail_ref\":\"local://.sunlight/projections/compatibility/projection_compat_agent_a_0001/candidate-deltas\",\"quarantine_refs\":[\"quarantine://compat/projection_compat_agent_a_0001/env\"]}"));
    assert!(stdout.contains(
        "\"last_import_attempt\":{\"compat_import_operation_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"native_operation_ids\":[\"op_compat_import_auth_0001\"]"));
    assert!(stdout.contains("\"native_revision_ids\":[\"rev_auth_nullability_compat_0001\"]"));
}

#[test]
fn inspect_json_fixture_compat_projection_last_import_visibility() {
    let repo = TestRepo::new("inspect-fixture-compat-projection-last-import-visibility");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_compat_agent_a_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect compat projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains(
        "\"compatibility_projection\":{\"baseline\":{\"resolved_view_id\":\"view_base_0001\""
    ));
    assert!(stdout.contains(
        "\"last_import_attempt\":{\"compat_import_operation_id\":\"op_compat_import_auth_0001\""
    ));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_compat_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"native_operation_ids\":[\"op_compat_import_auth_0001\"]"));
    assert!(stdout.contains("\"native_revision_ids\":[\"rev_auth_nullability_compat_0001\"]"));
}

#[test]
fn inspect_json_fixture_projection_store_mismatch_reports_local_quarantine_metadata() {
    let repo = TestRepo::new("inspect-fixture-projection-store-mismatch");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\",\"integrity_status\":\"failed\""));
    assert!(stdout.contains(
        "\"local_quarantine\":{\"privacy_class\":\"local_only\",\"state\":\"quarantined\""
    ));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"local_root_verification\":null"));
    assert!(!stdout.contains("\"content_verification\":\"verified\""));
}

#[test]
fn inspect_json_fixture_projection_store_mismatch_writes_quarantine_record_with_root() {
    let repo = TestRepo::new("inspect-fixture-projection-store-mismatch-quarantine-record");
    let projection_root = repo.path().join("projection-root");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--integrity-fixture")
        .arg("store-mismatch")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));

    let record = fs::read_to_string(quarantine_record_path(&projection_root))
        .expect("quarantine record should be written");
    assert_projection_quarantine_record_json(&record);
}

#[test]
fn inspect_json_fixture_projection_scan_missing_blob_reports_local_quarantine_metadata() {
    let repo = TestRepo::new("inspect-fixture-projection-scan-missing-blob");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("scan-missing-blob")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\",\"integrity_status\":\"failed\""));
    assert!(stdout.contains(
        "\"local_quarantine\":{\"privacy_class\":\"local_only\",\"state\":\"quarantined\""
    ));
    assert!(stdout.contains("\"reason\":\"store_integrity_mismatch\""));
    assert!(stdout.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(stdout.contains("\"cache_reuse_allowed\":false"));
    assert!(stdout.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
    assert!(stdout.contains("\"local_root_verification\":null"));
    assert!(!stdout.contains("\"content_verification\":\"verified\""));
}

#[test]
fn inspect_json_fixture_projection_store_verified_reports_manifest_integrity() {
    let repo = TestRepo::new("inspect-fixture-projection-store-verified");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--integrity-fixture")
        .arg("verified")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"local_store_integrity\":{\"privacy_class\":\"local_only\",\"integrity_status\":\"verified\""));
    assert!(stdout.contains("\"source_truth\":\"immutable_store_manifest\""));
    assert!(stdout.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(stdout.contains("\"manifest_digest\":\"sha256:"));
    assert!(stdout.contains("\"root_ref\":{\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains("\"local_filesystem_source_truth\":false"));
    assert!(stdout.contains("\"local_quarantine\":null"));
    assert!(stdout.contains("\"local_root_verification\":null"));
}

#[test]
fn inspect_json_fixture_projection_reports_missing_local_root() {
    let repo = TestRepo::new("inspect-fixture-projection-root-missing");
    let projection_root = repo.path().join("missing-projection-root");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"missing\""));
    assert!(stdout.contains("\"content_verification\":\"verification_error\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_root_missing\"]"));
    assert!(stdout.contains("\"exists\":false"));
    assert!(stdout.contains("\"is_dir\":false"));
    assert!(stdout.contains("\"files\":0"));
    assert!(stdout.contains("\"bytes\":0"));
    assert!(stdout.contains("\"sample_paths\":[]"));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[test]
fn inspect_json_fixture_projection_reports_file_local_root() {
    let repo = TestRepo::new("inspect-fixture-projection-root-file");
    let projection_root = repo.write_file("projection-root-file", "not a directory\n");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
        .arg("--projection-root")
        .arg(&projection_root)
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect projection should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.projection\""));
    assert!(stdout.contains("\"record_type\":\"projection\""));
    assert!(stdout.contains("\"id\":\"projection_exec_auth_profile_0001\""));
    assert!(stdout.contains("\"local_root_verification\":{\"projection_root\":{\"path\":\""));
    assert!(stdout.contains("\"verification_state\":\"not_directory\""));
    assert!(stdout.contains("\"content_verification\":\"verification_error\""));
    assert!(stdout.contains("\"dirty_local\":null"));
    assert!(stdout.contains("\"verification_errors\":[\"projection_root_not_directory\"]"));
    assert!(stdout.contains("\"exists\":true"));
    assert!(stdout.contains("\"is_dir\":false"));
    assert!(stdout.contains("\"directories\":0"));
    assert!(stdout.contains("\"files\":0"));
    assert!(stdout.contains("\"bytes\":0"));
    assert!(stdout.contains("\"scan_error\":null"));
}

#[test]
fn inspect_json_fixture_basic_app_path_returns_artifact_snapshot() {
    let repo = TestRepo::new("inspect-fixture-artifact");

    let output = sun()
        .arg("inspect")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.artifact\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"content_hash\":\"sha256:auth_trim_guard\""));
    assert!(stdout.contains("\"byte_length\":103"));
    assert!(stdout.contains("\"latest_operation_id\":\"op_auth_trim_guard_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"before_refs\":[{\"operation_transaction_id\":\"op_auth_trim_guard_0001\",\"content_hash\":\"sha256:auth_base\""));
    assert!(stdout.contains("\"after_refs\":[{\"operation_transaction_id\":\"op_auth_trim_guard_0001\",\"content_hash\":\"sha256:auth_trim_guard\""));
}

#[test]
fn compat_artifact_import_provenance_is_visible_on_artifact_inspect() {
    let repo = TestRepo::new("compat-artifact-import-provenance");

    let output = sun()
        .arg("inspect")
        .arg("artifact_src_auth_ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.artifact\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"compatibility_import\":{"));
    assert!(stdout.contains("\"kind\":\"compat_import\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_compat_import_auth_0001\""));
    assert!(stdout.contains("\"projection_id\":\"projection_compat_agent_a_0001\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_src_auth_ts_0001\"]"));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_compat_0002\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_compat_import_0001\""));
    assert!(stdout.contains("\"imported_artifact\":{\"candidate_delta_id\":\"compat_delta_src_auth_ts_0001\",\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\""));
}

#[test]
fn checkpoint_export_trace_is_exact_for_changed_fixture_artifact() {
    let repo = TestRepo::new("artifact-checkpoint-export-trace-changed");

    let output = sun()
        .arg("inspect")
        .arg("src/auth.ts")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.artifact\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_src_auth_ts\""));
    assert!(stdout.contains("\"checkpoint_export_trace\":{"));
    assert!(stdout.contains("\"operation_id\":\"op_auth_trim_guard_0001\""));
    assert!(stdout.contains("\"topic_revision_id\":\"rev_auth_nullability_0001\""));
    assert!(stdout.contains("\"resolved_view_id\":\"view_agent_a_after_patch_0001\""));
    assert!(stdout.contains("\"execution_evidence_id\":\"exec_auth_profile_tests_0001\""));
    assert!(stdout.contains("\"execution_result\":\"pass\""));
    assert!(stdout.contains("\"checkpoint_id\":\"checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(stdout.contains("\"git_ref\":\"refs/heads/sunlight/auth-profile-ready\""));
    assert!(stdout
        .contains("\"git_commit_ids\":[\"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"));
}

#[test]
fn checkpoint_export_trace_is_null_for_base_fixture_artifact() {
    let repo = TestRepo::new("artifact-checkpoint-export-trace-base");

    let output = sun()
        .arg("inspect")
        .arg("README.md")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.artifact\""));
    assert!(stdout.contains("\"artifact_id\":\"artifact_readme_md\""));
    assert!(stdout.contains("\"provenance\":null"));
    assert!(stdout.contains("\"checkpoint_export_trace\":null"));
    assert!(!stdout.contains("\"export_map_id\":\"export_map_checkpoint_auth_profile_ready_0001\""));
    assert!(!stdout.contains("\"git_commit_ids\""));
}

#[test]
fn inspect_json_fixture_basic_app_operation_returns_authored_context() {
    let repo = TestRepo::new("inspect-fixture-operation");

    let output = sun()
        .arg("inspect")
        .arg("operation:op_auth_trim_guard_0001")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.operation\""));
    assert!(stdout.contains("\"operation_transaction_id\":\"op_auth_trim_guard_0001\""));
    assert!(stdout.contains("\"view\":{\"resolved_view_id\":\"view_base_0001\",\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"mutation\":\"patch\""));
    assert!(stdout.contains("\"authored_context_id\":\"ctx_agent_a_gen_0001\""));
    assert!(stdout.contains("\"expected_path\":\"src/auth.ts\""));
    assert!(stdout.contains("\"expected_hash\":\"sha256:auth_base\""));
    assert!(stdout
        .contains("\"created_revision\":{\"topic_revision_id\":\"rev_auth_nullability_0001\""));
}

#[test]
fn inspect_json_fixture_basic_app_session_returns_typed_snapshot() {
    let repo = TestRepo::new("inspect-fixture-session");

    let output = sun()
        .arg("inspect")
        .arg("session:session_agent_a")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"command\":\"inspect.session\""));
    assert!(stdout.contains("\"current_generation_number\":2"));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0001\""));
    assert!(stdout.contains("\"session_generation_id\":\"gen_agent_a_0002\""));
    assert!(stdout.contains(
        "\"created_by\":{\"kind\":\"operation_transaction\",\"id\":\"op_auth_trim_guard_0001\"}"
    ));
}

#[test]
fn inspect_json_fixture_basic_app_missing_operation_returns_failure_envelope() {
    let repo = TestRepo::new("inspect-fixture-missing");

    let output = sun()
        .arg("inspect")
        .arg("operation:missing")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun inspect should run");

    assert_failure(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"code\":\"object_not_found\""));
    assert!(stdout.contains("\"message\":\"Sunlight object was not found\""));
    assert!(stdout.contains("\"selector\":\"missing\""));
    assert!(stdout.contains("\"object_type\":\"operation\""));
}

#[test]
fn no_fixture_same_actor_sessions_keep_distinct_generation_lineages() {
    let repo = TestRepo::new("real-session-generation-collision");
    init_local_git_repo(&repo);
    assert_success(&run_real_json(&repo, &["init"]));
    for topic in ["first", "second"] {
        assert_success(&run_real_json(
            &repo,
            &["topic", "create", topic, "--display-name", topic],
        ));
    }

    let first_start = run_real_json(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "first",
            "--view",
            "view_base_0001",
            "--actor",
            "shared-agent",
        ],
    );
    assert_success(&first_start);
    let second_start = run_real_json(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "second",
            "--view",
            "view_base_0001",
            "--actor",
            "shared-agent",
        ],
    );
    assert_success(&second_start);
    let first_start_generation = json_string_field(&stdout(&first_start), "session_generation_id");
    let second_start_generation =
        json_string_field(&stdout(&second_start), "session_generation_id");
    assert_eq!(first_start_generation, "gen_shared_agent_0001");
    assert_eq!(second_start_generation, "gen_shared_agent_second_0001");
    assert_ne!(first_start_generation, second_start_generation);

    let first_refresh = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_shared_agent",
            "--policy",
            "manual",
        ],
    );
    assert_success(&first_refresh);
    let first_generation = json_string_field(&stdout(&first_refresh), "session_generation_id");
    assert_eq!(first_generation, "gen_shared_agent_0002");

    let content = repo.write_file("second-session.txt", "second session\n");
    let second_write = run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "second.txt".as_ref(),
            "--session".as_ref(),
            "session_shared_agent_second".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            content.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    );
    assert_success(&second_write);
    let second_generation = json_string_field(&stdout(&second_write), "session_generation_id");
    assert_eq!(second_generation, "gen_shared_agent_second_0002");
    assert_ne!(first_generation, second_generation);

    for (session_id, generation_id, generation_number) in [
        ("session_shared_agent", first_generation.as_str(), 2),
        ("session_shared_agent_second", second_generation.as_str(), 2),
    ] {
        let status = run_real_json(&repo, &["status", "--session", session_id]);
        assert_success(&status);
        assert_eq!(
            json_string_field(&stdout(&status), "session_generation_id"),
            generation_id
        );
        let inspect = run_real_json(&repo, &["inspect", &format!("session:{session_id}")]);
        assert_success(&inspect);
        assert_eq!(
            json_string_field(&stdout(&inspect), "session_generation_id"),
            generation_id
        );

        let record = fs::read_to_string(
            repo.path()
                .join(".sunlight/session-generations")
                .join(format!("{generation_id}.json")),
        )
        .unwrap();
        assert!(record.contains(&format!("\"session_id\":\"{session_id}\"")));
        assert!(record.contains(&format!("\"session_generation_id\":\"{generation_id}\"")));
        assert!(record.contains(&format!("\"generation_number\":{generation_number}")));
    }

    for (session_id, generation_id) in [
        ("session_shared_agent", first_start_generation),
        ("session_shared_agent_second", second_start_generation),
    ] {
        let record = fs::read_to_string(
            repo.path()
                .join(".sunlight/session-generations")
                .join(format!("{generation_id}.json")),
        )
        .unwrap();
        assert!(record.contains(&format!("\"session_id\":\"{session_id}\"")));
        assert!(record.contains("\"generation_number\":1"));
    }
}

#[test]
fn no_fixture_session_refresh_persists_frontier_policies_and_write_context() {
    let repo = TestRepo::new("real-session-refresh");
    init_local_git_repo(&repo);
    assert_success(&run_real_json(&repo, &["init"]));
    assert_success(&run_real_json(
        &repo,
        &[
            "topic",
            "create",
            "dependency",
            "--display-name",
            "Dependency",
        ],
    ));
    assert_success(&run_real_json(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "dependency",
            "--view",
            "view_base_0001",
            "--actor",
            "dependency-agent",
        ],
    ));
    let dependency_v1 = repo.write_file("dependency-v1.txt", "dependency v1\n");
    let dependency_write_v1 = run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "dependency.txt".as_ref(),
            "--session".as_ref(),
            "session_dependency_agent".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            dependency_v1.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    );
    assert_success(&dependency_write_v1);
    let dependency_revision_v1 =
        json_string_field(&stdout(&dependency_write_v1), "topic_revision_id");

    assert_success(&run_real_json(
        &repo,
        &["topic", "create", "writer", "--display-name", "Writer"],
    ));
    let head = run_real_json(&repo, &["view", "resolve"]);
    assert_success(&head);
    let head_view = resolved_view_id(&stdout(&head));
    let writer_start = run_real_json(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "writer",
            "--view",
            &head_view,
            "--actor",
            "writer-agent",
        ],
    );
    assert_success(&writer_start);
    let writer_status = run_real_json(&repo, &["status", "--session", "session_writer_agent"]);
    assert_success(&writer_status);
    assert!(stdout(&writer_status).contains(&dependency_revision_v1));

    let dependency_hash = json_string_field(
        &stdout(&run_real_json(
            &repo,
            &[
                "read",
                "dependency.txt",
                "--session",
                "session_dependency_agent",
            ],
        )),
        "content_hash",
    );
    let dependency_v2 = repo.write_file("dependency-v2.txt", "dependency v2\n");
    let dependency_write_v2 = run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "dependency.txt".as_ref(),
            "--session".as_ref(),
            "session_dependency_agent".as_ref(),
            "--expect-hash".as_ref(),
            dependency_hash.as_ref(),
            "--content-file".as_ref(),
            dependency_v2.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    );
    assert_success(&dependency_write_v2);
    let dependency_revision_v2 =
        json_string_field(&stdout(&dependency_write_v2), "topic_revision_id");

    let before_refresh = run_real_json(&repo, &["status", "--session", "session_writer_agent"]);
    assert_success(&before_refresh);
    assert!(stdout(&before_refresh).contains(&format!(
        "\"available_newer_topic_heads\":{{\"topic_dependency\":\"{dependency_revision_v2}\"}}"
    )));
    assert!(stdout(&before_refresh).contains(&dependency_revision_v1));

    let manual = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_writer_agent",
            "--policy",
            "manual",
        ],
    );
    assert_success(&manual);
    let manual_stdout = stdout(&manual);
    assert!(manual_stdout.contains("\"changed\":true"));
    assert!(manual_stdout.contains("\"refresh_policy\":\"manual\""));
    assert!(manual_stdout.contains(&dependency_revision_v2));
    let manual_generation = json_string_field(&manual_stdout, "session_generation_id");
    let refreshed_read = run_real_json(
        &repo,
        &[
            "read",
            "dependency.txt",
            "--session",
            "session_writer_agent",
        ],
    );
    assert_success(&refreshed_read);
    assert!(stdout(&refreshed_read).contains("dependency v2"));

    let follow = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_writer_agent",
            "--policy",
            "follow",
        ],
    );
    assert_success(&follow);
    assert!(stdout(&follow).contains("\"refresh_policy\":\"follow\""));
    let follow_generation = json_string_field(&stdout(&follow), "session_generation_id");
    assert_ne!(manual_generation, follow_generation);
    let follow_noop = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_writer_agent",
            "--policy",
            "follow",
        ],
    );
    assert_success(&follow_noop);
    assert!(stdout(&follow_noop).contains("\"changed\":false"));
    assert!(stdout(&follow_noop).contains("policy_and_frontier_unchanged"));
    assert_eq!(
        json_string_field(&stdout(&follow_noop), "session_generation_id"),
        follow_generation
    );

    let dependency_hash_v2 = json_string_field(&stdout(&refreshed_read), "content_hash");
    let dependency_v3 = repo.write_file("dependency-v3.txt", "dependency v3\n");
    let dependency_write_v3 = run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "dependency.txt".as_ref(),
            "--session".as_ref(),
            "session_dependency_agent".as_ref(),
            "--expect-hash".as_ref(),
            dependency_hash_v2.as_ref(),
            "--content-file".as_ref(),
            dependency_v3.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    );
    assert_success(&dependency_write_v3);
    let dependency_revision_v3 =
        json_string_field(&stdout(&dependency_write_v3), "topic_revision_id");
    let own = repo.write_file("writer-after.txt", "writer context\n");
    let own_write = run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "writer-after.txt".as_ref(),
            "--session".as_ref(),
            "session_writer_agent".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            own.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    );
    assert_success(&own_write);
    let own_stdout = stdout(&own_write);
    assert!(own_stdout.contains(&dependency_revision_v2));
    assert!(!own_stdout.contains(&dependency_revision_v3));
    assert!(own_stdout.contains("\"refresh_policy\":\"follow\""));
    let own_operation_id = json_string_field(&own_stdout, "operation_transaction_id");
    let operation_record = fs::read_to_string(
        repo.path()
            .join(".sunlight/operations")
            .join(format!("{own_operation_id}.json")),
    )
    .unwrap();
    assert!(operation_record.contains(&dependency_revision_v2));

    let none = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_writer_agent",
            "--policy",
            "none",
        ],
    );
    assert_success(&none);
    assert!(stdout(&none).contains("\"refresh_policy\":\"none\""));
    assert!(stdout(&none).contains(&dependency_revision_v2));
    assert!(stdout(&none).contains(&dependency_revision_v3));
    let inspect = run_real_json(&repo, &["inspect", "session:session_writer_agent"]);
    assert_success(&inspect);
    assert!(stdout(&inspect).contains("\"refresh_policy\":\"none\""));
    assert!(stdout(&inspect).contains(&dependency_revision_v2));
    assert!(stdout(&inspect).contains(&dependency_revision_v3));
}

#[test]
fn no_fixture_session_refresh_conflict_keeps_last_good_generation_and_evidence() {
    let repo = TestRepo::new("real-session-refresh-conflict");
    init_local_git_repo(&repo);
    assert_success(&run_real_json(&repo, &["init"]));
    for (topic, actor) in [("right", "right-agent"), ("left", "left-agent")] {
        assert_success(&run_real_json(
            &repo,
            &["topic", "create", topic, "--display-name", topic],
        ));
        assert_success(&run_real_json(
            &repo,
            &[
                "session",
                "start",
                "--topic",
                topic,
                "--view",
                "view_base_0001",
                "--actor",
                actor,
            ],
        ));
    }
    let right = repo.write_file("right-shared.txt", "right version\n");
    assert_success(&run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "shared.txt".as_ref(),
            "--session".as_ref(),
            "session_right_agent".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            right.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    ));
    let left_note = repo.write_file("left-note.txt", "left note\n");
    assert_success(&run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "left.txt".as_ref(),
            "--session".as_ref(),
            "session_left_agent".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            left_note.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    ));
    let head = run_real_json(&repo, &["view", "resolve"]);
    assert_success(&head);
    let head_view = resolved_view_id(&stdout(&head));
    assert_success(&run_real_json(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "right",
            "--view",
            &head_view,
            "--actor",
            "review-agent",
        ],
    ));
    let before = run_real_json(&repo, &["status", "--session", "session_review_agent"]);
    assert_success(&before);
    let generation_before = json_string_field(&stdout(&before), "session_generation_id");
    let view_before = json_string_field(&stdout(&before), "resolved_view_id");

    let left_shared = repo.write_file("left-shared.txt", "left version\n");
    assert_success(&run_real_json_os(
        &repo,
        &[
            "write".as_ref(),
            "shared.txt".as_ref(),
            "--session".as_ref(),
            "session_left_agent".as_ref(),
            "--expect-hash".as_ref(),
            "new".as_ref(),
            "--content-file".as_ref(),
            left_shared.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
        ],
    ));
    let blocked = run_real_json(
        &repo,
        &[
            "session",
            "refresh",
            "session_review_agent",
            "--policy",
            "manual",
        ],
    );
    assert_failure(&blocked);
    let blocked_stdout = stdout(&blocked);
    assert!(blocked_stdout.contains("\"code\":\"session_refresh_blocked\""));
    assert!(blocked_stdout.contains("\"candidate_resolved_view_id\":"));
    assert!(blocked_stdout.contains("\"conflict_ids\":[\"conflict_shared_txt_0001\"]"));
    let after = run_real_json(&repo, &["status", "--session", "session_review_agent"]);
    assert_success(&after);
    assert_eq!(
        json_string_field(&stdout(&after), "session_generation_id"),
        generation_before
    );
    assert_eq!(
        json_string_field(&stdout(&after), "resolved_view_id"),
        view_before
    );
    let evidence = run_real_json(&repo, &["inspect", "conflict:conflict_shared_txt_0001"]);
    assert_success(&evidence);
    assert!(stdout(&evidence).contains("same_artifact_conflict"));
    assert!(repo
        .path()
        .join(".sunlight/conflicts/conflict_shared_txt_0001.json")
        .is_file());
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn inspect_fixture_operation(repo: &Path, operation_id: &str) -> String {
    let output = sun()
        .arg("inspect")
        .arg(format!("operation:{operation_id}"))
        .arg("--fixture")
        .arg("basic-app")
        .arg("--json")
        .current_dir(repo)
        .output()
        .expect("sun inspect operation should run");
    assert_success(&output);
    stdout(&output)
}

fn tree_hash(stdout: &str) -> String {
    json_string_field(stdout, "tree_hash")
}

fn resolved_view_id(stdout: &str) -> String {
    json_string_field(stdout, "resolved_view_id")
}

fn json_string_field(stdout: &str, field: &str) -> String {
    let needle = format!("\"{field}\":\"");
    let start = stdout
        .find(&needle)
        .unwrap_or_else(|| panic!("missing field `{field}` in stdout:\n{stdout}"))
        + needle.len();
    let end = stdout[start..]
        .find('"')
        .unwrap_or_else(|| panic!("unterminated field `{field}` in stdout:\n{stdout}"));
    stdout[start..start + end].to_string()
}

fn quarantine_record_path(projection_root: &Path) -> PathBuf {
    projection_root
        .join(".sunlight")
        .join("quarantine")
        .join("projections")
        .join("projection_exec_auth_profile_0001")
        .join("execution_store_integrity_failed.json")
}

fn assert_projection_quarantine_record_json(record: &str) {
    assert!(record.contains("\"privacy_class\":\"local_only\""));
    assert!(record.contains("\"state\":\"quarantined\""));
    assert!(record.contains("\"reason\":\"store_integrity_mismatch\""));
    assert!(record.contains("\"reason_code\":\"execution_store_integrity_failed\""));
    assert!(record.contains("\"projection_id\":\"projection_exec_auth_profile_0001\""));
    assert!(record.contains("\"resolved_view_id\":\"view_base_0001\""));
    assert!(record.contains("\"root_ref\":{\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\",\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\"}"));
    assert!(record.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(record.contains("\"manifest_ref\":\"objects/projection-manifests/sha256/"));
    assert!(record.contains("\"manifest_digest\":\"sha256:"));
    assert!(record
        .contains("\"quarantine_refs\":{\"cache\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(record.contains("\"native_error\":\"native-error:execution_store_integrity_failed:projection_exec_auth_profile_0001\""));
    assert!(record.contains("\"projection\":\"projection:projection_exec_auth_profile_0001\""));
    assert!(
        record.contains("\"provenance\":{\"created_from_content_tree\":\"tree_fixture_base_0001\"")
    );
    assert!(record.contains("\"repository_id\":\"repo_fixture_basic_app\""));
    assert!(record.contains("\"source_truth\":\"immutable_store_manifest\""));
    assert!(record.contains("\"local_filesystem_source_truth\":false"));
    assert!(record.contains("\"durable_record\":\"local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json\""));
    assert!(record.contains("\"cache_reuse_allowed\":false"));
    assert!(record.contains("\"cache_invalidation_reason\":\"execution_store_integrity_failed\""));
}

fn resolve_fixture_view_id(repo: &Path, include: &str) -> String {
    let output = sun()
        .arg("view")
        .arg("resolve")
        .arg("--fixture")
        .arg("basic-app")
        .arg("--include")
        .arg(include)
        .arg("--json")
        .current_dir(repo)
        .output()
        .expect("sun view resolve should run");
    assert_success(&output);
    resolved_view_id(&stdout(&output))
}

fn init_local_git_repo(repo: &TestRepo) -> String {
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.name", "Sun CLI Test"]);
    git(
        repo.path(),
        &["config", "user.email", "sun-cli-test@example.invalid"],
    );
    repo.write_file("base.txt", "base\n");
    git(repo.path(), &["add", "base.txt"]);
    git(repo.path(), &["commit", "-m", "base"]);
    git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string()
}

fn run_real_json(repo: &TestRepo, args: &[&str]) -> Output {
    let mut command = sun();
    command.args(args).arg("--json").current_dir(repo.path());
    command.output().expect("real Sunlight command should run")
}

fn run_real_json_os(repo: &TestRepo, args: &[&OsStr]) -> Output {
    let mut command = sun();
    command.args(args).arg("--json").current_dir(repo.path());
    command.output().expect("real Sunlight command should run")
}

fn start_native_session(repo: &TestRepo, slug: &str) {
    let init = sun()
        .arg("init")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun init should run");
    assert_success(&init);

    let topic = sun()
        .arg("topic")
        .arg("create")
        .arg(slug)
        .arg("--display-name")
        .arg("Native Test")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun topic create should run");
    assert_success(&topic);

    let topic_id = format!("topic_{}", slug.replace('-', "_"));
    let session = sun()
        .arg("session")
        .arg("start")
        .arg("--topic")
        .arg(&topic_id)
        .arg("--view")
        .arg("view_base_0001")
        .arg("--actor")
        .arg("agent-a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun session start should run");
    assert_success(&session);
}

#[cfg(windows)]
fn windows_isolation_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[cfg(windows)]
fn sunlight_appcontainer_profile_dirs() -> Vec<String> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let mut profiles = fs::read_dir(PathBuf::from(local_app_data).join("Packages"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.to_ascii_lowercase().starts_with("sunlight-"))
        .collect::<Vec<_>>();
    profiles.sort();
    profiles
}

fn create_real_base_checkpoint(repo: &TestRepo) -> String {
    let checkpoint = sun()
        .arg("checkpoint")
        .arg("create")
        .arg("--view")
        .arg("view_base_0001")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real base checkpoint should run");
    assert_success(&checkpoint);
    json_string_field(&stdout(&checkpoint), "checkpoint_id")
}

fn create_real_compat_projection(repo: &TestRepo) -> (String, PathBuf, String) {
    create_real_compat_projection_at(repo, ".sunlight/projections")
}

fn create_real_compat_projection_at(
    repo: &TestRepo,
    managed_root: &str,
) -> (String, PathBuf, String) {
    let output = sun()
        .arg("compat")
        .arg("project")
        .arg("--session")
        .arg("session_agent_a")
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat project should run");
    assert_success(&output);
    let stdout = stdout(&output);
    assert_valid_json(&stdout);
    let projection_id = json_string_field(&stdout, "projection_id");
    let generation = json_string_field(&stdout, "session_generation_id");
    let root = repo
        .path()
        .join(managed_root)
        .join("compat")
        .join(&projection_id)
        .join("root");
    assert!(root.is_dir());
    (projection_id, root, generation)
}

fn materialize_real_projection_copy(
    repo: &TestRepo,
    view_id: &str,
    purpose: &str,
    projection_root: &Path,
) -> Output {
    sun()
        .args([
            "project",
            "materialize",
            "--view",
            view_id,
            "--purpose",
            purpose,
            "--strategy",
            "copy",
            "--no-copy-fallback",
            "--projection-root",
        ])
        .arg(projection_root)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("real projection materialization should run")
}

fn projection_cache_entry_roots(repo: &TestRepo) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(repo.path().join(".sunlight/cache/projections/v1"))
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with(".staging-")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn make_test_file_writable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    #[cfg(windows)]
    permissions.set_readonly(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o644);
    }
    fs::set_permissions(path, permissions).unwrap();
}

fn set_projection_default_root(repo: &TestRepo, root: &str) {
    let config_path = repo.path().join(".sunlight/config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    let start = "default_root = \"";
    let value_start = config.find(start).unwrap() + start.len();
    let value_end = config[value_start..].find('"').unwrap() + value_start;
    let mut updated = config;
    updated.replace_range(value_start..value_end, root);
    fs::write(config_path, updated).unwrap();
}

fn real_compat_diff(repo: &TestRepo, projection_id: &str) -> String {
    let output = sun()
        .arg("compat")
        .arg("diff")
        .arg("--projection")
        .arg(projection_id)
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat diff should run");
    assert_success(&output);
    let stdout = stdout(&output);
    assert_valid_json(&stdout);
    stdout
}

fn real_compat_import(
    repo: &TestRepo,
    projection_id: &str,
    candidate_id: &str,
    session_generation_id: Option<&str>,
) -> Output {
    let mut command = sun();
    command
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg(projection_id)
        .arg("--candidate")
        .arg(candidate_id);
    if let Some(generation) = session_generation_id {
        command.arg("--session-generation").arg(generation);
    }
    let output = command
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .expect("sun compat import should run");
    assert_valid_json(&stdout(&output));
    output
}

fn real_compat_import_many(
    repo: &TestRepo,
    projection_id: &str,
    candidate_ids: &[String],
    session_generation_id: Option<&str>,
) -> Output {
    let mut command = sun();
    command
        .arg("compat")
        .arg("import")
        .arg("--projection")
        .arg(projection_id);
    for candidate_id in candidate_ids {
        command.arg("--candidate").arg(candidate_id);
    }
    if let Some(generation) = session_generation_id {
        command.arg("--session-generation").arg(generation);
    }
    let output = command
        .arg("--json")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_valid_json(&stdout(&output));
    output
}

fn candidate_id_for_path(diff_json: &str, path: &str) -> String {
    let path_marker = format!("\"path\":\"{}\"", path.replace('\\', "\\\\"));
    let path_index = diff_json
        .find(&path_marker)
        .unwrap_or_else(|| panic!("candidate path missing: {path}"));
    let prefix = &diff_json[..path_index];
    let id_marker = "\"candidate_delta_id\":\"";
    let id_start = prefix
        .rfind(id_marker)
        .unwrap_or_else(|| panic!("candidate id missing before path: {path}"))
        + id_marker.len();
    let remainder = &diff_json[id_start..];
    remainder[..remainder.find('"').expect("candidate id closing quote")].to_string()
}

fn assert_valid_json(value: &str) {
    parse_json_record(value.as_bytes()).unwrap_or_else(|error| {
        panic!("expected valid JSON, got {error}: {value}");
    });
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git {}: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        stdout(&output),
        stderr(&output)
    );
    stdout(&output)
}

fn git_ref_exists(repo: &Path, git_ref: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", git_ref])
        .output()
        .expect("git rev-parse should run")
        .status
        .success()
}

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "sun-cli-json-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write_file(&self, name: &str, body: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, body).unwrap();
        path
    }
}

fn write_nested_file(root: &Path, relative_path: &str, body: &str) {
    let path = root.join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn auth_trim_guard_patch() -> &'static str {
    "--- a/src/auth.ts\n+++ b/src/auth.ts\n@@ -1,3 +1,4 @@\n export function login(email: string) {\n-  return email.trim().toLowerCase();\n+  const normalized = email.trim().toLowerCase();\n+  return normalized;\n }\n"
}

fn bad_auth_patch() -> &'static str {
    "--- a/src/auth.ts\n+++ b/src/auth.ts\n@@ -1,3 +1,3 @@\n export function login(email: string) {\n-  return email.toUpperCase();\n+  return email.trim().toLowerCase();\n }\n"
}
