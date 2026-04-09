// Phase 5 tests continue under DIFF-2026-001: terminal lifecycle and event handling live behind Clawin-owned platform abstractions.

use clawin_platform::{
    FakeTerminalSession, TerminalEvent, TerminalKeyCode, TerminalKeyEvent, TerminalKeyModifiers,
    TerminalSession, TerminalSize,
};

#[test]
fn fake_terminal_session_tracks_lifecycle_and_scripted_events() {
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(120, 40),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('a'))),
            Some(TerminalEvent::Resize(TerminalSize::new(100, 30))),
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
        ],
    );

    terminal.enter().expect("enter should succeed");
    assert_eq!(terminal.size(), TerminalSize::new(120, 40));
    assert_eq!(
        terminal
            .poll_event(std::time::Duration::from_millis(0))
            .expect("first poll should succeed"),
        Some(TerminalEvent::Key(TerminalKeyEvent::from_char('a')))
    );
    assert_eq!(
        terminal
            .poll_event(std::time::Duration::from_millis(0))
            .expect("resize poll should succeed"),
        Some(TerminalEvent::Resize(TerminalSize::new(100, 30)))
    );
    assert_eq!(
        terminal
            .poll_event(std::time::Duration::from_millis(0))
            .expect("idle poll should succeed"),
        None
    );
    assert_eq!(
        terminal
            .poll_event(std::time::Duration::from_millis(0))
            .expect("enter poll should succeed"),
        Some(TerminalEvent::Key(TerminalKeyEvent::new(
            TerminalKeyCode::Enter,
            TerminalKeyModifiers::NONE,
        )))
    );
    terminal.leave().expect("leave should succeed");

    assert!(terminal.entered());
    assert!(terminal.left());
}
