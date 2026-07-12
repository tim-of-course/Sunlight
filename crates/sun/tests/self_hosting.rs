use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use sunlight_core::records::{parse_json_record, JsonValue};

#[test]
fn self_hosting_real_repository_acceptance() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("sun crate should be inside the repository");
    let source_status_before = git(source, &["status", "--porcelain"]);
    let source_head = git(source, &["rev-parse", "HEAD"]);
    let temp = TempDir::new("sun-self-hosting");
    let clone = temp.path().join("repository");
    let inputs = temp.path().join("inputs");
    fs::create_dir_all(&inputs).unwrap();

    let cloned = Command::new("git")
        .args(["clone", "--local", "--no-hardlinks", "--quiet"])
        .arg(source)
        .arg(&clone)
        .output()
        .expect("git clone --local should run");
    assert_output_success(&cloned, "git clone --local");
    git(&clone, &["config", "user.name", "Sun Self-hosting Test"]);
    git(
        &clone,
        &["config", "user.email", "sun-self-hosting@example.invalid"],
    );
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), source_head);
    let tracked = git(&clone, &["ls-files"]);
    let tracked_files = tracked.lines().count();
    let tracked_bytes: u64 = tracked
        .lines()
        .map(|path| fs::metadata(clone.join(path)).unwrap().len())
        .sum();
    assert!(tracked_files >= 25, "expected a production-sized file set");
    assert!(
        tracked_bytes >= 200_000,
        "expected realistic repository bytes"
    );

    let init = sun_json(&clone, ["init", "--json"]);
    assert_eq!(string(&init, &["data", "command"]), "repository.init");
    let repository_id = string(&init, &["data", "repository_id"]).to_string();
    assert!(repository_id.starts_with("repo-"));
    assert!(clone.join(".sunlight/records/native-state.json").is_file());
    assert!(clone
        .join(".sunlight/checkpoints/checkpoint_base_0001.json")
        .is_file());
    let repository_status = sun_json(&clone, ["status", "--json"]);
    let ingested_artifacts = number(
        &repository_status,
        &["data", "repository", "artifact_count"],
    );
    assert!(ingested_artifacts >= 50);
    assert!(ingested_artifacts <= tracked_files as u64);

    create_topic(&clone, "self-hosting-native", "Self-hosting Native");
    create_topic(&clone, "self-hosting-peer", "Self-hosting Peer");
    start_session(&clone, "self-hosting-native", "self-hosting-native-agent");
    start_session(&clone, "self-hosting-peer", "self-hosting-peer-agent");
    let native_session = "session_self_hosting_native_agent";
    let peer_session = "session_self_hosting_peer_agent";

    let read = sun_json(
        &clone,
        ["read", "Cargo.toml", "--session", native_session, "--json"],
    );
    let cargo_hash =
        string_at_path(&read, &["data", "artifacts"], 0, &["content_hash"]).to_string();
    let base_cargo = string(&read, &["data", "content", "bytes"]).to_string();
    assert!(base_cargo.contains("[workspace]"));
    let list = sun_json(
        &clone,
        ["list", "crates", "--session", native_session, "--json"],
    );
    assert!(array(&list, &["data", "artifacts"]).len() >= 10);
    let search = sun_json(
        &clone,
        [
            "search",
            "sunlight-core",
            "--session",
            native_session,
            "--json",
        ],
    );
    assert!(!array(&search, &["data", "matches"]).is_empty());

    let patch_file = inputs.join("cargo.patch");
    fs::write(
        &patch_file,
        "--- a/Cargo.toml\n+++ b/Cargo.toml\n@@\n-[workspace]\n+# persisted self-hosting native patch\n+[workspace]\n",
    )
    .unwrap();
    let patched = sun_json_os(
        &clone,
        [
            "patch".as_ref(),
            "Cargo.toml".as_ref(),
            "--session".as_ref(),
            native_session.as_ref(),
            "--expect-hash".as_ref(),
            cargo_hash.as_ref(),
            "--patch-file".as_ref(),
            patch_file.as_os_str(),
            "--json".as_ref(),
        ],
    );
    assert_eq!(string(&patched, &["data", "command"]), "artifact.patch");
    let patched_hash = mutation_after_hash(&patched).to_string();

    let note = inputs.join("native-note.txt");
    fs::write(&note, b"native persisted note\n").unwrap();
    let note_write = write_artifact(
        &clone,
        native_session,
        "self-hosting/native-note.txt",
        &note,
        "new",
    );
    let note_hash = mutation_after_hash(&note_write).to_string();
    let movable = inputs.join("movable.txt");
    fs::write(&movable, b"move provenance bytes\n").unwrap();
    let movable_write = write_artifact(
        &clone,
        native_session,
        "self-hosting/to-move.txt",
        &movable,
        "new",
    );
    let movable_hash = mutation_after_hash(&movable_write).to_string();
    let moved = sun_json(
        &clone,
        [
            "move",
            "self-hosting/to-move.txt",
            "self-hosting/moved.txt",
            "--session",
            native_session,
            "--expect-hash",
            &movable_hash,
            "--json",
        ],
    );
    assert_eq!(string(&moved, &["data", "command"]), "artifact.move");

    let disposable = inputs.join("disposable.txt");
    fs::write(&disposable, b"delete provenance bytes\n").unwrap();
    let disposable_write = write_artifact(
        &clone,
        native_session,
        "self-hosting/delete-me.txt",
        &disposable,
        "new",
    );
    let disposable_hash = mutation_after_hash(&disposable_write).to_string();
    let deleted = sun_json(
        &clone,
        [
            "delete",
            "self-hosting/delete-me.txt",
            "--session",
            native_session,
            "--expect-hash",
            &disposable_hash,
            "--json",
        ],
    );
    assert_eq!(string(&deleted, &["data", "command"]), "artifact.delete");
    let metadata = sun_json(
        &clone,
        [
            "metadata",
            "set",
            "self-hosting/native-note.txt",
            "--classification",
            "source",
            "--session",
            native_session,
            "--expect-hash",
            &note_hash,
            "--json",
        ],
    );
    assert_eq!(
        string(&metadata, &["data", "command"]),
        "artifact.metadata_set"
    );
    let native_revision = string(&metadata, &["data", "ids", "topic_revision_id"]).to_string();

    for response in [
        &patched,
        &note_write,
        &movable_write,
        &moved,
        &disposable_write,
        &deleted,
        &metadata,
    ] {
        let operation = string(response, &["data", "ids", "operation_transaction_id"]);
        let inspected = sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("operation:{operation}"),
                "--json".into(),
            ],
        );
        assert_eq!(
            string(&inspected, &["data", "command"]),
            "inspect.operation"
        );
        assert_eq!(
            string(&inspected, &["data", "operation", "session_id"]),
            native_session
        );
        assert!(
            string(&inspected, &["data", "operation", "authored_context_id"]).starts_with("view_")
        );
        assert_eq!(
            string(&inspected, &["data", "operation", "topic_id"]),
            "topic_self_hosting_native"
        );
        assert!(
            string(&inspected, &["data", "operation", "topic_revision_id"])
                .starts_with("rev_self_hosting_native_")
        );
    }

    let peer_read = sun_json(
        &clone,
        ["read", "Cargo.toml", "--session", peer_session, "--json"],
    );
    assert_eq!(
        string_at_path(&peer_read, &["data", "artifacts"], 0, &["content_hash"]),
        cargo_hash
    );
    let peer_cargo = inputs.join("peer-Cargo.toml");
    fs::write(
        &peer_cargo,
        base_cargo.replacen("[workspace]", "# peer conflicting patch\n[workspace]", 1),
    )
    .unwrap();
    let peer_write = write_artifact(&clone, peer_session, "Cargo.toml", &peer_cargo, &cargo_hash);
    let peer_revision = string(&peer_write, &["data", "ids", "topic_revision_id"]).to_string();

    let native_selection = format!("topic_self_hosting_native:{native_revision}");
    let resolved = sun_json_owned(
        &clone,
        vec![
            "view".into(),
            "resolve".into(),
            "--base".into(),
            "checkpoint_base_0001".into(),
            "--include".into(),
            native_selection.clone(),
            "--json".into(),
        ],
    );
    assert!(array(&resolved, &["data", "conflict_ids"]).is_empty());
    let native_view = string(&resolved, &["data", "resolved_view_id"]).to_string();

    let peer_selection = format!("topic_self_hosting_peer:{peer_revision}");
    let conflict_a = resolve_conflict(&clone, &native_selection, &peer_selection);
    let conflict_b = resolve_conflict(&clone, &peer_selection, &native_selection);
    assert_eq!(
        value(&conflict_a, &["data", "conflict_ids"]),
        value(&conflict_b, &["data", "conflict_ids"])
    );
    assert_eq!(
        value(&conflict_a, &["data", "topic_frontier"]),
        value(&conflict_b, &["data", "topic_frontier"])
    );
    assert_eq!(
        value(&conflict_a, &["data", "resolved_view_id"]),
        value(&conflict_b, &["data", "resolved_view_id"])
    );
    assert!(matches!(
        value(&conflict_a, &["data", "tree_identity"]),
        JsonValue::Null
    ));
    let conflict_id = string_at(array(&conflict_a, &["data", "conflict_ids"]), 0).to_string();
    let conflict_inspect = sun_json_owned(
        &clone,
        vec![
            "inspect".into(),
            format!("conflict:{conflict_id}"),
            "--json".into(),
        ],
    );
    assert_eq!(
        string(&conflict_inspect, &["data", "conflict", "kind"]),
        "same_artifact_conflict"
    );

    fs::write(
        clone.join("Cargo.toml"),
        b"manager clone working-tree corruption must not become truth\n",
    )
    .unwrap();
    let materialized_root = temp.path().join("persisted-projection");
    let materialized = sun_json_os(
        &clone,
        [
            "project".as_ref(),
            "materialize".as_ref(),
            "--view".as_ref(),
            native_view.as_ref(),
            "--purpose".as_ref(),
            "inspection".as_ref(),
            "--projection-root".as_ref(),
            materialized_root.as_os_str(),
            "--json".as_ref(),
        ],
    );
    let line_ending = if base_cargo.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let persisted_cargo = base_cargo.replacen(
        "[workspace]",
        &format!("# persisted self-hosting native patch{line_ending}[workspace]"),
        1,
    );
    assert_eq!(
        fs::read_to_string(materialized_root.join("Cargo.toml")).unwrap(),
        persisted_cargo
    );
    assert_ne!(
        fs::read(materialized_root.join("Cargo.toml")).unwrap(),
        fs::read(clone.join("Cargo.toml")).unwrap()
    );
    let inspection_projection = string(&materialized, &["data", "projection_id"]).to_string();

    let compat = sun_json(
        &clone,
        ["compat", "project", "--session", native_session, "--json"],
    );
    let compat_id = string(&compat, &["data", "projection_id"]).to_string();
    let generation = string(&compat, &["data", "ids", "session_generation_id"]).to_string();
    let compat_root = clone
        .join(".sunlight/projections/compat")
        .join(&compat_id)
        .join("root");
    fs::write(
        compat_root.join("self-hosting/native-note.txt"),
        b"compatibility modified note\n",
    )
    .unwrap();
    fs::write(
        compat_root.join("self-hosting/compat-added.txt"),
        b"compatibility added bytes\n",
    )
    .unwrap();
    fs::remove_file(compat_root.join("self-hosting/moved.txt")).unwrap();
    fs::rename(
        compat_root.join("scripts/check-manager-scratchpad.ps1"),
        compat_root.join("scripts/check-scratchpad-selfhost.ps1"),
    )
    .unwrap();
    let diff = sun_json_owned(
        &clone,
        vec![
            "compat".into(),
            "diff".into(),
            "--projection".into(),
            compat_id.clone(),
            "--json".into(),
        ],
    );
    let candidates = array(&diff, &["data", "candidates"]);
    assert_eq!(candidates.len(), 4);
    let mut candidate_ids = Vec::new();
    for path in [
        "self-hosting/native-note.txt",
        "self-hosting/compat-added.txt",
        "self-hosting/moved.txt",
        "scripts/check-scratchpad-selfhost.ps1",
    ] {
        candidate_ids.push(candidate_for_path(candidates, path));
    }
    let rename_candidate = candidates
        .iter()
        .find(|candidate| string(candidate, &["path"]) == "scripts/check-scratchpad-selfhost.ps1")
        .unwrap();
    assert_eq!(string(rename_candidate, &["operation_kind"]), "move");
    assert_eq!(
        string(rename_candidate, &["source_path"]),
        "scripts/check-manager-scratchpad.ps1"
    );

    let mut import_args = vec![
        "compat".to_string(),
        "import".to_string(),
        "--projection".to_string(),
        compat_id.clone(),
    ];
    for id in &candidate_ids {
        import_args.push("--candidate".into());
        import_args.push(id.clone());
    }
    import_args.extend(["--session-generation".into(), generation, "--json".into()]);
    let imported = sun_json_owned(&clone, import_args);
    assert_eq!(number(&imported, &["data", "selected_delta_count"]), 4);
    let imported_artifacts = array(&imported, &["data", "imported_artifacts"]);
    assert_eq!(imported_artifacts.len(), 4);
    let import_operation =
        string(&imported, &["data", "ids", "operation_transaction_id"]).to_string();
    assert_eq!(
        string(&imported, &["data", "operation", "id"]),
        import_operation
    );
    assert_eq!(
        array(
            &imported,
            &["data", "operation", "mutation_payload", "selected_deltas"]
        )
        .len(),
        4
    );
    let imported_view = string(&imported, &["data", "resolved_view_id"]).to_string();
    let imported_read = sun_json(
        &clone,
        [
            "read",
            "self-hosting/compat-added.txt",
            "--session",
            native_session,
            "--json",
        ],
    );
    assert_eq!(
        string(&imported_read, &["data", "content", "bytes"]),
        "compatibility added bytes\n"
    );

    let run = run_execution(&clone, &imported_view);
    assert_eq!(string(&run, &["data", "result", "status"]), "pass");
    assert_eq!(
        string(&run, &["data", "result", "termination_reason"]),
        "command_exit"
    );
    assert_eq!(
        string(&run, &["data", "runtime_policy", "network"]),
        "not_enforced"
    );
    assert_eq!(
        string(
            &run,
            &["data", "runtime_policy", "enforcement", "process_tree"]
        ),
        if cfg!(windows) {
            "windows_job_object_kill_on_close"
        } else {
            "not_enforced"
        }
    );
    let execution_id = string(&run, &["data", "execution_id"]).to_string();
    assert!(array(&run, &["data", "promotion_candidates"])
        .iter()
        .any(|output| string(output, &["output_path"]) == "self-hosting/execution-output.txt"));
    let execution_status = sun_json_owned(
        &clone,
        vec![
            "status".into(),
            "--execution".into(),
            execution_id.clone(),
            "--json".into(),
        ],
    );
    assert_eq!(
        string(&execution_status, &["data", "promotion_status"]),
        "promotion_required"
    );
    let execution_inspect = sun_json_owned(
        &clone,
        vec![
            "inspect".into(),
            format!("execution:{execution_id}"),
            "--json".into(),
        ],
    );
    assert_eq!(
        string(&execution_inspect, &["data", "execution", "source_truth"]),
        "sunlight_persisted_execution"
    );
    assert_eq!(
        string(
            &execution_inspect,
            &["data", "execution", "result", "status"]
        ),
        "pass"
    );
    let promoted = sun_json_owned(
        &clone,
        vec![
            "execution".into(),
            "promote-output".into(),
            execution_id.clone(),
            "--path".into(),
            "self-hosting/execution-output.txt".into(),
            "--session".into(),
            native_session.into(),
            "--classification".into(),
            "source_like_delta".into(),
            "--json".into(),
        ],
    );
    assert_eq!(
        string(
            &promoted,
            &["data", "operation", "execution_provenance", "execution_id"]
        ),
        execution_id
    );
    let promoted_view = string(&promoted, &["data", "view", "resolved_view_id"]).to_string();
    let promoted_read = sun_json(
        &clone,
        [
            "read",
            "self-hosting/execution-output.txt",
            "--session",
            native_session,
            "--json",
        ],
    );
    assert_eq!(
        string(&promoted_read, &["data", "content", "bytes"]),
        "bounded execution output\n"
    );

    let checkpoint = sun_json_owned(
        &clone,
        vec![
            "checkpoint".into(),
            "create".into(),
            "--view".into(),
            promoted_view,
            "--json".into(),
        ],
    );
    let checkpoint_id = string(&checkpoint, &["data", "checkpoint_id"]).to_string();
    let branch = "refs/heads/sunlight/self-hosting-acceptance";
    let policy = sun_json_owned(
        &clone,
        vec![
            "policy".into(),
            "check-export".into(),
            "--checkpoint".into(),
            checkpoint_id.clone(),
            "--branch".into(),
            branch.into(),
            "--json".into(),
        ],
    );
    let report_id = string(&policy, &["data", "validation_report_id"]).to_string();
    assert!(bool_value(&policy, &["data", "validation_report", "ok"]));
    let report_path = clone
        .join(".sunlight/records/validation-reports")
        .join(format!("{report_id}.json"));
    assert!(report_path.is_file());
    parse_json_record(&fs::read(&report_path).unwrap()).unwrap();
    let explained = sun_json_owned(
        &clone,
        vec![
            "policy".into(),
            "explain".into(),
            report_id.clone(),
            "--json".into(),
        ],
    );
    assert_eq!(string(&explained, &["data", "command"]), "policy.explain");
    assert_eq!(
        string(&explained, &["data", "validation_report_id"]),
        report_id
    );

    let exported = sun_json_owned(
        &clone,
        vec![
            "git".into(),
            "export".into(),
            "--checkpoint".into(),
            checkpoint_id.clone(),
            "--branch".into(),
            branch.into(),
            "--execute-local".into(),
            "--json".into(),
        ],
    );
    assert_eq!(string(&exported, &["data", "lifecycle_state"]), "exported");
    assert_eq!(
        git_show(&clone, branch, "Cargo.toml"),
        persisted_cargo.as_bytes()
    );
    assert_eq!(
        git_show(&clone, branch, "self-hosting/native-note.txt"),
        b"compatibility modified note\n"
    );
    assert_eq!(
        git_show(&clone, branch, "self-hosting/compat-added.txt"),
        b"compatibility added bytes\n"
    );
    assert_eq!(
        git_show(&clone, branch, "self-hosting/execution-output.txt"),
        b"bounded execution output\n"
    );
    assert!(!git_path_exists(&clone, branch, "self-hosting/moved.txt"));
    assert!(git_path_exists(
        &clone,
        branch,
        "scripts/check-scratchpad-selfhost.ps1"
    ));
    assert!(!git_path_exists(
        &clone,
        branch,
        "scripts/check-manager-scratchpad.ps1"
    ));

    let export_map_id = string(&exported, &["data", "ids", "export_map_id"]).to_string();
    let selectors = [
        sun_json(
            &clone,
            ["status", "--topic", "self-hosting-native", "--json"],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                "topic:self-hosting-native".into(),
                "--json".into(),
            ],
        ),
        sun_json(&clone, ["status", "--session", native_session, "--json"]),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("session:{native_session}"),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                "artifact:Cargo.toml".into(),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("operation:{import_operation}"),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "status".into(),
                "--compat-import".into(),
                import_operation.clone(),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "status".into(),
                "--projection".into(),
                inspection_projection.clone(),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("projection:{compat_id}"),
                "--json".into(),
            ],
        ),
        execution_status,
        execution_inspect,
        sun_json_owned(
            &clone,
            vec![
                "status".into(),
                "--checkpoint".into(),
                checkpoint_id.clone(),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("checkpoint:{checkpoint_id}"),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "status".into(),
                "--export".into(),
                export_map_id.clone(),
                "--json".into(),
            ],
        ),
        sun_json_owned(
            &clone,
            vec![
                "inspect".into(),
                format!("export:{export_map_id}"),
                "--json".into(),
            ],
        ),
        explained,
    ];
    for selector in selectors {
        assert_eq!(bool_value(&selector, &["ok"]), true);
        assert!(string(&selector, &["data", "command"]).contains('.'));
    }

    assert_eq!(
        git(source, &["status", "--porcelain"]),
        source_status_before
    );
    assert_eq!(git(source, &["rev-parse", "HEAD"]), source_head);
    assert_eq!(string(&init, &["data", "repository_id"]), repository_id);
    assert_ne!(patched_hash, cargo_hash);
}

