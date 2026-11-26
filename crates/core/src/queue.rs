//! Same-session reinforcement admission (L-05, VK-06). The decision is the
//! proved `cadence_kernel::queue::admit` (Lean: an admitted word is not
//! already queued, the queue stays within capacity, and no word is repeated
//! more than its per-session limit).
pub use cadence_kernel::queue::{Admit, admit};
