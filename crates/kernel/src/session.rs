//! The session lifecycle as one decision table (A-05, #31). Every change of
//! the engine's session state goes through [`step`]; the engine carries out
//! the clock command and the outcome. Events that are not legal in a state
//! are [`Step::Refused`], and the engine reports them as a typed error.

/// Session lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Before the session starts.
    Ready,
    /// Practice accepts input; the active clock may run.
    Running,
    /// Practice is paused (explicit, focus loss, paste, too small, a failed
    /// save, or an invalid measurement); the active clock is stopped.
    Paused,
    /// A phase's budget is used up; the interlude is showing.
    PhaseComplete,
    /// The review is showing; no more typing in this session.
    Review,
    /// The final checkpoint was requested and is not yet acknowledged.
    Finishing,
    /// The final checkpoint was acknowledged.
    Saved,
}

/// What happened, as the engine reports it to the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Enter on the start screen.
    Start,
    /// An explicit pause, focus loss, a paste or a too-small terminal.
    Pause,
    /// A checkpoint save failed.
    SaveFailed,
    /// A measurement invariant failed; practice stops for good.
    Invalidate,
    /// Esc or Enter in the pause controls.
    Resume,
    /// A phase's deadline passed with typing phases still ahead.
    Deadline,
    /// The deadline of the last scored phase passed: the session is
    /// complete; a closing review may follow as one more typing phase (#49).
    LastDeadline,
    /// Enter or Esc after the interlude grace.
    NextPhase,
    /// The plan reached its review phase or its end.
    PlanDone,
    /// `e` in the pause controls, or Ctrl-C during practice or an interlude.
    EndEarly,
    /// An exit was requested; the final checkpoint becomes due.
    Finish,
    /// The final checkpoint was acknowledged.
    FinalSaved,
    /// A checkpoint acknowledgement passed its bound while practice runs
    /// (#51): practice pauses, nothing changes but the clock.
    SaveStalled,
    /// The stalled checkpoint was acknowledged: the stall's pause ends.
    SaveCaughtUp,
}

/// What the engine does with the active clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// Leave it as it is.
    Keep,
    /// Close the open segment, if any.
    Stop,
    /// Open a new segment (it opens only after the first content key).
    Resume,
}

/// What the step means for the persisted session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No change.
    Unchanged,
    /// The plan was finished.
    Completed,
    /// The session ends before its plan did: abandoned, unless it was
    /// already completed.
    Unfinished,
}

/// The decision for one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Move to `next`, apply `clock` and `outcome`.
    Move {
        /// The next state.
        next: State,
        /// The clock command.
        clock: Clock,
        /// The status consequence.
        outcome: Outcome,
    },
    /// The event is not legal in this state.
    Refused,
}

