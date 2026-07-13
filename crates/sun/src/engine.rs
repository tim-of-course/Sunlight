use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
}

pub fn execute_engine(context: &EngineContext, request: EngineRequest) -> EngineResponse {
    let EngineCommandInput::Arguments(arguments) = request.command;
    let output = Rc::new(RefCell::new(OutputBuffer::new(request.max_stdout_bytes)));
    let command = CommandContext {
        json: request.output_format == EngineOutputFormat::Json,
        args: arguments,
        repo_root: context.repository_root.clone(),
        cancellation: Arc::clone(&context.cancellation),
        output: Rc::clone(&output),
    };
    let result = run(&command);
    let json = command.json;
    drop(command);
    let (emitted, stdout_overflowed) = Rc::try_unwrap(output)
        .expect("engine output has no remaining owners")
        .into_inner()
        .into_parts();
    match result {
        Ok(()) => EngineResponse {
            success: true,
            stdout: emitted,
            stderr: String::new(),
            stdout_overflowed,
            stderr_overflowed: false,
        },
        Err(error) if json => {
            use std::fmt::Write as _;
            let mut stdout = OutputBuffer::new(request.max_stdout_bytes);
            writeln!(stdout, "{}", failure_envelope(&error))
                .expect("writing engine output cannot fail");
            let (stdout, stdout_overflowed) = stdout.into_parts();
            EngineResponse {
                success: false,
                stdout,
                stderr: String::new(),
                stdout_overflowed,
                stderr_overflowed: false,
            }
        }
        Err(error) => {
            use std::fmt::Write as _;
            let mut stderr = OutputBuffer::new(request.max_stderr_bytes);
            writeln!(stderr, "sun: {}", error.message)
                .expect("writing engine diagnostics cannot fail");
            let (stderr, stderr_overflowed) = stderr.into_parts();
            EngineResponse {
                success: false,
                stdout: emitted,
                stderr,
                stdout_overflowed,
                stderr_overflowed,
            }
        }
    }
}
