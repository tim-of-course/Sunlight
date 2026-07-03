use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use sunlight_core::artifacts::{
    ArtifactIoError, InMemoryArtifactStore, ListResponse, ReadResponse, SearchResponse,
    SessionView, SessionVisibleArtifactView,
};
use sunlight_core::repository::{
    init_repository, RepositoryConfig, CURRENT_STORAGE_SCHEMA_VERSION,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                println!("{}", failure_envelope(&error));
            } else {
                eprintln!("sun: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Clone)]
struct CommandContext {
    json: bool,
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct CliError {
    code: &'static str,
    message: String,
    details: Vec<(&'static str, String)>,
}

impl CliError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    fn with_detail(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.details.push((key, value.into()));
        self
    }
}

fn run(args: Vec<String>) -> Result<(), CliError> {
    let json = args.iter().any(|arg| arg == "--json");
    let args = args
        .into_iter()
        .filter(|arg| arg != "--json")
        .collect::<Vec<_>>();
    let ctx = CommandContext { json, args };

    match ctx.args.as_slice() {
        [] => {
            print_help();
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
            Ok(())
        }
        [command] if command == "init" => init(&ctx, PathBuf::from(".")),
        [command, flag, path] if command == "init" && flag == "--repo" => {
            init(&ctx, PathBuf::from(path))
        }
        [command, flag] if command == "init" && flag == "--help" => {
            print_init_help();
            Ok(())
        }
        [command, ..] if command == "init" => {
            Err(invalid_request("usage: sun init [--repo <path>]"))
        }
        [scope, command, ..] if scope == "topic" && command == "create" => {
            Err(unimplemented_command(
                "topic.create",
                "sun topic create is parsed, but topic records are not persisted yet",
            ))
        }
        [scope, command, ..] if scope == "session" && command == "start" => {
            Err(unimplemented_command(
                "session.start",
                "sun session start is parsed, but session records are not persisted yet",
            ))
        }
        [command, ..] if command == "read" => artifact_read(&ctx),
        [command, ..] if command == "list" => artifact_list(&ctx),
        [command, ..] if command == "search" => artifact_search(&ctx),
        [command] if command == "status" => status(&ctx),
        [command, flag, _] if command == "status" && (flag == "--session" || flag == "--topic") => {
            status(&ctx)
        }
        [command, ..] if command == "status" => Err(invalid_request(
            "usage: sun status [--session <session>|--topic <topic>]",
        )),
        [command, ..] if command == "inspect" => inspect(&ctx),
        [command, ..] => Err(invalid_request(format!("unknown command `{command}`"))
            .with_detail("command", command.clone())),
    }
}

fn init(ctx: &CommandContext, repo_root: PathBuf) -> Result<(), CliError> {
    let report = init_repository(&repo_root).map_err(|error| {
        invalid_request(error.to_string()).with_detail("command", "repository.init")
    })?;

    if ctx.json {
        println!(
            "{}",
            init_success_envelope(
                &report.repository_id,
                &report.repo_root.display().to_string(),
                &report.sunlight_dir.display().to_string(),
                report.created_config,
                report.created_gitignore,
                report.created_directories.len(),
            )
        );
    } else {
        println!("initialized Sunlight repository");
        println!("repo_root = {}", report.repo_root.display());
        println!("sunlight_dir = {}", report.sunlight_dir.display());
        println!("repository_id = {}", report.repository_id);
        println!("created_config = {}", report.created_config);
        println!("created_gitignore = {}", report.created_gitignore);
        println!("created_directories = {}", report.created_directories.len());
    }

    Ok(())
}

fn artifact_read(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "read", 1, 1)?;
    let store = fixture_store(&options.fixture)?;
    let response = store
        .read(&options.session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", read_success_envelope(&response));
    } else {
        print!("{}", response.content.bytes);
    }

    Ok(())
}

fn artifact_list(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "list", 0, 1)?;
    let prefix = options.operands.first().map(String::as_str).unwrap_or("");
    let store = fixture_store(&options.fixture)?;
    let response = store
        .list(&options.session_id, prefix)
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", list_success_envelope(&response));
    } else {
        for artifact in response.artifacts {
            println!("{}", artifact.path);
        }
    }

    Ok(())
}

