//! The review ladder (L-06, VK-06). The decisions are the proved
//! `cadence_kernel::review` functions (Lean: an error resets to the first
//! rung, the next due day is in the future, clean typing before the due day
//! is practice, promotion needs two clean days, the rung stays on the
//! ladder, and a completion before the recorded clean day is refused).
pub use cadence_kernel::review::{
    Fifo, FifoStep, State, Step, Streak, fifo_admit, fifo_contains, fifo_serve, is_due,
    ladder_step, valid_ladder,
};
