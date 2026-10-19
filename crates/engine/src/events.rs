//! Typed input events and the admission envelope (A-02, L-03).
//!
//! Events are ordered and semantic: keys, pause, stage boundaries and
//! errors are never coalesced. Terminal input is stamped when read at
//! ingress (`at`), before the engine queue; the engine records
//! consumption order separately via `seq`.

use cadence_core::{EventSeq, MonoTick, TrainingChar};

/// A normalized key event after input-owner resolution (U-03): during
/// practice every printable character (including `q`, `?`, `:`, `/`,
/// digits) is training input; Esc pauses; Ctrl-C follows the documented
/// quit path and is never a training character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A content insertion at the typed tail.
    Content(TrainingChar),
    /// Delete one logical training unit (including the physical Q6
    /// top-numpad key mapped to Backspace — just another Backspace).
    Backspace,
    /// Enter: submits a numpad entry; navigation elsewhere.
    Enter,
    /// Escape: pause/controls during practice; back otherwise.
    Escape,
    /// Ctrl-C: documented quit/flush path.
    Interrupt,
    /// F2 changes visual text size without becoming scored content.
    ToggleTextSize,
    /// Any other non-training key (arrows, function keys, Tab) —
    /// admitted for accounting, interpreted per input owner.
    Other,
}

/// External events admitted to the reducer, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    /// Initializes durable identity once, before practice begins.
    StorageReady {
        /// Next unused session identity.
        next_session: cadence_core::SessionId,
        /// Local calendar day.
        day: i64,
        /// Previously durable history.
        history: cadence_core::progress::HistorySummary,
    },
    /// Terminal acknowledgement, matched against the entire pending key.
    SaveResult(cadence_core::progress::SaveResult),
    /// Paste is rejected and measurement is interrupted.
    PasteRejected,
    /// Focus loss interrupts measurement.
    FocusLost,
    /// A normalized key event.
    Key(Key),
    /// Display preference admitted through the reducer (CLI or explicit control).
    SetTextSize(crate::view::TextSize),
    /// Runtime terminal capability admitted independently of display preference.
    SetTextSupport(crate::view::TextSupport),
    /// Viewport geometry changed; layout-dependent state changes are
    /// explicit preparation events before paint (A-01).
    Resized {
        /// New column count.
        columns: u16,
        /// New row count.
        rows: u16,
    },
    /// Coalescible redraw/timer hint (A-04): explicit elapsed time is
    /// sampled at reduction, so a dropped tick cannot change a budget.
    Tick,
}

/// The sequenced admission envelope: one ordered external event with its
/// ingress timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    /// Arrival sequence within this stream.
    pub seq: EventSeq,
    /// Ingress timestamp (monotonic microseconds since the run origin).
    pub at: MonoTick,
    /// The event itself.
    pub kind: EventKind,
}
