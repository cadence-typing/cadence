//! Single-flight checkpoint state; runtime alone performs durable I/O.
use super::practice::merge_track;
use super::{DomainOutcome, Engine, Screen, Transition, invalid};
use crate::view::Durability;
use cadence_core::progress::{
    Checkpoint, HistorySummary, ProgressStatus, SaveKey, SaveResult, SessionProgress,
};
use cadence_core::session::SessionState;
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
    pub(super) failed: Option<Checkpoint>,
    last_ack: Option<SaveKey>,
    sent_at: Option<MonoTick>,
    last_dispatch: Option<MonoTick>,
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
        self.history = history;
        self.storage = Some(Storage {
            session,
            day,
            sequence: CommitSeq::INITIAL,
            pending: None,
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
        storage.sent_at = None;
        match response.result {
            Ok(()) => {
                storage.last_ack = Some(response.key);
                if !self.invalid {
                    // The typed watermark belongs in the modeline; a routine
                    // acknowledgement must not keep a diagnostic banner open.
                    self.notice = None;
                }
                if self.finishing
                    && !self.checkpoint_due
                    && checkpoint.progress.status == self.status
                {
                    self.session_state = SessionState::Saved;
                    t.quit = true;
                    t.exit_failed = false;
                }
            }
            Err(message) => {
                storage.failed = Some(checkpoint);
                if let Some(at) = self.last_at {
                    self.timing.pause(at)?;
                }
                self.session_state = SessionState::Paused;
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
        self.timing.pause(at)?;
        if self.status != ProgressStatus::Completed {
            self.status = ProgressStatus::Abandoned;
            t.outcome = DomainOutcome::SessionAbandoned;
        }
        if let Some(ledger) = self.ledger.as_mut() {
            ledger.mark_interrupted();
        }
        self.record_exercise()?;
        self.finishing = true;
        self.session_state = SessionState::Finishing;
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
                && at.elapsed_since(sent)?.as_us() > CHECKPOINT_CADENCE_US
            {
                return Err(invalid(
                    "checkpoint acknowledgement exceeded the 2-second watchdog",
                ));
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
        };
        let sequence = storage.sequence.next()?;
        let checkpoint = Checkpoint::new(storage.session, sequence, progress)?;
        if let Some(storage) = self.storage.as_mut() {
            storage.sequence = sequence;
            storage.pending = Some(checkpoint.clone());
            storage.sent_at = Some(at);
            storage.last_dispatch = Some(at);
        }
        self.checkpoint_due = false;
        t.checkpoint = Some(checkpoint);
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
