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

use cadence_core::{CoreError, UtcDay, review};

/// A queued due item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueEntry {
    /// The reviewable unit's identity.
    pub word: cadence_core::WordId,
    /// The day the item became due (admission basis).
    pub due_since: UtcDay,
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
}

/// The next ladder state after a recorded outcome on `day` (L-06): the
/// proved `cadence_kernel::review::ladder_step`. `Ok(None)` is clean typing
/// before the word is due: practice, the state is kept.
///
/// # Errors
/// Returns [`CoreError::InvalidIdentity`] for an invalid ladder or rung,
/// [`CoreError::BackwardTime`] when the completion predates the recorded
/// clean day, and [`CoreError::ArithmeticOverflow`] when a day overflows.
pub fn ladder_step(
    current: ReviewState,
    error: bool,
    day: UtcDay,
    intervals: &[i64],
) -> Result<Option<ReviewState>, CoreError> {
    let state = review::State {
        rung: current.interval_index,
        streak: current.clean_streak.map(|(count, on)| review::Streak {
            count,
            day: on.days_since_epoch(),
        }),
        due: current.due.map(UtcDay::days_since_epoch),
    };
    match review::ladder_step(state, error, day.days_since_epoch(), intervals) {
        review::Step::Practice => Ok(None),
        review::Step::Invalid => Err(CoreError::InvalidIdentity {
            kind: "ReviewState",
            reason: "invalid review ladder or current interval",
        }),
        review::Step::Rollback => Err(CoreError::BackwardTime {
            admitted: current.clean_streak.map_or(0, |(_, on)| {
                u64::try_from(on.days_since_epoch()).unwrap_or(0)
            }),
            arriving: u64::try_from(day.days_since_epoch()).unwrap_or(0),
        }),
        review::Step::Overflow => Err(CoreError::ArithmeticOverflow {
            operation: "review due day",
        }),
        review::Step::Next(next) => Ok(Some(ReviewState {
            interval_index: next.rung,
            clean_streak: match next.streak {
                None => None,
                Some(streak) => Some((streak.count, UtcDay::from_days_since_epoch(streak.day)?)),
            },
            due: next.due.map(UtcDay::from_days_since_epoch).transpose()?,
        })),
    }
}

/// Whether a scheduled word is due on `today`: the proved
/// `cadence_kernel::review::is_due`.
#[must_use]
pub fn is_due(state: ReviewState, today: UtcDay) -> bool {
    review::is_due(
        state.due.map(UtcDay::days_since_epoch),
        today.days_since_epoch(),
    )
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;
    use cadence_core::WordId;

    fn word(id: u64) -> WordId {
        WordId::new(id).unwrap()
    }

    fn day(offset: i64) -> UtcDay {
        UtcDay::from_days_since_epoch(20_000i64.checked_add(offset).unwrap()).unwrap()
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

    fn due_at(rung: usize, due: i64) -> ReviewState {
        ReviewState {
            interval_index: rung,
            clean_streak: None,
            due: Some(day(due)),
        }
    }

    const LADDER: [i64; 5] = [1, 3, 7, 14, 30];

    #[test]
    fn error_returns_to_one_day_level() {
        let state = ReviewState {
            interval_index: 3,
            clean_streak: Some((1, day(0))),
            due: Some(day(7)),
        };
        let next = ladder_step(state, true, day(2), &LADDER).unwrap().unwrap();
        assert_eq!(next.interval_index, 0);
        assert_eq!(next.due, Some(day(3)));
        assert!(next.clean_streak.is_none());
    }

    #[test]
    fn clean_before_due_is_practice() {
        assert_eq!(
            ladder_step(ReviewState::fresh(), false, day(0), &LADDER),
            Ok(None)
        );
        assert_eq!(ladder_step(due_at(0, 5), false, day(4), &LADDER), Ok(None));
    }

    #[test]
    fn two_clean_days_promote_once() {
        let first = ladder_step(due_at(0, 0), false, day(0), &LADDER)
            .unwrap()
            .unwrap();
        assert_eq!(first.interval_index, 0, "one clean alone does not promote");
        assert_eq!(first.due, Some(day(1)));
        let second = ladder_step(first, false, day(1), &LADDER).unwrap().unwrap();
        assert_eq!(second.interval_index, 1, "two distinct-day cleans promote");
        assert_eq!(second.due, Some(day(4)), "3-day interval after promotion");
        assert!(second.clean_streak.is_none());
    }

    #[test]
    fn same_day_clean_does_not_promote() {
        // A second clean on the day of the counted one does not complete
        // the pair, even when the word is due again that day.
        let state = ReviewState {
            interval_index: 0,
            clean_streak: Some((1, day(2))),
            due: Some(day(2)),
        };
        let same_day = ladder_step(state, false, day(2), &LADDER).unwrap().unwrap();
        assert_eq!(same_day.interval_index, 0);
        assert_eq!(same_day.clean_streak, Some((1, day(2))));
        let second = ladder_step(same_day, false, day(3), &LADDER)
            .unwrap()
            .unwrap();
        assert_eq!(second.interval_index, 1);
    }

    #[test]
    fn rollback_cannot_grant_future_success() {
        let state = ReviewState {
            interval_index: 0,
            clean_streak: Some((1, day(5))),
            due: Some(day(1)),
        };
        assert!(matches!(
            ladder_step(state, false, day(4), &LADDER),
            Err(CoreError::BackwardTime { .. })
        ));
    }

    #[test]
    fn exhausted_ladder_stays_at_top() {
        let top = ReviewState {
            interval_index: 4,
            clean_streak: Some((1, day(0))),
            due: Some(day(1)),
        };
        let again = ladder_step(top, false, day(2), &LADDER).unwrap().unwrap();
        assert_eq!(again.interval_index, 4);
        assert_eq!(again.due, Some(day(32)));
        let single = ladder_step(due_at(0, 1), false, day(1), &[1])
            .unwrap()
            .unwrap();
        assert_eq!(single.interval_index, 0);
    }

    #[test]
    fn invalid_ladder_is_refused() {
        for ladder in [&[][..], &[2, 3], &[1, 1], &[1, 3, 2]] {
            assert!(matches!(
                ladder_step(due_at(0, 0), false, day(0), ladder),
                Err(CoreError::InvalidIdentity { .. })
            ));
        }
        assert!(ladder_step(due_at(5, 0), false, day(0), &LADDER).is_err());
    }

    #[test]
    fn due_is_on_or_before_today() {
        assert!(is_due(due_at(0, 3), day(3)));
        assert!(!is_due(due_at(0, 4), day(3)));
        assert!(!is_due(ReviewState::fresh(), day(3)));
    }
}
