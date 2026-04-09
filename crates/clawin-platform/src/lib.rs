#![forbid(unsafe_code)]

//! Platform abstraction traits and baseline implementations for Clawin.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Cursor, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    Event as CrosstermEvent, KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent,
    KeyModifiers as CrosstermKeyModifiers,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{event, execute};
use serde_json::Value;

const PROJECT_DIRECTORY_NAME: &str = ".clawin";
const PROJECT_MANIFEST_NAME: &str = "CLAWIN.md";

/// Shell execution and process management abstraction.
pub trait ShellAdapter {
    /// Return a stable backend label.
    fn shell_name(&self) -> &'static str;
}

/// Secure storage abstraction.
pub trait SecureStorage {
    /// Store a secret under the provided key.
    fn put(&self, key: &str, value: &str);

    /// Load a secret by key.
    fn get(&self, key: &str) -> Option<String>;
}

/// Terminal capability abstraction.
pub trait TerminalCapabilities {
    /// Whether the current process can drive interactive terminal flows.
    fn is_interactive(&self) -> bool;

    /// Whether the current process supports color output.
    fn supports_color(&self) -> bool;
}

/// Path normalization and naming policy abstraction.
pub trait PathPolicy {
    /// Resolve the user home directory used for Clawin global storage.
    fn home_dir(&self) -> Option<PathBuf>;

    /// Normalize a path for use as a stable config key.
    fn normalize_for_config_key(&self, path: &Path) -> String;

    /// Sanitize a path into a filesystem-safe session directory fragment.
    fn sanitize_for_session_dir(&self, path: &Path) -> String {
        let normalized = self.normalize_for_config_key(path);
        let mut sanitized = String::with_capacity(normalized.len());
        let mut previous_was_separator = false;

        for ch in normalized.chars() {
            let lower = ch.to_ascii_lowercase();
            let mapped = match lower {
                'a'..='z' | '0'..='9' | '-' | '_' => Some(lower),
                '/' | '\\' | ':' | '.' => Some('-'),
                _ => None,
            };

            let Some(mapped) = mapped else {
                continue;
            };

            if mapped == '-' {
                if previous_was_separator {
                    continue;
                }
                previous_was_separator = true;
            } else {
                previous_was_separator = false;
            }

            sanitized.push(mapped);
        }

        sanitized.trim_matches('-').to_owned()
    }

    /// Return the reserved project metadata directory name.
    fn project_directory_name(&self) -> &'static str;

    /// Return the reserved project manifest name.
    fn project_manifest_name(&self) -> &'static str;
}

/// Browser and external launcher abstraction.
pub trait BrowserLauncher {
    /// Return a stable backend label.
    fn launcher_name(&self) -> &'static str;
}

/// Portable process spawn request used by MCP and future shell-backed integrations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessSpawnRequest {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Spawned process handle with stdout pipe ownership and controlled teardown.
pub trait SpawnedProcess: Send {
    /// Take ownership of the process stdout pipe once.
    fn take_stdout(&mut self) -> std::io::Result<Box<dyn Read + Send>>;

    /// Write bytes to process stdin.
    fn write_stdin(&mut self, bytes: &[u8]) -> std::io::Result<()>;

    /// Flush process stdin.
    fn flush_stdin(&mut self) -> std::io::Result<()>;

    /// Terminate the process.
    fn kill(&mut self) -> std::io::Result<()>;

    /// Poll the process exit status.
    fn try_wait(&mut self) -> std::io::Result<Option<i32>>;
}

/// Process spawning abstraction kept at the platform layer per ADR-0004.
pub trait ProcessSpawner: Send + Sync {
    /// Spawn a new process using the provided command, arguments, and environment overrides.
    fn spawn(&self, request: &ProcessSpawnRequest) -> std::io::Result<Box<dyn SpawnedProcess>>;
}

/// Portable git worktree descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorktree {
    path: PathBuf,
    branch: Option<String>,
    head_commit: Option<String>,
    main_worktree: bool,
}

impl GitWorktree {
    /// Create a git worktree descriptor.
    pub fn new(
        path: PathBuf,
        branch: Option<String>,
        head_commit: Option<String>,
        main_worktree: bool,
    ) -> Self {
        Self {
            path,
            branch,
            head_commit,
            main_worktree,
        }
    }

