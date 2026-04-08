#![forbid(unsafe_code)]

//! External integration placeholders for the Phase 1 skeleton.

/// Minimal integration hub used to reserve the crate boundary.
#[derive(Clone, Debug, Default)]
pub struct IntegrationHub {
    providers: Vec<String>,
}

impl IntegrationHub {
    /// Create an empty integration hub.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a placeholder provider label.
    pub fn register_provider(&mut self, provider: impl Into<String>) {
        self.providers.push(provider.into());
    }

    /// Borrow the registered provider labels.
    pub fn providers(&self) -> &[String] {
        &self.providers
    }
}
