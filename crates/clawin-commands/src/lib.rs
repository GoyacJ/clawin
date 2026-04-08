#![forbid(unsafe_code)]

//! Command registry placeholders for the Phase 1 skeleton.

/// Minimal command registry used to freeze crate boundaries.
#[derive(Clone, Debug, Default)]
pub struct CommandRegistry {
    command_names: Vec<String>,
}

impl CommandRegistry {
    /// Create an empty command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a placeholder command name.
    pub fn register(&mut self, command_name: impl Into<String>) {
        self.command_names.push(command_name.into());
    }

    /// Borrow the registered command names.
    pub fn command_names(&self) -> &[String] {
        &self.command_names
    }
}
