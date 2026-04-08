#![forbid(unsafe_code)]

//! Tool registry placeholders for the Phase 1 skeleton.

/// Minimal tool registry used to freeze crate boundaries.
#[derive(Clone, Debug, Default)]
pub struct ToolRegistry {
    tool_names: Vec<String>,
}

impl ToolRegistry {
    /// Create an empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a placeholder tool name.
    pub fn register(&mut self, tool_name: impl Into<String>) {
        self.tool_names.push(tool_name.into());
    }

    /// Borrow the registered tool names.
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }
}
