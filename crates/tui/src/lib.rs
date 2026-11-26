//! `cadence-tui` — pure presentation (A-01, U-01, U-02).
//!
//! Owns: theme/palette adaptation, pure layout and paint from
//! `&PreparedView`, and shared Ratatui cells plus native line metadata
//! consumed by live presentation and headless capture.
//!
//! Must not own: mutating learning/application state, filesystem access
//! or jobs. Paint is a total function of the prepared view, theme and
//! viewport; it is total for every nonzero bounded viewport including
//! 1×1, without panic or out-of-bounds access.

mod destinations;
mod modeline;
mod native_text;
pub mod paint;
mod practice;
mod settings;
mod stages;
pub mod theme;

pub use paint::paint;
pub use theme::{Palette, SemanticRole};
