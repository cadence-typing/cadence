//! FIFO due-review queue (L-05, L-06, VK-06).
//!
//! Review takes the oldest continuously eligible item. Admission order
//! is monotonic when an item first becomes due; a newly imported
//! overdue item joins at the tail rather than jumping ahead. An item
//! cannot be queued twice. Following review, the item is removed and
//! its next due date computed. Two clean completions on different
//! sessions/days promote an item; an error returns it to the 1-day
//! level. Wall-clock rollback cannot make items instantly overdue or
//! grant future success (guarded at admission/service time).

use std::collections::HashSet;
use std::collections::VecDeque;

use cadence_core::{CoreError, UtcDay};

/// A queued due item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueEntry {
    /// The reviewable unit's identity.
    pub word: cadence_core::WordId,
    /// The day the item became due (admission basis).
    pub due_since: UtcDay,
}

/// Outcome of a served review attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewOutcome {
    /// Clean completion on a session/day distinct from the previous
    /// clean completion: promotes toward the next interval.
    Clean {
        /// The day the clean completion happened.
        on: UtcDay,
    },
    /// Clean completion within the same session/day as the previous
    /// one: practice, not an independent retention check.
    SameDayClean {
        /// The day it happened.
        on: UtcDay,
    },
    /// An error: return to the 1-day level.
    Error {
        /// The day it happened.
        on: UtcDay,
    },
}

/// Per-item review state kept by the learner model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewState {
    /// Index into the policy interval ladder (0 = 1-day level).
    pub interval_index: usize,
    /// Clean completions counted toward promotion, with the day of the
    /// latest one.
    pub clean_streak: Option<(u32, UtcDay)>,
    /// The item's current due day (None = not scheduled).
    pub due: Option<UtcDay>,
}

impl ReviewState {
    /// Fresh, unscheduled state.
    #[must_use]
    pub const fn fresh() -> Self {
        Self {
            interval_index: 0,
            clean_streak: None,
            due: None,
        }
    }
}

/// The FIFO admission queue with uniqueness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DueQueue {
    entries: VecDeque<QueueEntry>,
    members: HashSet<u64>,
}

impl DueQueue {
    /// An empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the word is already queued.
    #[must_use]
    pub fn contains(&self, word: cadence_core::WordId) -> bool {
        self.members.contains(&word.get())
    }

    /// Admits an item at the tail when it first becomes due. A
    /// newly-imported overdue item joins at the tail rather than
    /// jumping ahead (L-05); duplicates are refused.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when the item is already
    /// queued (an item cannot be queued twice).
    pub fn admit(
        &mut self,
        word: cadence_core::WordId,
        due_since: UtcDay,
    ) -> Result<(), CoreError> {
        if self.contains(word) {
            return Err(CoreError::InvalidIdentity {
                kind: "DueQueue",
                reason: "an item cannot be queued twice",
            });
        }
        self.members.insert(word.get());
        self.entries.push_back(QueueEntry { word, due_since });
        Ok(())
    }

    /// Peeks the head (oldest continuously eligible item).
    #[must_use]
    pub fn head(&self) -> Option<QueueEntry> {
        self.entries.front().copied()
    }

    /// Removes and returns the head; consuming removes the pending
    /// entry so duplicate completions cannot serve twice.
    #[must_use]
    pub fn serve_head(&mut self) -> Option<QueueEntry> {
        let entry = self.entries.pop_front()?;
        self.members.remove(&entry.word.get());
        Some(entry)
    }

    /// Applies a review outcome and computes the next state (L-06).
    /// Checked calendar arithmetic; wall-clock rollback (an `on` day
    /// before the recorded clean day) cannot grant promotion.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] on calendar rollback, or
    /// [`CoreError::CapacityExhausted`] when the interval ladder is
    /// exhausted (the item stays at the top level rather than
    /// wrapping).
    pub fn next_state(
        &self,
        current: ReviewState,
        outcome: ReviewOutcome,
        intervals: &[i64],
        today: UtcDay,
    ) -> Result<ReviewState, CoreError> {
        if intervals.is_empty() {
            return Err(CoreError::CapacityExhausted {
                limit: "review interval ladder",
            });
        }
        match outcome {
            ReviewOutcome::Error { on }
            | ReviewOutcome::Clean { on }
            | ReviewOutcome::SameDayClean { on }
                if days_between(today, on)? < 0 =>
            {
                // The completion day is in the future relative to the
                // injected calendar: rollback/forgery guard.
                Err(CoreError::BackwardTime {
                    admitted: u64::try_from(on.days_since_epoch()).unwrap_or(0),
                    arriving: u64::try_from(today.days_since_epoch()).unwrap_or(0),
                })
            }
            ReviewOutcome::Error { .. } => {
                let due = today.checked_add_days(intervals[0])?;
                Ok(ReviewState {
                    interval_index: 0,
                    clean_streak: None,
                    due: Some(due),
                })
            }
            ReviewOutcome::SameDayClean { .. } => {
                // Practice, not an independent retention check: keep the
                // ladder position, reschedule at the same interval.
                let interval = intervals.get(current.interval_index).copied().ok_or(
                    CoreError::CapacityExhausted {
                        limit: "review interval ladder",
                    },
                )?;
                let due = today.checked_add_days(interval)?;
                Ok(ReviewState {
                    interval_index: current.interval_index,
                    clean_streak: current.clean_streak,
                    due: Some(due),
                })
            }
            ReviewOutcome::Clean { on } => {
                let streak = match current.clean_streak {
                    // A second clean on a distinct day completes the pair.
                    Some((count, previous_day)) if days_between(on, previous_day)? > 0 => {
                        Some((count.saturating_add(1), on))
                    }
                    Some((count, previous_day)) if days_between(on, previous_day)? == 0 => {
                        // Same-day repetition degrades to same-day clean.
                        Some((count, previous_day))
                    }
                    Some((_count, previous_day)) => {
                        return Err(CoreError::BackwardTime {
                            admitted: u64::try_from(previous_day.days_since_epoch()).unwrap_or(0),
                            arriving: u64::try_from(on.days_since_epoch()).unwrap_or(0),
                        });
                    }
                    None => Some((1, on)),
                };
                let promote = matches!(streak, Some((count, _)) if count >= 2);
                // Promotion clamps at the ladder top: an exhausted item
                // stays at the longest interval rather than erroring or
                // wrapping (L-06).
                let ladder_top = intervals.len().saturating_sub(1);
                let next_index = if promote {
                    current.interval_index.saturating_add(1).min(ladder_top)
                } else {
                    current.interval_index
                };
                let interval =
                    intervals
                        .get(next_index)
                        .copied()
                        .ok_or(CoreError::CapacityExhausted {
                            limit: "review interval ladder",
                        })?;
                let due = today.checked_add_days(interval)?;
                Ok(ReviewState {
                    interval_index: next_index,
                    clean_streak: if promote { None } else { streak },
                    due: Some(due),
                })
            }
        }
    }
}

