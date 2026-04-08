use clap::Parser;

/// Top-level CLI surface for the current Clawin bootstrap entrypoint.
#[derive(Debug, Parser)]
#[command(
    name = "clawin",
    bin_name = "clawin",
    version,
    about = "Terminal coding agent rebuilt in Rust.",
    long_about = None,
    disable_help_subcommand = true
)]
pub struct Cli;