    /// Worktree path on disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current branch name when known.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Current HEAD commit when known.
    pub fn head_commit(&self) -> Option<&str> {
        self.head_commit.as_deref()
    }

    /// Whether this entry represents the canonical main worktree.
    pub fn is_main_worktree(&self) -> bool {
        self.main_worktree
    }
}

/// Git/worktree abstraction kept at the platform layer per ADR-0004.
pub trait GitWorktreeAdapter: Send + Sync {
    /// Resolve the canonical git root for the provided path.
    fn canonical_git_root(&self, path: &Path) -> std::io::Result<Option<PathBuf>>;

    /// Resolve the current branch name for the provided working tree.
    fn current_branch(&self, path: &Path) -> std::io::Result<Option<String>>;

    /// Resolve the current HEAD commit for the provided working tree.
    fn head_commit(&self, path: &Path) -> std::io::Result<Option<String>>;

    /// List worktrees currently attached to a repository root.
    fn list_worktrees(&self, repo_root: &Path) -> std::io::Result<Vec<GitWorktree>>;

    /// Create a new git worktree for the provided repository root.
    fn create_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> std::io::Result<GitWorktree>;

    /// Remove a git worktree from the provided repository root.
    fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
        force: bool,
    ) -> std::io::Result<()>;

    /// Whether the working tree currently has uncommitted changes.
    fn is_dirty(&self, path: &Path) -> std::io::Result<bool>;
}

/// Portable terminal size snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    width: u16,
    height: u16,
}

impl TerminalSize {
    /// Create a terminal size snapshot.
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Width in terminal cells.
    pub const fn width(self) -> u16 {
        self.width
    }

    /// Height in terminal cells.
    pub const fn height(self) -> u16 {
        self.height
    }
}

/// Portable terminal key code used by the REPL event loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalKeyCode {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Esc,
}

/// Minimal modifier snapshot needed for Phase 5 bindings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalKeyModifiers {
    control: bool,
}

impl TerminalKeyModifiers {
    pub const NONE: Self = Self { control: false };
    pub const CONTROL: Self = Self { control: true };

    /// Whether the control modifier is pressed.
    pub const fn control(self) -> bool {
        self.control
    }
}

/// Portable terminal key event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalKeyEvent {
    code: TerminalKeyCode,
    modifiers: TerminalKeyModifiers,
}

impl TerminalKeyEvent {
    /// Create a key event from the provided code and modifiers.
    pub const fn new(code: TerminalKeyCode, modifiers: TerminalKeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Convenience constructor for plain character input.
    pub const fn from_char(ch: char) -> Self {
        Self::new(TerminalKeyCode::Char(ch), TerminalKeyModifiers::NONE)
    }

    /// Key code for this event.
    pub const fn code(self) -> TerminalKeyCode {
        self.code
    }

    /// Modifier snapshot for this event.
    pub const fn modifiers(self) -> TerminalKeyModifiers {
        self.modifiers
    }
}

/// Portable terminal events surfaced to the REPL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalEvent {
    Key(TerminalKeyEvent),
    Resize(TerminalSize),
}

/// Terminal session abstraction covering raw-mode lifecycle, I/O, and event polling.
pub trait TerminalSession {
    /// Enter interactive terminal mode.
    fn enter(&mut self) -> std::io::Result<()>;

    /// Leave interactive terminal mode and restore the terminal.
    fn leave(&mut self) -> std::io::Result<()>;

    /// Current terminal size.
    fn size(&self) -> TerminalSize;

    /// Poll the next terminal event within the provided timeout.
    fn poll_event(&mut self, timeout: Duration) -> std::io::Result<Option<TerminalEvent>>;

    /// Writable output target used by the REPL renderer.
    fn writer(&mut self) -> &mut dyn Write;
}

/// No-op shell adapter placeholder.
#[derive(Clone, Debug, Default)]
pub struct NoopShellAdapter;

impl ShellAdapter for NoopShellAdapter {
    fn shell_name(&self) -> &'static str {
        "noop-shell"
    }
}

/// In-memory secure storage placeholder.
#[derive(Clone, Debug, Default)]
pub struct InMemorySecureStorage {
    entries: Arc<Mutex<BTreeMap<String, String>>>,
}

