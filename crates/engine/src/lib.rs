//! `cadence-engine` — the single state owner (A-01, A-02).
//!
//! The public mutation API is conceptually
//! `reduce(&mut self, envelope: EventEnvelope) -> Transition`: bounded
//! owned effects, dirty/presentation information and domain outcomes.
//! Snapshot/view preparation is an explicit engine operation; Ratatui
//! paint receives `&PreparedView`. Paint cannot correct scroll state,
//! initiate I/O, alter counters or advance time.
//!
//! Must not own: Ratatui types, worker execution, blocking I/O. No
//! reverse edge into runtime/tui/driver.
//!
//! Status: the reducer, admission and view preparation land with P1/P3.