fn artifact_search(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "search", 1, 1)?;
    let store = fixture_store(&options.fixture)?;
    let response = store
        .search(&options.session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", search_success_envelope(&response));
    } else {
        for item in response.matches {
            println!("{}:{}:{}", item.path, item.line, item.snippet);
        }
    }

    Ok(())
}

fn status(ctx: &CommandContext) -> Result<(), CliError> {
    let config = require_repository_config(".")?;
    let command = match ctx.args.as_slice() {
        [_, flag, _] if flag == "--session" => "status.session",
        [_, flag, _] if flag == "--topic" => "status.topic",
        _ => "status.repository",
    };

    Err(unimplemented_command(
        command,
        format!(
            "sun status is parsed for repository {}, but native status records are not persisted yet",
            config.repository_id
        ),
    ))
}

fn inspect(_ctx: &CommandContext) -> Result<(), CliError> {
    require_repository_config(".")?;
    Err(CliError::new(
        "object_not_found",
        "Sunlight object was not found",
    ))
}

fn require_repository_config(repo_root: impl Into<PathBuf>) -> Result<RepositoryConfig, CliError> {
    let repo_root = repo_root.into();
    let config_path = repo_root.join(".sunlight").join("config.toml");
    if !config_path.is_file() {
        return Err(CliError::new(
            "not_initialized",
            "Sunlight repository is not initialized",
        ));
    }

    let body = fs::read_to_string(&config_path).map_err(|error| {
        CliError::new("not_initialized", "Sunlight repository is not initialized")
            .with_detail("path", config_path.display().to_string())
            .with_detail("source", error.to_string())
    })?;

    RepositoryConfig::from_toml(&body, config_path.clone()).map_err(|error| {
        CliError::new("not_initialized", "Sunlight repository is not initialized")
            .with_detail("path", config_path.display().to_string())
            .with_detail("source", error.to_string())
    })
}

fn invalid_request(message: impl Into<String>) -> CliError {
    CliError::new("invalid_request", message)
}

fn unimplemented_command(command: &'static str, message: impl Into<String>) -> CliError {
    CliError::new("invalid_request", message).with_detail("command", command)
}

#[derive(Debug)]
struct ArtifactCommandOptions {
    session_id: String,
    fixture: String,
    operands: Vec<String>,
}

fn parse_artifact_options(
    ctx: &CommandContext,
    command: &'static str,
    min_operands: usize,
    max_operands: usize,
) -> Result<ArtifactCommandOptions, CliError> {
    let mut session_id = None;
    let mut fixture = None;
    let mut operands = Vec::new();
    let mut args = ctx.args.iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --session <session>"))
                })?;
                session_id = Some(value.clone());
            }
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
                })?;
                fixture = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun {command}"
                )));
            }
            value => operands.push(value.to_string()),
        }
    }

    if operands.len() < min_operands || operands.len() > max_operands {
        return Err(invalid_request(artifact_usage(command)));
    }

    let session_id = session_id.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --session <session>"))
    })?;
    let fixture = fixture.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
    })?;

    Ok(ArtifactCommandOptions {
        session_id,
        fixture,
        operands,
    })
}

fn artifact_usage(command: &str) -> String {
    match command {
        "read" => "usage: sun read <path-or-artifact-id> --session <session> --fixture basic-app",
        "list" => "usage: sun list [path-prefix] --session <session> --fixture basic-app",
        "search" => "usage: sun search <query> --session <session> --fixture basic-app",
        _ => "usage: sun <artifact-command> --session <session> --fixture basic-app",
    }
    .to_string()
}

