//! A single SQLite writer with one reserved command and result slot.
//!
//! Submission never blocks the owner. A worker's terminal outcome remains
//! in the result channel until consumed; a wakeup is not the save fact.
//! Shutdown joins only after the owner has consumed the final receipt.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cadence_core::progress::{Checkpoint, SaveResult};
use cadence_data::{ProgressStore, StoreBootstrap, StoreError};

/// One in-flight checkpoint, matching the engine's save-state contract.
const SAVE_SLOT_COUNT: usize = 1;
/// One in-flight history query (#81): the engine keeps one per kind.
const HISTORY_SLOT_COUNT: usize = 1;

/// What the writer is asked to do. A query never delays a save past the
/// one ahead of it, and its answer has its own channel.
enum Request {
    // Boxed: a checkpoint is far larger than a query.
    Save(Box<Checkpoint>),
    History(cadence_engine::jobs::JobTicket, usize),
}

/// A history query's answer, with the ticket it answers (#81).
pub type HistoryAnswer = (
    cadence_engine::jobs::JobTicket,
    Result<Vec<cadence_core::progress::HistoryPoint>, String>,
);
/// Startup watchdog is wall time, never practice time.
const STORE_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Save transport failures, distinct from a database commit rejection.
#[derive(Debug)]
pub enum WriterError {
    /// Database startup failed before entering the terminal.
    Store(StoreError),
    /// A thread could not be created.
    Spawn(std::io::Error),
    /// Store startup did not finish within its watchdog.
    StartupTimeout,
    /// The owner has not yet consumed its reserved terminal result.
    Busy,
    /// The writer exited without a terminal result.
    Disconnected,
    /// The worker panicked; no success is fabricated.
    Panicked,
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(f),
            Self::Spawn(error) => write!(f, "cannot start progress writer: {error}"),
            Self::StartupTimeout => f.write_str("progress writer startup timed out"),
            Self::Busy => f.write_str("progress writer's reserved slot is occupied"),
            Self::Disconnected => {
                f.write_str("progress writer disconnected before acknowledgement")
            }
            Self::Panicked => {
                f.write_str("progress writer panicked; unacknowledged progress is not saved")
            }
        }
    }
}

impl std::error::Error for WriterError {}

/// Owner-side handle. The database and its advisory lock live on the worker.
pub struct SaveWorker {
    requests: SyncSender<Request>,
    results: Receiver<SaveResult>,
    history: Receiver<HistoryAnswer>,
    thread: JoinHandle<()>,
}

impl SaveWorker {
    /// Opens a private/profile database on the writer before terminal entry.
    pub fn open(
        profile: Option<PathBuf>,
        learner_window: usize,
        layout: Option<cadence_core::Layout>,
        audit: Vec<(&'static str, String)>,
    ) -> Result<(Self, StoreBootstrap), WriterError> {
        let (requests, incoming) =
            mpsc::sync_channel(SAVE_SLOT_COUNT.saturating_add(HISTORY_SLOT_COUNT));
        let (answers, history) = mpsc::sync_channel(HISTORY_SLOT_COUNT);
        let (outgoing, results) = mpsc::sync_channel(SAVE_SLOT_COUNT);
        let (startup, ready) = mpsc::sync_channel(SAVE_SLOT_COUNT);
        let thread = thread::Builder::new()
            .name("cadence-progress".to_string())
            .spawn(move || {
                writer_loop(
                    profile,
                    learner_window,
                    (layout, audit),
                    incoming,
                    (outgoing, answers),
                    startup,
                );
            })
            .map_err(WriterError::Spawn)?;
        match ready.recv_timeout(STORE_STARTUP_TIMEOUT) {
            Ok(Ok(bootstrap)) => Ok((
                Self {
                    requests,
                    results,
                    history,
                    thread,
                },
                bootstrap,
            )),
            Ok(Err(error)) => {
                thread
                    .join()
                    .map_err(|_panic_payload| WriterError::Panicked)?;
                Err(WriterError::Store(error))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(WriterError::StartupTimeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(WriterError::Disconnected),
        }
    }

    /// Leaves the checkpoint in its owner's pending slot when submission fails.
    pub fn try_submit(&self, pending: &mut Option<Checkpoint>) -> Result<(), WriterError> {
        let Some(checkpoint) = pending.take() else {
            return Ok(());
        };
        match self.requests.try_send(Request::Save(Box::new(checkpoint))) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(Request::Save(checkpoint))) => {
                *pending = Some(*checkpoint);
                Err(WriterError::Busy)
            }
            Err(TrySendError::Disconnected(Request::Save(checkpoint))) => {
                *pending = Some(*checkpoint);
                Err(WriterError::Disconnected)
            }
            Err(TrySendError::Full(Request::History(..))) => Err(WriterError::Busy),
            Err(TrySendError::Disconnected(Request::History(..))) => Err(WriterError::Disconnected),
        }
    }

    /// Asks for the newest `limit` sessions' history (#81).
    ///
    /// # Errors
    /// Busy or disconnected.
    pub fn try_query(
        &self,
        ticket: cadence_engine::jobs::JobTicket,
        limit: usize,
    ) -> Result<(), WriterError> {
        self.requests
            .try_send(Request::History(ticket, limit))
            .map_err(|error| match error {
                TrySendError::Full(_) => WriterError::Busy,
                TrySendError::Disconnected(_) => WriterError::Disconnected,
            })
    }

    /// One history answer, if any.
    ///
    /// # Errors
    /// The writer stopped.
    pub fn try_history(&self) -> Result<Option<HistoryAnswer>, WriterError> {
        match self.history.try_recv() {
            Ok(answer) => Ok(Some(answer)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(WriterError::Disconnected),
        }
    }

    /// Consumes at most one retained terminal result without waiting.
    pub fn try_result(&self) -> Result<Option<SaveResult>, WriterError> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(WriterError::Disconnected),
        }
    }