fn create_topic(repo: &Path, slug: &str, display: &str) {
    let response = sun_json(
        repo,
        ["topic", "create", slug, "--display-name", display, "--json"],
    );
    assert_eq!(string(&response, &["data", "topic", "slug"]), slug);
}

fn start_session(repo: &Path, topic: &str, actor: &str) {
    let response = sun_json(
        repo,
        [
            "session",
            "start",
            "--topic",
            topic,
            "--view",
            "view_base_0001",
            "--actor",
            actor,
            "--json",
        ],
    );
    assert_eq!(string(&response, &["data", "session", "actor_id"]), actor);
}

fn write_artifact(
    repo: &Path,
    session: &str,
    path: &str,
    content_file: &Path,
    expected_hash: &str,
) -> JsonValue {
    sun_json_os(
        repo,
        [
            "write".as_ref(),
            path.as_ref(),
            "--session".as_ref(),
            session.as_ref(),
            "--expect-hash".as_ref(),
            expected_hash.as_ref(),
            "--content-file".as_ref(),
            content_file.as_os_str(),
            "--classification".as_ref(),
            "source".as_ref(),
            "--json".as_ref(),
        ],
    )
}

fn resolve_conflict(repo: &Path, first: &str, second: &str) -> JsonValue {
    let selections = format!("{first},{second}");
    sun_json_owned(
        repo,
        vec![
            "view".into(),
            "resolve".into(),
            "--base".into(),
            "checkpoint_base_0001".into(),
            "--include".into(),
            selections,
            "--json".into(),
        ],
    )
}

