use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser, error::ErrorKind};
use clawin_config::{ClawinPaths, StaticConfigStore};
use clawin_core::{RuntimeCapabilities, SessionId, SessionRuntime};
use clawin_platform::{ClawinPathPolicy, StaticTerminalCapabilities, TerminalCapabilities};
use tracing::debug;

use crate::Cli;

/// Execute the bootstrap flow from process arguments.
pub fn run() -> ExitCode {
    run_from(std::env::args_os())
}

/// Execute the bootstrap flow from an arbitrary argument list.
pub fn run_from<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();

    if args.len() <= 1 {
        return print_help();
    }

    match Cli::try_parse_from(args) {
        Ok(cli) => match dispatch(cli) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error:#}");
                ExitCode::from(1)
            }
        },
        Err(error) => render_cli_error(error),
    }
}

fn dispatch(_cli: Cli) -> Result<ExitCode> {
    let state = bootstrap_state()?;

    debug!(session_id = %state.runtime.session_id(), "phase 1 bootstrap skeleton dispatched");
    println!("clawin bootstrap skeleton is not implemented yet.");

    Ok(ExitCode::SUCCESS)
}

fn print_help() -> ExitCode {
    let mut command = Cli::command();
    command.print_help().expect("help should render");
    println!();
    ExitCode::SUCCESS
}

fn render_cli_error(error: clap::Error) -> ExitCode {
    let exit_code = match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
        _ => ExitCode::from(2),
    };

    if error.use_stderr() {
        eprint!("{error}");
    } else {
        print!("{error}");
    }

    exit_code
}

fn bootstrap_state() -> Result<BootstrapState> {
    let project_root = std::env::current_dir()?;
    let global_root = home_dir_fallback().join(".clawin");
    let paths = ClawinPaths::new(global_root, project_root);
    let config = StaticConfigStore::new(paths.clone(), 1);
    let terminal = StaticTerminalCapabilities::new(true, true);
    let path_policy = ClawinPathPolicy;
    let runtime = SessionRuntime::new(
        SessionId::from_static("phase-1-bootstrap"),
        RuntimeCapabilities::new(terminal.is_interactive(), false),
    );

    Ok(BootstrapState {
        runtime,
        config,
        terminal,
        path_policy,
    })
}

fn home_dir_fallback() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

struct BootstrapState {
    #[allow(dead_code)]
    runtime: SessionRuntime,
    #[allow(dead_code)]
    config: StaticConfigStore,
    #[allow(dead_code)]
    terminal: StaticTerminalCapabilities,
    #[allow(dead_code)]
    path_policy: ClawinPathPolicy,
}