fn fixture_store(fixture: &str) -> Result<InMemoryArtifactStore, CliError> {
    match fixture {
        "basic-app" => Ok(InMemoryArtifactStore::fixture_basic_app()),
        _ => Err(invalid_request(format!("unknown fixture `{fixture}`"))
            .with_detail("fixture", fixture.to_string())),
    }
}

fn artifact_error(error: ArtifactIoError) -> CliError {
    let message = match &error {
        ArtifactIoError::PathPolicyViolation { .. } => {
            "path is rejected by repository path policy".to_string()
        }
        ArtifactIoError::PathNotFound { path, .. } => format!("path `{path}` was not found"),
        ArtifactIoError::SessionNotFound { session_id } => {
            format!("session `{session_id}` was not found")
        }
        _ => error.to_string(),
    };

    let mut cli_error = CliError::new(error.code(), message);
    match error {
        ArtifactIoError::PathPolicyViolation {
            path,
            policy_id,
            reason,
            session_generation_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("policy_id", policy_id)
                .with_detail("reason", reason.as_str())
                .with_detail("session_generation_id", session_generation_id);
        }
        ArtifactIoError::PathNotFound {
            path,
            session_generation_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("session_generation_id", session_generation_id);
        }
        ArtifactIoError::SessionNotFound { session_id } => {
            cli_error = cli_error.with_detail("session_id", session_id);
        }
        ArtifactIoError::MissingContent { content_ref } => {
            cli_error = cli_error.with_detail("content_ref", content_ref);
        }
        ArtifactIoError::NonUtf8Content { path } => {
            cli_error = cli_error.with_detail("path", path);
        }
        ArtifactIoError::PreconditionFailed {
            failed_precondition,
            path,
            artifact_id,
            expected,
            actual,
            session_generation_id,
            resolved_view_id,
        } => {
            cli_error = cli_error
                .with_detail("failed_precondition", failed_precondition)
                .with_detail("path", path)
                .with_detail("expected", expected)
                .with_detail("session_generation_id", session_generation_id)
                .with_detail("resolved_view_id", resolved_view_id);
            if let Some(artifact_id) = artifact_id {
                cli_error = cli_error.with_detail("artifact_id", artifact_id);
            }
            if let Some(actual) = actual {
                cli_error = cli_error.with_detail("actual", actual);
            }
        }
        ArtifactIoError::PatchApplyFailed {
            path,
            artifact_id,
            content_hash,
            failed_hunk,
            session_generation_id,
            resolved_view_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("artifact_id", artifact_id)
                .with_detail("content_hash", content_hash)
                .with_detail("failed_hunk", failed_hunk.to_string())
                .with_detail("session_generation_id", session_generation_id)
                .with_detail("resolved_view_id", resolved_view_id);
        }
    }
    cli_error
}

fn init_success_envelope(
    repository_id: &str,
    repo_root: &str,
    sunlight_dir: &str,
    created_config: bool,
    created_gitignore: bool,
    created_directories: usize,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"repository.init\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"repository_id\":\"{}\"}},",
            "\"view\":null,",
            "\"repository\":{{",
            "\"initialized\":true,",
            "\"storage_schema_version\":{},",
            "\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\",",
            "\"operation_semantics_version\":\"file_ops_v1\",",
            "\"git_interop_policy\":\"default_local_mvp\"",
            "}},",
            "\"init\":{{",
            "\"repo_root\":\"{}\",",
            "\"sunlight_dir\":\"{}\",",
            "\"created_config\":{},",
            "\"created_gitignore\":{},",
            "\"created_directories\":{}",
            "}}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(repository_id),
        json_escape(repository_id),
        CURRENT_STORAGE_SCHEMA_VERSION,
        json_escape(repo_root),
        json_escape(sunlight_dir),
        created_config,
        created_gitignore,
        created_directories,
    )
}

