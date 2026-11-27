//! Destinations, help and command dispatch (U-01, U-03, #80). Keys outside
//! typing resolve to typed actions through the command table
//! ([`crate::commands`]); every action, from a key or a headless `action`,
//! is carried out here, and only when the current table offers it. Train
//! and Numpad start an ordinary single-track plan: the engine asks for it
//! ([`DomainOutcome::PlanRequested`](crate::DomainOutcome)) and the app
//! builds it exactly as for `--track`, so those sessions are stored like
//! any other.

use cadence_core::MonoTick;
use cadence_core::settings::{Accepts, KEYS};
use cadence_trainer::Phase;

use crate::ViewportState;
use crate::commands::{Action, Binding, CommandState, Destination, bindings_for};
use crate::view::{DestinationView, PauseReason};
use crate::{CoreError, DomainOutcome, Key, Screen, Transition};

/// The tracks Train offers, in order.
pub const TRAIN_TRACKS: [Phase; 5] = [
    Phase::WarmUp,
    Phase::AdaptiveWords,
    Phase::NumberRow,
    Phase::Numpad,
    Phase::Prose,
];

/// Train's track labels, aligned with [`TRAIN_TRACKS`].
const TRAIN_LABELS: [&str; 5] = ["warm-up", "words", "number row", "numpad", "prose"];

/// The session lengths offered: the accepted `session.minutes` values.
fn lengths() -> &'static [i64] {
    match KEYS.iter().find(|key| key.name == "session.minutes") {
        Some(key) => match key.accepts {
            Accepts::IntegerIn(values) => values,
            Accepts::IntegerRange(..)
            | Accepts::Boolean
            | Accepts::OneOf(_)
            | Accepts::SubsetOf(_)
            | Accepts::PackNames
            | Accepts::Theme => &[],
        },
        None => &[],
    }
}

/// Where the learner is outside practice, and the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Navigation {
    destination: Destination,
    /// Index into [`TRAIN_TRACKS`].
    track: usize,
    /// Index into the offered lengths (the shortest by default).
    length: usize,
    help: bool,
}

const fn cycle(index: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        match index.checked_add(1) {
            Some(next) if next < len => next,
            Some(_) | None => 0,
        }
    } else {
        match index.checked_sub(1) {
            Some(previous) => previous,
            None => len.saturating_sub(1),
        }
    }
}

impl crate::Engine {
    /// The command state the table is chosen by.
    fn command_state(&self) -> CommandState {
        CommandState {
            mode: self.view_mode(),
            enter_submits: self.screen == Screen::Practice
                && self
                    .current_track()
                    .is_ok_and(|track| track == cadence_core::progress::Track::Numpad),
            pause_reason: self.pause_reason,
            destination: self.navigation.destination,
            help: self.navigation.help,
            overlay: if self.setup.is_some() {
                crate::commands::Overlay::Setup
            } else if self.settings.is_open() {
                crate::commands::Overlay::Settings
            } else {
                crate::commands::Overlay::None
            },
        }
    }