impl SecureStorage for InMemorySecureStorage {
    fn put(&self, key: &str, value: &str) {
        self.entries
            .lock()
            .expect("storage lock should be available")
            .insert(key.to_owned(), value.to_owned());
    }

    fn get(&self, key: &str) -> Option<String> {
        self.entries
            .lock()
            .expect("storage lock should be available")
            .get(key)
            .cloned()
    }
}

/// Static terminal capabilities placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticTerminalCapabilities {
    interactive: bool,
    color: bool,
}

impl StaticTerminalCapabilities {
    /// Create a static terminal capability snapshot.
    pub fn new(interactive: bool, color: bool) -> Self {
        Self { interactive, color }
    }
}

impl TerminalCapabilities for StaticTerminalCapabilities {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn supports_color(&self) -> bool {
        self.color
    }
}

/// Runtime-detected terminal capabilities for the current process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemTerminalCapabilities {
    interactive: bool,
    color: bool,
}

impl SystemTerminalCapabilities {
    /// Snapshot the terminal capabilities from the running process.
    pub fn detect() -> Self {
        let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        let color = interactive
            && std::env::var_os("NO_COLOR").is_none()
            && std::env::var("TERM")
                .map(|term| term != "dumb")
                .unwrap_or(true);

        Self { interactive, color }
    }
}

impl TerminalCapabilities for SystemTerminalCapabilities {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn supports_color(&self) -> bool {
        self.color
    }
}

/// Runtime terminal session backed by crossterm and stdout.
#[derive(Debug)]
pub struct SystemTerminalSession {
    stdout: std::io::Stdout,
    entered: bool,
}

impl SystemTerminalSession {
    /// Create a system terminal session that can later enter raw/alternate mode.
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            stdout: std::io::stdout(),
            entered: false,
        })
    }
}

impl TerminalSession for SystemTerminalSession {
    fn enter(&mut self) -> std::io::Result<()> {
        if self.entered {
            return Ok(());
        }

        enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen)?;
        self.entered = true;
        Ok(())
    }

    fn leave(&mut self) -> std::io::Result<()> {
        if !self.entered {
            return Ok(());
        }

        execute!(self.stdout, LeaveAlternateScreen)?;
        disable_raw_mode()?;
        self.entered = false;
        Ok(())
    }

    fn size(&self) -> TerminalSize {
        let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
        TerminalSize::new(width, height)
    }

    fn poll_event(&mut self, timeout: Duration) -> std::io::Result<Option<TerminalEvent>> {
        if !event::poll(timeout)? {
            return Ok(None);
        }

        loop {
            match event::read()? {
                CrosstermEvent::Key(key) => {
                    if let Some(mapped) = map_key_event(key) {
                        return Ok(Some(TerminalEvent::Key(mapped)));
                    }
                }
                CrosstermEvent::Resize(width, height) => {
                    return Ok(Some(TerminalEvent::Resize(TerminalSize::new(
                        width, height,
                    ))));
                }
                _ => return Ok(None),
            }
        }
    }

    fn writer(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }
}

/// In-memory terminal session used by REPL and bootstrap tests.
#[derive(Debug)]
pub struct FakeTerminalSession {
    size: TerminalSize,
    events: VecDeque<Option<TerminalEvent>>,
    output: Cursor<Vec<u8>>,
    entered: bool,
    left: bool,
}

impl FakeTerminalSession {
    /// Create a fake terminal with scripted events.
    pub fn new(size: TerminalSize, scripted_events: Vec<Option<TerminalEvent>>) -> Self {
        Self {
            size,
            events: VecDeque::from(scripted_events),
            output: Cursor::new(Vec::new()),
            entered: false,
            left: false,
        }
    }

    /// Whether `enter()` has been called.
    pub fn entered(&self) -> bool {
        self.entered
    }

    /// Whether `leave()` has been called.
    pub fn left(&self) -> bool {
        self.left
    }

    /// Read the captured output buffer as UTF-8 lossily.
    pub fn output(&self) -> String {
        String::from_utf8_lossy(self.output.get_ref()).into_owned()
    }
}

