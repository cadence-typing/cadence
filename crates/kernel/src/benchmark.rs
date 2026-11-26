//! Benchmark probes (P-03, #63): when a transfer probe is due, and whether
//! a probe result may be compared with earlier ones.
//!
//! A probe is due after every `every` completed sessions that included
//! prose, if an unseen probe passage is available; when none is, the
//! answer says so instead of silently skipping. A result is comparable only
//! when the whole probe window was typed without a pause, an early end or a
//! paste, in a live (not synthetic) session, on the same layout and in the
//! same strata class as the earlier probes.

/// Whether this session carries a probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Probe now.
    Probe,
    /// Not yet: fewer completed sessions than the interval.
    NotYet,
    /// Due, but no unseen probe passage is left.
    NoPassage,
}

/// What happened to one probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// The whole probe window was typed.
    pub completed_window: bool,
    /// Practice paused during the probe.
    pub paused: bool,
    /// The session ended during the probe.
    pub ended_early: bool,
    /// A paste was refused during the probe.
    pub pasted: bool,
    /// The session was synthetic (headless driver), not live.
    pub synthetic: bool,
    /// The keyboard layout matches the earlier probes'.
    pub same_layout: bool,
    /// The passage's strata class matches the earlier probes'.
    pub same_strata: bool,
}

/// Whether a probe is due after `completed_since_probe` completed sessions
/// with prose, every `every` sessions, given whether an unseen passage is
/// available.
#[must_use]
pub fn due(completed_since_probe: u64, every: u64, unseen_available: bool) -> Due {
    if completed_since_probe < every {
        return Due::NotYet;
    }
    if unseen_available {
        Due::Probe
    } else {
        Due::NoPassage
    }
}

/// Whether a probe result may be compared with earlier probes.
#[must_use]
pub fn comparable(facts: Facts) -> bool {
    facts.completed_window
        && !facts.paused
        && !facts.ended_early
        && !facts.pasted
        && !facts.synthetic
        && facts.same_layout
        && facts.same_strata
}