fn days_between(later: UtcDay, earlier: UtcDay) -> Result<i64, CoreError> {
    later.days_since(earlier)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use cadence_core::WordId;

    fn word(id: u64) -> WordId {
        WordId::new(id).unwrap()
    }

    fn day(offset: i64) -> UtcDay {
        UtcDay::from_days_since_epoch(20_000 + offset).unwrap()
    }

    #[test]
    fn serving_removes_membership_so_no_double_service() {
        let mut queue = DueQueue::new();
        queue.admit(word(1), day(0)).unwrap();
        let served = queue.serve_head().unwrap();
        assert_eq!(served.word, word(1));
        assert!(queue.serve_head().is_none());
        // Re-admission after service is legal (it became due again).
        assert!(queue.admit(word(1), day(1)).is_ok());
    }

    #[test]
    fn error_returns_to_one_day_level() {
        let queue = DueQueue::new();
        let state = ReviewState {
            interval_index: 3,
            clean_streak: Some((1, day(0))),
            due: Some(day(7)),
        };
        let next = queue
            .next_state(
                state,
                ReviewOutcome::Error { on: day(2) },
                &[1, 3, 7, 14, 30],
                day(2),
            )
            .unwrap();
        assert_eq!(next.interval_index, 0);
        assert_eq!(next.due, Some(day(3)));
        assert!(next.clean_streak.is_none());
    }

    #[test]
    fn two_clean_days_promote_once() {
        let queue = DueQueue::new();
        let intervals = [1i64, 3, 7, 14, 30];
        let first = queue
            .next_state(
                ReviewState::fresh(),
                ReviewOutcome::Clean { on: day(0) },
                &intervals,
                day(0),
            )
            .unwrap();
        assert_eq!(first.interval_index, 0, "one clean alone does not promote");
        let second = queue
            .next_state(
                first,
                ReviewOutcome::Clean { on: day(1) },
                &intervals,
                day(1),
            )
            .unwrap();
        assert_eq!(second.interval_index, 1, "two distinct-day cleans promote");
        assert_eq!(second.due, Some(day(4)), "3-day interval after promotion");
    }

    #[test]
    fn same_day_clean_does_not_promote() {
        let queue = DueQueue::new();
        let intervals = [1i64, 3, 7];
        let first = queue
            .next_state(
                ReviewState::fresh(),
                ReviewOutcome::Clean { on: day(0) },
                &intervals,
                day(0),
            )
            .unwrap();
        let same_day = queue
            .next_state(
                first,
                ReviewOutcome::SameDayClean { on: day(0) },
                &intervals,
                day(0),
            )
            .unwrap();
        assert_eq!(same_day.interval_index, 0);
        // A later distinct-day clean can still complete the pair.
        let second = queue
            .next_state(
                same_day,
                ReviewOutcome::Clean { on: day(2) },
                &intervals,
                day(2),
            )
            .unwrap();
        assert_eq!(second.interval_index, 1);
    }

    #[test]
    fn rollback_cannot_grant_future_success() {
        let queue = DueQueue::new();
        let intervals = [1i64, 3];
        let error = queue.next_state(
            ReviewState::fresh(),
            ReviewOutcome::Clean { on: day(5) },
            &intervals,
            day(1),
        );
        assert!(matches!(error, Err(CoreError::BackwardTime { .. })));
    }

    #[test]
    fn exhausted_ladder_stays_at_top() {
        let queue = DueQueue::new();
        let intervals = [1i64];
        let top = ReviewState {
            interval_index: 0,
            clean_streak: None,
            due: None,
        };
        let promoted = queue
            .next_state(top, ReviewOutcome::Clean { on: day(1) }, &intervals, day(2))
            .unwrap();
        let _ = promoted;
        // With a single-level ladder, promotion is infeasible: the next
        // clean either errors or stays at level 0.
        let again = queue
            .next_state(
                promoted,
                ReviewOutcome::Clean { on: day(4) },
                &intervals,
                day(5),
            )
            .unwrap();
        assert_eq!(again.interval_index, 0);
    }
}
