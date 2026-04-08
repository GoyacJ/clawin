#![forbid(unsafe_code)]

//! Minimal slash command registry and built-in commands for Phase 3.

use std::collections::BTreeMap;

use clawin_core::{
    ClawinError, ClawinResult, CommandExecutionResult, CommandKind, CommandSource, CommandSpec,
    ParsedCommandInvocation, SessionRuntime,
};

type CommandLoader = fn() -> Box<dyn LocalCommand>;

/// Local command interface used by lazy-loaded command implementations.
pub trait LocalCommand: Send + Sync {
    fn execute(
        &self,
        invocation: &ParsedCommandInvocation,
        runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult>;
}

#[derive(Clone)]
struct RegisteredCommand {
    spec: CommandSpec,
    load: CommandLoader,
}

/// Slash command registry with stable specs and lazy-loaded handlers.
#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: Vec<RegisteredCommand>,
    lookup: BTreeMap<String, usize>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRegistry")
            .field("commands", &self.command_specs().collect::<Vec<_>>())
            .finish()
    }
}

impl CommandRegistry {
    /// Create an empty command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lazy-loaded local command.
    pub fn register_local(&mut self, spec: CommandSpec, load: CommandLoader) {
        let index = self.commands.len();
        self.lookup.insert(spec.name.clone(), index);
        for alias in &spec.aliases {
            self.lookup.insert(alias.clone(), index);
        }
        self.commands.push(RegisteredCommand { spec, load });
    }

    /// Borrow the command spec for a given canonical name or alias.
    pub fn spec(&self, name: &str) -> Option<&CommandSpec> {
        self.lookup
            .get(name)
            .and_then(|index| self.commands.get(*index))
            .map(|entry| &entry.spec)
    }

    /// Iterate over registered command specs once per canonical command.
    pub fn command_specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.iter().map(|entry| &entry.spec)
    }

    /// Parse a raw slash command into its canonical invocation form.
    pub fn parse(&self, raw: &str) -> ClawinResult<ParsedCommandInvocation> {
        let trimmed = raw.trim();
        if !trimmed.starts_with('/') {
            return Err(ClawinError::InvalidCommandInvocation {
                message: "slash commands must start with '/'".to_owned(),
            });
        }

        let body = trimmed.trim_start_matches('/').trim();
        if body.is_empty() {
            return Err(ClawinError::InvalidCommandInvocation {
                message: "missing command name".to_owned(),
            });
        }

        let (raw_name, args) = match body.split_once(char::is_whitespace) {
            Some((name, args)) => (name, args.trim()),
            None => (body, ""),
        };

        let Some(index) = self.lookup.get(raw_name) else {
            return Err(ClawinError::UnknownCommand {
                name: raw_name.to_owned(),
            });
        };
        let command = &self.commands[*index];

        Ok(ParsedCommandInvocation {
            raw_name: raw_name.to_owned(),
            command_name: command.spec.name.clone(),
            args: args.to_owned(),
        })
    }

    /// Execute a raw slash command by resolving the spec, loading the handler, and running it.
    pub fn execute(
        &self,
        raw: &str,
        runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        let invocation = self.parse(raw)?;
        let Some(spec) = self.spec(&invocation.command_name).cloned() else {
            return Err(ClawinError::UnknownCommand {
                name: invocation.command_name,
            });
        };
        let handler = self.load(&spec.name)?;
        handler.execute(&invocation, runtime)
    }

    fn load(&self, name: &str) -> ClawinResult<Box<dyn LocalCommand>> {
        let Some(index) = self.lookup.get(name) else {
            return Err(ClawinError::UnknownCommand {
                name: name.to_owned(),
            });
        };

        Ok((self.commands[*index].load)())
    }
}

/// Build the Phase 3 built-in command registry.
pub fn builtin_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register_local(
        CommandSpec {
            name: "help".to_owned(),
            description: "Show help and available commands".to_owned(),
            aliases: vec!["?".to_owned()],
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
        },
        || Box::new(HelpCommand),
    );
    registry
}

struct HelpCommand;

impl LocalCommand for HelpCommand {
    fn execute(
        &self,
        _invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        Ok(CommandExecutionResult {
            command_name: "help".to_owned(),
            output: "Available commands:\n/help - Show help and available commands\n".to_owned(),
        })
    }
}
