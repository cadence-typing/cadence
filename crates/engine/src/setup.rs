//! First-run setup (P-01, #78): shown when there is no config file (a fresh
//! profile, or once after upgrading). It asks only the layout, whether a
//! numpad is present and the session length; Enter accepts what is shown.
//! The choice is written through the config worker like any settings
//! write, and practice starts only after the acknowledgement: the engine
//! then reports [`DomainOutcome::SetupComplete`](crate::DomainOutcome) and
//! the app rebuilds the store and plan from the written file.

use cadence_core::settings::{KEYS, Setting};

use crate::jobs::JobKind;
use crate::settings::SettingsRequest;
use crate::view::{SettingRow, SettingsView};
use crate::{Direction, Key, Transition};

/// Whether `key` leaves setup to the start screen's handling: Ctrl-C and
/// `q` quit, as they do on the start screen.
#[must_use]
pub(crate) fn passes_through(key: Key) -> bool {
    match key {
        Key::Interrupt => true,
        Key::Content(c) => c.get() == 'q',
        Key::Arrow(_)
        | Key::Enter
        | Key::Escape
        | Key::Backspace
        | Key::ToggleTextSize
        | Key::Tab
        | Key::BackTab
        | Key::Other => false,
    }
}

/// The keys setup asks, in order.
pub const SETUP_KEYS: [&str; 3] = ["keyboard.layout", "keyboard.numpad", "session.minutes"];

/// The setup draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Setup {
    cursor: usize,
    draft: Vec<Setting>,
}

impl crate::Engine {
    /// Opens first-run setup on the start screen (#78), drafted from the
    /// current settings (the defaults when there is no file).
    #[must_use]
    pub fn with_first_run(mut self) -> Self {
        let draft = SETUP_KEYS
            .iter()
            .map(|name| {
                self.settings.value(name).cloned().unwrap_or_else(|| {
                    KEYS.iter()
                        .find(|key| key.name == *name)
                        .map_or(Setting::Boolean(false), |key| key.default.setting())
                })
            })
            .collect();
        self.setup = Some(Setup { cursor: 0, draft });
        self
    }

    pub(crate) fn setup_key(&mut self, key: Key, t: &mut Transition) {
        let waiting = self.settings.view_pending();
        let Some(setup) = self.setup.as_mut() else {
            return;
        };
        let last = SETUP_KEYS.len().saturating_sub(1);
        match key {
            Key::Arrow(Direction::Up) => setup.cursor = setup.cursor.saturating_sub(1),
            Key::Arrow(Direction::Down) => setup.cursor = setup.cursor.saturating_add(1).min(last),
            Key::Arrow(direction @ (Direction::Left | Direction::Right)) if !waiting => {
                if let (Some(name), Some(slot)) = (
                    SETUP_KEYS.get(setup.cursor),
                    setup.draft.get_mut(setup.cursor),
                ) && let Some(spec) = KEYS.iter().find(|key| key.name == *name)
                    && let Some(next) =
                        crate::settings::step(spec.accepts, slot, direction == Direction::Right)
                {
                    *slot = next;
                }
            }
            Key::Enter if !waiting => {
                // Every answer is written, defaults included: the file then
                // exists and setup is not shown again.
                let edits = SETUP_KEYS
                    .iter()
                    .copied()
                    .zip(setup.draft.iter().cloned())
                    .collect();
                let request = SettingsRequest::Write {
                    based_on: self.settings.digest(),
                    edits,
                };
                if let Some(ticket) = self.jobs.start(JobKind::ConfigWrite) {
                    self.settings.begin(ticket, request);
                    t.job = Some(ticket);
                }
            }
            Key::Arrow(_)
            | Key::Enter
            | Key::Escape
            | Key::Content(_)
            | Key::Backspace
            | Key::Interrupt
            | Key::ToggleTextSize
            | Key::Tab
            | Key::BackTab
            | Key::Other => {}
        }
    }

    pub(crate) fn setup_view(&self) -> Option<SettingsView> {
        let setup = self.setup.as_ref()?;
        Some(SettingsView {
            rows: SETUP_KEYS
                .iter()
                .zip(&setup.draft)
                .map(|(name, value)| SettingRow {
                    key: name,
                    value: value.to_string(),
                    editable: true,
                    changed: false,
                    // Setup answers apply to the very first session.
                    applies_now: true,
                })
                .collect(),
            cursor: setup.cursor,
            saving: self.settings.view_pending(),
            notice: self.settings.notice(),
        })
    }
}
