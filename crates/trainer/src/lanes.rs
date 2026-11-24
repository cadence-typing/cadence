//! Selection lanes, quotas and repetition caps (L-05, VK-05).
//!
//! Adaptive words use blocks of `block_slots` scheduled word slots in
//! the fixed interleaved lane order. Feasible complete blocks carry the
//! 6/2/2 quota truthfully; fallback switches are explicit decisions
//! that never rewrite the requested lane and then claim the original
//! quota was fulfilled.

use std::collections::HashMap;

use crate::policy::{Lane, SelectionPolicy};
use cadence_core::{CoreError, WordId};

/// One scheduled slot in a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneSlot {
    /// The lane this slot was requested as.
    pub requested: Lane,
    /// The lane actually served (differs only via explicit fallback).
    pub served: Lane,
    /// The selected word (never chosen here; the planner fills it).
    pub word: WordId,
}

/// A complete or partial block with an honest feasibility record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPlan {
    /// Scheduled slots, in policy lane order.
    pub slots: Vec<LaneSlot>,
    /// Whether the block is complete and feasible: every slot served
    /// its requested lane.
    pub complete_feasible: bool,
    /// Fallback explanations (requested → served count), empty when
    /// feasible.
    pub fallbacks: Vec<(Lane, Lane, u32)>,
}

/// Tracks quota consumption and repetition/focus caps per block
/// (VK-05: counters never overrun; feasible blocks have the required
/// lane allocations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaTracker {
    policy: SelectionPolicy,
    served: HashMap<Lane, u32>,
    word_occurrences: HashMap<u64, u32>,
    focus_owned: HashMap<u64, u32>,
    consecutive: Option<u64>,
    consecutive_run: u32,
}

impl QuotaTracker {
    /// Creates a tracker for one block under the validated policy.
    #[must_use]
    pub fn new(policy: SelectionPolicy) -> Self {
        Self {
            policy,
            served: HashMap::new(),
            word_occurrences: HashMap::new(),
            focus_owned: HashMap::new(),
            consecutive: None,
            consecutive_run: 0,
        }
    }

    fn quota_for(&self, lane: Lane) -> u32 {
        match lane {
            Lane::Target => self.policy.target_slots,
            Lane::Review => self.policy.review_slots,
            Lane::Broad => self.policy.broad_slots,
        }
    }

    /// Whether serving `lane` now would exceed its quota.
    #[must_use]
    pub fn lane_open(&self, lane: Lane) -> bool {
        self.served.get(&lane).copied().unwrap_or(0) < self.quota_for(lane)
    }

    /// Whether `word` is admissible: occurrence cap, consecutive cap
    /// and (for the target lane) focus ownership cap.
    #[must_use]
    pub fn word_admissible(&self, word: WordId, lane: Lane, focus_id: Option<u64>) -> bool {
        let occurrences = self.word_occurrences.get(&word.get()).copied().unwrap_or(0);
        if occurrences >= self.policy.maximum_word_occurrences_per_block {
            return false;
        }
        if self.consecutive == Some(word.get())
            && self.consecutive_run >= self.policy.maximum_consecutive_word_occurrences
        {
            return false;
        }
        if lane == Lane::Target
            && let Some(focus) = focus_id
        {
            let owned = self.focus_owned.get(&focus).copied().unwrap_or(0);
            if owned >= self.policy.maximum_focus_owned_target_slots {
                return false;
            }
        }
        true
    }

    /// Records a served slot; rejects quota overruns with a typed
    /// error (counters never overrun).
    ///
    /// # Errors
    /// Returns [`CoreError::CapacityExhausted`] when the lane quota or
    /// a repetition cap would be exceeded.
    pub fn record(
        &mut self,
        lane: Lane,
        word: WordId,
        focus_id: Option<u64>,
    ) -> Result<(), CoreError> {
        let served = self.served.get(&lane).copied().unwrap_or(0);
        if served >= self.quota_for(lane) {
            return Err(CoreError::CapacityExhausted {
                limit: "lane quota for this block",
            });
        }
        let occurrences = self.word_occurrences.get(&word.get()).copied().unwrap_or(0);
        if occurrences >= self.policy.maximum_word_occurrences_per_block {
            return Err(CoreError::CapacityExhausted {
                limit: "word occurrence cap per block",
            });
        }
        if self.consecutive == Some(word.get())
            && self.consecutive_run >= self.policy.maximum_consecutive_word_occurrences
        {
            return Err(CoreError::CapacityExhausted {
                limit: "consecutive word cap",
            });
        }
        if lane == Lane::Target
            && let Some(focus) = focus_id
        {
            let owned = self.focus_owned.get(&focus).copied().unwrap_or(0);
            if owned >= self.policy.maximum_focus_owned_target_slots {
                return Err(CoreError::CapacityExhausted {
                    limit: "focus-owned target slots per block",
                });
            }
            self.focus_owned.insert(focus, owned.saturating_add(1));
        }
        if self.consecutive == Some(word.get()) {
            self.consecutive_run = self.consecutive_run.saturating_add(1);
        } else {
            self.consecutive = Some(word.get());
            self.consecutive_run = 1;
        }
        self.word_occurrences
            .insert(word.get(), occurrences.saturating_add(1));
        self.served.insert(lane, served.saturating_add(1));
        Ok(())
    }

