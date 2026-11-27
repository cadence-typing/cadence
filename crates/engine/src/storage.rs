//! Single-flight checkpoint state; runtime alone performs durable I/O.
use super::practice::merge_track;
use super::{DomainOutcome, Engine, Screen, Transition, invalid};
use crate::view::{Durability, PauseReason};
use cadence_core::progress::{
    Checkpoint, HistorySummary, ProgressStatus, SaveKey, SaveResult, SessionProgress,
};
use cadence_core::session::{SessionEvent, SessionState, save_overdue};
use cadence_core::{CommitSeq, CoreError, MonoTick, SessionId};

/// Maximum running checkpoint interval and acknowledgement watchdog.
const CHECKPOINT_CADENCE_US: u64 = 2_000_000;
/// A final flush gets a separate wall-clock bound; practice is already stopped.
const FINAL_FLUSH_WATCHDOG_US: u64 = 10_000_000;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Storage {
    session: SessionId,
    day: i64,
    sequence: CommitSeq,
    pending: Option<Checkpoint>,
    /// The causal group of the pending checkpoint (#71).
    pending_group: Option<u64>,
    pub(super) failed: Option<Checkpoint>,
    last_ack: Option<SaveKey>,
    sent_at: Option<MonoTick>,
    last_dispatch: Option<MonoTick>,
}
impl Storage {
    /// The newest dispatched and the newest acknowledged sequence.
    pub(super) fn watermarks(&self) -> (Option<u64>, Option<u64>) {
        let dispatched = self.sequence.get();
        (
            (dispatched > 0).then_some(dispatched),
            self.last_ack.map(|key| key.sequence.get()),
        )
    }