    /// Closes the writer after all required acknowledgements were consumed.
    /// Never call while a dispatched checkpoint is still outstanding.
    pub fn finish(self) -> Result<(), WriterError> {
        let Self {
            requests,
            results,
            history,
            thread,
        } = self;
        drop(requests);
        drop(history);
        // Disconnecting results also unblocks publication on abnormal exit.
        drop(results);
        thread
            .join()
            .map_err(|_panic_payload| WriterError::Panicked)
    }
}

/// The test-only slow-save fault: `CADENCE_FAULT_SLOW_SAVE="SEQ:MS"`.
#[cfg(feature = "test-faults")]
fn slow_save_fault() -> Option<(u64, std::time::Duration)> {
    let value = std::env::var("CADENCE_FAULT_SLOW_SAVE").ok()?;
    let (sequence, millis) = value.split_once(':')?;
    Some((
        sequence.parse().ok()?,
        std::time::Duration::from_millis(millis.parse().ok()?),
    ))
}

fn writer_loop(
    profile: Option<PathBuf>,
    learner_window: usize,
    (layout, audit): (Option<cadence_core::Layout>, Vec<(&'static str, String)>),
    requests: Receiver<Request>,
    (results, answers): (SyncSender<SaveResult>, SyncSender<HistoryAnswer>),
    startup: SyncSender<Result<StoreBootstrap, StoreError>>,
) {
    let opened = ProgressStore::open(profile.as_deref()).and_then(|mut store| {
        // The first chosen layout adopts the history recorded before
        // layouts existed (#77).
        let next = store.bootstrap(1)?.next_session;
        if let Some(layout) = layout {
            store.adopt_unspecified(layout, next)?;
        }
        // Score-affecting settings changes are recorded at the session they
        // first apply to (#79).
        store.audit_settings(next, &audit)?;
        let bootstrap = store.bootstrap_for(learner_window, layout)?;
        Ok((store, bootstrap))
    });
    let mut store = match opened {
        Ok((store, bootstrap)) => {
            if startup.send(Ok(bootstrap)).is_err() {
                return;
            }
            store
        }
        Err(error) => {
            if startup.send(Err(error)).is_err() {
                return;
            }
            return;
        }
    };
    #[cfg(feature = "test-faults")]
    let slow = slow_save_fault();
    while let Ok(request) = requests.recv() {
        let checkpoint = match request {
            Request::Save(checkpoint) => *checkpoint,
            Request::History(ticket, limit) => {
                let answer = store
                    .history_points(limit)
                    .map_err(|error| error.to_string());
                if answers.send((ticket, answer)).is_err() {
                    return;
                }
                continue;
            }
        };
        let key = checkpoint.key;
        #[cfg(feature = "test-faults")]
        if let Some((sequence, delay)) = slow
            && key.sequence.get() == sequence
        {
            thread::sleep(delay);
        }
        let outcome = store
            .apply_checkpoint(&checkpoint)
            .map(|_| ())
            .map_err(|e| e.to_string());
        // Blocking is permitted on this worker, never on the owner thread.
        // The complete key accompanies failures as well as successes.
        if results
            .send(SaveResult {
                key,
                result: outcome,
            })
            .is_err()
        {
            return;
        }
    }
}
