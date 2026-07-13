use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = env::args().skip(1).collect();
    let current_dir = match env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("sun: cannot resolve current directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    sun::cli_main(args, current_dir)
}
