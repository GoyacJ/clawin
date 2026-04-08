use std::fmt;

/// Stable identifier for a process/session scope.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Build a session identifier from a static label.
    pub fn from_static(value: &'static str) -> Self {
        Self(value.to_owned())
    }

    /// Build a session identifier from an owned value.
    pub fn from_owned(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the raw identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable identifier for a conversation inside a session.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ConversationId(String);

impl ConversationId {
    /// Build a conversation identifier from a static label.
    pub fn from_static(value: &'static str) -> Self {
        Self(value.to_owned())
    }

    /// Borrow the raw identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Monotonic identifier for a single turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TurnId(u64);

impl TurnId {
    /// Create a new turn identifier from a monotonic number.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Borrow the raw numeric value.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TurnId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}
