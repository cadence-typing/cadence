//! The decisions Cadence's scores and durable progress rest on, as total
//! `no_std` functions.
//!
//! `cadence-core` (attempt ledger, metrics, progress) and `cadence-data`
//! (checkpoint admission) call these functions for every decision named
//! here; nothing is verified as a copy. The functions listed in
//! `[package.metadata.charon] start_from` are translated to Lean 4 by Charon
//! and Aeneas and the theorems under `proofs/Proofs` are stated over that
//! translation (see `docs/TAUTEN.md`).
//!
//! The code is written for translation: no derives are reached from the
//! translated functions, no floats, no iterators or closures (explicit
//! `while` loops over `get`), and no panicking arithmetic or indexing.
#![no_std]

pub mod barrier;
pub mod benchmark;
pub mod calendar;
pub mod checkpoint;
pub mod counters;
pub mod cycle;
pub mod evidence;
pub mod first;
pub mod focus;
pub mod label;
pub mod lanes;
pub mod ledger;
pub mod pace;
pub mod plan;
pub mod progress;
pub mod queue;
pub mod ratio;
pub mod request;
pub mod review;
pub mod select;
pub mod session;
pub mod slots;
pub mod time;
