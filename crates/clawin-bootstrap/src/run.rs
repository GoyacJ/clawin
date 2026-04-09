use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, error::ErrorKind};
use clawin_commands::{CommandRegistry, builtin_command_registry_with_extensions};
use clawin_config::{JsonlSessionStore, LoadedConfigSnapshot};
use clawin_core::{
    BridgePointer, CancellationFlag, ClawinResult, ConversationRequest, EngineEvent, EngineOutcome,
    ModelDriver, ModelDriverFuture, ModelRequest, PassthroughPermissionResolver, PermissionMode,
    RestoredSession, ResumeQuery, RuntimeCapabilities, SessionId, SessionRuntime, SessionStore,
    TurnLoopConfig, WorktreeManager, looks_like_transcript_path, resolve_resume_target,
};
use clawin_engine::{ConversationEngine, EngineServices};
use clawin_integrations::{
    BridgePointerStore, BridgeTransportConnector, LoadedPluginsSnapshot, LoadedSkillsSnapshot,
    McpManager, UnavailableBridgeConnector, load_plugins_snapshot, load_skills_snapshot,
};
use clawin_platform::{
    ClawinPathPolicy, GitWorktreeAdapter, PathPolicy, ProcessSpawner, SystemGitWorktreeAdapter,
    SystemProcessSpawner, SystemTerminalCapabilities, SystemTerminalSession, TerminalCapabilities,
    TerminalSession,
};
use clawin_tools::{ToolRegistry, builtin_tool_registry_with_mcp};
use clawin_ui::{ReplConfig, ReplController, run_repl_session};
use tracing::debug;

use crate::Cli;
use crate::cli::{CliCommand, RemoteControlOptions};
use crate::print::{PrintModeError, run_print_mode};
use crate::remote_control::run_remote_control_session;
use crate::worktree::GitSessionWorktreeManager;

/// Requested bootstrap mode for Phase 7A session restore flows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionBootstrapMode {
    Fresh,
    Continue,
    Resume(String),
}

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

    match Cli::try_parse_from(args).and_then(Cli::validate) {
        Ok(cli) => match dispatch(cli) {
            Ok(code) => code,
            Err(DispatchError::Cli(error)) => render_cli_error(error),
            Err(DispatchError::Runtime(error)) => {
                eprintln!("{error:#}");
                ExitCode::from(1)
            }
        },
        Err(error) => render_cli_error(error),
    }
}

enum DispatchError {
    Cli(clap::Error),
    Runtime(anyhow::Error),
}

impl From<anyhow::Error> for DispatchError {
    fn from(error: anyhow::Error) -> Self {
        Self::Runtime(error)
    }
}

fn dispatch(cli: Cli) -> std::result::Result<ExitCode, DispatchError> {
    if let Some(command) = cli.command.clone() {
        return dispatch_subcommand(command);
    }

    let print_options = cli.print_options();
    let mode = if cli.continue_session {
        SessionBootstrapMode::Continue
    } else if let Some(resume) = cli.resume.clone() {
        SessionBootstrapMode::Resume(resume)
    } else {
        SessionBootstrapMode::Fresh
    };
    let session = bootstrap_session_with_mode(mode)?;

    debug!(
        session_id = %session.runtime().session_id(),
        project_key = session.config().project_key(),
        command_count = session.commands().command_specs().count(),
        tool_count = session.tools().tool_specs().count(),
        engine_session_id = %session.engine().session_id(),
        interactive = session.runtime().capabilities().interactive_terminal(),
        "phase 5 bootstrap session assembled"
    );

    if let Some(print_options) = print_options {
        return match run_print_mode(
            session,
            Arc::new(UnavailablePrintModelDriver),
            print_options,
        ) {
            Ok(code) => Ok(code),
            Err(PrintModeError::Cli(error)) => Err(DispatchError::Cli(error)),
            Err(PrintModeError::Runtime(error)) => Err(DispatchError::Runtime(error)),
        };
    }

    if session.runtime().capabilities().interactive_terminal() {
        let mut terminal =
            SystemTerminalSession::new().context("failed to create terminal session")?;
        Ok(run_bootstrapped_session_with_terminal(
            session,
            Arc::new(UnavailableModelDriver),
            &mut terminal,
        )?)
    } else {
        Ok(run_non_interactive_session(session)?)
    }
}

