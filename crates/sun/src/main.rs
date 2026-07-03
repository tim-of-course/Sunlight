use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use sunlight_core::repository::{init_repository, RepositoryConfig, CURRENT_STORAGE_SCHEMA_VERSION};

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
        [scope, command, ..] if scope == "topic" && command == "create" => Err(unimplemented_command(
            "topic.create",
            "sun topic create is parsed, but topic records are not persisted yet",
        )),
        [scope, command, ..] if scope == "session" && command == "start" => Err(
            unimplemented_command(
                "session.start",
                "sun session start is parsed, but session records are not persisted yet",
            ),
        ),
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

Commands:
  init       Create the conservative local .sunlight repository layout
  topic      Parse Phase 1 topic commands; persistence is not implemented yet
  session    Parse Phase 1 session commands; persistence is not implemented yet
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
