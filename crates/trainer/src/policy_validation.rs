//! Resource and cross-field bounds for decoded or programmatically supplied policies.
use crate::policy::{Lane, PolicyError, SessionPolicy, TrainerPolicy};

fn require(condition: bool, what: &'static str) -> Result<(), PolicyError> {
    if condition {
        Ok(())
    } else {
        Err(PolicyError::Inconsistent { what })
    }
}
impl SessionPolicy {
    /// Validates allocation arrays and supported resource bounds.
    ///
    /// # Errors
    /// Rejects duplicate phases, mismatched arrays, invalid defaults and budgets.
    pub fn validate(&self) -> Result<(), PolicyError> {
        let count = self.phase_order.len();
        require(
            (1..=6).contains(&count)
                && count == self.phase_seconds.len()
                && count == self.phase_minimum_seconds.len(),
            "session arrays must match and contain 1..=6 phases",
        )?;
        let distinct: std::collections::BTreeSet<_> = self.phase_order.iter().collect();
        require(distinct.len() == count, "session phases must be unique")?;
        require(
            self.minimum_budget_seconds > 0
                && self.minimum_budget_seconds <= self.default_budget_seconds
                && self.default_budget_seconds <= self.maximum_budget_seconds
                && self.maximum_budget_seconds <= 86_400,
            "session budgets must be ordered, positive and at most one day",
        )?;
        let total = self
            .phase_seconds
            .iter()
            .try_fold(0u64, |sum, value| sum.checked_add(*value));
        require(
            total == Some(self.default_budget_seconds),
            "default phase seconds must sum to default budget",
        )?;
        require(
            self.phase_seconds
                .iter()
                .zip(&self.phase_minimum_seconds)
                .all(|(seconds, minimum)| *minimum > 0 && seconds >= minimum),
            "phase minimums must be positive and no greater than defaults",
        )?;
        let minimum = self
            .phase_minimum_seconds
            .iter()
            .try_fold(0u64, |sum, value| sum.checked_add(*value));
        require(
            minimum.is_some_and(|sum| sum <= self.minimum_budget_seconds),
            "minimum budget must satisfy all phase minimums",
        )?;
        require(
            self.phase_seconds
                .iter()
                .zip(&self.phase_minimum_seconds)
                .any(|(seconds, minimum)| seconds > minimum),
            "default allocation needs positive residual weight",
        )
    }

    /// Residual default allocations are the weights for every budget.
    ///
    /// # Errors
    /// Rejects invalid session policies rather than truncating parallel arrays.
    pub fn allocation_weights(&self) -> Result<Vec<u64>, PolicyError> {
        self.validate()?;
        self.phase_seconds
            .iter()
            .zip(&self.phase_minimum_seconds)
            .map(|(seconds, minimum)| {
                seconds
                    .checked_sub(*minimum)
                    .ok_or(PolicyError::Inconsistent {
                        what: "phase minimum exceeds default",
                    })
            })
            .collect()
    }
}
impl TrainerPolicy {
    /// Validates every planner policy before allocation or materialization.
    ///
    /// # Errors
    /// Rejects unsupported bounds and inconsistent arrays or thresholds.
    pub fn validate(&self) -> Result<(), PolicyError> {
        self.session.validate()?;
        let s = &self.selection;
        require(
            (1..=256).contains(&s.block_slots) && s.lane_order.len() == s.block_slots,
            "selection block must have 1..=256 matching lane slots",
        )?;
        require(
            (s.block_slots..=4096).contains(&s.maximum_candidates_per_slot),
            "candidate bound must cover a block and be at most 4096",
        )?;
        for (lane, quota) in [
            (Lane::Target, s.target_slots),
            (Lane::Review, s.review_slots),
            (Lane::Broad, s.broad_slots),
        ] {
            require(
                u64::try_from(s.lane_order.iter().filter(|value| **value == lane).count()).ok()
                    == Some(u64::from(quota)),
                "lane counts must exactly match quotas",
            )?;
        }
        let slots = u32::try_from(s.block_slots).map_err(|_| PolicyError::Inconsistent {
            what: "block slots overflow",
        })?;
        require(
            (1..=slots).contains(&s.maximum_word_occurrences_per_block)
                && (1..=s.maximum_word_occurrences_per_block)
                    .contains(&s.maximum_consecutive_word_occurrences),
            "repetition caps must be positive and bounded by block size",
        )?;
        require(
            (1..=slots).contains(&s.maximum_focus_owned_target_slots),
            "focus cap must be positive and bounded by block size",
        )?;
        let stats = self.statistics;
        require(
            (1..=65_536).contains(&stats.recent_occurrence_window)
                && (1..=stats.recent_occurrence_window)
                    .contains(&stats.minimum_observations_for_confident_label)
                && (1..=65_536).contains(&stats.minimum_sessions_for_confident_label),
            "statistics confidence bounds must be positive and fit the evidence window",
        )?;
        require(
            (1..=1_000_000).contains(&stats.error_prior_alpha)
                && (1..=1_000_000).contains(&stats.error_prior_beta),
            "error priors must be positive and bounded",
        )?;
        require(
            self.weights.total() > 0,
            "utility weight sum must be positive",
        )?;
        let r = &self.review;
        require(
            !r.interval_days.is_empty()
                && r.interval_days.len() <= 64
                && r.interval_days.first() == Some(&1)
                && r.interval_days.iter().all(|day| (1..=36_500).contains(day))
                && r.interval_days
                    .windows(2)
                    .all(|pair| matches!(pair, [left, right] if left < right)),
            "review ladder must start at one day and strictly increase within 100 years",
        )?;
        require(
            r.independent_clean_sessions_to_promote == 2
                && r.maximum_same_session_reinforcements <= 1,
            "review implementation supports two independent clean sessions and at most one reinforcement",
        )?;
        let p = self.pace;
        require(
            p.desired_first_pass.get() > p.slow_down_below.get() && p.slow_down_below.get() > 0,
            "pace thresholds must be positive and ordered",
        )?;
        require(
            p.maximum_raise.get() > 0 && p.maximum_lower.get() > 0,
            "pace adjustment bounds must be positive",
        )?;
        require(
            (1..=65_536).contains(&p.qualifying_blocks_to_raise)
                && (1..=p.qualifying_blocks_to_raise).contains(&p.minimum_sessions_to_raise)
                && (1..=1_000_000).contains(&p.minimum_first_attempt_positions_per_block),
            "pace evidence bounds must be positive and coherent",
        )?;
        let b = self.benchmark;
        require(
            (1..=65_536).contains(&b.every_completed_sessions)
                && (1..=86_400).contains(&b.probe_seconds)
                && (1..=86_400).contains(&b.remaining_prose_seconds)
                && b.require_unseen_text
                && b.exclude_interrupted
                && b.exclude_synthetic,
            "benchmark must have bounded positive cadence and comparable unseen nonsynthetic evidence",
        )?;
        if let Some((_, seconds)) = self
            .session
            .phase_order
            .iter()
            .zip(&self.session.phase_seconds)
            .find(|(phase, _)| **phase == crate::policy::Phase::Prose)
        {
            require(
                b.probe_seconds.checked_add(b.remaining_prose_seconds) == Some(*seconds),
                "benchmark replacement must match default prose allocation",
            )?;
        }
        Ok(())
    }
}
