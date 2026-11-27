//! The driver of the save protocol (#89): every flag and sequence of the
//! single-flight protocol is the proved kernel machine's
//! (`cadence_core::save::step`, held in `Engine::save`); this module holds
//! the payloads and timestamps, measures the watchdogs and carries out the
//! machine's commands. The runtime alone performs durable I/O.
use super::practice::merge_track;
use super::{DomainOutcome, Engine, Screen, Transition, invalid};
use crate::view::{Durability, PauseReason};
use cadence_core::progress::{
    Checkpoint, HistorySummary, ProgressStatus, SaveKey, SaveResult, SessionProgress,
};
use cadence_core::save::{Command, Event};
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
    /// The payload in flight: present exactly while the machine says
    /// `pending`.
    pending: Option<Checkpoint>,
    /// The causal group of the pending checkpoint (#71).
    pending_group: Option<u64>,
    /// The newest acknowledged key, for its digest in diagnostics.
    last_ack: Option<SaveKey>,
    sent_at: Option<MonoTick>,
    last_dispatch: Option<MonoTick>,
}
impl Storage {
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
            pending: None,
            pending_group: None,
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
        let seq = response.key.sequence.get();
        if response.result.is_ok() {
            storage.last_ack = Some(response.key);
        }
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
        match response.result {
            Ok(()) => {
                let command = self.save_step(Event::Acked { seq });
                if let cadence_core::progress::Evidence::Chunked(chunked) = &checkpoint.evidence {
                    self.learning.chunks.acknowledged(chunked.through);
                }
                if !self.invalid {
                    // The typed watermark belongs in the modeline; a routine
                    // acknowledgement must not keep a diagnostic banner open.
                    self.notice = None;
                }
                match command {
                    // #51: the stalled save caught up: its pause ends (only
                    // a pause whose reason is still the stall).
                    Command::Resume => {
                        if self.session_state == SessionState::Paused
                            && self.pause_reason == Some(PauseReason::SavingSlow)
                            && !self.invalid
                        {
                            let at = self.last_at;
                            self.transition(at, SessionEvent::SaveCaughtUp)?;
                            self.pause_reason = None;
                        }
                    }
                    Command::Quit { success: true } => {
                        self.transition(None, SessionEvent::FinalSaved)?;
                        t.quit = true;
                        t.exit_failed = false;
                    }
                    Command::None
                    | Command::Dispatch { .. }
                    | Command::Pause
                    | Command::Quit { success: false } => {}
                }
            }
            Err(message) => {
                if self.save_step(Event::AckFailed { seq }) != Command::Pause {
                    return Err(invalid("a failed acknowledgement did not pause"));
                }
                let bounded: String = message.chars().take(256).collect();
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
    /// Steps the save machine; the driver carries out the command.
    pub(super) fn save_step(&mut self, event: Event) -> Command {
        let (next, command) = cadence_core::save::step(self.save, event);
        self.save = next;
        command
    }
    /// Something a checkpoint should carry changed.
    pub(super) fn mark_due(&mut self) {
        let _none = self.save_step(Event::Due);
    }
    pub(super) fn retry_save(&mut self) {
        if self.save.failed {
            // The snapshot is rebuilt on dispatch, including the retained tail.
            let _none = self.save_step(Event::Retry);
            self.notice = Some("Retrying durable save; practice remains paused.".to_string());
        }
    }
    pub(super) fn request_exit(
        &mut self,
        at: MonoTick,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        if self.save.finishing {
            if self.save_step(Event::ExitRequested) == (Command::Quit { success: false }) {
                t.quit = true;
                t.exit_failed = true;
                self.notice=Some("Exited without saving the latest tail; only the last acknowledged checkpoint is durable.".to_string());
            }
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
        let _none = self.save_step(Event::ExitRequested);
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
        let sent = storage.sent_at;
        let last_dispatch = storage.last_dispatch;
        if self.save.finishing
            && let Some(sent) = sent
            && at.elapsed_since(sent)?.as_us() >= FINAL_FLUSH_WATCHDOG_US
            && self.save_step(Event::FinalTimeout) == (Command::Quit { success: false })
        {
            self.notice =
                Some("Final save timed out; only acknowledged progress is durable.".to_string());
            t.quit = true;
            t.exit_failed = true;
            return Ok(());
        }
        if self.session_state == SessionState::Running {
            if let Some(sent) = sent
                && save_overdue(at.elapsed_since(sent)?.as_us(), CHECKPOINT_CADENCE_US)
                && self.save_step(Event::Stalled) == Command::Pause
            {
                // #51: a slow save stalls practice (the clock stops, nothing
                // is scored) until the acknowledgement arrives, instead of
                // invalidating the session.
                self.transition(Some(at), SessionEvent::SaveStalled)?;
                self.pause_reason = Some(PauseReason::SavingSlow);
                return Ok(());
            }
            if last_dispatch.is_none()
                || last_dispatch.is_some_and(|last| {
                    at.elapsed_since(last)
                        .is_ok_and(|d| d.as_us() >= CHECKPOINT_CADENCE_US)
                })
            {
                self.mark_due();
            }
        }
        Ok(())
    }
    pub(super) fn dispatch_checkpoint(
        &mut self,
        at: MonoTick,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        if self.storage.is_none() {
            return Ok(());
        }
        let Command::Dispatch { seq } = self.save_step(Event::Flush) else {
            return Ok(());
        };
        let Some(storage) = self.storage.as_ref() else {
            return Err(invalid("dispatch without storage"));
        };
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
        let sequence = CommitSeq::from_raw(seq);
        let checkpoint = Checkpoint::new(
            storage.session,
            sequence,
            progress,
            self.learning.chunks.evidence(&self.learning.evidence),
        )?;
        let group = self.ack_group.take().unwrap_or(self.consumed.get());
        if let Some(storage) = self.storage.as_mut() {
            storage.pending_group = Some(group);
            storage.pending = Some(checkpoint.clone());
            storage.sent_at = Some(at);
            storage.last_dispatch = Some(at);
        }
        t.checkpoint = Some(checkpoint);
        t.checkpoint_group = Some(group);
        Ok(())
    }
    pub(super) fn durability(&self) -> Durability {
        let Some(storage) = self.storage.as_ref() else {
            return Durability::Transient;
        };
        let last_ack = storage.last_ack.map(|key| key.sequence);
        if self.save.failed {
            Durability::Failed { last_ack }
        } else if self.save.pending {
            Durability::Saving { last_ack }
        } else if let Some(sequence) = last_ack {
            Durability::Checkpointed { sequence }
        } else {
            Durability::Local
        }
    }
}
