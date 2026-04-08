#![forbid(unsafe_code)]

//! Bootstrap entrypoints for the `clawin` binary.

mod cli;
mod run;

pub use cli::Cli;
pub use run::{run, run_from};