impl TerminalSession for FakeTerminalSession {
    fn enter(&mut self) -> std::io::Result<()> {
        self.entered = true;
        Ok(())
    }

    fn leave(&mut self) -> std::io::Result<()> {
        self.left = true;
        Ok(())
    }

    fn size(&self) -> TerminalSize {
        self.size
    }

    fn poll_event(&mut self, _timeout: Duration) -> std::io::Result<Option<TerminalEvent>> {
        let next = self.events.pop_front().unwrap_or(None);
        if let Some(TerminalEvent::Resize(size)) = next {
            self.size = size;
        }
        Ok(next)
    }

    fn writer(&mut self) -> &mut dyn Write {
        &mut self.output
    }
}

/// Fixed Clawin naming policy for project metadata and path normalization.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClawinPathPolicy;

impl PathPolicy for ClawinPathPolicy {
    fn home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }

    fn normalize_for_config_key(&self, path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn project_directory_name(&self) -> &'static str {
        PROJECT_DIRECTORY_NAME
    }

    fn project_manifest_name(&self) -> &'static str {
        PROJECT_MANIFEST_NAME
    }
}

/// Runtime git/worktree adapter backed by the system `git` executable.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemGitWorktreeAdapter;

impl GitWorktreeAdapter for SystemGitWorktreeAdapter {
    fn canonical_git_root(&self, path: &Path) -> std::io::Result<Option<PathBuf>> {
        Ok(run_git(path, ["rev-parse", "--show-toplevel"])?
            .map(|stdout| PathBuf::from(stdout.trim())))
    }

    fn current_branch(&self, path: &Path) -> std::io::Result<Option<String>> {
        Ok(run_git(path, ["branch", "--show-current"])?
            .map(|stdout| stdout.trim().to_owned())
            .filter(|branch| !branch.is_empty()))
    }

    fn head_commit(&self, path: &Path) -> std::io::Result<Option<String>> {
        Ok(run_git(path, ["rev-parse", "HEAD"])?
            .map(|stdout| stdout.trim().to_owned())
            .filter(|commit| !commit.is_empty()))
    }

    fn list_worktrees(&self, repo_root: &Path) -> std::io::Result<Vec<GitWorktree>> {
        let Some(stdout) = run_git(repo_root, ["worktree", "list", "--porcelain"])? else {
            return Ok(Vec::new());
        };

        Ok(parse_worktree_list(&stdout))
    }

    fn create_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> std::io::Result<GitWorktree> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "add", "-b", branch_name])
            .arg(worktree_path)
            .output()?;
        ensure_git_success(output)?;

        Ok(GitWorktree::new(
            worktree_path.to_path_buf(),
            Some(branch_name.to_owned()),
            self.head_commit(worktree_path)?,
            false,
        ))
    }

    fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        _branch_name: &str,
        force: bool,
    ) -> std::io::Result<()> {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "remove"]);
        if force {
            command.arg("--force");
        }
        let output = command.arg(worktree_path).output()?;
        ensure_git_success(output)
    }

    fn is_dirty(&self, path: &Path) -> std::io::Result<bool> {
        Ok(run_git(path, ["status", "--porcelain"])?
            .map(|stdout| !stdout.trim().is_empty())
            .unwrap_or(false))
    }
}

/// No-op browser launcher placeholder.
#[derive(Clone, Debug, Default)]
pub struct NoopBrowserLauncher;

impl BrowserLauncher for NoopBrowserLauncher {
    fn launcher_name(&self) -> &'static str {
        "noop-browser"
    }
}

/// Runtime process spawner backed by `std::process::Command`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessSpawner;

impl ProcessSpawner for SystemProcessSpawner {
    fn spawn(&self, request: &ProcessSpawnRequest) -> std::io::Result<Box<dyn SpawnedProcess>> {
        let mut command = Command::new(&request.command);
        command.args(&request.args);
        command.envs(&request.env);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("spawned process did not expose stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("spawned process did not expose stdout pipe"))?;

        Ok(Box::new(SystemSpawnedProcess {
            child,
            stdin: Some(stdin),
            stdout: Some(stdout),
        }))
    }
}

struct SystemSpawnedProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
}

