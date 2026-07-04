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