#[cfg(windows)]
fn run_execution(repo: &Path, view_id: &str) -> JsonValue {
    sun_json(
        repo,
        [
            "run",
            "--view",
            view_id,
            "--json",
            "--",
            "powershell.exe",
            "-NoProfile",
            "-Command",
            "New-Item -ItemType Directory -Force self-hosting | Out-Null; [IO.File]::WriteAllText('self-hosting/execution-output.txt', \"bounded execution output`n\")",
        ],
    )
}

#[cfg(not(windows))]
fn run_execution(repo: &Path, view_id: &str) -> JsonValue {
    sun_json(
        repo,
        [
            "run",
            "--view",
            view_id,
            "--json",
            "--",
            "sh",
            "-c",
            "mkdir -p self-hosting && printf 'bounded execution output\\n' > self-hosting/execution-output.txt",
        ],
    )
}

fn candidate_for_path(candidates: &[JsonValue], path: &str) -> String {
    candidates
        .iter()
        .find(|candidate| string(candidate, &["path"]) == path)
        .map(|candidate| string(candidate, &["candidate_delta_id"]).to_string())
        .unwrap_or_else(|| panic!("missing compatibility candidate for {path}"))
}

fn sun_json<const N: usize>(repo: &Path, args: [&str; N]) -> JsonValue {
    sun_json_os(repo, args.map(OsStr::new))
}