fn dispatch_subcommand(command: CliCommand) -> std::result::Result<ExitCode, DispatchError> {
    match command {
        CliCommand::RemoteControl(options) => dispatch_remote_control(options),
    }
}

fn dispatch_remote_control(
    options: RemoteControlOptions,
) -> std::result::Result<ExitCode, DispatchError> {
    let pointer = if options.continue_bridge {
        Some(resolve_bridge_continue_pointer()?)
    } else {
        None
    };
    let mode = pointer
        .as_ref()
        .map(|pointer| SessionBootstrapMode::Resume(pointer.transcript_path.display().to_string()))
        .unwrap_or(SessionBootstrapMode::Fresh);
    let session = bootstrap_session_with_mode(mode)?;

    Ok(run_remote_control_session(
        session,
        Arc::new(UnavailableModelDriver),
        options.name,
        pointer,
    )?)
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

fn bootstrap_context(mode: SessionBootstrapMode) -> Result<BootstrapContext> {
    let original_cwd =
        std::env::current_dir().context("failed to read current working directory")?;
    bootstrap_context_from_with_dependencies(
        original_cwd,
        SystemTerminalCapabilities::detect(),
        ClawinPathPolicy,
        mode,
        BootstrapDependencies::new(
            Arc::new(SystemProcessSpawner),
            Arc::new(SystemGitWorktreeAdapter),
            Arc::new(UnavailableBridgeConnector),
        ),
    )
}

fn bootstrap_context_from<P, T>(
    original_cwd: PathBuf,
    terminal: T,
    path_policy: P,
) -> Result<BootstrapContext>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    T: TerminalCapabilities,
{
    bootstrap_context_from_with_dependencies(
        original_cwd,
        terminal,
        path_policy,
        SessionBootstrapMode::Fresh,
        BootstrapDependencies::new(
            Arc::new(SystemProcessSpawner),
            Arc::new(SystemGitWorktreeAdapter),
            Arc::new(UnavailableBridgeConnector),
        ),
    )
}

struct BootstrapDependencies<G> {
    process_spawner: Arc<dyn ProcessSpawner>,
    git_adapter: Arc<G>,
    bridge_connector: Arc<dyn BridgeTransportConnector>,
    initialize_session: bool,
}

impl<G> BootstrapDependencies<G> {
    fn new(
        process_spawner: Arc<dyn ProcessSpawner>,
        git_adapter: Arc<G>,
        bridge_connector: Arc<dyn BridgeTransportConnector>,
    ) -> Self {
        Self {
            process_spawner,
            git_adapter,
            bridge_connector,
            initialize_session: true,
        }
    }
}

fn bootstrap_context_from_with_dependencies<P, T, G>(
    original_cwd: PathBuf,
    terminal: T,
    path_policy: P,
    mode: SessionBootstrapMode,
    dependencies: BootstrapDependencies<G>,
) -> Result<BootstrapContext>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    T: TerminalCapabilities,
    G: GitWorktreeAdapter + Send + Sync + 'static,
{
    let BootstrapDependencies {
        process_spawner,
        git_adapter,
        bridge_connector,
        initialize_session,
    } = dependencies;
    let initial_config = clawin_config::load_startup_config(original_cwd.clone(), &path_policy)
        .context("failed to load startup config")?;
    let capabilities = RuntimeCapabilities::new(terminal.is_interactive(), false);
    let preliminary_runtime = SessionRuntime::new(
        generate_session_id(),
        capabilities,
        original_cwd.clone(),
        initial_config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );
    let store = Arc::new(JsonlSessionStore::new(
        initial_config.paths().clone(),
        path_policy.clone(),
        Arc::clone(&git_adapter),
    ));
    preliminary_runtime.set_session_store(store.clone());
    preliminary_runtime.set_worktree_manager(Arc::new(GitSessionWorktreeManager::new(
        path_policy.clone(),
        Arc::clone(&git_adapter),
    )));

    let restored_session = resolve_bootstrap_mode(&mode, &preliminary_runtime, store.as_ref())
        .context("failed to resolve requested session restore state")?;

    let final_config = match restored_session.as_ref() {
        Some(restored)
            if restored.canonical_project_root != initial_config.paths().project_root() =>
        {
            clawin_config::load_startup_config(
                restored.canonical_project_root.clone(),
                &path_policy,
            )
            .context("failed to reload startup config for restored session")?
        }
        _ => initial_config,
    };
    let file_skills = load_skills_snapshot(&final_config);
    let plugins = load_plugins_snapshot(&final_config);
    let merged_skills = LoadedSkillsSnapshot::from_parts(
        {
            let mut skills = file_skills.skills().to_vec();
            skills.extend(plugins.loaded_skills());
            skills
        },
        file_skills.errors().to_vec(),
    );
    let mcp_manager = Arc::new(
        McpManager::from_loaded_config(&final_config, process_spawner)
            .map_err(|error| anyhow!("failed to assemble MCP manager: {error}"))?,
    );
    mcp_manager
        .set_plugin_servers(&plugins)
        .map_err(|error| anyhow!("failed to merge plugin MCP servers: {error}"))?;
    let runtime = match restored_session.as_ref() {
        Some(restored) => build_restored_runtime(
            &original_cwd,
            RuntimeCapabilities::new(
                terminal.is_interactive(),
                mcp_manager.has_configured_servers(),
            ),
            PermissionMode::Default,
            restored,
            Arc::new(JsonlSessionStore::new(
                final_config.paths().clone(),
                path_policy.clone(),
                Arc::clone(&git_adapter),
            )),
            Arc::new(GitSessionWorktreeManager::new(
                path_policy.clone(),
                Arc::clone(&git_adapter),
            )),
        ),
        None => {
            let runtime = SessionRuntime::new(
                generate_session_id(),
                RuntimeCapabilities::new(
                    terminal.is_interactive(),
                    mcp_manager.has_configured_servers(),
                ),
                original_cwd,
                final_config.paths().project_root().to_path_buf(),
                PermissionMode::Default,
            );
            runtime.set_session_store(Arc::new(JsonlSessionStore::new(
                final_config.paths().clone(),
                path_policy.clone(),
                Arc::clone(&git_adapter),
            )));
            runtime.set_worktree_manager(Arc::new(GitSessionWorktreeManager::new(
                path_policy.clone(),
                Arc::clone(&git_adapter),
            )));
            runtime
        }
    };

    runtime.set_bridge_controller(Arc::new(clawin_integrations::BridgeManager::new(
        final_config.paths().clone(),
        path_policy.clone(),
        Arc::clone(&git_adapter),
        bridge_connector,
    )));

    if mode == SessionBootstrapMode::Fresh && initialize_session {
        runtime
            .session_store()
            .expect("fresh runtime should always have a session store")
            .initialize_session(&runtime)
            .map_err(|error| anyhow!("failed to initialize session transcript: {error}"))?;
    }

    Ok(BootstrapContext {
        runtime,
        config: final_config,
        mcp_manager,
        skills: merged_skills,
        plugins,
        restored_session,
    })
}

fn generate_session_id() -> SessionId {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    SessionId::from_owned(format!("bootstrap-{millis}-{}", std::process::id()))
}

pub fn bootstrap_session() -> Result<BootstrappedSession> {
    let context = bootstrap_context(SessionBootstrapMode::Fresh)?;
    Ok(BootstrappedSession::new(context))
}

fn bootstrap_session_with_mode(mode: SessionBootstrapMode) -> Result<BootstrappedSession> {
    let context = bootstrap_context(mode)?;
    Ok(BootstrappedSession::new(context))
}

/// Build a bootstrapped session from explicit cwd, terminal, and path policy inputs.
pub fn bootstrap_session_from<P, T>(
    original_cwd: PathBuf,
    terminal: T,
    path_policy: P,
) -> Result<BootstrappedSession>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    T: TerminalCapabilities,
{
    let context = bootstrap_context_from(original_cwd, terminal, path_policy)?;
    Ok(BootstrappedSession::new(context))
}

/// Build a bootstrapped session from explicit cwd, terminal, path policy, and restore mode inputs.
pub fn bootstrap_session_from_request<P, T>(
    original_cwd: PathBuf,
    terminal: T,
    path_policy: P,
    mode: SessionBootstrapMode,
) -> Result<BootstrappedSession>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    T: TerminalCapabilities,
{
    let context = bootstrap_context_from_with_dependencies(
        original_cwd,
        terminal,
        path_policy,
        mode,
        BootstrapDependencies::new(
            Arc::new(SystemProcessSpawner),
            Arc::new(SystemGitWorktreeAdapter),
            Arc::new(UnavailableBridgeConnector),
        ),
    )?;
    Ok(BootstrappedSession::new(context))
}

/// Build a bootstrapped session from explicit cwd, terminal, path policy, and process spawner inputs.
pub fn bootstrap_session_from_with_process_spawner<P, T>(
    original_cwd: PathBuf,
    terminal: T,
    path_policy: P,
    process_spawner: Arc<dyn ProcessSpawner>,
) -> Result<BootstrappedSession>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    T: TerminalCapabilities,
{
    let context = bootstrap_context_from_with_dependencies(
        original_cwd,
        terminal,
        path_policy,
        SessionBootstrapMode::Fresh,
        BootstrapDependencies::new(
            process_spawner,
            Arc::new(SystemGitWorktreeAdapter),
            Arc::new(UnavailableBridgeConnector),
        ),
    )?;
    Ok(BootstrappedSession::new(context))
}

/// Route a bootstrapped session through either the interactive REPL or the stable non-interactive stub.
pub fn run_bootstrapped_session_with_terminal(
    session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    terminal: &mut dyn TerminalSession,
) -> Result<ExitCode> {
    if !session.runtime().capabilities().interactive_terminal() {
        return run_non_interactive_session(session);
    }

    let cancellation_slot = Arc::new(Mutex::new(None));
    let (runtime, commands, tools, engine) = session.into_repl_parts();
    let mut controller =
        ReplController::new(runtime, commands, tools, engine, driver, cancellation_slot);
    run_repl_session(&mut controller, terminal, ReplConfig::default())
        .context("interactive repl session failed")?;
    Ok(ExitCode::SUCCESS)
}

fn run_non_interactive_session(_session: BootstrappedSession) -> Result<ExitCode> {
    println!("clawin non-interactive mode is not implemented yet.");
    Ok(ExitCode::SUCCESS)
}

struct BootstrapContext {
    runtime: SessionRuntime,
    config: LoadedConfigSnapshot,
    mcp_manager: Arc<McpManager>,
    skills: LoadedSkillsSnapshot,
    plugins: LoadedPluginsSnapshot,
    restored_session: Option<RestoredSession>,
}

/// Reusable non-interactive bootstrap assembly for tests and future Phase 5 UI entrypoints.
#[derive(Debug)]
pub struct BootstrappedSession {
    runtime: SessionRuntime,
    config: LoadedConfigSnapshot,
    mcp_manager: Arc<McpManager>,
    skills: LoadedSkillsSnapshot,
    plugins: LoadedPluginsSnapshot,
    commands: CommandRegistry,
    tools: ToolRegistry,
    engine: ConversationEngine,
}

impl BootstrappedSession {
    fn new(context: BootstrapContext) -> Self {
        let commands = builtin_command_registry_with_extensions(
            Arc::clone(&context.mcp_manager),
            context.skills.clone(),
            context.plugins.clone(),
        );
        let tools = builtin_tool_registry_with_mcp(Arc::clone(&context.mcp_manager));
        let engine = if let Some(restored) = context.restored_session.as_ref() {
            ConversationEngine::restore(restored.session_id.clone(), restored.transcript.clone())
        } else {
            ConversationEngine::new(context.runtime.session_id().clone())
        };

        Self {
            runtime: context.runtime,
            config: context.config,
            mcp_manager: context.mcp_manager,
            skills: context.skills,
            plugins: context.plugins,
            commands,
            tools,
            engine,
        }
    }

    /// Borrow the assembled runtime.
    pub fn runtime(&self) -> &SessionRuntime {
        &self.runtime
    }

    /// Borrow the loaded config snapshot.
    pub fn config(&self) -> &LoadedConfigSnapshot {
        &self.config
    }

    /// Borrow the assembled MCP manager.
    pub fn mcp(&self) -> &Arc<McpManager> {
        &self.mcp_manager
    }

    /// Borrow the assembled skills snapshot.
    pub fn skills(&self) -> &LoadedSkillsSnapshot {
        &self.skills
    }

    /// Borrow the assembled plugins snapshot.
    pub fn plugins(&self) -> &LoadedPluginsSnapshot {
        &self.plugins
    }

    /// Borrow the assembled command registry.
    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    /// Borrow the assembled tool registry.
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Borrow the retained conversation engine.
    pub fn engine(&self) -> &ConversationEngine {
        &self.engine
    }

    /// Submit a prompt or slash command through the bootstrapped engine with an injected model driver.
    pub async fn submit_with_driver<F>(
        &mut self,
        driver: &dyn ModelDriver,
        request: ConversationRequest,
        config: TurnLoopConfig,
        cancellation: CancellationFlag,
        on_event: F,
    ) -> ClawinResult<EngineOutcome>
    where
        F: FnMut(EngineEvent),
    {
        self.submit_with_driver_and_resolver(
            driver,
            request,
            config,
            &PassthroughPermissionResolver,
            cancellation,
            on_event,
        )
        .await
    }

    /// Submit a prompt or slash command with an explicit permission resolver.
    pub async fn submit_with_driver_and_resolver<F>(
        &mut self,
        driver: &dyn ModelDriver,
        request: ConversationRequest,
        config: TurnLoopConfig,
        permission_resolver: &dyn clawin_core::PermissionResolver,
        cancellation: CancellationFlag,
        on_event: F,
    ) -> ClawinResult<EngineOutcome>
    where
        F: FnMut(EngineEvent),
    {
        let transcript_len_before = self.engine.transcript().len();
        if let Some(prompt) = request_prompt(&request) {
            persist_last_prompt(&self.runtime, prompt)?;
        }
        let services = EngineServices::new(
            &self.runtime,
            &self.commands,
            &self.tools,
            driver,
            permission_resolver,
            cancellation,
        );
        let result = self
            .engine
            .submit_message(&services, request, config, on_event)
            .await;
        persist_transcript_delta(
            &self.runtime,
            transcript_len_before,
            self.engine.transcript(),
        )?;
        result
    }

    fn into_repl_parts(
        self,
    ) -> (
        SessionRuntime,
        CommandRegistry,
        ToolRegistry,
        ConversationEngine,
    ) {
        (self.runtime, self.commands, self.tools, self.engine)
    }
}

fn resolve_bootstrap_mode<S>(
    mode: &SessionBootstrapMode,
    runtime: &SessionRuntime,
    store: &S,
) -> ClawinResult<Option<RestoredSession>>
where
    S: SessionStore,
{
    match mode {
        SessionBootstrapMode::Fresh => Ok(None),
        SessionBootstrapMode::Continue => {
            let session = store.resolve_resume(runtime, ResumeQuery::Continue)?;
            session
                .ok_or_else(|| clawin_core::ClawinError::InvalidConfiguration {
                    message: "no resumable session found in the current project scope".to_owned(),
                })
                .map(Some)
        }
        SessionBootstrapMode::Resume(token) => resolve_resume_token(runtime, store, token),
    }
}

fn resolve_resume_token<S>(
    runtime: &SessionRuntime,
    store: &S,
    token: &str,
) -> ClawinResult<Option<RestoredSession>>
where
    S: SessionStore,
{
    match resolve_resume_target(runtime, store, token)? {
        Some(session) => Ok(Some(session)),
        None if looks_like_transcript_path(token) => {
            Err(clawin_core::ClawinError::InvalidConfiguration {
                message: format!("no session transcript found at `{token}`"),
            })
        }
        None => Err(clawin_core::ClawinError::InvalidConfiguration {
            message: format!("no session matched `{token}`"),
        }),
    }
}

fn build_restored_runtime(
    original_cwd: &Path,
    capabilities: RuntimeCapabilities,
    permission_mode: PermissionMode,
    restored: &RestoredSession,
    session_store: Arc<dyn SessionStore>,
    worktree_manager: Arc<dyn WorktreeManager>,
) -> SessionRuntime {
    let runtime = SessionRuntime::new(
        restored.session_id.clone(),
        capabilities,
        original_cwd.to_path_buf(),
        restored.canonical_project_root.clone(),
        permission_mode,
    );
    runtime.set_active_project_root(restored.active_project_root.clone());
    runtime.set_current_cwd(restored.active_project_root.clone());
    runtime.set_active_worktree(restored.worktree_state.clone());
    runtime.set_session_transcript_path(restored.transcript_path.clone());
    runtime.set_session_store(session_store);
    runtime.set_worktree_manager(worktree_manager);
    runtime
}

fn request_prompt(request: &ConversationRequest) -> Option<&str> {
    match request {
        ConversationRequest::Prompt(prompt) => Some(prompt.as_str()),
        ConversationRequest::SlashCommand(_) => None,
    }
}

fn persist_last_prompt(runtime: &SessionRuntime, prompt: &str) -> ClawinResult<()> {
    if let Some(store) = runtime.session_store() {
        store.save_last_prompt(runtime, prompt)?;
    }
    Ok(())
}

fn persist_transcript_delta(
    runtime: &SessionRuntime,
    previous_len: usize,
    transcript: &[clawin_core::ConversationMessage],
) -> ClawinResult<()> {
    let Some(store) = runtime.session_store() else {
        return Ok(());
    };

    for message in transcript.iter().skip(previous_len) {
        store.append_message(runtime, message)?;
    }
    Ok(())
}

fn resolve_bridge_continue_pointer() -> Result<BridgePointer> {
    let original_cwd =
        std::env::current_dir().context("failed to read current working directory")?;
    let path_policy = ClawinPathPolicy;
    let git_adapter = Arc::new(SystemGitWorktreeAdapter);
    let config = clawin_config::load_startup_config(original_cwd.clone(), &path_policy)
        .context("failed to load startup config for remote control continue")?;
    let runtime = SessionRuntime::new(
        generate_session_id(),
        RuntimeCapabilities::new(false, false),
        original_cwd,
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );
    let store = BridgePointerStore::new(
        config.paths().clone(),
        path_policy,
        Arc::clone(&git_adapter),
    );

    store
        .resolve_continue(&runtime)?
        .ok_or_else(|| anyhow!("no valid bridge pointer found in the current project scope"))
}

struct UnavailableModelDriver;

impl ModelDriver for UnavailableModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        Box::pin(async {
            Err(clawin_core::ClawinError::NotImplemented {
                subsystem: "interactive model driver",
            })
        })
    }
}

struct UnavailablePrintModelDriver;

impl ModelDriver for UnavailablePrintModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        Box::pin(async {
            Err(clawin_core::ClawinError::ModelDriver {
                message: "headless model driver is not implemented yet".to_owned(),
            })
        })
    }
}
