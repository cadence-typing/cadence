//! Bounded candidate selection (L-05, VK-04). The utility and the scan are
//! the proved `cadence_kernel::select` functions (Lean: the utility is the
//! weighted mean, within `0..=10_000` and monotone in every term; the scan
//! returns only an eligible candidate, one with the greatest
//! `(utility, tie_break, id)`, `Empty` exactly when none is eligible and
//! `TooLarge` exactly when the vector exceeds the bound).
pub use cadence_kernel::select::{BASIS, Candidate, Scan, Terms, scan, utility};