fn read_success_envelope(response: &ReadResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"artifacts\":[{}],",
            "\"content\":{{\"encoding\":\"{}\",\"bytes\":\"{}\"}}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        artifact_json(&response.artifact),
        json_escape(&response.content.encoding),
        json_escape(&response.content.bytes),
    )
}

fn list_success_envelope(response: &ListResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"artifacts\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        response
            .artifacts
            .iter()
            .map(artifact_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn search_success_envelope(response: &SearchResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"matches\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        response
            .matches
            .iter()
            .map(|item| {
                format!(
                    concat!(
                        "{{",
                        "\"artifact_id\":\"{}\",",
                        "\"path\":\"{}\",",
                        "\"content_hash\":\"{}\",",
                        "\"line\":{},",
                        "\"snippet\":\"{}\"",
                        "}}"
                    ),
                    json_escape(&item.artifact_id),
                    json_escape(&item.path),
                    json_escape(&item.content_hash),
                    item.line,
                    json_escape(&item.snippet),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn view_json(view: &SessionView) -> String {
    format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{{",
            "\"kind\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"tree_hash\":\"{}\"",
            "}}",
            "}}"
        ),
        json_escape(&view.resolved_view_id),
        json_escape(&view.session_generation_id),
        json_escape(&view.tree_identity.kind),
        json_escape(&view.tree_identity.repository_id),
        json_escape(&view.tree_identity.tree_hash),
    )
}

fn artifact_json(artifact: &SessionVisibleArtifactView) -> String {
    format!(
        concat!(
            "{{",
            "\"artifact_id\":\"{}\",",
            "\"path\":\"{}\",",
            "\"kind\":\"{}\",",
            "\"content_hash\":\"{}\",",
            "\"byte_length\":{},",
            "\"classification\":\"{}\",",
            "\"executable\":{},",
            "\"tombstone\":{}",
            "}}"
        ),
        json_escape(&artifact.artifact_id),
        json_escape(&artifact.path),
        artifact.kind.as_str(),
        json_escape(&artifact.content_hash),
        artifact.byte_length,
        json_escape(&artifact.classification),
        artifact.executable,
        artifact.tombstone,
    )
}

fn failure_envelope(error: &CliError) -> String {
    format!(
        "{{\"ok\":false,\"error\":{{\"code\":\"{}\",\"message\":\"{}\",\"details\":{}}}}}",
        json_escape(error.code),
        json_escape(&error.message),
        details_json(&error.details),
    )
}

fn details_json(details: &[(&'static str, String)]) -> String {
    if details.is_empty() {
        return "{}".to_string();
    }

    let fields = details
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            character if character.is_control() => format!("\\u{:04x}", character as u32)
                .chars()
                .collect::<Vec<_>>(),
            character => vec![character],
        })
        .collect()
}

fn print_help() {
    println!(
        "\
sun

Usage:
  sun init [--repo <path>]
  sun topic create <slug> --display-name <name> --json
  sun session start --topic <topic> --view <view-selector> --actor <actor-id> --json
  sun read <path> --session <session> --fixture basic-app [--json]
  sun list [path-prefix] --session <session> --fixture basic-app [--json]
  sun search <query> --session <session> --fixture basic-app [--json]

Commands:
  init       Create the conservative local .sunlight repository layout
  topic      Parse Phase 1 topic commands; persistence is not implemented yet
  session    Parse Phase 1 session commands; persistence is not implemented yet
  read       Read a fixture artifact by repository-relative path
  list       List fixture artifacts by optional path prefix
  search     Search fixture artifact text literally
"
    );
}

fn print_init_help() {
    println!(
        "\
sun init

Usage:
  sun init [--repo <path>]

Creates .sunlight/config.toml, the initial repository layout, and a conservative
.sunlight/.gitignore fragment. Existing config and policy files are preserved.
"
    );
}
