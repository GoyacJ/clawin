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
}

/// Shared result alias for domain-layer operations.
pub type ClawinResult<T> = Result<T, ClawinError>;