    /// Whether all quotas are exactly met (complete feasible block).
    #[must_use]
    pub fn quotas_exactly_met(&self) -> bool {
        self.served.get(&Lane::Target).copied().unwrap_or(0) == self.policy.target_slots
            && self.served.get(&Lane::Review).copied().unwrap_or(0) == self.policy.review_slots
            && self.served.get(&Lane::Broad).copied().unwrap_or(0) == self.policy.broad_slots
    }

    /// The validated policy this tracker enforces.
    #[must_use]
    pub fn policy(&self) -> &SelectionPolicy {
        &self.policy
    }
}

/// The documented fallback order per lane (L-05): explicit only.
#[must_use]
pub fn fallback_order(lane: Lane) -> &'static [Lane] {
    match lane {
        Lane::Target => &[Lane::Target, Lane::Review, Lane::Broad],
        Lane::Review => &[Lane::Review, Lane::Broad, Lane::Target],
        Lane::Broad => &[Lane::Broad, Lane::Review, Lane::Target],
    }
}

/// Builds a block plan skeleton from the policy lane order: each slot
/// with its requested lane. The planner fills words and records
/// fallbacks through [`QuotaTracker::record`].
#[must_use]
pub fn block_skeleton(policy: &SelectionPolicy) -> Vec<Lane> {
    policy.lane_order.clone()
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;
    use crate::policy::parse_policy;

    fn selection() -> SelectionPolicy {
        let policy = parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v2.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap();
        policy.selection
    }

    fn word(id: u64) -> WordId {
        WordId::new(id).unwrap()
    }

    #[test]
    fn policy_lane_order_matches_documented_interleaving() {
        let policy = selection();
        // 6/2/2 in the documented interleaved order.
        let keys: Vec<&str> = block_skeleton(&policy)
            .iter()
            .map(|lane| lane.key())
            .collect();
        assert_eq!(
            keys,
            vec![
                "target", "broad", "target", "review", "target", "target", "broad", "target",
                "review", "target"
            ]
        );
    }

    #[test]
    fn quotas_never_overrun() {
        let mut tracker = QuotaTracker::new(selection());
        for id in 1..=2 {
            tracker.record(Lane::Review, word(id), None).unwrap();
        }
        match tracker.record(Lane::Review, word(9), None) {
            Err(CoreError::CapacityExhausted { limit }) => {
                assert!(limit.contains("lane quota"));
            }
            other => panic!("expected quota error, got {other:?}"),
        }
    }

    #[test]
    fn word_occurrence_cap_enforced() {
        let mut tracker = QuotaTracker::new(selection());
        let repeated = word(5);
        assert!(tracker.record(Lane::Target, repeated, Some(1)).is_ok());
        // A second occurrence in one block exceeds the shipped cap of 1.
        assert!(tracker.record(Lane::Target, repeated, Some(2)).is_err());
        assert!(tracker.record(Lane::Target, word(6), Some(2)).is_ok());
    }

    #[test]
    fn focus_owned_target_slots_capped_at_three() {
        let mut tracker = QuotaTracker::new(selection());
        let focus = 77u64;
        assert!(tracker.record(Lane::Target, word(1), Some(focus)).is_ok());
        assert!(tracker.record(Lane::Target, word(2), Some(focus)).is_ok());
        assert!(tracker.record(Lane::Target, word(3), Some(focus)).is_ok());
        // A fourth target slot for the same focus unit is refused.
        assert!(!tracker.word_admissible(word(4), Lane::Target, Some(focus)));
        // …but a different focus may still own a slot.
        assert!(tracker.word_admissible(word(4), Lane::Target, Some(78)));
    }

    #[test]
    fn complete_feasible_block_meets_6_2_2_exactly() {
        let mut tracker = QuotaTracker::new(selection());
        let mut next_word = 1u64;
        for lane in block_skeleton(&selection()) {
            let word = word(next_word);
            next_word = next_word.checked_add(1).unwrap();
            let focus = if lane == Lane::Target {
                Some(next_word.checked_rem(10).unwrap())
            } else {
                None
            };
            tracker.record(lane, word, focus).unwrap();
        }
        assert!(tracker.quotas_exactly_met());
    }

    #[test]
    fn fallback_orders_are_explicit() {
        assert_eq!(
            fallback_order(Lane::Target),
            &[Lane::Target, Lane::Review, Lane::Broad]
        );
        assert_eq!(
            fallback_order(Lane::Review),
            &[Lane::Review, Lane::Broad, Lane::Target]
        );
    }
}
