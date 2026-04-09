use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::{StructuredInputMessage, StructuredOutputMessage};
use crate::runtime::SessionRuntime;
use crate::{ClawinResult, SessionId};

/// Entry mode used when attaching a remote control bridge to a local session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeMode {
    Standalone,
    ReplAttached,
}

impl BridgeMode {
    /// Stable label used in status rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::ReplAttached => "repl_attached",
        }
    }
}

/// Lifecycle state for the remote control bridge worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeState {
    Ready,
    Connected,
    Reconnecting,
    Failed,
    Stopped,
}

impl BridgeState {
    /// Stable label used in status rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Connected => "connected",
            Self::Reconnecting => "reconnecting",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Source marker persisted alongside bridge pointers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgePointerSource {
    Standalone,
    Repl,
}

impl BridgePointerSource {
    /// Stable label used in status rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Repl => "repl",
        }
    }
}

/// Persisted local pointer for reconnecting a bridge session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgePointer {
    pub bridge_session_id: String,
    pub environment_id: String,
    pub source: BridgePointerSource,
    pub local_session_id: SessionId,
    pub transcript_path: PathBuf,
}

/// Stable bridge status snapshot exposed to commands, bootstrap, and UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeStatusSnapshot {
    pub state: BridgeState,
    pub mode: Option<BridgeMode>,
    pub source: Option<BridgePointerSource>,
    pub name: Option<String>,
    pub bridge_session_id: Option<String>,
    pub environment_id: Option<String>,
    pub local_session_id: Option<SessionId>,
    pub transcript_path: Option<PathBuf>,
    pub last_error: Option<String>,
}

impl Default for BridgeStatusSnapshot {
    fn default() -> Self {
        Self {
            state: BridgeState::Ready,
            mode: None,
            source: None,
            name: None,
            bridge_session_id: None,
            environment_id: None,
            local_session_id: None,
            transcript_path: None,
            last_error: None,
        }
    }
}

impl BridgeStatusSnapshot {
    /// Create a stable placeholder status before any bridge session starts.
    pub fn ready() -> Self {
        Self::default()
    }
}

/// Slash-command actions that request bridge lifecycle changes from the controller.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BridgeCommandAction {
    Start {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Stop,
}

/// Channel-oriented local session host reused by standalone and REPL bridge workers.
pub trait BridgeSessionHost: Send + Sync {
    /// Forward one remote-originated structured input message into the local session.
    fn send_input(&self, message: StructuredInputMessage) -> ClawinResult<()>;

    /// Receive the next structured output emitted by the local session, waiting up to `timeout`.
    fn recv_output(&self, timeout: Duration) -> ClawinResult<Option<StructuredOutputMessage>>;

    /// Notify the host that the transport disconnected or is shutting down.
    fn notify_transport_closed(&self, reason: &str) -> ClawinResult<()>;
}

/// Runtime-visible bridge lifecycle control surface injected by bootstrap.
pub trait BridgeController: Send + Sync {
    /// Return the current bridge status snapshot.
    fn status(&self) -> ClawinResult<BridgeStatusSnapshot>;

    /// Start or continue a bridge worker against the provided local session host.
    fn start(
        &self,
        runtime: &SessionRuntime,
        host: Arc<dyn BridgeSessionHost>,
        mode: BridgeMode,
        source: BridgePointerSource,
        name: Option<String>,
        pointer: Option<BridgePointer>,
    ) -> ClawinResult<BridgeStatusSnapshot>;

    /// Stop the currently active bridge worker, if one exists.
    fn stop(&self) -> ClawinResult<BridgeStatusSnapshot>;

    /// Wait for the active bridge worker to reach a terminal state.
    fn wait_for_terminal_state(&self) -> ClawinResult<BridgeStatusSnapshot>;
}
