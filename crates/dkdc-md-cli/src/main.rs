use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = dkdc_md_cli::run(std::env::args()) {
        eprintln!("Error: {e:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