impl SpawnedProcess for SystemSpawnedProcess {
    fn take_stdout(&mut self) -> std::io::Result<Box<dyn Read + Send>> {
        self.stdout
            .take()
            .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
            .ok_or_else(|| std::io::Error::other("stdout pipe already taken"))
    }

    fn write_stdin(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stdin pipe is unavailable"))?
            .write_all(bytes)
    }

    fn flush_stdin(&mut self) -> std::io::Result<()> {
        self.stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stdin pipe is unavailable"))?
            .flush()
    }

    fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        self.child
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or_default()))
    }
}

/// Captured process spawn invocation for deterministic tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpawnInvocation {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Scripted stdout plan returned by `FakeProcessSpawner`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeProcessPlan {
    stdout_bytes: Vec<u8>,
    exit_code: Option<i32>,
}

impl FakeProcessPlan {
    /// Create a plan from raw stdout bytes.
    pub fn from_stdout(stdout_bytes: Vec<u8>) -> Self {
        Self {
            stdout_bytes,
            exit_code: None,
        }
    }

    /// Create a plan from a list of framed JSON-RPC messages.
    pub fn from_json_messages(messages: Vec<Value>) -> Self {
        let stdout_bytes = messages
            .into_iter()
            .flat_map(frame_json_message)
            .collect::<Vec<_>>();
        Self::from_stdout(stdout_bytes)
    }

    /// Attach a final exit code to the plan.
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.exit_code = Some(exit_code);
        self
    }
}

/// In-memory process spawner used by MCP tests.
#[derive(Clone, Debug, Default)]
pub struct FakeProcessSpawner {
    plans: Arc<Mutex<VecDeque<FakeProcessPlan>>>,
    invocations: Arc<Mutex<Vec<SpawnInvocation>>>,
}

#[derive(Clone, Debug, Default)]
struct FakeRepository {
    worktrees: Vec<GitWorktree>,
    dirty: BTreeMap<PathBuf, bool>,
}

/// In-memory git/worktree adapter used by Phase 7A tests.
#[derive(Clone, Debug, Default)]
pub struct FakeGitWorktreeAdapter {
    repositories: Arc<Mutex<BTreeMap<PathBuf, FakeRepository>>>,
}

impl FakeGitWorktreeAdapter {
    /// Create an empty fake adapter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a repository root and its known worktrees.
    pub fn register_repository(&self, repo_root: PathBuf, worktrees: Vec<PathBuf>) {
        let worktrees = worktrees
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                GitWorktree::new(path, None, Some("fake-head".to_owned()), index == 0)
            })
            .collect::<Vec<_>>();
        self.repositories
            .lock()
            .expect("fake git repositories lock should be available")
            .insert(
                repo_root,
                FakeRepository {
                    worktrees,
                    dirty: BTreeMap::new(),
                },
            );
    }

    /// Set the dirty flag for a known worktree path.
    pub fn set_dirty(&self, path: &Path, dirty: bool) -> std::io::Result<()> {
        let Some((repo_root, _)) = self.find_repository_for_path(path) else {
            return Err(std::io::Error::other("repository not registered"));
        };

        let mut repositories = self
            .repositories
            .lock()
            .expect("fake git repositories lock should be available");
        let repository = repositories
            .get_mut(&repo_root)
            .expect("registered repository should still exist");
        repository.dirty.insert(path.to_path_buf(), dirty);
        Ok(())
    }

    /// Resolve the canonical git root for a path.
    pub fn canonical_git_root(&self, path: &Path) -> std::io::Result<Option<PathBuf>> {
        GitWorktreeAdapter::canonical_git_root(self, path)
    }

    /// Resolve the current branch for a path.
    pub fn current_branch(&self, path: &Path) -> std::io::Result<Option<String>> {
        GitWorktreeAdapter::current_branch(self, path)
    }

    /// Resolve the current HEAD commit for a path.
    pub fn head_commit(&self, path: &Path) -> std::io::Result<Option<String>> {
        GitWorktreeAdapter::head_commit(self, path)
    }

    /// List registered worktrees for a repository root.
    pub fn list_worktrees(&self, repo_root: &Path) -> std::io::Result<Vec<GitWorktree>> {
        GitWorktreeAdapter::list_worktrees(self, repo_root)
    }

    /// Create and register a fake worktree.
    pub fn create_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> std::io::Result<GitWorktree> {
        GitWorktreeAdapter::create_worktree(self, repo_root, worktree_path, branch_name)
    }

    /// Remove a registered fake worktree.
    pub fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
        force: bool,
    ) -> std::io::Result<()> {
        GitWorktreeAdapter::remove_worktree(self, repo_root, worktree_path, branch_name, force)
    }

    /// Whether the provided path is currently marked dirty.
    pub fn is_dirty(&self, path: &Path) -> std::io::Result<bool> {
        GitWorktreeAdapter::is_dirty(self, path)
    }

    fn find_repository_for_path(&self, path: &Path) -> Option<(PathBuf, FakeRepository)> {
        self.repositories
            .lock()
            .expect("fake git repositories lock should be available")
            .iter()
            .find(|(repo_root, repository)| {
                path.starts_with(repo_root)
                    || repository
                        .worktrees
                        .iter()
                        .any(|worktree| path.starts_with(worktree.path()))
            })
            .map(|(repo_root, repository)| (repo_root.clone(), repository.clone()))
    }
}

