#![forbid(unsafe_code)]

//! Minimal slash command registry and built-in commands for Phase 6A.

use std::collections::BTreeMap;
use std::sync::Arc;

use clawin_core::{
    BridgeCommandAction, BridgeStatusSnapshot, ClawinError, ClawinResult, CommandEffect,
    CommandExecutionResult, CommandKind, CommandSource, CommandSpec, ParsedCommandInvocation,
    SessionPreview, SessionRuntime, resolve_resume_target,
};
use clawin_integrations::{
    LoadedPluginCommand, LoadedPluginsSnapshot, LoadedSkill, LoadedSkillsSnapshot, McpManager,
};

type CommandLoader = Arc<dyn Fn() -> Box<dyn LocalCommand> + Send + Sync>;

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
    pub fn register_local<L>(&mut self, spec: CommandSpec, load: L)
    where
        L: Fn() -> Box<dyn LocalCommand> + Send + Sync + 'static,
    {
        let index = self.commands.len();
        self.lookup.insert(spec.name.clone(), index);
        for alias in &spec.aliases {
            self.lookup.insert(alias.clone(), index);
        }
        self.commands.push(RegisteredCommand {
            spec,
            load: Arc::new(load),
        });
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

/// Build the baseline built-in command registry.
pub fn builtin_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    register_help_command(&mut registry);
    register_resume_command(&mut registry);
    register_remote_control_command(&mut registry);
    registry
}

/// Build the built-in command registry with Phase 6A MCP support.
pub fn builtin_command_registry_with_mcp(manager: Arc<McpManager>) -> CommandRegistry {
    let mut registry = builtin_command_registry();
    register_mcp_command(&mut registry, manager);
    registry
}

/// Build the built-in command registry with MCP, skills, and plugin runtime extensions.
pub fn builtin_command_registry_with_extensions(
    manager: Arc<McpManager>,
    skills: LoadedSkillsSnapshot,
    plugins: LoadedPluginsSnapshot,
) -> CommandRegistry {
    let mut registry = builtin_command_registry_with_mcp(manager);
    let merged_skills = merged_skills(&skills, &plugins);

    register_skills_command(&mut registry, merged_skills.clone());
    register_plugin_status_command(&mut registry, plugins.clone());

    for skill in merged_skills {
        if registry.spec(skill.command_name()).is_none() {
            register_dynamic_skill_command(&mut registry, skill);
        }
    }

    for command in plugins.loaded_commands() {
        if registry.spec(command.name()).is_none() {
            register_dynamic_plugin_command(&mut registry, command);
        }
    }

    registry
}

