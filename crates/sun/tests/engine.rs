use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sun::{execute_engine, EngineCommandInput, EngineContext, EngineOutputFormat, EngineRequest};

#[test]
fn engine_output_limits_bound_retained_bytes_and_report_overflow() {
    let temp = TempDir::new("sun-engine-output-limit");
    let engine = EngineContext::new(temp.path()).unwrap();

    let stdout_limit = 17;
    let response = execute_engine(
        &engine,
        EngineRequest {
            command: EngineCommandInput::Arguments(Vec::new()),
            output_format: EngineOutputFormat::Human,
            max_stdout_bytes: Some(stdout_limit),
            max_stderr_bytes: Some(5),
        },
    );
    assert!(response.success);
    assert!(response.stdout_overflowed);
    assert!(!response.stderr_overflowed);
    assert!(response.stdout.len() <= stdout_limit);
    assert!(response.stderr.is_empty());

    let stderr_limit = 5;
    let response = execute_engine(
        &engine,
        EngineRequest {
            command: EngineCommandInput::Arguments(vec!["not-a-command".to_string()]),
            output_format: EngineOutputFormat::Human,
            max_stdout_bytes: Some(stdout_limit),
            max_stderr_bytes: Some(stderr_limit),
        },
    );
    assert!(!response.success);
    assert!(!response.stdout_overflowed);
    assert!(response.stderr_overflowed);
    assert!(response.stderr.len() <= stderr_limit);
}

#[test]
fn explicit_engine_roots_keep_repositories_and_command_files_separate() {
    let temp = TempDir::new("sun-explicit-engine-roots");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("README.md"), "first repository\n").unwrap();
    fs::write(second.join("README.md"), "second repository\n").unwrap();

    assert!(EngineContext::new(Path::new(".")).is_err());
    let first_engine = EngineContext::new(&first).unwrap();
    let second_engine = EngineContext::new(&second).unwrap();
    let first_init = run(&first_engine, &["init"]);
    let second_init = run(&second_engine, &["init"]);
    assert_ne!(
        first_init["data"]["repository_id"],
        second_init["data"]["repository_id"]
    );

    for engine in [&first_engine, &second_engine] {
        run(
            engine,
            &["topic", "create", "rooted", "--display-name", "Rooted"],
        );
        run(
            engine,
            &[
                "session",
                "start",
                "--topic",
                "rooted",
                "--view",
                "view_base_0001",
                "--actor",
                "root-agent",
            ],
        );
    }

    let first_read = run(
        &first_engine,
        &["read", "README.md", "--session", "session_root_agent"],
    );
    let second_read = run(
        &second_engine,
        &["read", "README.md", "--session", "session_root_agent"],
    );
    assert_eq!(first_read["data"]["content"]["bytes"], "first repository\n");
    assert_eq!(
        second_read["data"]["content"]["bytes"],
        "second repository\n"
    );

    fs::write(first.join("content.tmp"), "first-only mutation\n").unwrap();
    let first_write = run(
        &first_engine,
        &[
            "write",
            "rooted.txt",
            "--session",
            "session_root_agent",
            "--expect-hash",
            "new",
            "--content-file",
            "content.tmp",
            "--classification",
            "source",
        ],
    );
    assert_eq!(first_write["data"]["command"], "artifact.write");
    let second_list = run(&second_engine, &["list", "--session", "session_root_agent"]);
    assert!(second_list["data"]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|artifact| artifact["path"] != "rooted.txt"));
}

fn run(engine: &EngineContext, args: &[&str]) -> Value {
    let response = execute_engine(
        engine,
        EngineRequest {
            command: EngineCommandInput::Arguments(
                args.iter().map(|argument| argument.to_string()).collect(),
            ),
            output_format: EngineOutputFormat::Json,
            max_stdout_bytes: None,
            max_stderr_bytes: None,
        },
    );
    assert!(
        response.success,
        "engine {args:?}: stdout={} stderr={}",
        response.stdout, response.stderr
    );
    serde_json::from_str(&response.stdout).unwrap()
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