impl GitWorktreeAdapter for FakeGitWorktreeAdapter {
    fn canonical_git_root(&self, path: &Path) -> std::io::Result<Option<PathBuf>> {
        Ok(self
            .find_repository_for_path(path)
            .map(|(repo_root, _)| repo_root))
    }

    fn current_branch(&self, path: &Path) -> std::io::Result<Option<String>> {
        Ok(self
            .find_repository_for_path(path)
            .and_then(|(_, repository)| {
                repository
                    .worktrees
                    .into_iter()
                    .find(|worktree| worktree.path() == path)
                    .and_then(|worktree| worktree.branch().map(str::to_owned))
            }))
    }

    fn head_commit(&self, path: &Path) -> std::io::Result<Option<String>> {
        Ok(self
            .find_repository_for_path(path)
            .and_then(|(_, repository)| {
                repository
                    .worktrees
                    .into_iter()
                    .find(|worktree| worktree.path() == path)
                    .and_then(|worktree| worktree.head_commit().map(str::to_owned))
            }))
    }

    fn list_worktrees(&self, repo_root: &Path) -> std::io::Result<Vec<GitWorktree>> {
        Ok(self
            .repositories
            .lock()
            .expect("fake git repositories lock should be available")
            .get(repo_root)
            .map(|repository| repository.worktrees.clone())
            .unwrap_or_default())
    }

    fn create_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> std::io::Result<GitWorktree> {
        let worktree = GitWorktree::new(
            worktree_path.to_path_buf(),
            Some(branch_name.to_owned()),
            Some("fake-head".to_owned()),
            false,
        );
        let mut repositories = self
            .repositories
            .lock()
            .expect("fake git repositories lock should be available");
        let repository = repositories
            .get_mut(repo_root)
            .ok_or_else(|| std::io::Error::other("repository not registered"))?;
        repository.worktrees.push(worktree.clone());
        Ok(worktree)
    }

    fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
        _branch_name: &str,
        _force: bool,
    ) -> std::io::Result<()> {
        let mut repositories = self
            .repositories
            .lock()
            .expect("fake git repositories lock should be available");
        let repository = repositories
            .get_mut(repo_root)
            .ok_or_else(|| std::io::Error::other("repository not registered"))?;
        repository
            .worktrees
            .retain(|worktree| worktree.path() != worktree_path);
        repository.dirty.remove(worktree_path);
        Ok(())
    }

    fn is_dirty(&self, path: &Path) -> std::io::Result<bool> {
        let Some((repo_root, _)) = self.find_repository_for_path(path) else {
            return Err(std::io::Error::other("repository not registered"));
        };

        let repositories = self
            .repositories
            .lock()
            .expect("fake git repositories lock should be available");
        let repository = repositories
            .get(&repo_root)
            .expect("registered repository should still exist");
        Ok(repository.dirty.get(path).copied().unwrap_or(false))
    }
}

