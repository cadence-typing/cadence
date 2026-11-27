//! Test-only fault hooks for the persistence campaigns (#88). Compiled only
//! with the `test-faults` feature; a release build has none (`xtask dist`
//! refuses a binary that contains the hook's variable name).
//!
//! `CADENCE_FAULT_STORE="POINT"` or `"POINT:N"` aborts the process, as a
//! kill would, at the `N`-th time (default the first) it reaches `POINT`:
//! no destructor runs and no transaction is rolled back by the program.
//! `CADENCE_FAULT_STORE=double-apply` makes the store fold every new chunk
//! twice: the campaigns' negative calibration.

#[cfg(feature = "test-faults")]
mod enabled {
    use std::sync::atomic::{AtomicU64, Ordering};

    static HITS: AtomicU64 = AtomicU64::new(0);

    fn spec() -> Option<(String, u64)> {
        let value = std::env::var("CADENCE_FAULT_STORE").ok()?;
        Some(match value.split_once(':') {
            Some((point, nth)) => (point.to_string(), nth.parse().ok()?),
            None => (value, 1),
        })
    }

    pub(crate) fn point(name: &str) {
        if let Some((point, nth)) = spec()
            && point == name
            && HITS.fetch_add(1, Ordering::SeqCst).saturating_add(1) == nth
        {
            eprintln!("cadence: test fault: abort at {name} #{nth}");
            std::process::abort();
        }
    }

    pub(crate) fn double_apply() -> bool {
        spec().is_some_and(|(point, _)| point == "double-apply")
    }
}

#[cfg(feature = "test-faults")]
pub(crate) use enabled::{double_apply, point};

/// No hook in a build without `test-faults`.
#[cfg(not(feature = "test-faults"))]
#[inline]
pub(crate) const fn point(_name: &str) {}

/// Never, in a build without `test-faults`.
#[cfg(not(feature = "test-faults"))]
#[inline]
pub(crate) const fn double_apply() -> bool {
    false
}