/// The session lifecycle table. Every cell is listed: no wildcard arm can
/// silently admit a new state or event.
#[must_use]
pub fn step(state: State, event: Event) -> Step {
    match event {
        Event::Start => match state {
            State::Ready => Step::Move {
                next: State::Running,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Running
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
        },
        Event::Pause => match state {
            State::Ready
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Running => Step::Move {
                next: State::Paused,
                clock: Clock::Stop,
                outcome: Outcome::Unchanged,
            },
        },
        Event::SaveFailed => match state {
            State::Ready => Step::Move {
                next: State::Ready,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Running => Step::Move {
                next: State::Paused,
                clock: Clock::Stop,
                outcome: Outcome::Unchanged,
            },
            State::Paused => Step::Move {
                next: State::Paused,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::PhaseComplete => Step::Move {
                next: State::PhaseComplete,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Review => Step::Move {
                next: State::Review,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Finishing => Step::Move {
                next: State::Finishing,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Saved => Step::Move {
                next: State::Saved,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
        },
        Event::Invalidate => match state {
            State::Ready | State::Paused | State::PhaseComplete => Step::Move {
                next: State::Paused,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Running => Step::Move {
                next: State::Paused,
                clock: Clock::Stop,
                outcome: Outcome::Unchanged,
            },
            State::Review => Step::Move {
                next: State::Review,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Finishing => Step::Move {
                next: State::Finishing,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
            State::Saved => Step::Move {
                next: State::Saved,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
        },
        Event::Resume => match state {
            State::Ready
            | State::Running
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Paused => Step::Move {
                next: State::Running,
                clock: Clock::Resume,
                outcome: Outcome::Unchanged,
            },
        },
        Event::Deadline => match state {
            State::Ready
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Running => Step::Move {
                next: State::PhaseComplete,
                clock: Clock::Stop,
                outcome: Outcome::Unchanged,
            },
        },
        Event::LastDeadline => match state {
            State::Ready
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Running => Step::Move {
                next: State::PhaseComplete,
                clock: Clock::Stop,
                outcome: Outcome::Completed,
            },
        },
        Event::NextPhase => match state {
            State::Ready
            | State::Running
            | State::Paused
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::PhaseComplete => Step::Move {
                next: State::Running,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
        },
        Event::PlanDone => match state {
            State::Ready | State::Paused | State::Review | State::Finishing | State::Saved => {
                Step::Refused
            }
            State::Running | State::PhaseComplete => Step::Move {
                next: State::Review,
                clock: Clock::Stop,
                outcome: Outcome::Completed,
            },
        },
        Event::EndEarly => match state {
            State::Ready | State::Review | State::Finishing | State::Saved => Step::Refused,
            State::Running | State::Paused | State::PhaseComplete => Step::Move {
                next: State::Review,
                clock: Clock::Stop,
                outcome: Outcome::Unfinished,
            },
        },
        Event::Finish => match state {
            State::Ready
            | State::Running
            | State::Paused
            | State::PhaseComplete
            | State::Review => Step::Move {
                next: State::Finishing,
                clock: Clock::Stop,
                outcome: Outcome::Unfinished,
            },
            State::Finishing | State::Saved => Step::Refused,
        },
        Event::FinalSaved => match state {
            State::Ready
            | State::Running
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Saved => Step::Refused,
            State::Finishing => Step::Move {
                next: State::Saved,
                clock: Clock::Keep,
                outcome: Outcome::Unchanged,
            },
        },
        Event::SaveStalled => match state {
            State::Ready
            | State::Paused
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Running => Step::Move {
                next: State::Paused,
                clock: Clock::Stop,
                outcome: Outcome::Unchanged,
            },
        },
        Event::SaveCaughtUp => match state {
            State::Ready
            | State::Running
            | State::PhaseComplete
            | State::Review
            | State::Finishing
            | State::Saved => Step::Refused,
            State::Paused => Step::Move {
                next: State::Running,
                clock: Clock::Resume,
                outcome: Outcome::Unchanged,
            },
        },
    }
}

/// The persisted session status the table's outcomes act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The plan is in progress.
    Open,
    /// The last scored (non-review) phase reached its deadline (#49).
    Completed,
    /// The session ended before its plan did.
    Abandoned,
}

/// The status after a step's outcome: a completed session stays completed
/// whatever follows, so ending during the closing review (#49) or exiting
/// keeps it completed; an unfinished end of an open plan abandons it.
#[must_use]
pub fn settle(status: Status, outcome: Outcome) -> Status {
    match outcome {
        Outcome::Completed => Status::Completed,
        Outcome::Unchanged => status,
        Outcome::Unfinished => match status {
            Status::Open => Status::Abandoned,
            Status::Completed => Status::Completed,
            Status::Abandoned => Status::Abandoned,
        },
    }
}

/// Whether a checkpoint acknowledgement outstanding for `elapsed_us` is
/// overdue under the bound `bound_us`.
#[must_use]
pub fn overdue(elapsed_us: u64, bound_us: u64) -> bool {
    elapsed_us > bound_us
}
