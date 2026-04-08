#![forbid(unsafe_code)]

//! Conversation engine placeholders for the Phase 1 skeleton.

use clawin_core::{SessionId, TurnId};

/// Minimal conversation engine shell used to freeze crate boundaries.
#[derive(Clone, Debug)]
pub struct ConversationEngine {
    session_id: SessionId,
    turn_count: u64,
}

impl ConversationEngine {
    /// Create a new placeholder engine bound to a session.
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            turn_count: 0,
        }
    }

    /// Borrow the current session identifier.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Return the number of turns started in this placeholder engine.
    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }

    /// Advance the placeholder engine by one turn.
    pub fn begin_turn(&mut self) -> TurnId {
        self.turn_count += 1;
        TurnId::new(self.turn_count)
    }
}
