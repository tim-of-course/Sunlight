use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn sun() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sun"))
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
    assert!(stdout.contains("\"strategy\":\"copy\""));
    assert!(stdout.contains("\"root_ref\":{\"value\":\"local://.sunlight/projections/execution/projection_exec_auth_profile_0001\",\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"}"));
    assert!(stdout.contains("\"tree_identity\":{\"kind\":\"SingleRepoTree\",\"repository_id\":\"repo_fixture_basic_app\",\"tree_hash\":\"tree_fixture_"));
    assert!(stdout.contains("\"cache_key\":\"projection-cache:repo_fixture_basic_app:"));
    assert!(stdout.contains(":execution:copy:read_only_source_private_outputs\""));
    assert!(stdout.contains("\"retention_state\":\"active\""));
    assert!(stdout.contains("\"policy\":{\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\",\"operation_semantics_version\":\"file_ops_v1\",\"writable_policy\":\"read_only_source_private_outputs\",\"store_integrity_policy\":\"verify_before_reuse\",\"privacy_class\":\"local_only\"}"));
    assert!(stdout.contains("\"record_type\":\"projection\""));
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
    assert!(stdout.contains("\"baseline_manifest_ref\":\"objects/projection-baselines/repo_fixture_basic_app/"));
    assert!(stdout.contains("\"writable_policy\":\"writable_with_explicit_import\""));
    assert!(stdout.contains("\"store_integrity_policy\":\"verify_on_import\""));
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
    assert!(
        stdout.contains("\"message\":\"resolved view has conflicts and cannot be projected\"")
    );
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
fn inspect_json_fixture_projection_returns_local_only_metadata() {
    let repo = TestRepo::new("inspect-fixture-projection");

    let output = sun()
        .arg("inspect")
        .arg("projection:projection_exec_auth_profile_0001")
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
