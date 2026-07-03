use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use sunlight_core::repository::init_repository;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sun: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.as_slice() {
        [] => {
            print_help();
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
            Ok(())
        }
        [command] if command == "init" => init(PathBuf::from(".")),
        [command, flag, path] if command == "init" && flag == "--repo" => init(PathBuf::from(path)),
        [command, flag] if command == "init" && flag == "--help" => {
            print_init_help();
            Ok(())
        }
        [command, ..] if command == "init" => Err("usage: sun init [--repo <path>]".to_string()),
        [scope, command, ..] if scope == "topic" && command == "create" => {
            Err("sun topic create is parsed, but topic records are not persisted yet".to_string())
        }
        [scope, command, ..] if scope == "session" && command == "start" => Err(
            "sun session start is parsed, but session records are not persisted yet".to_string(),
        ),
        [command, ..] => Err(format!("unknown command `{command}`")),
    }
}

fn init(repo_root: PathBuf) -> Result<(), String> {
    let report = init_repository(&repo_root).map_err(|error| error.to_string())?;

    println!("initialized Sunlight repository");
    println!("repo_root = {}", report.repo_root.display());
    println!("sunlight_dir = {}", report.sunlight_dir.display());
    println!("repository_id = {}", report.repository_id);
    println!("created_config = {}", report.created_config);
    println!("created_gitignore = {}", report.created_gitignore);
    println!("created_directories = {}", report.created_directories.len());

    Ok(())
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
