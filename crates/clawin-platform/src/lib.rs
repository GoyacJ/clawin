#![forbid(unsafe_code)]

//! Platform abstraction traits and placeholder implementations for Phase 1.

use std::collections::BTreeMap;
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

/// Fixed Clawin naming policy for project metadata.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClawinPathPolicy;

impl PathPolicy for ClawinPathPolicy {
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
