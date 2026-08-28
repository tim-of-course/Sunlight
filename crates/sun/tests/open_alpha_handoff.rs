use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sunlight_core::repo_state::RealRepoState;

#[cfg(windows)]
const PYTHON: &str = "python";
#[cfg(not(windows))]
const PYTHON: &str = "python3";

#[test]
fn open_alpha_oa05_exact_checkpoint_exports_to_safe_buildable_git_handoff() {
    let temp = TempDir::new("sun-oa05-git-handoff");
    let repo = temp.path().join("repository");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join(".gitignore"), "__pycache__/\n").unwrap();
    fs::write(repo.join("README.md"), "# OA-05 sacrificial repository\n").unwrap();
    let failing_source = "def answer():\n    return 41\n";
    let passing_source = failing_source.replace("41", "42");
    fs::write(repo.join("app.py"), failing_source).unwrap();
    fs::write(
        repo.join("test_app.py"),
        concat!(
            "import unittest\n",
            "import app\n\n",
            "class AnswerTest(unittest.TestCase):\n",
            "    def test_answer_is_exact(self):\n",
            "        self.assertEqual(app.answer(), 42)\n\n",
            "if __name__ == '__main__':\n",
            "    unittest.main()\n",
        ),
    )
    .unwrap();
    git_ok(&repo, &["init", "--quiet"]);
    git_ok(&repo, &["config", "user.name", "Sun OA-05 Test"]);
    git_ok(&repo, &["config", "user.email", "sun-oa05@example.invalid"]);
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "--quiet", "-m", "failing base"]);
    let base_commit = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    assert!(git(&repo, &["remote"]).trim().is_empty());

    sun_ok(&repo, &["init"]);

    // Required red evidence: the focused behavior fails on the exact imported base view.
    let failing = sun_ok(
        &repo,
        &[
            "run",
            "--view",
            "view_base_0001",
            "--",
            PYTHON,
            "-m",
            "unittest",
            "-v",
        ],
    );
    assert_eq!(failing["data"]["result"]["status"], "fail");

    sun_ok(
        &repo,
        &[
            "topic",
            "create",
            "fix-answer",
            "--display-name",
            "Fix answer",
        ],
    );
    let session = sun_ok(
        &repo,
        &[
            "session",
            "start",
            "--topic",
            "topic_fix_answer",
            "--view",
            "view_base_0001",
            "--actor",
            "oa05-agent",
        ],
    );
    let session_id = string_at(&session, &["data", "ids", "session_id"]);
    let read = sun_ok(&repo, &["read", "app.py", "--session", &session_id]);
    let expect_hash = string_at(&read, &["data", "artifacts", "0", "content_hash"]);
    let content_file = temp.path().join("passing-app.py");
    fs::write(&content_file, &passing_source).unwrap();
    let write = sun_ok(
        &repo,
        &[
            "write",
            "app.py",
            "--session",
            &session_id,
            "--expect-hash",
            &expect_hash,
            "--content-file",
            content_file.to_str().unwrap(),
            "--classification",
            "source",
        ],
    );
    let view_id = string_at(&write, &["data", "view", "resolved_view_id"]);
    let revision_id = string_at(&write, &["data", "ids", "topic_revision_id"]);
    sun_ok(
        &repo,
        &[
            "topic",
            "complete",
            "--topic",
            "topic_fix_answer",
            "--revision",
            &revision_id,
            "--session",
            &session_id,
            "--summary",
            "Focused behavior and full build pass",
        ],
    );

    // Required green evidence plus full-repository validation on the same exact view/tree.
    let focused = sun_ok(
        &repo,
        &[
            "run", "--view", &view_id, "--", PYTHON, "-m", "unittest", "-v",
        ],
    );
    assert_eq!(
        focused["data"]["result"]["status"], "pass",
        "focused execution: {focused}"
    );
    let full_build = sun_ok(
        &repo,
        &[
            "run",
            "--view",
            &view_id,
            "--",
            PYTHON,
            "-m",
            "compileall",
            "-q",
            ".",
        ],
    );
    assert_eq!(full_build["data"]["result"]["status"], "pass");
    assert_eq!(full_build["data"]["view"]["resolved_view_id"], view_id);
    let build_execution_id = string_at(&full_build, &["data", "execution_id"]);
    let checkpoint = sun_ok(
        &repo,
        &[
            "checkpoint",
            "create",
            "--view",
            &view_id,
            "--execution",
            &build_execution_id,
        ],
    );
    let checkpoint_id = string_at(&checkpoint, &["data", "checkpoint_id"]);
    assert_eq!(
        checkpoint["data"]["checkpoint"]["tree_identity"],
        full_build["data"]["tree_identity"]
    );

    // Dirty index/worktree/ignored data must remain untouched and cannot become export content.
    fs::write(repo.join("README.md"), "dirty user README\n").unwrap();
    fs::write(repo.join("dirty-staged.txt"), "staged user data\n").unwrap();
    git_ok(&repo, &["add", "dirty-staged.txt"]);
    fs::create_dir_all(repo.join("__pycache__")).unwrap();
    fs::write(repo.join("__pycache__/ignored-leak.pyc"), "ignored\n").unwrap();
    let dirty_before = user_git_status(&repo);
    let readme_before = fs::read(repo.join("README.md")).unwrap();

    let branch = "refs/heads/sunlight/oa05-handoff";
    let export = sun_ok(
        &repo,
        &[
            "git",
            "export",
            "--checkpoint",
            &checkpoint_id,
            "--branch",
            branch,
            "--execute-local",
        ],
    );
    let commit_id = export["data"]["git_commit_ids"][0]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(git(&repo, &["rev-parse", branch]).trim(), commit_id);
    assert_eq!(user_git_status(&repo), dirty_before);
    assert_eq!(fs::read(repo.join("README.md")).unwrap(), readme_before);

    let exported_paths = git(&repo, &["ls-tree", "-r", "--name-only", &commit_id]);
    assert!(exported_paths.lines().any(|path| path == "app.py"));
    assert!(!exported_paths
        .lines()
        .any(|path| path == "dirty-staged.txt"));
    assert!(!exported_paths
        .lines()
        .any(|path| path.starts_with("__pycache__/")));
    assert!(!exported_paths
        .lines()
        .any(|path| path.starts_with(".sunlight/")));
    let checkpoint_state = RealRepoState::load(&repo).unwrap();
    let frozen = checkpoint_state
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
        .unwrap();
    let mut frozen_paths = frozen
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    frozen_paths.sort_unstable();
    assert_eq!(exported_paths.lines().collect::<Vec<_>>(), frozen_paths);
    for entry in frozen.entries.iter().filter(|entry| !entry.tombstone) {
        assert_eq!(
            git_bytes(&repo, &["show", &format!("{commit_id}:{}", entry.path)]),
            entry.bytes,
            "exported bytes differ from checkpoint for {}",
            entry.path
        );
    }
    assert_eq!(
        git(&repo, &["show", &format!("{commit_id}:app.py")]),
        passing_source
    );

    // An existing unrelated branch is never moved.
    let unrelated = git(
        &repo,
        &["commit-tree", "HEAD^{tree}", "-m", "unrelated collision"],
    )
    .trim()
    .to_string();
    let collision = "refs/heads/sunlight/oa05-collision";
    git_ok(&repo, &["update-ref", collision, &unrelated]);
    let rejected = sun_result(
        &repo,
        &[
            "git",
            "export",
            "--checkpoint",
            &checkpoint_id,
            "--branch",
            collision,
            "--execute-local",
        ],
        false,
    );
    assert_eq!(rejected["error"]["code"], "export_target_ref_conflict");
    assert_eq!(git(&repo, &["rev-parse", collision]).trim(), unrelated);

    // Retrying the same exact handoff is idempotent and does not duplicate native mappings.
    let retry = sun_ok(
        &repo,
        &[
            "git",
            "export",
            "--checkpoint",
            &checkpoint_id,
            "--branch",
            branch,
            "--execute-local",
        ],
    );
    assert_eq!(retry["data"]["git_commit_ids"][0], commit_id);
    let native = RealRepoState::load(&repo).unwrap();
    assert_eq!(
        native
            .export_maps
            .iter()
            .filter(|map| map.checkpoint_id == checkpoint_id && map.git_ref == branch)
            .count(),
        1
    );

    let diff = git(&repo, &["diff", &format!("{base_commit}..{commit_id}")]);
    assert!(diff.contains("-    return 41"));
    assert!(diff.contains("+    return 42"));

    // A normal Git-only consumer can inspect and validate the exported commit without Sunlight.
    let consumer = temp.path().join("git-consumer");
    git_ok(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            consumer.to_str().unwrap(),
            &commit_id,
        ],
    );
    command_ok(
        Command::new(PYTHON)
            .args(["-m", "unittest", "-v"])
            .current_dir(&consumer),
    );
    command_ok(
        Command::new(PYTHON)
            .args(["-m", "compileall", "-q", "."])
            .current_dir(&consumer),
    );
    assert!(git(&repo, &["remote"]).trim().is_empty());
    println!(
        "OA-05 evidence {}",
        serde_json::json!({
            "base_commit": base_commit,
            "failing_execution_id": failing["data"]["execution_id"],
            "focused_execution_id": focused["data"]["execution_id"],
            "full_execution_id": build_execution_id,
            "view_id": view_id,
            "checkpoint_id": checkpoint_id,
            "tree_identity": checkpoint["data"]["checkpoint"]["tree_identity"],
            "export_map_id": export["data"]["ids"]["export_map_id"],
            "exported_commit": commit_id,
            "branch": branch,
            "consumer_validation": "pass"
        })
    );
}

fn sun_ok(repo: &Path, args: &[&str]) -> Value {
    sun_result(repo, args, true)
}

fn sun_result(repo: &Path, args: &[&str], success: bool) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_sun"))
        .args(args)
        .arg("--json")
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "sun {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn string_at(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for component in path {
        current = if let Ok(index) = component.parse::<usize>() {
            &current[index]
        } else {
            &current[*component]
        };
    }
    current.as_str().unwrap().to_string()
}

fn git_ok(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git(repo: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

fn git_bytes(repo: &Path, args: &[&str]) -> Vec<u8> {
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
    output.stdout
}

fn user_git_status(repo: &Path) -> String {
    git(repo, &["status", "--porcelain=v1", "--untracked-files=all"])
        .lines()
        .filter(|line| !line.trim_start().starts_with("?? .sunlight/"))
        .map(|line| format!("{line}\n"))
        .collect()
}

fn command_ok(command: &mut Command) {
    let Output {
        status,
        stdout,
        stderr,
    } = command.output().unwrap();
    assert!(
        status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
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
