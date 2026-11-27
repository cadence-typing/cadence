//! The save protocol machine (VP-03, #89): the proved
//! `cadence_kernel::save::step`, which `cadence_engine::storage` drives.

pub use cadence_kernel::save::{Command, Event, State, initial, step};
