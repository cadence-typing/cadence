//! Live terminal lifecycle and ingress (A-01, A-02, L-03).
//!
//! The runtime owns terminal setup/teardown and crossterm ingress with
//! ingress-time stamping; the composition root (the `cadence` binary)
//! owns the owner loop that feeds the engine reducer and the shared
//! paint path. This keeps `cadence-runtime` free of any UI dependency:
//! key-ownership decisions (U-03) belong to the engine, not here.

use std::io::Stdout;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;

use cadence_core::{LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::{EventKind, Key};

/// Ingress poll cadence (A-04: coalescible timer hint; explicit elapsed
/// time is sampled at reduction, so a dropped tick cannot change a
/// budget). A named policy constant, not a naked literal.
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Failures of terminal handling.
#[derive(Debug)]
pub enum TerminalError {
    /// Setup, teardown, polling or drawing failed.
    Io(String),
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "terminal error: {message}"),
        }
    }
}

impl std::error::Error for TerminalError {}

/// Owns the live terminal session: raw mode + alternate screen entered
/// on construction, restored on explicit [`TerminalGuard::restore`] with
/// a best-effort drop fallback (every exit path restores the user's
/// terminal, including errors).
pub struct TerminalGuard {
    terminal: Option<ratatui::Terminal<CrosstermBackend<Stdout>>>,
}

impl TerminalGuard {
    /// Enters raw mode and the alternate screen.
    ///
    /// # Errors
    /// Returns [`TerminalError`] when setup fails; anything already
    /// entered is restored before the error is returned.
    pub fn enter() -> Result<Self, TerminalError> {
        enable_raw_mode().map_err(io_error)?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(io_error(error));
        }
        let backend = CrosstermBackend::new(stdout);
        match ratatui::Terminal::new(backend) {
            Ok(terminal) => Ok(Self {
                terminal: Some(terminal),
            }),
            Err(error) => {
                let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(io_error(error))
            }
        }
    }

    /// Draws one frame through the caller's painter closure (the shared
    /// paint path; the runtime owns lifecycle, not layout).
    ///
    /// # Errors
    /// Returns [`TerminalError`] on draw failure.
    pub fn draw<P>(&mut self, painter: P) -> Result<(), TerminalError>
    where
        P: FnOnce(&mut ratatui::Frame<'_>),
    {
        let Some(terminal) = self.terminal.as_mut() else {
            return Err(TerminalError::Io("terminal already restored".to_string()));
        };
        terminal
            .draw(|frame| painter(frame))
            .map(|_| ())
            .map_err(io_error)
    }

    /// Restores the terminal explicitly (cursor, alternate screen, raw
    /// mode). Prefer this over relying on drop so failures surface.
    ///
    /// # Errors
    /// Returns [`TerminalError`] when restoration fails.
    pub fn restore(mut self) -> Result<(), TerminalError> {
        let result = self.restore_inner();
        self.terminal = None;
        result
    }

    fn restore_inner(&mut self) -> Result<(), TerminalError> {
        if let Some(terminal) = self.terminal.as_mut() {
            let _ = terminal.show_cursor();
        }
        execute!(std::io::stdout(), LeaveAlternateScreen).map_err(io_error)?;
        disable_raw_mode().map_err(io_error)
    }

    /// Waits one poll window and reads at most one event, translated to
    /// the engine's normalized kind. `None` means the poll timed out
    /// (a coalescible timer hint, A-04).
    ///
    /// # Errors
    /// Returns [`TerminalError`] on poll/read failure.
    pub fn next_event(&mut self) -> Result<Option<EventKind>, TerminalError> {
        if !event::poll(POLL_TIMEOUT).map_err(io_error)? {
            return Ok(None);
        }
        let event = event::read().map_err(io_error)?;
        Ok(Some(translate(event)))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort fallback; errors are silent because Drop cannot
        // propagate them, and the explicit path reports first.
        if self.terminal.is_some() {
            let _ = self.restore_inner();
        }
    }
}

/// Microseconds since the loop origin (checked conversion).
#[must_use]
pub fn elapsed_since(origin: Instant) -> MonoTick {
    let micros = origin.elapsed().as_micros();
    MonoTick::from_us(u64::try_from(micros).unwrap_or(u64::MAX))
}

fn io_error(error: std::io::Error) -> TerminalError {
    TerminalError::Io(error.to_string())
}

/// Translates a crossterm event into the engine's normalized kinds
/// (L-02 alphabet enforcement at admission; key ownership is the
/// engine's decision, not the translator's).
fn translate(event: CrosstermEvent) -> EventKind {
    match event {
        CrosstermEvent::Key(key) => EventKind::Key(translate_key(key)),
        CrosstermEvent::Resize(columns, rows) => EventKind::Resized { columns, rows },
        CrosstermEvent::Paste(_) => {
            // Bracketed paste is rejected as training evidence (L-03);
            // P3 raises the classified paste event. Until then it is not
            // keystroke input.
            EventKind::Key(Key::Other)
        }
        _ => EventKind::Key(Key::Other),
    }
}

fn translate_key(key: KeyEvent) -> Key {
    // Release events carry no input; held-key auto-repeat is ignored
    // where the terminal reports the kind (L-03).
    if key.kind == KeyEventKind::Release {
        return Key::Other;
    }
    match key.code {
        crossterm::event::KeyCode::Char(character) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl-C follows the documented quit path; other control
                // combinations are not training characters (U-03).
                if character == 'c' {
                    Key::Interrupt
                } else {
                    Key::Other
                }
            } else if let Ok(training) = TrainingChar::new(character, LayoutSupportsPound::US) {
                Key::Content(training)
            } else {
                Key::Other
            }
        }
        // The physical Q6 top-numpad key mapped to Backspace is simply
        // another Backspace event (U-03).
        crossterm::event::KeyCode::Backspace | crossterm::event::KeyCode::Delete => Key::Backspace,
        crossterm::event::KeyCode::Enter => Key::Enter,
        crossterm::event::KeyCode::Esc => Key::Escape,
        _ => Key::Other,
    }
}