impl FakeProcessSpawner {
    /// Create a scripted fake spawner.
    pub fn new(plans: Vec<FakeProcessPlan>) -> Self {
        Self {
            plans: Arc::new(Mutex::new(VecDeque::from(plans))),
            invocations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Borrow the spawn invocations recorded so far.
    pub fn invocations(&self) -> Vec<SpawnInvocation> {
        self.invocations
            .lock()
            .expect("fake process invocations lock should be available")
            .clone()
    }
}

impl ProcessSpawner for FakeProcessSpawner {
    fn spawn(&self, request: &ProcessSpawnRequest) -> std::io::Result<Box<dyn SpawnedProcess>> {
        self.invocations
            .lock()
            .expect("fake process invocations lock should be available")
            .push(SpawnInvocation {
                command: request.command.clone(),
                args: request.args.clone(),
                env: request.env.clone(),
            });

        let Some(plan) = self
            .plans
            .lock()
            .expect("fake process plans lock should be available")
            .pop_front()
        else {
            return Err(std::io::Error::other("no fake process plan available"));
        };

        Ok(Box::new(FakeSpawnedProcess {
            stdout: Some(Cursor::new(plan.stdout_bytes)),
            stdin: Cursor::new(Vec::new()),
            killed: false,
            exit_code: plan.exit_code,
        }))
    }
}

struct FakeSpawnedProcess {
    stdout: Option<Cursor<Vec<u8>>>,
    stdin: Cursor<Vec<u8>>,
    killed: bool,
    exit_code: Option<i32>,
}

impl SpawnedProcess for FakeSpawnedProcess {
    fn take_stdout(&mut self) -> std::io::Result<Box<dyn Read + Send>> {
        self.stdout
            .take()
            .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
            .ok_or_else(|| std::io::Error::other("stdout pipe already taken"))
    }

    fn write_stdin(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.stdin.write_all(bytes)
    }

    fn flush_stdin(&mut self) -> std::io::Result<()> {
        self.stdin.flush()
    }

    fn kill(&mut self) -> std::io::Result<()> {
        self.killed = true;
        Ok(())
    }

    fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        if self.killed {
            Ok(Some(self.exit_code.unwrap_or_default()))
        } else {
            Ok(self.exit_code)
        }
    }
}

fn map_key_event(event: CrosstermKeyEvent) -> Option<TerminalKeyEvent> {
    let modifiers = if event.modifiers.contains(CrosstermKeyModifiers::CONTROL) {
        TerminalKeyModifiers::CONTROL
    } else {
        TerminalKeyModifiers::NONE
    };

    let code = match event.code {
        CrosstermKeyCode::Char(ch) => TerminalKeyCode::Char(ch),
        CrosstermKeyCode::Enter => TerminalKeyCode::Enter,
        CrosstermKeyCode::Backspace => TerminalKeyCode::Backspace,
        CrosstermKeyCode::Delete => TerminalKeyCode::Delete,
        CrosstermKeyCode::Left => TerminalKeyCode::Left,
        CrosstermKeyCode::Right => TerminalKeyCode::Right,
        CrosstermKeyCode::Home => TerminalKeyCode::Home,
        CrosstermKeyCode::End => TerminalKeyCode::End,
        CrosstermKeyCode::Esc => TerminalKeyCode::Esc,
        _ => return None,
    };

    Some(TerminalKeyEvent::new(code, modifiers))
}

fn frame_json_message(value: Value) -> Vec<u8> {
    let payload = serde_json::to_vec(&value).expect("json message should serialize");
    let mut framed = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
    framed.extend(payload);
    framed
}

fn run_git<I, S>(path: &Path, args: I) -> std::io::Result<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }

    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

fn ensure_git_success(output: std::process::Output) -> std::io::Result<()> {
    if output.status.success() {
        return Ok(());
    }

    Err(std::io::Error::other(
        String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    ))
}

fn parse_worktree_list(stdout: &str) -> Vec<GitWorktree> {
    let mut worktrees = Vec::new();
    let mut current_path = None;
    let mut current_branch = None;
    let mut current_head = None;

    for line in stdout.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = current_path.take() {
                worktrees.push(GitWorktree::new(
                    path,
                    current_branch.take(),
                    current_head.take(),
                    worktrees.is_empty(),
                ));
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(value.to_owned());
            continue;
        }
        if let Some(value) = line.strip_prefix("HEAD ") {
            current_head = Some(value.to_owned());
        }
    }

    worktrees
}
