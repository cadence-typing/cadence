//! Terminal ownership and normalized ingress. No scoring or UI policy here.

use std::io::{self, Stdout};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::{presenter::Presenter, text_protocol};
use cadence_core::{LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::view::{LineRendition, TextSupport};
use cadence_engine::{EventKind, Key};
use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Size;
use signal_hook::{
    SigId,
    consts::{SIGINT, SIGTERM},
    low_level::unregister,
};

/// Poll at most this long so save results and phase deadlines are serviced.
const POLL_TIMEOUT: Duration = Duration::from_millis(20);

/// Terminal/clock adapter failures, never substituted with a valid timestamp.
#[derive(Debug)]
pub enum TerminalError {
    /// A terminal operation failed.
    Io(io::Error),
    /// The run outlived the representable microsecond clock.
    ClockExhausted,
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "terminal error: {error}"),
            Self::ClockExhausted => f.write_str("run clock exhausted; measurement stopped"),
        }
    }
}
impl std::error::Error for TerminalError {}

#[derive(PartialEq, Eq)]
enum Lifecycle {
    Active,
    RestoreAttempted,
}

/// Owns raw mode, alternate screen, paste/focus reporting and the renderer.
pub struct TerminalGuard {
    interrupted: Arc<AtomicBool>,
    signal_ids: Vec<SigId>,
    terminal: Presenter<Stdout>,
    lifecycle: Lifecycle,
}

impl TerminalGuard {
    /// Enters a terminal session. Any partial setup is restored on failure.
    pub fn enter() -> Result<Self, TerminalError> {
        enable_raw_mode().map_err(TerminalError::Io)?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableFocusChange
        )
        .map_err(setup_error)?;
        let (columns, rows) = crossterm::terminal::size().map_err(setup_error)?;
        let support = text_protocol::probe(&mut stdout, columns, crossterm::cursor::position)
            .map_err(setup_error)?;
        let terminal = Presenter::new(stdout, Size::new(columns, rows), support);
        let mut guard = Self {
            terminal,
            lifecycle: Lifecycle::Active,
            interrupted: Arc::new(AtomicBool::new(false)),
            signal_ids: Vec::new(),
        };
        for signal in [SIGINT, SIGTERM] {
            let id = signal_hook::flag::register(signal, Arc::clone(&guard.interrupted))
                .map_err(TerminalError::Io)?;
            guard.signal_ids.push(id);
        }
        Ok(guard)
    }

    /// Draws through the caller's shared painter without mutating product state.
    pub fn draw<P>(&mut self, painter: P) -> Result<(), TerminalError>
    where
        P: FnOnce(&mut Buffer, &mut [LineRendition]),
    {
        let size = self.terminal.size().map_err(TerminalError::Io)?;
        self.terminal.draw(size, painter).map_err(TerminalError::Io)
    }

    /// Actual terminal geometry for reducer-owned minimum-size handling.
    pub fn viewport(&self) -> Result<(u16, u16), TerminalError> {
        let size = self.terminal.size().map_err(TerminalError::Io)?;
        Ok((size.width, size.height))
    }

    /// Observed startup capability, admitted separately through the engine reducer.
    pub const fn text_support(&self) -> TextSupport {
        self.terminal.text_support()
    }

    /// Explicit teardown surfaces errors; Drop remains the fallback.
    pub fn restore(mut self) -> Result<(), TerminalError> {
        self.restore_state().map_err(TerminalError::Io)
    }

    fn restore_state(&mut self) -> io::Result<()> {
        let rows = self.terminal.restore_rows();
        let terminal = restore_terminal();
        // Never retry row commands after leaving the alternate screen.
        self.lifecycle = Lifecycle::RestoreAttempted;
        rows.and(terminal)
    }

    /// Reads at most one semantic event. A timeout is a replaceable clock hint.
    pub fn next_event(&mut self) -> Result<Option<EventKind>, TerminalError> {
        if self.interrupted.swap(false, Ordering::Relaxed) {
            return Ok(Some(EventKind::Key(Key::Interrupt)));
        }
        if !event::poll(POLL_TIMEOUT).map_err(TerminalError::Io)? {
            return Ok(None);
        }
        let event = event::read().map_err(TerminalError::Io)?;
        Ok(Some(translate(event)))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.lifecycle == Lifecycle::Active
            && let Err(error) = self.restore_state()
        {
            eprintln!("cadence: terminal restoration failed: {error}");
        }
        for id in self.signal_ids.drain(..) {
            if !unregister(id) {
                eprintln!("cadence: terminal signal handler was already removed");
            }
        }
    }
}

fn setup_error(error: io::Error) -> TerminalError {
    if let Err(restore) = restore_terminal() {
        eprintln!("cadence: terminal setup cleanup failed: {restore}");
    }
    TerminalError::Io(error)
}

fn restore_terminal() -> io::Result<()> {
    // Always attempt raw-mode cleanup even if the output stream failed.
    let display = execute!(
        io::stdout(),
        Show,
        DisableBracketedPaste,
        DisableFocusChange,
        LeaveAlternateScreen
    );
    let raw = disable_raw_mode();
    display.and(raw)
}

/// Checked monotonic microseconds since the process timing origin.
pub fn elapsed_since(origin: Instant) -> Result<MonoTick, TerminalError> {
    u64::try_from(origin.elapsed().as_micros())
        .map(MonoTick::from_us)
        .map_err(|_| TerminalError::ClockExhausted)
}

fn translate(event: Event) -> EventKind {
    match event {
        Event::Key(key) => EventKind::Key(translate_key(key)),
        Event::Resize(columns, rows) => EventKind::Resized { columns, rows },
        Event::Paste(_) => EventKind::PasteRejected,
        Event::FocusLost => EventKind::FocusLost,
        _ => EventKind::Key(Key::Other),
    }
}

fn translate_key(key: KeyEvent) -> Key {
    if key.kind != KeyEventKind::Press {
        return Key::Other;
    }
    match key.code {
        KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if character == 'c' {
                Key::Interrupt
            } else {
                Key::Other
            }
        }
        KeyCode::Char(_)
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            Key::Other
        }
        KeyCode::Char(character) => match TrainingChar::new(character, LayoutSupportsPound::UK) {
            Ok(training) => Key::Content(training),
            Err(_) => Key::Other,
        },
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        KeyCode::F(2) => Key::ToggleTextSize,
        _ => Key::Other,
    }
}