fn sun_json_owned(repo: &Path, args: Vec<String>) -> JsonValue {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sun"));
    command.args(args).current_dir(repo);
    parse_success(command.output().expect("sun command should run"))
}

fn sun_json_os<const N: usize>(repo: &Path, args: [&OsStr; N]) -> JsonValue {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sun"));
    command.args(args).current_dir(repo);
    parse_success(command.output().expect("sun command should run"))
}

fn parse_success(output: Output) -> JsonValue {
    assert_output_success(&output, "sun command");
    let stdout = String::from_utf8(output.stdout).expect("sun JSON should be UTF-8");
    let parsed = parse_json_record(stdout.as_bytes())
        .unwrap_or_else(|error| panic!("invalid sun JSON: {error}\n{stdout}"));
    assert_eq!(bool_value(&parsed, &["ok"]), true, "{stdout}");
    parsed
}

fn assert_output_success(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "{command} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn value<'a>(json: &'a JsonValue, path: &[&str]) -> &'a JsonValue {
    path.iter().fold(json, |value, key| match value {
        JsonValue::Object(object) => object
            .get(*key)
            .unwrap_or_else(|| panic!("missing JSON field {key} in path {path:?}: {value:?}")),
        _ => panic!("non-object JSON value in path {path:?}"),
    })
}

