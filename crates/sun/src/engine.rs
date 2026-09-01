use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::{failure_envelope, run, CommandContext, OutputBuffer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineOutputFormat {
    Json,
    Human,
}

#[derive(Debug, Clone)]
pub enum EngineCommandInput {
    Arguments(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub command: EngineCommandInput,
    pub output_format: EngineOutputFormat,
    pub max_stdout_bytes: Option<usize>,
    pub max_stderr_bytes: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct EngineContext {
    pub(crate) repository_root: PathBuf,
    pub(crate) cancellation: Arc<AtomicBool>,
}

impl EngineContext {
    pub fn new(repository_root: impl AsRef<Path>) -> Result<Self, String> {
        let requested = repository_root.as_ref();
        if !requested.is_absolute() {
            return Err(format!(
                "engine repository root must be absolute: `{}`",
                requested.display()
            ));
        }
        let repository_root = fs::canonicalize(requested).map_err(|error| {
            format!(
                "cannot canonicalize repository `{}`: {error}",
                requested.display()
            )
        })?;
        if !repository_root.is_dir() {
            return Err(format!(
                "repository `{}` is not a directory",
                repository_root.display()
            ));
        }
        Ok(Self {
            repository_root,
            cancellation: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    pub(crate) fn with_cancellation(mut self, cancellation: Arc<AtomicBool>) -> Self {
        self.cancellation = cancellation;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineResponse {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_overflowed: bool,
    pub stderr_overflowed: bool,
    pub concurrency_retry_count: usize,
    pub writer_wait_ms: u128,
}

const CONCURRENCY_RETRY_LIMIT: usize = 8;
const CONCURRENCY_RETRY_MAX_DELAY_MS: u64 = 64;
pub(crate) const WRITER_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn execute_engine(context: &EngineContext, request: EngineRequest) -> EngineResponse {
    execute_engine_with_writer_wait_timeout(context, request, WRITER_WAIT_TIMEOUT)
}

pub(crate) fn execute_engine_with_writer_wait_timeout(
    context: &EngineContext,
    request: EngineRequest,
    writer_wait_timeout: Duration,
) -> EngineResponse {
    let (mut response, writer_wait) = sunlight_core::repo_state::with_repository_writer_lock_wait(
        writer_wait_timeout,
        Arc::clone(&context.cancellation),
        || execute_engine_with_writer_wait(context, request),
    );
    response.writer_wait_ms = writer_wait.as_millis();
    response
}

fn execute_engine_with_writer_wait(
    context: &EngineContext,
    request: EngineRequest,
) -> EngineResponse {
    let EngineRequest {
        command,
        output_format,
        max_stdout_bytes,
        max_stderr_bytes,
    } = request;
    let EngineCommandInput::Arguments(arguments) = command;
    let json = output_format == EngineOutputFormat::Json;
    let retry_allowed = command_allows_automatic_concurrency_retry(&arguments);
    let mut retry_count = 0usize;

    loop {
        let output = Rc::new(RefCell::new(OutputBuffer::new(max_stdout_bytes)));
        let command = CommandContext {
            json,
            args: arguments.clone(),
            repo_root: context.repository_root.clone(),
            cancellation: Arc::clone(&context.cancellation),
            output: Rc::clone(&output),
        };
        let result = run(&command);
        drop(command);
        let (emitted, stdout_overflowed) = Rc::try_unwrap(output)
            .expect("engine output has no remaining owners")
            .into_inner()
            .into_parts();

        if let Err(error) = &result {
            if retry_allowed
                && is_transient_concurrency_error(error.code)
                && retry_count < CONCURRENCY_RETRY_LIMIT
                && !context.cancellation.load(Ordering::Acquire)
            {
                let delay_ms = concurrency_retry_delay_ms(retry_count);
                retry_count += 1;
                thread::sleep(Duration::from_millis(delay_ms));
                continue;
            }
        }

        return match result {
            Ok(()) => EngineResponse {
                success: true,
                stdout: emitted,
                stderr: String::new(),
                stdout_overflowed,
                stderr_overflowed: false,
                concurrency_retry_count: retry_count,
                writer_wait_ms: 0,
            },
            Err(error) if json => {
                use std::fmt::Write as _;
                let mut stdout = OutputBuffer::new(max_stdout_bytes);
                writeln!(stdout, "{}", failure_envelope(&error))
                    .expect("writing engine output cannot fail");
                let (stdout, stdout_overflowed) = stdout.into_parts();
                EngineResponse {
                    success: false,
                    stdout,
                    stderr: String::new(),
                    stdout_overflowed,
                    stderr_overflowed: false,
                    concurrency_retry_count: retry_count,
                    writer_wait_ms: 0,
                }
            }
            Err(error) => {
                use std::fmt::Write as _;
                let mut stderr = OutputBuffer::new(max_stderr_bytes);
                writeln!(stderr, "sun: {}", error.message)
                    .expect("writing engine diagnostics cannot fail");
                let (stderr, stderr_overflowed) = stderr.into_parts();
                EngineResponse {
                    success: false,
                    stdout: emitted,
                    stderr,
                    stdout_overflowed,
                    stderr_overflowed,
                    concurrency_retry_count: retry_count,
                    writer_wait_ms: 0,
                }
            }
        };
    }
}

fn concurrency_retry_delay_ms(retry_count: usize) -> u64 {
    (1u64 << retry_count.min(6)).min(CONCURRENCY_RETRY_MAX_DELAY_MS)
}

fn is_transient_concurrency_error(code: &str) -> bool {
    code == "concurrent_state_update"
}

fn command_allows_automatic_concurrency_retry(arguments: &[String]) -> bool {
    match arguments {
        [command, ..]
            if matches!(
                command.as_str(),
                "init"
                    | "read"
                    | "list"
                    | "search"
                    | "patch"
                    | "write"
                    | "move"
                    | "delete"
                    | "status"
                    | "inspect"
            ) =>
        {
            true
        }
        [scope, command, ..] => matches!(
            (scope.as_str(), command.as_str()),
            ("topic", "create" | "complete")
                | ("session", "start" | "refresh")
                | ("view", "resolve")
                | ("metadata", "set")
                | ("execution", "promote-output")
                | ("checkpoint", "create")
                | ("policy", "check-export" | "check-commit" | "explain")
                | ("compat", "diff" | "import")
                | ("projection", "quarantine-cleanup")
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{command_allows_automatic_concurrency_retry, concurrency_retry_delay_ms};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn retries_native_commands_but_never_replays_external_side_effects() {
        for command in [
            args(&["write", "src/lib.rs"]),
            args(&["topic", "create", "change"]),
            args(&["view", "resolve"]),
            args(&["status"]),
            args(&["compat", "import"]),
        ] {
            assert!(command_allows_automatic_concurrency_retry(&command));
        }
        for command in [
            args(&["run", "--", "cargo", "test"]),
            args(&["git", "export"]),
            args(&["project", "materialize"]),
            args(&["compat", "capture"]),
        ] {
            assert!(!command_allows_automatic_concurrency_retry(&command));
        }
    }

    #[test]
    fn eight_retries_remain_a_short_bounded_fallback() {
        let delays = (0..8).map(concurrency_retry_delay_ms).collect::<Vec<_>>();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 32, 64, 64]);
        assert_eq!(delays.iter().sum::<u64>(), 191);
    }
}
