//! Review context (plan 0004): like-for-like comparison with earlier
//! completed sessions and this session's most frequent slips. Read-only.
use std::collections::BTreeSet;

use super::{Engine, Screen};
use crate::summary::{SummaryMetrics, percent_label};
use crate::view::{Baseline, RateUnit, ReviewContext, Slip};
use cadence_core::calendar::days_ago;
use cadence_core::learning::{KeyLane, Skill};
use cadence_core::progress::{SessionRecap, Track, TrackProgress};

/// An earlier session's rows without its closing review (#60).
fn headline_tracks(tracks: &[TrackProgress]) -> Vec<TrackProgress> {
    tracks
        .iter()
        .filter(|t| t.track != Track::Review)
        .cloned()
        .collect()
}

/// Slips named in the review sentence; more would stop being one sentence.
const SLIPS_SHOWN: usize = 3;

impl Engine {
    /// Whole days since an earlier session: the proved
    /// `cadence_kernel::calendar::days_ago` (never negative).
    fn days_ago(&self, recap: &SessionRecap) -> i64 {
        self.storage
            .as_ref()
            .map_or(0, |storage| days_ago(storage.day(), recap.day))
    }

    /// The most recent session with typing, for the start screen.
    pub(super) fn last_session(&self) -> Option<Baseline> {
        let recap = self.history.recent.first()?;
        Some(Baseline {
            days_ago: self.days_ago(recap),
            completed: recap.completed,
            tracks: headline_tracks(&recap.tracks),
        })
    }

    pub(super) fn review_context(&self) -> super::ReviewContext {
        if self.screen != Screen::Summary {
            return ReviewContext::default();
        }
        // Like for like on the headline tracks: a review row never makes a
        // session incomparable, nor enters the comparison (#60).
        let shared = |tracks: &[TrackProgress]| -> BTreeSet<Track> {
            tracks
                .iter()
                .map(|t| t.track)
                .filter(|track| *track != Track::Review)
                .collect()
        };
        let tracks = shared(&self.tracks);
        let session = self
            .history
            .recent
            .iter()
            .find(|recap| shared(&recap.tracks) == tracks)
            .map(|recap| Baseline {
                days_ago: self.days_ago(recap),
                completed: recap.completed,
                tracks: headline_tracks(&recap.tracks),
            });
        let per_track = self
            .tracks
            .iter()
            .filter(|current| current.track != Track::Review)
            .filter_map(|current| {
                self.history.recent.iter().find_map(|recap| {
                    recap
                        .tracks
                        .iter()
                        .find(|t| t.track == current.track)
                        .map(|t| Baseline {
                            days_ago: self.days_ago(recap),
                            completed: recap.completed,
                            tracks: vec![t.clone()],
                        })
                })
            })
            .collect();
        let mut slips: Vec<Slip> = self
            .learning
            .evidence
            .skills
            .iter()
            .filter(|(key, tally)| {
                key.lane == KeyLane::Main
                    && matches!(key.skill, Skill::Bigram(..))
                    && tally.errors > 0
            })
            .map(|(key, tally)| Slip {
                skill: super::presentation::skill_label(*key),
                errors: tally.errors,
                attempts: tally.attempts,
            })
            .collect();
        slips.sort_by(|a, b| b.errors.cmp(&a.errors).then(a.attempts.cmp(&b.attempts)));
        slips.truncate(SLIPS_SHOWN);
        ReviewContext {
            ended_early: self.ended_early,
            session,
            tracks: per_track,
            slips,
        }
    }

    /// One line printed after the terminal is restored.
    #[must_use]
    pub fn recap(&self) -> Option<String> {
        if self.tracks.is_empty() {
            return None;
        }
        let ending = if self.ended_early {
            " · ended early"
        } else {
            ""
        };
        let seconds = self.active.as_us().checked_div(1_000_000).unwrap_or(0);
        let active = format!(
            "{}:{:02} active",
            seconds.checked_div(60).unwrap_or(0),
            seconds.checked_rem(60).unwrap_or(0)
        );
        let metrics = SummaryMetrics::from_reports(&self.tracks, !self.invalid).ok()?;
        let speed = metrics.speed.map_or_else(
            || "no qualified rate".to_string(),
            |rate| {
                let unit = match metrics.speed_unit {
                    RateUnit::WordsPerMinute => "correct WPM",
                    RateUnit::DigitsPerMinute => "correct digits/min",
                };
                format!("{:.1} {unit}", rate.as_f64())
            },
        );
        let accuracy = metrics.first_pass_accuracy.map_or_else(
            || "no attempts".to_string(),
            |a| format!("{} first-pass", percent_label(a)),
        );
        Some(format!("cadence: {speed} · {accuracy} · {active}{ending}"))
    }
}