fn string<'a>(json: &'a JsonValue, path: &[&str]) -> &'a str {
    match value(json, path) {
        JsonValue::String(value) => value,
        other => panic!("expected string at {path:?}, got {other:?}"),
    }
}

fn string_at(values: &[JsonValue], index: usize) -> &str {
    match &values[index] {
        JsonValue::String(value) => value,
        other => panic!("expected string at array index {index}, got {other:?}"),
    }
}

fn string_at_path<'a>(
    json: &'a JsonValue,
    array_path: &[&str],
    index: usize,
    item_path: &[&str],
) -> &'a str {
    string(&array(json, array_path)[index], item_path)
}

fn mutation_after_hash(json: &JsonValue) -> &str {
    string_at_path(json, &["data", "artifacts"], 0, &["after_hash"])
}

fn number(json: &JsonValue, path: &[&str]) -> u64 {
    match value(json, path) {
        JsonValue::Number(value) => value.parse().unwrap(),
        other => panic!("expected number at {path:?}, got {other:?}"),
    }
}

fn bool_value(json: &JsonValue, path: &[&str]) -> bool {
    match value(json, path) {
        JsonValue::Bool(value) => *value,
        other => panic!("expected bool at {path:?}, got {other:?}"),
    }
}

fn array<'a>(json: &'a JsonValue, path: &[&str]) -> &'a [JsonValue] {
    match value(json, path) {
        JsonValue::Array(value) => value,
        other => panic!("expected array at {path:?}, got {other:?}"),
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git command should run");
    assert_output_success(&output, "git command");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn git_show(repo: &Path, git_ref: &str, path: &str) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show", &format!("{git_ref}:{path}")])
        .output()
        .expect("git show should run");
    assert_output_success(&output, "git show");
    output.stdout
}

fn git_path_exists(repo: &Path, git_ref: &str, path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e", &format!("{git_ref}:{path}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git cat-file should run")
        .success()
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        make_tree_writable(&self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_tree_writable(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            make_tree_writable(&path);
        }
        if let Ok(metadata) = fs::metadata(&path) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
            }
        }
    }
}
