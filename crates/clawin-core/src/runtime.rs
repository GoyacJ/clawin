use std::time::SystemTime;

use crate::SessionId;

/// Minimal process/session-scoped capabilities exposed during Phase 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    interactive_terminal: bool,
    mcp_available: bool,
}

impl RuntimeCapabilities {
    /// Create a new runtime capability snapshot.
    pub fn new(interactive_terminal: bool, mcp_available: bool) -> Self {
        Self {
            interactive_terminal,
            mcp_available,
        }
    }

    /// Whether the current process can drive an interactive terminal UI.
    pub fn interactive_terminal(self) -> bool {
        self.interactive_terminal
    }

    /// Whether MCP transports are currently wired.
    pub fn mcp_available(self) -> bool {
        self.mcp_available
    }
}

/// Minimal session-scoped runtime state placeholder.
#[derive(Clone, Debug)]
pub struct SessionRuntime {
    session_id: SessionId,
    launched_at: SystemTime,
    capabilities: RuntimeCapabilities,
}

impl SessionRuntime {
    /// Create a new runtime container for the current process/session.
    pub fn new(session_id: SessionId, capabilities: RuntimeCapabilities) -> Self {
        Self {
            session_id,
            launched_at: SystemTime::now(),
            capabilities,
        }
    }

    /// Borrow the current session identifier.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Read the launch timestamp.
    pub fn launched_at(&self) -> SystemTime {
        self.launched_at
    }

    /// Read the currently known runtime capabilities.
    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities
    }
}