    /// The bindings honoured now (#80): the footer, help and headless
    /// validation read this.
    #[must_use]
    pub fn commands(&self) -> &'static [Binding] {
        bindings_for(self.command_state())
    }

    pub(crate) const fn navigation_destination(&self) -> Destination {
        self.navigation.destination
    }

    /// Whether `action` is offered now.
    #[must_use]
    pub fn offers(&self, action: Action) -> bool {
        self.commands()
            .iter()
            .any(|binding| binding.action == action)
    }

    /// The bindings help lists: those of the state beneath it.
    pub(crate) fn help_view(&self) -> Option<&'static [Binding]> {
        self.navigation.help.then(|| {
            bindings_for(CommandState {
                help: false,
                ..self.command_state()
            })
        })
    }

    pub(crate) fn destination_view(&self) -> DestinationView {
        let minutes = lengths()
            .get(self.navigation.length)
            .copied()
            .and_then(|m| u64::try_from(m).ok())
            .unwrap_or(0);
        DestinationView {
            selected: self.navigation.destination,
            tracks: &TRAIN_LABELS,
            track: self.navigation.track,
            minutes,
        }
    }

    /// A key resolves through the table outside typing and overlays.
    pub(crate) fn routes_commands(&self, key: Key) -> bool {
        (self.setup.is_none() || crate::setup::passes_through(key))
            && !self.settings.is_open()
            && !matches!(key, Key::Interrupt | Key::ToggleTextSize)
            && self.view_mode() != crate::view::ViewMode::Typing
    }

    fn begin_today(&mut self, at: MonoTick) -> Result<DomainOutcome, CoreError> {
        if self.viewport != ViewportState::Usable {
            return Ok(DomainOutcome::None);
        }
        self.screen = Screen::Practice;
        self.transition(Some(at), cadence_core::session::SessionEvent::Start)?;
        self.open_exercise(at)?;
        Ok(DomainOutcome::SessionStarted)
    }

    /// Carries out an offered action.
    pub(crate) fn perform(
        &mut self,
        at: MonoTick,
        action: Action,
        t: &mut Transition,
    ) -> Result<(), CoreError> {
        if !self.offers(action) {
            return Ok(());
        }
        match action {
            Action::Begin => {
                t.outcome = match self.navigation.destination {
                    Destination::Today => self.begin_today(at)?,
                    // Progress reads history; it starts nothing.
                    Destination::Progress => DomainOutcome::None,
                    Destination::Train | Destination::Numpad => {
                        let phase = if self.navigation.destination == Destination::Numpad {
                            Phase::Numpad
                        } else {
                            TRAIN_TRACKS
                                .get(self.navigation.track)
                                .copied()
                                .unwrap_or(Phase::AdaptiveWords)
                        };
                        DomainOutcome::PlanRequested {
                            phase,
                            minutes: self.destination_view().minutes,
                        }
                    }
                };
            }
            Action::Quit | Action::SaveAndExit | Action::ForceExit | Action::FinishInvalid => {
                self.request_exit(at, t)?;
            }
            Action::Pause => self.pause_for(at, PauseReason::Requested)?,
            Action::Resume => self.resume(at)?,
            Action::EndAndReview => {
                if !self.invalid && !self.save.failed {
                    t.outcome = self.end_early(at)?;
                }
            }
            Action::Continue => {
                if self.viewport == ViewportState::Usable && self.interlude_settled(at)? {
                    t.outcome = self.advance_phase(at)?;
                }
            }
            Action::RetrySave => self.retry_save(),
            Action::SubmitEntry => t.outcome = self.practice_key(at, Key::Enter)?,
            Action::TextSize => self.text_size = self.text_size.next(),
            Action::NextDestination | Action::PreviousDestination => {
                let nav = &mut self.navigation;
                let all = Destination::ALL;
                let at_index = all.iter().position(|d| *d == nav.destination).unwrap_or(0);
                let next = cycle(at_index, all.len(), action == Action::NextDestination);
                nav.destination = all.get(next).copied().unwrap_or_default();
                // Entering Progress asks the worker for history (#81).
                if nav.destination == Destination::Progress {
                    t.job = self.jobs.start(crate::jobs::JobKind::History);
                    self.progress.begin(t.job);
                }
            }
            Action::Help => self.navigation.help = !self.navigation.help,
            Action::Settings => {
                if self.settings_openable() {
                    // A reopened overlay is a new requester: a late answer
                    // to the previous one is stale and changes nothing (#86).
                    self.jobs.restart_kind(crate::jobs::JobKind::ConfigWrite);
                    self.jobs.restart_kind(crate::jobs::JobKind::ConfigReload);
                    self.settings.open();
                }
            }
            Action::NextTrack | Action::PreviousTrack => {
                let nav = &mut self.navigation;
                nav.track = cycle(nav.track, TRAIN_TRACKS.len(), action == Action::NextTrack);
            }
            Action::Longer => {
                let nav = &mut self.navigation;
                nav.length = nav
                    .length
                    .saturating_add(1)
                    .min(lengths().len().saturating_sub(1));
            }
            Action::Shorter => {
                self.navigation.length = self.navigation.length.saturating_sub(1);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::cycle;

    #[test]
    fn destinations_and_tracks_wrap() {
        assert_eq!(cycle(0, 3, true), 1);
        assert_eq!(cycle(2, 3, true), 0);
        assert_eq!(cycle(0, 3, false), 2);
        assert_eq!(cycle(0, 0, true), 0);
    }
}
