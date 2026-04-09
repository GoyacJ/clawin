use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

/// Structured/headless stdin input formats exposed by `clawin --print`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum PrintInputFormat {
    #[default]
    Text,
    StreamJson,
}

/// Structured/headless stdout output formats exposed by `clawin --print`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum PrintOutputFormat {
    #[default]
    Text,
    Json,
    StreamJson,
}

/// Stable print-mode options derived from the top-level CLI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintOptions {
    pub input_format: PrintInputFormat,
    pub output_format: PrintOutputFormat,
    pub verbose: bool,
    pub prompt: Option<String>,
}

/// Stable options for the standalone remote control bridge worker.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct RemoteControlOptions {
    /// Optional bridge session name forwarded to the remote bridge connector.
    #[arg(value_name = "NAME", conflicts_with = "continue_bridge")]
    pub name: Option<String>,

    /// Continue the freshest valid bridge pointer in the current project scope.
    #[arg(long = "continue", conflicts_with = "name")]
    pub continue_bridge: bool,
}

/// Top-level subcommands supported by the Clawin bootstrap entrypoint.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum CliCommand {
    /// Start or resume a standalone remote control bridge worker.
    #[command(alias = "rc")]
    RemoteControl(RemoteControlOptions),
}

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
pub struct Cli {
    /// Enter headless non-interactive print mode.
    #[arg(short = 'p', long = "print")]
    pub print_mode: bool,

    /// Resume the most recent session visible from the current project scope.
    #[arg(long = "continue", conflicts_with = "resume")]
    pub continue_session: bool,

    /// Resume a session by id, search term, or explicit `.jsonl` path.
    #[arg(long, value_name = "SESSION", conflicts_with = "continue_session")]
    pub resume: Option<String>,

    /// Select the stdin shape for `--print`.
    #[arg(
        long = "input-format",
        value_enum,
        default_value_t = PrintInputFormat::Text,
        requires = "print_mode"
    )]
    pub input_format: PrintInputFormat,

    /// Select the stdout shape for `--print`.
    #[arg(
        long = "output-format",
        value_enum,
        default_value_t = PrintOutputFormat::Text,
        requires = "print_mode"
    )]
    pub output_format: PrintOutputFormat,

    /// Enable verbose headless output required by `--output-format=stream-json`.
    #[arg(long, requires = "print_mode")]
    pub verbose: bool,

    /// Prompt text used by `--print --input-format=text` when stdin is not piped.
    #[arg(value_name = "PROMPT", requires = "print_mode")]
    pub prompt: Option<String>,

    /// Top-level bootstrap subcommands.
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

impl Cli {
    /// Validate CLI combinations that require semantic checks beyond clap's shape validation.
    pub fn validate(self) -> Result<Self, clap::Error> {
        if self.command.is_some()
            && (self.print_mode
                || self.continue_session
                || self.resume.is_some()
                || self.prompt.is_some())
        {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "top-level session flags cannot be combined with a subcommand",
            ));
        }

        if self.output_format == PrintOutputFormat::StreamJson && !self.verbose {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--output-format=stream-json requires --verbose",
            ));
        }

        if self.input_format == PrintInputFormat::StreamJson && self.prompt.is_some() {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "positional prompt is not supported with --input-format=stream-json",
            ));
        }

        Ok(self)
    }

    /// Return normalized print-mode options when `--print` is enabled.
    pub fn print_options(&self) -> Option<PrintOptions> {
        (self.command.is_none() && self.print_mode).then(|| PrintOptions {
            input_format: self.input_format,
            output_format: self.output_format,
            verbose: self.verbose,
            prompt: self.prompt.clone(),
        })
    }
}
