//! `cadence-tui` — pure presentation (A-01, U-01, U-02).
//!
//! Owns: theme/palette adaptation, pure layout and paint from
//! `&PreparedView`, and the single shared Ratatui draw + cell-capture path
//! used by both live mode and the headless driver's `TestBackend`.
//!
//! Must not own: mutating learning/application state, filesystem access
//! or jobs. Paint is a total function of the prepared view, theme and
//! viewport; it is total for every nonzero bounded viewport including
//! 1×1, without panic or out-of-bounds access.
//!
//! Status: themes and the shared draw path land with P1/P5.

pub mod theme;

pub use theme::{Palette, SemanticRole, ThemeError};
