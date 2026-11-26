//! The worker job table (VP-01, VP-02, #75): every non-save worker result
//! passes the proved `cadence_kernel::ticket::admit` (current epoch, the
//! pending request). One fixed slot per job kind keeps the table bounded
//! whatever arrives; an accepted result is retained until consumed.

use cadence_core::ticket::{Admit, admit};

/// Kinds of ticketed jobs. `Ping` is a no-op used to exercise the path;
/// imports join them (#84).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// A no-op round trip.
    Ping,
    /// An acknowledged `config.toml` write (#79).
    ConfigWrite,
    /// A `config.toml` reload (#79).
    ConfigReload,
    /// The Progress destination's bounded history query (#81).
    History,
}

/// One slot per kind.
const KINDS: usize = 4;

impl JobKind {
    const fn index(self) -> usize {
        match self {
            Self::Ping => 0,
            Self::ConfigWrite => 1,
            Self::ConfigReload => 2,
            Self::History => 3,
        }
    }
}

/// A job's ticket: its kind, epoch and request id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobTicket {
    /// The kind.
    pub kind: JobKind,
    /// The epoch it was requested in.
    pub epoch: u64,
    /// Its request id within the epoch.
    pub request: u64,
}

/// A worker's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    /// Done.
    Succeeded,
    /// Failed, with a reason.
    Failed(String),
    /// A config write or reload succeeded: the file's validated values, by
    /// key, and its digest (`None` when there is no file).
    Settings {
        /// Every key's value.
        values: Vec<(&'static str, cadence_core::settings::Setting)>,
        /// The file's digest.
        digest: Option<cadence_core::settings::ConfigDigest>,
    },
    /// The history query's sessions, oldest first (#81).
    History(Vec<cadence_core::progress::HistoryPoint>),
}

/// One kind's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobSlot {
    /// The current epoch.
    pub epoch: u64,
    /// The pending request.
    pub pending: Option<u64>,
    /// The next request id (restarts with the epoch).
    pub next_request: u64,
    /// The retained accepted result: its request and outcome.
    pub retained: Option<(u64, JobOutcome)>,
    /// Results accepted so far.
    pub accepted: u64,
}

/// The bounded job table: one slot per kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Jobs {
    slots: [JobSlot; KINDS],
}

impl Jobs {
    /// A kind's slot.
    #[must_use]
    pub fn slot(&self, kind: JobKind) -> Option<&JobSlot> {
        self.slots.get(kind.index())
    }

    /// Starts a job: a fresh request id in the current epoch, now pending.
    pub fn start(&mut self, kind: JobKind) -> Option<JobTicket> {
        let slot = self.slots.get_mut(kind.index())?;
        slot.next_request = slot.next_request.saturating_add(1);
        slot.pending = Some(slot.next_request);
        Some(JobTicket {
            kind,
            epoch: slot.epoch,
            request: slot.next_request,
        })
    }

    /// The screen or session that asked restarted: a new epoch, nothing
    /// pending, request ids start again.
    pub fn restart(&mut self) {
        for slot in &mut self.slots {
            slot.epoch = slot.epoch.saturating_add(1);
            slot.pending = None;
            slot.next_request = 0;
        }
    }

    /// Admits a result through the proved ticket decision; returns whether
    /// it was accepted.
    pub fn result(&mut self, ticket: JobTicket, outcome: JobOutcome) -> bool {
        let Some(slot) = self.slots.get_mut(ticket.kind.index()) else {
            return false;
        };
        let decision = admit(
            ticket.epoch,
            ticket.request,
            slot.epoch,
            slot.pending.unwrap_or(0),
            slot.pending.is_some(),
        );
        if decision != Admit::Accept {
            return false;
        }
        slot.pending = None;
        slot.retained = Some((ticket.request, outcome));
        slot.accepted = slot.accepted.saturating_add(1);
        true
    }

    /// Takes the retained result of a kind.
    pub fn consume(&mut self, kind: JobKind) -> Option<(u64, JobOutcome)> {
        self.slots
            .get_mut(kind.index())
            .and_then(|slot| slot.retained.take())
    }
}
