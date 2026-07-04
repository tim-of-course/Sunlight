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
    assert!(stdout
        .contains("\"message\":\"selected compatibility candidate is cache or build output\""));
    assert!(stdout.contains("\"candidate_delta_ids\":[\"compat_delta_dist_bundle_0001\"]"));
    assert!(stdout
        .contains("\"reason\":\"cache, build, and ignored candidates are blocked by default\""));
    assert!(stdout.contains("\"imported_artifacts\":[]"));
    assert!(stdout.contains("\"operation_transaction_id\":null"));
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
