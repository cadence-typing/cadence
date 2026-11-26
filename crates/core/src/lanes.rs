//! Block quotas and repetition caps (L-05, VK-05). The decisions are the
//! proved `cadence_kernel::lanes` functions (Lean: a recorded slot never
//! overruns its lane quota, the word occurrence cap, the consecutive cap or
//! the focus-owned cap; an inadmissible word is never recorded; a block is
//! complete exactly when every quota is met).
pub use cadence_kernel::lanes::{Admit, Facts, Record, admissible, complete, record};
