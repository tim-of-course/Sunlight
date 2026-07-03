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
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
