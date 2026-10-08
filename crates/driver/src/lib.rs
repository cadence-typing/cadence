//! `cadence-driver` — headless protocol and replay (A-01, H-01..H-04).
//!
//! Owns: the v1 NDJSON request/response protocol with strict validation
//! and the bounded response cache, ordered request admission, virtual
//! clock and fixture service capabilities, causal/durable barriers, and
//! trace recording/replay through the same engine reducer and shared
//! renderer as live mode.
//!
//! Must not own: an alternative engine or renderer. `TestBackend`
//! replaces only the terminal output adapter.
//!
//! Status: protocol and drivers land with P4.
