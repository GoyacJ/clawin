#![forbid(unsafe_code)]

//! Shared types, errors, and runtime models used across the Clawin workspace.

mod error;
mod ids;
mod runtime;

pub use error::{ClawinError, ClawinResult};
pub use ids::{ConversationId, SessionId, TurnId};
pub use runtime::{RuntimeCapabilities, SessionRuntime};