fn register_help_command(registry: &mut CommandRegistry) {
    registry.register_local(
        CommandSpec {
            name: "help".to_owned(),
            description: "Show help and available commands".to_owned(),
            aliases: vec!["?".to_owned()],
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        || Box::new(HelpCommand),
    );
}

fn register_resume_command(registry: &mut CommandRegistry) {
    registry.register_local(
        CommandSpec {
            name: "resume".to_owned(),
            description: "List or restore recent sessions in the current project scope".to_owned(),
            aliases: vec!["continue".to_owned()],
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        || Box::new(ResumeCommand),
    );
}

fn register_remote_control_command(registry: &mut CommandRegistry) {
    registry.register_local(
        CommandSpec {
            name: "remote-control".to_owned(),
            description: "Start, inspect, or stop the remote control bridge".to_owned(),
            aliases: vec!["rc".to_owned()],
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        || Box::new(RemoteControlCommand),
    );
}

fn register_mcp_command(registry: &mut CommandRegistry, manager: Arc<McpManager>) {
    registry.register_local(
        CommandSpec {
            name: "mcp".to_owned(),
            description: "List or reload configured MCP servers".to_owned(),
            aliases: Vec::new(),
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        move || {
            Box::new(McpCommand {
                manager: Arc::clone(&manager),
            })
        },
    );
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
            effect: None,
        })
    }
}

struct ResumeCommand;
struct RemoteControlCommand;

impl LocalCommand for ResumeCommand {
    fn execute(
        &self,
        invocation: &ParsedCommandInvocation,
        runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        let Some(store) = runtime.session_store() else {
            return Err(ClawinError::InvalidConfiguration {
                message: "session resume is unavailable because no session store is attached"
                    .to_owned(),
            });
        };

        if invocation.args.trim().is_empty() {
            let previews = store.list_recent_sessions(runtime)?;
            return Ok(CommandExecutionResult {
                command_name: "resume".to_owned(),
                output: render_resume_listing(&previews),
                effect: None,
            });
        }

        let trimmed = invocation.args.trim();
        let resolved = resolve_resume_target(runtime, store.as_ref(), trimmed)?;

        let Some(session) = resolved else {
            return Ok(CommandExecutionResult {
                command_name: "resume".to_owned(),
                output: format!("No session matched `{trimmed}`.\n"),
                effect: None,
            });
        };

        Ok(CommandExecutionResult {
            command_name: "resume".to_owned(),
            output: format!(
                "Resuming session `{}` from {}.\n",
                session.session_id,
                session.transcript_path.display()
            ),
            effect: Some(CommandEffect::ResumeSession { session }),
        })
    }
}

impl LocalCommand for RemoteControlCommand {
    fn execute(
        &self,
        invocation: &ParsedCommandInvocation,
        runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        let trimmed = invocation.args.trim();
        match trimmed {
            "" => Ok(CommandExecutionResult {
                command_name: "remote-control".to_owned(),
                output: "Starting remote control bridge.\n".to_owned(),
                effect: Some(CommandEffect::BridgeControl {
                    action: BridgeCommandAction::Start { name: None },
                }),
            }),
            "stop" => Ok(CommandExecutionResult {
                command_name: "remote-control".to_owned(),
                output: "Stopping remote control bridge.\n".to_owned(),
                effect: Some(CommandEffect::BridgeControl {
                    action: BridgeCommandAction::Stop,
                }),
            }),
            "status" => Ok(CommandExecutionResult {
                command_name: "remote-control".to_owned(),
                output: render_remote_control_status(
                    runtime
                        .bridge_controller()
                        .ok_or_else(|| ClawinError::InvalidConfiguration {
                            message: "remote control bridge is unavailable".to_owned(),
                        })?
                        .status()?,
                ),
                effect: None,
            }),
            _ if trimmed.starts_with("status ") || trimmed.starts_with("stop ") => {
                Err(ClawinError::InvalidCommandInvocation {
                    message: "usage: /remote-control [name|status|stop]".to_owned(),
                })
            }
            _ => Ok(CommandExecutionResult {
                command_name: "remote-control".to_owned(),
                output: format!("Starting remote control bridge `{trimmed}`.\n"),
                effect: Some(CommandEffect::BridgeControl {
                    action: BridgeCommandAction::Start {
                        name: Some(trimmed.to_owned()),
                    },
                }),
            }),
        }
    }
}

fn register_skills_command(registry: &mut CommandRegistry, skills: Vec<LoadedSkill>) {
    registry.register_local(
        CommandSpec {
            name: "skills".to_owned(),
            description: "List loaded skills and their sources".to_owned(),
            aliases: Vec::new(),
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        move || {
            Box::new(SkillsCommand {
                skills: skills.clone(),
            })
        },
    );
}

fn register_plugin_status_command(registry: &mut CommandRegistry, plugins: LoadedPluginsSnapshot) {
    registry.register_local(
        CommandSpec {
            name: "plugin".to_owned(),
            description: "List plugin runtime status and contributions".to_owned(),
            aliases: Vec::new(),
            kind: CommandKind::Local,
            source: CommandSource::Builtin,
            origin_label: None,
        },
        move || {
            Box::new(PluginStatusCommand {
                plugins: plugins.clone(),
            })
        },
    );
}

fn register_dynamic_skill_command(registry: &mut CommandRegistry, skill: LoadedSkill) {
    let spec = CommandSpec {
        name: skill.command_name().to_owned(),
        description: skill.description().to_owned(),
        aliases: Vec::new(),
        kind: CommandKind::Local,
        source: CommandSource::Dynamic,
        origin_label: Some(skill.origin_label().to_owned()),
    };

    registry.register_local(spec, move || {
        Box::new(SkillCommand {
            skill: skill.clone(),
        })
    });
}

fn register_dynamic_plugin_command(registry: &mut CommandRegistry, command: LoadedPluginCommand) {
    let spec = CommandSpec {
        name: command.name().to_owned(),
        description: command.description().to_owned(),
        aliases: Vec::new(),
        kind: CommandKind::Local,
        source: CommandSource::Dynamic,
        origin_label: Some(format!("plugin:{}", command.plugin_id())),
    };

    registry.register_local(spec, move || {
        Box::new(PluginMarkdownCommand {
            command: command.clone(),
        })
    });
}

struct McpCommand {
    manager: Arc<McpManager>,
}

impl LocalCommand for McpCommand {
    fn execute(
        &self,
        invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        let subcommand = if invocation.args.is_empty() {
            "list"
        } else {
            invocation.args.as_str()
        };

        let output = match subcommand {
            "list" => self.manager.render_status_report("MCP servers:"),
            "reload" => {
                self.manager.reload()?;
                self.manager.render_status_report("MCP servers reloaded:")
            }
            _ => {
                return Err(ClawinError::InvalidCommandInvocation {
                    message: "usage: /mcp [list|reload]".to_owned(),
                });
            }
        };

        Ok(CommandExecutionResult {
            command_name: "mcp".to_owned(),
            output,
            effect: None,
        })
    }
}

struct SkillsCommand {
    skills: Vec<LoadedSkill>,
}

impl LocalCommand for SkillsCommand {
    fn execute(
        &self,
        _invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        Ok(CommandExecutionResult {
            command_name: "skills".to_owned(),
            output: render_skills_listing(&self.skills),
            effect: None,
        })
    }
}

struct PluginStatusCommand {
    plugins: LoadedPluginsSnapshot,
}

impl LocalCommand for PluginStatusCommand {
    fn execute(
        &self,
        _invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        Ok(CommandExecutionResult {
            command_name: "plugin".to_owned(),
            output: render_plugin_listing(&self.plugins),
            effect: None,
        })
    }
}

struct SkillCommand {
    skill: LoadedSkill,
}

impl LocalCommand for SkillCommand {
    fn execute(
        &self,
        _invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        Ok(CommandExecutionResult {
            command_name: self.skill.command_name().to_owned(),
            output: render_skill_command_output(&self.skill),
            effect: None,
        })
    }
}

struct PluginMarkdownCommand {
    command: LoadedPluginCommand,
}

impl LocalCommand for PluginMarkdownCommand {
    fn execute(
        &self,
        _invocation: &ParsedCommandInvocation,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<CommandExecutionResult> {
        Ok(CommandExecutionResult {
            command_name: self.command.name().to_owned(),
            output: render_plugin_command_output(&self.command),
            effect: None,
        })
    }
}

fn render_resume_listing(previews: &[SessionPreview]) -> String {
    if previews.is_empty() {
        return "Recent sessions:\n(no sessions found)\n".to_owned();
    }

    let mut lines = vec!["Recent sessions:".to_owned()];
    for preview in previews {
        let prompt = preview.last_prompt.as_deref().unwrap_or("(no prompt)");
        lines.push(format!(
            "- {} prompt={} path={}",
            preview.session_id,
            prompt,
            preview.transcript_path.display()
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_remote_control_status(status: BridgeStatusSnapshot) -> String {
    let mut lines = vec![format!("Remote control bridge: {}", status.state.as_str())];
    if let Some(mode) = status.mode {
        lines.push(format!("mode={}", mode.as_str()));
    }
    if let Some(source) = status.source {
        lines.push(format!("source={}", source.as_str()));
    }
    if let Some(name) = status.name {
        lines.push(format!("name={name}"));
    }
    if let Some(environment_id) = status.environment_id {
        lines.push(format!("environment_id={environment_id}"));
    }
    if let Some(bridge_session_id) = status.bridge_session_id {
        lines.push(format!("bridge_session_id={bridge_session_id}"));
    }
    if let Some(local_session_id) = status.local_session_id {
        lines.push(format!("local_session_id={local_session_id}"));
    }
    if let Some(transcript_path) = status.transcript_path {
        lines.push(format!("transcript_path={}", transcript_path.display()));
    }
    if let Some(last_error) = status.last_error {
        lines.push(format!("last_error={last_error}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn merged_skills(
    skills: &LoadedSkillsSnapshot,
    plugins: &LoadedPluginsSnapshot,
) -> Vec<LoadedSkill> {
    let mut merged = skills.skills().to_vec();
    merged.extend(plugins.loaded_skills());
    merged.sort_by(|left, right| left.command_name().cmp(right.command_name()));
    merged
}

fn render_skills_listing(skills: &[LoadedSkill]) -> String {
    if skills.is_empty() {
        return "Loaded skills:\n(no skills loaded)\n".to_owned();
    }

    let mut lines = vec!["Loaded skills:".to_owned()];
    for skill in skills {
        lines.push(format!(
            "- {} source={} description={}",
            skill.display_label(),
            skill.source_label(),
            skill.description()
        ));
    }
    format!("{}\n", lines.join("\n"))
}

fn render_skill_command_output(skill: &LoadedSkill) -> String {
    let allowed_tools = if skill.tools().is_empty() {
        "(none)".to_owned()
    } else {
        skill.tools().join(", ")
    };

    format!(
        "Skill `{}` loaded from {}.\nDescription: {}\nAllowed tools: {}\nMarkdown:\n{}\n\n",
        skill.display_label(),
        skill
            .source_label()
            .replace("plugin(", "plugin ")
            .replace(')', ""),
        skill.description(),
        allowed_tools,
        skill.markdown()
    )
}

fn render_plugin_listing(plugins: &LoadedPluginsSnapshot) -> String {
    if plugins.plugins().is_empty() {
        return "Plugins:\n(no plugins loaded)\n\n".to_owned();
    }

    let mut lines = vec!["Plugins:".to_owned()];
    for plugin in plugins.plugins() {
        let command_count = plugin.command_names().len() + plugin.skill_command_names().len();
        let mut line = format!(
            "- {} scope={} status={} commands={} skills={} mcp_servers={}",
            plugin.id(),
            plugin.source().as_str(),
            plugin.status().as_str(),
            command_count,
            plugin.skill_command_names().len(),
            plugin.mcp_server_names().len()
        );
        if let Some(error) = plugin.errors().first() {
            line.push_str(" error=");
            line.push_str(error);
        }
        lines.push(line);
    }
    lines.push(String::new());
    format!("{}\n", lines.join("\n"))
}

fn render_plugin_command_output(command: &LoadedPluginCommand) -> String {
    format!(
        "Plugin command `{}` loaded from plugin `{}`.\nDescription: {}\nMarkdown:\n{}\n\n",
        command.name(),
        command.plugin_id(),
        command.description(),
        command.markdown()
    )
}
