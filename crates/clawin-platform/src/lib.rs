#![forbid(unsafe_code)]

//! Platform abstraction traits and baseline implementations for Clawin.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// No-op browser launcher placeholder.
#[derive(Clone, Debug, Default)]
pub struct NoopBrowserLauncher;

impl BrowserLauncher for NoopBrowserLauncher {
    fn launcher_name(&self) -> &'static str {
        "noop-browser"
    }
}
