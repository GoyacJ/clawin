use std::ffi::OsString;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, error::ErrorKind};
use clawin_config::LoadedConfigSnapshot;
use clawin_core::{RuntimeCapabilities, SessionId, SessionRuntime};
use clawin_platform::{ClawinPathPolicy, SystemTerminalCapabilities, TerminalCapabilities};
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
    let args = normalize_args(args);

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
    let context = bootstrap_context()?;

    debug!(
        session_id = %context.runtime.session_id(),
        project_key = context.config.project_key(),
        "phase 2 bootstrap context assembled"
    );
    println!("clawin interactive session is not implemented yet.");

    Ok(ExitCode::SUCCESS)
}

fn normalize_args<I, T>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    if args.is_empty() {
        vec![OsString::from("clawin")]
    } else {
        args
    }
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

fn bootstrap_context() -> Result<BootstrapContext> {
    let original_cwd =
        std::env::current_dir().context("failed to read current working directory")?;
    let terminal = SystemTerminalCapabilities::detect();
    let path_policy = ClawinPathPolicy;
    let config = clawin_config::load_startup_config(original_cwd.clone(), &path_policy)
        .context("failed to load startup config")?;
    let runtime = SessionRuntime::new(
        generate_session_id(),
        RuntimeCapabilities::new(terminal.is_interactive(), false),
        original_cwd,
        config.paths().project_root().to_path_buf(),
    );

    Ok(BootstrapContext {
        runtime,
        config,
        terminal,
        path_policy,
    })
}

fn generate_session_id() -> SessionId {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    SessionId::from_owned(format!("bootstrap-{millis}-{}", std::process::id()))
}

struct BootstrapContext {
    #[allow(dead_code)]
    runtime: SessionRuntime,
    #[allow(dead_code)]
    config: LoadedConfigSnapshot,
    #[allow(dead_code)]
    terminal: SystemTerminalCapabilities,
    #[allow(dead_code)]
    path_policy: ClawinPathPolicy,
}
