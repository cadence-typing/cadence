//! The single-flight save protocol as a step machine (VP-03, #89). The
//! engine's storage is its driver: it holds the checkpoint payloads and the
//! timestamps, measures the watchdogs, and carries out the commands; every
//! flag and sequence number of the protocol lives here, so the invariants
//! proved over `step` (`proofs/Proofs/Save.lean`) hold in every reachable
//! state, not only to a model checker's bound.

/// What the protocol remembers between events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// The newest dispatched checkpoint sequence (0 before the first).
    pub sequence: u64,
    /// The newest acknowledged sequence (0 before the first).
    pub last_ack: u64,
    /// A checkpoint is in flight.
    pub pending: bool,
    /// The last checkpoint failed and has not been retried.
    pub failed: bool,
    /// A checkpoint is due at the next flush.
    pub due: bool,
    /// The session is finishing: the next acknowledgement with nothing due
    /// is the final one.
    pub finishing: bool,
    /// Practice is stalled on a slow acknowledgement (#51).
    pub stalled: bool,
}

/// What happens to the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Something changed that a checkpoint should carry.
    Due,
    /// The reducer's dispatch point: send a checkpoint if one may go.
    Flush,
    /// The store acknowledged sequence `seq`.
    Acked {
        /// The acknowledged sequence.
        seq: u64,
    },
    /// The store refused or failed sequence `seq`.
    AckFailed {
        /// The failed sequence.
        seq: u64,
    },
    /// The learner asked to retry a failed save.
    Retry,
    /// The learner asked to end the session.
    ExitRequested,
    /// The final save's watchdog expired.
    FinalTimeout,
    /// The running save's acknowledgement is overdue.
    Stalled,
}

/// What the driver does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Nothing.
    None,
    /// Build and send checkpoint `seq`.
    Dispatch {
        /// Its sequence.
        seq: u64,
    },
    /// Pause practice (a failed or stalled save).
    Pause,
    /// The stalled save caught up: resume practice.
    Resume,
    /// Exit; `success` only after the final acknowledgement.
    Quit {
        /// Whether everything is durable.
        success: bool,
    },
}

/// The state before anything was dispatched.
#[must_use]
pub const fn initial() -> State {
    State {
        sequence: 0,
        last_ack: 0,
        pending: false,
        failed: false,
        due: false,
        finishing: false,
        stalled: false,
    }
}

fn flush(s: State) -> (State, Command) {
    if !s.due || s.pending || s.failed {
        return (s, Command::None);
    }
    match s.sequence.checked_add(1) {
        Some(seq) => (
            State {
                sequence: seq,
                pending: true,
                due: false,
                ..s
            },
            Command::Dispatch { seq },
        ),
        None => (s, Command::None),
    }
}

fn acked(s: State, seq: u64) -> (State, Command) {
    if !s.pending || seq != s.sequence {
        return (s, Command::None);
    }
    let next = State {
        last_ack: seq,
        pending: false,
        stalled: false,
        ..s
    };
    if s.finishing && !s.due {
        (next, Command::Quit { success: true })
    } else if s.stalled {
        (next, Command::Resume)
    } else {
        (next, Command::None)
    }
}

fn ack_failed(s: State, seq: u64) -> (State, Command) {
    if !s.pending || seq != s.sequence {
        return (s, Command::None);
    }
    (
        State {
            pending: false,
            failed: true,
            stalled: false,
            ..s
        },
        Command::Pause,
    )
}

fn exit_requested(s: State) -> (State, Command) {
    if s.finishing {
        return if s.failed {
            (s, Command::Quit { success: false })
        } else {
            (s, Command::None)
        };
    }
    (
        State {
            finishing: true,
            due: true,
            stalled: false,
            ..s
        },
        Command::None,
    )
}

/// One step of the save protocol.
#[must_use]
pub fn step(s: State, event: Event) -> (State, Command) {
    match event {
        Event::Due => (State { due: true, ..s }, Command::None),
        Event::Flush => flush(s),
        Event::Acked { seq } => acked(s, seq),
        Event::AckFailed { seq } => ack_failed(s, seq),
        Event::Retry => {
            if s.failed {
                (
                    State {
                        failed: false,
                        due: true,
                        ..s
                    },
                    Command::None,
                )
            } else {
                (s, Command::None)
            }
        }
        Event::ExitRequested => exit_requested(s),
        Event::FinalTimeout => {
            if s.finishing && s.pending {
                (s, Command::Quit { success: false })
            } else {
                (s, Command::None)
            }
        }
        Event::Stalled => {
            if s.pending && !s.finishing && !s.stalled {
                (State { stalled: true, ..s }, Command::Pause)
            } else {
                (s, Command::None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_flight_retry_and_a_clean_exit() {
        let (s, c) = step(initial(), Event::Flush);
        assert_eq!(c, Command::None, "nothing due");
        let (s, _) = step(s, Event::Due);
        let (s, c) = step(s, Event::Flush);
        assert_eq!(c, Command::Dispatch { seq: 1 });
        let (s, _) = step(s, Event::Due);
        assert_eq!(step(s, Event::Flush).1, Command::None, "one in flight");
        let (s, c) = step(s, Event::AckFailed { seq: 1 });
        assert_eq!(c, Command::Pause);
        assert_eq!(step(s, Event::Flush).1, Command::None, "failed waits");
        let (s, _) = step(s, Event::Retry);
        let (s, c) = step(s, Event::Flush);
        assert_eq!(c, Command::Dispatch { seq: 2 });
        let (s, c) = step(s, Event::ExitRequested);
        assert_eq!(c, Command::None);
        // The pre-exit save's acknowledgement is not the final one.
        let (s, c) = step(s, Event::Acked { seq: 2 });
        assert_eq!(c, Command::None);
        let (s, c) = step(s, Event::Flush);
        assert_eq!(c, Command::Dispatch { seq: 3 });
        assert_eq!(step(s, Event::Acked { seq: 2 }).1, Command::None, "stale");
        let (s, c) = step(s, Event::Acked { seq: 3 });
        assert_eq!(c, Command::Quit { success: true });
        assert_eq!((s.last_ack, s.sequence), (3, 3));
    }

    #[test]
    fn a_stall_pauses_once_and_the_ack_resumes() {
        let (s, _) = step(initial(), Event::Due);
        let (s, _) = step(s, Event::Flush);
        let (s, c) = step(s, Event::Stalled);
        assert_eq!(c, Command::Pause);
        assert_eq!(step(s, Event::Stalled).1, Command::None);
        assert_eq!(step(s, Event::Acked { seq: 1 }).1, Command::Resume);
        // A failed final save quits unsuccessfully on a second request.
        let (s, _) = step(s, Event::ExitRequested);
        let (s, _) = step(s, Event::AckFailed { seq: 1 });
        assert_eq!(
            step(s, Event::ExitRequested).1,
            Command::Quit { success: false }
        );
    }
}