    /// Local calendar day of this session.
    pub(super) const fn day(&self) -> i64 {
        self.day
    }
}
impl Engine {
    pub(super) fn storage_ready(
        &mut self,
        session: SessionId,
        day: i64,
        history: HistorySummary,
    ) -> Result<(), CoreError> {
        if self.storage.is_some() || self.screen != Screen::Today {
            return Err(invalid("storage readiness must occur once before starting"));
        }
        // Shown once: on the first session planned with a changed corpus
        // (#8), due words the vetted pack retired are named as dropped.
        let corpus = self.plan.corpus_digest.to_hex();
        if self.plan.retired_due > 0
            && history
                .latest_corpus_digest
                .as_ref()
                .is_some_and(|latest| *latest != corpus)
            && self.notice.is_none()
        {
            let count = self.plan.retired_due;
            let plural = if count == 1 { "" } else { "s" };
            self.notice = Some(format!(
                "{count} due word{plural} left the vetted word pack and will not be reviewed."
            ));
        }
        self.history = history;
        self.storage = Some(Storage {
            session,
            day,
            sequence: CommitSeq::INITIAL,
            pending: None,
            pending_group: None,
            failed: None,
            last_ack: None,
            sent_at: None,
            last_dispatch: None,
        });
        Ok(())
    }
    pub(super) fn save_result(
        &mut self,
        response: SaveResult,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        let Some(storage) = self.storage.as_mut() else {
            return Ok(());
        };
        if !storage
            .pending
            .as_ref()
            .is_some_and(|p| p.key == response.key)
        {
            return Ok(());
        }
        let Some(checkpoint) = storage.pending.take() else {
            return Err(invalid("missing pending checkpoint"));
        };
        // A follow-up dispatched while this acknowledgement is consumed is
        // this checkpoint's child (#71).
        self.ack_group = storage.pending_group.take();
        storage.sent_at = None;
        match response.result {
            Ok(()) => {
                storage.last_ack = Some(response.key);
                if let cadence_core::progress::Evidence::Chunked(chunked) = &checkpoint.evidence {
                    self.learning.chunks.acknowledged(chunked.through);
                }
                if !self.invalid {
                    // The typed watermark belongs in the modeline; a routine
                    // acknowledgement must not keep a diagnostic banner open.
                    self.notice = None;
                }
                // #51: the stalled save caught up: its pause ends (only a
                // pause whose reason is still the stall).
                if self.session_state == SessionState::Paused
                    && self.pause_reason == Some(PauseReason::SavingSlow)
                    && !self.invalid
                {
                    let at = self.last_at;
                    self.transition(at, SessionEvent::SaveCaughtUp)?;
                    self.pause_reason = None;
                }
                if self.finishing
                    && !self.checkpoint_due
                    && checkpoint.progress.status == self.status
                {
                    self.transition(None, SessionEvent::FinalSaved)?;
                    t.quit = true;
                    t.exit_failed = false;
                }
            }
            Err(message) => {
                storage.failed = Some(checkpoint);
                let bounded: String = message.chars().take(256).collect();
                let durable = storage.last_ack.map_or_else(
                    || "no checkpoint acknowledged".to_string(),
                    |key| {
                        format!(
                            "last durable checkpoint {}.{}",
                            key.session.get(),
                            key.sequence.get()
                        )
                    },
                );
                let at = self.last_at;
                self.transition(at, SessionEvent::SaveFailed)?;
                if self.pause_reason == Some(PauseReason::SavingSlow) {
                    // The failure takes over: `r` retries, then Esc resumes.
                    self.pause_reason = None;
                }
                self.notice = Some(format!(
                    "Save failed: {bounded}; {durable}. Paused; r retries. Ctrl-C twice requests an unsuccessful exit; the unacknowledged tail may be lost."
                ));
            }
        }
        Ok(())
    }
    pub(super) fn retry_save(&mut self) {
        if let Some(storage) = self.storage.as_mut()
            && storage.failed.take().is_some()
        {
            // Rebuild the cumulative snapshot only on dispatch, including retained tail.
            self.checkpoint_due = true;
            self.notice = Some("Retrying durable save; practice remains paused.".to_string());
        }
    }
    pub(super) fn request_exit(
        &mut self,
        at: MonoTick,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        if self.finishing && self.storage.as_ref().is_some_and(|s| s.failed.is_some()) {
            t.quit = true;
            t.exit_failed = true;
            self.notice=Some("Exited without saving the latest tail; only the last acknowledged checkpoint is durable.".to_string());
            return Ok(());
        }
        if self.finishing {
            return Ok(());
        }
        if self.screen == Screen::Today && self.session_state == SessionState::Ready {
            t.quit = true;
            return Ok(());
        }
        if self.status != ProgressStatus::Completed {
            t.outcome = DomainOutcome::SessionAbandoned;
        }
        let in_probe = self.in_probe();
        self.transition(Some(at), SessionEvent::Finish)?;
        if let Some(ledger) = self.ledger.as_mut() {
            ledger.mark_interrupted();
        }
        self.record_exercise()?;
        if in_probe {
            self.record_probe(false, true)?;
        }
        self.finishing = true;
        self.checkpoint_due = true;
        if self.storage.is_none() {
            t.quit = true;
        } else {
            self.notice = Some("Finishing: waiting for final durable acknowledgement.".to_string());
        }
        Ok(())
    }
    pub(super) fn watchdog(&mut self, at: MonoTick, t: &mut Transition) -> Result<(), CoreError> {
        let Some(storage) = self.storage.as_ref() else {
            return Ok(());
        };
        if self.finishing
            && let Some(sent) = storage.sent_at
            && at.elapsed_since(sent)?.as_us() >= FINAL_FLUSH_WATCHDOG_US
        {
            self.notice =
                Some("Final save timed out; only acknowledged progress is durable.".to_string());
            t.quit = true;
            t.exit_failed = true;
            return Ok(());
        }
        if self.session_state == SessionState::Running {
            if let Some(sent) = storage.sent_at
                && save_overdue(at.elapsed_since(sent)?.as_us(), CHECKPOINT_CADENCE_US)
            {
                // #51: a slow save stalls practice (the clock stops, nothing
                // is scored) until the acknowledgement arrives, instead of
                // invalidating the session.
                self.transition(Some(at), SessionEvent::SaveStalled)?;
                self.pause_reason = Some(PauseReason::SavingSlow);
                return Ok(());
            }
            if storage.last_dispatch.is_none()
                || storage.last_dispatch.is_some_and(|last| {
                    at.elapsed_since(last)
                        .is_ok_and(|d| d.as_us() >= CHECKPOINT_CADENCE_US)
                })
            {
                self.checkpoint_due = true;
            }
        }
        Ok(())
    }
    pub(super) fn dispatch_checkpoint(
        &mut self,
        at: MonoTick,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        if !self.checkpoint_due {
            return Ok(());
        }
        let Some(storage) = self.storage.as_ref() else {
            self.checkpoint_due = false;
            return Ok(());
        };
        if storage.pending.is_some() || storage.failed.is_some() {
            return Ok(());
        }
        let mut tracks = self.tracks.clone();
        if let Some(current) = self.current_progress()? {
            merge_track(&mut tracks, current)?;
        }
        let progress = SessionProgress {
            profile: "default".to_string(),
            day: storage.day,
            policy_digest: self.plan.policy_digest.to_hex(),
            corpus_digest: self.plan.corpus_digest.to_hex(),
            status: self.status,
            tracks,
            active_micros: self.active.as_us(),
            utc_offset_minutes: self.utc_offset_minutes,
            layout: self.layout,
            content_class: self.content_class,
            pack_digest: self.pack_digest.clone(),
        };
        let sequence = storage.sequence.next()?;
        let checkpoint = Checkpoint::new(
            storage.session,
            sequence,
            progress,
            self.learning.chunks.evidence(&self.learning.evidence),
        )?;
        let group = self.ack_group.take().unwrap_or(self.consumed.get());
        if let Some(storage) = self.storage.as_mut() {
            storage.sequence = sequence;
            storage.pending_group = Some(group);
            storage.pending = Some(checkpoint.clone());
            storage.sent_at = Some(at);
            storage.last_dispatch = Some(at);
        }
        self.checkpoint_due = false;
        t.checkpoint = Some(checkpoint);
        t.checkpoint_group = Some(group);
        Ok(())
    }
    pub(super) fn durability(&self) -> Durability {
        let Some(storage) = self.storage.as_ref() else {
            return Durability::Transient;
        };
        let last_ack = storage.last_ack.map(|key| key.sequence);
        if storage.failed.is_some() {
            Durability::Failed { last_ack }
        } else if storage.pending.is_some() {
            Durability::Saving { last_ack }
        } else if let Some(sequence) = last_ack {
            Durability::Checkpointed { sequence }
        } else {
            Durability::Local
        }
    }
}
