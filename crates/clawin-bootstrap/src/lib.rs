#![forbid(unsafe_code)]

//! Bootstrap entrypoints for the `clawin` binary.

mod cli;
mod print;
mod remote_control;
mod run;
mod worktree;

pub use cli::{Cli, PrintInputFormat, PrintOptions, PrintOutputFormat, RemoteControlOptions};
pub use remote_control::run_remote_control_session;
pub use run::{
    BootstrappedSession, SessionBootstrapMode, bootstrap_session, bootstrap_session_from,
    bootstrap_session_from_request, bootstrap_session_from_with_process_spawner, run,
    run_bootstrapped_session_with_terminal, run_from,
};
