use thiserror::Error;

/// Shared error type for the Phase 1 workspace skeleton.
#[derive(Debug, Error)]
pub enum ClawinError {
    /// Returned when a subsystem exists but the concrete behavior is not yet migrated.
    #[error("{subsystem} is not implemented yet")]
    NotImplemented {
        /// The user-visible subsystem label.
        subsystem: &'static str,
    },

    /// Returned when static configuration assumptions are invalid.
    #[error("invalid configuration: {message}")]
    InvalidConfiguration {
        /// A machine-readable description of the issue.
        message: String,
    },

    /// Returned when a slash command references no registered implementation.
    #[error("unknown command: {name}")]
    UnknownCommand {
        /// The unresolved slash command name without the leading `/`.
        name: String,
    },

    /// Returned when a slash command cannot be parsed.
    #[error("invalid command invocation: {message}")]
    InvalidCommandInvocation {
        /// A machine-readable explanation of the parse failure.
        message: String,
    },

    /// Returned when a tool name cannot be resolved.
    #[error("unknown tool: {name}")]
    UnknownTool {
        /// The unresolved tool name.
        name: String,
    },

    /// Returned when a tool input payload fails schema validation.
    #[error("invalid input for tool {tool}: {message}")]
    ToolInputInvalid {
        /// The tool whose input failed validation.
        tool: String,
        /// A machine-readable explanation of the validation failure.
        message: String,
    },

    /// Returned when a tool implementation hits an internal execution failure.
    #[error("tool execution failed for {tool}: {message}")]
    ToolExecution {
        /// The tool whose execution failed.
        tool: String,
        /// A machine-readable explanation of the failure.
        message: String,
    },
}

/// Shared result alias for domain-layer operations.
pub type ClawinResult<T> = Result<T, ClawinError>;
