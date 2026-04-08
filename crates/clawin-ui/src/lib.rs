#![forbid(unsafe_code)]

//! TUI placeholders for the Phase 1 skeleton.

/// Fixed TUI renderer direction from ADR-0003.
pub const TUI_RENDERER: &str = "ratatui";

/// Fixed terminal event backend from ADR-0003.
pub const TUI_EVENT_BACKEND: &str = "crossterm";

/// Minimal UI surface marker for future REPL/TUI work.
#[derive(Clone, Debug, Default)]
pub struct UiSurface;

impl UiSurface {
    /// Return the renderer selected for the Phase 1 skeleton.
    pub fn renderer(&self) -> &'static str {
        TUI_RENDERER
    }

    /// Return the terminal event backend selected for the Phase 1 skeleton.
    pub fn event_backend(&self) -> &'static str {
        TUI_EVENT_BACKEND
    }
}
