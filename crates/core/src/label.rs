//! Item labels and list cycling (#39). The decisions are the proved
//! `cadence_kernel::label::of` (Lean: `Fresh` exactly when nothing makes the
//! item a repetition; `Again` exactly for a reinforcement) and
//! `cadence_kernel::cycle::advance` (`wrapped` exactly at the last index).
pub use cadence_kernel::cycle::{Advance, advance};
pub use cadence_kernel::label::{Facts, Label, of};
