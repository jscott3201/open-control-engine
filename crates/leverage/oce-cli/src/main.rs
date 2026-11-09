#![forbid(unsafe_code)]
//! The `oce` executable: a thin shell over [`oce_cli::run`] with the process's own streams.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut streams = oce_cli::Streams {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        stderr_is_terminal: oce_cli::Streams::process_stderr_is_terminal(),
    };
    let exit = oce_cli::run(std::env::args_os(), &mut streams);
    ExitCode::from(exit.code())
}
