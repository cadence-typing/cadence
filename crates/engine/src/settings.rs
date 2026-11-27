//! The settings overlay (U-04, #79). It edits the keys of `config.toml`
//! (#76) as a draft; Enter asks the worker to write the draft atomically
//! (a `ConfigWrite` job, its result admitted by the proved `ticket::admit`),
//! and nothing reads as saved until that acknowledgement. Each changed key
//! then applies when the proved `settings::takes_effect` says: display
//! settings at once, anything score-affecting from the next session. A
//! failed or conflicting write keeps the previous settings and shows one
//! diagnostic. `r` reloads the file; an invalid file keeps the last valid
//! settings.

use cadence_core::settings::{Accepts, ConfigDigest, Effect, KEYS, Setting, effect};

use crate::jobs::{JobKind, JobOutcome, JobTicket};

/// With `display.key_guide = "auto"`, the guide shows for this many
/// numeric sessions, then hides (#82).
pub const KEY_GUIDE_SESSIONS: usize = 3;
use crate::view::{SettingRow, SettingsView};

/// What the worker is asked to do for a settings job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRequest {
    /// Apply `edits` to the file, which must still have digest `based_on`.
    Write {
        /// The digest the edit was based on (`None`: there was no file).
        based_on: Option<ConfigDigest>,
        /// The changed keys and their new values.
        edits: Vec<(&'static str, Setting)>,
    },
    /// Read and validate the file again.
    Reload,
}

/// The last settings outcome shown to the learner.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Notice {
    Saved,
    Reloaded,
    Problem(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Overlay {
    cursor: usize,
    draft: Vec<Setting>,
}

/// The effective settings, the open overlay and the job in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsState {
    /// Effective values, one per entry of [`KEYS`].
    values: Vec<Setting>,
    digest: Option<ConfigDigest>,
    overlay: Option<Overlay>,
    pending: Option<(JobTicket, SettingsRequest)>,
    /// Saved keys that apply from the next session.
    held: Vec<&'static str>,
    /// The validated theme file `display.theme` names, if it names one (#82).
    theme_file: Option<cadence_core::settings::ThemeColors>,
    notice: Option<Notice>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            values: KEYS.iter().map(|key| key.default.setting()).collect(),
            digest: None,
            overlay: None,
            pending: None,
            held: Vec::new(),
            theme_file: None,
            notice: None,
        }
    }
}

/// The next value of an editable setting in `direction` (+1 or -1), or
/// `None` when the key is edited in the file only (lists).
pub(crate) fn step(accepts: Accepts, current: &Setting, forward: bool) -> Option<Setting> {
    let cycle = |options: &[&str], now: &str| -> Option<String> {
        let len = options.len();
        let at = options.iter().position(|o| *o == now);
        let next = match (at, forward) {
            (Some(i), true) => i.checked_add(1).and_then(|n| n.checked_rem(len)),
            (Some(i), false) => Some(i.checked_sub(1).unwrap_or(len.saturating_sub(1))),
            (None, true | false) => Some(0),
        }?;
        options.get(next).map(|s| (*s).to_string())
    };
    match (accepts, current) {
        (Accepts::IntegerIn(options), Setting::Integer(n)) => {
            let at = options.iter().position(|o| o == n).unwrap_or(0);
            let next = if forward {
                at.saturating_add(1).min(options.len().saturating_sub(1))
            } else {
                at.saturating_sub(1)
            };
            options.get(next).copied().map(Setting::Integer)
        }
        (Accepts::IntegerRange(low, high), Setting::Integer(n)) => {
            Some(Setting::Integer(if forward {
                n.saturating_add(1).min(high)
            } else {
                n.saturating_sub(1).max(low)
            }))
        }
        (Accepts::Boolean, Setting::Boolean(b)) => Some(Setting::Boolean(!b)),
        (Accepts::OneOf(options), Setting::Text(s)) => cycle(options, s).map(Setting::Text),
        (Accepts::Theme, Setting::Text(s)) => {
            cycle(&["dark", "light", "terminal"], s).map(Setting::Text)
        }
        (Accepts::SubsetOf(_) | Accepts::PackNames, _)
        | (
            Accepts::IntegerIn(_)
            | Accepts::IntegerRange(..)
            | Accepts::Boolean
            | Accepts::OneOf(_)
            | Accepts::Theme,
            Setting::Integer(_) | Setting::Boolean(_) | Setting::Text(_) | Setting::List(_),
        ) => None,
    }
}

/// A short label for a key.
fn label(name: &str) -> &str {
    name.split_once('.').map_or(name, |(_, key)| key)
}

impl SettingsState {
    /// Starts from the values read at startup (flags applied) and the
    /// file's digest.
    pub(crate) fn loaded(values: &[(&'static str, Setting)], digest: Option<ConfigDigest>) -> Self {
        let mut state = Self::default();
        state.replace(values, digest);
        state
    }

    fn replace(&mut self, values: &[(&'static str, Setting)], digest: Option<ConfigDigest>) {
        for (index, key) in KEYS.iter().enumerate() {
            if let (Some((_, value)), Some(slot)) = (
                values.iter().find(|(name, _)| *name == key.name),
                self.values.get_mut(index),
            ) {
                *slot = value.clone();
            }
        }
        self.digest = digest;
    }

    /// The validated theme file in use, if any (#82).
    pub(crate) const fn theme_file(&self) -> Option<cadence_core::settings::ThemeColors> {
        self.theme_file
    }

    /// Starts with a theme file read at startup, or the reason it was not
    /// used (the built-in palette stays).
    pub(crate) fn start_theme(
        &mut self,
        theme: Option<Result<cadence_core::settings::ThemeColors, String>>,
    ) {
        match theme {
            Some(Ok(colours)) => self.theme_file = Some(colours),
            Some(Err(reason)) => {
                self.notice = Some(Notice::Problem(reason));
                if let Some(slot) = KEYS
                    .iter()
                    .position(|key| key.name == "display.theme")
                    .and_then(|index| self.values.get_mut(index))
                {
                    *slot = Setting::Text("dark".into());
                }
            }
            None => {}
        }
    }

    /// A key's effective value.
    pub(crate) fn value(&self, name: &str) -> Option<&Setting> {
        KEYS.iter()
            .position(|key| key.name == name)
            .and_then(|index| self.values.get(index))
    }

    pub(crate) const fn digest(&self) -> Option<ConfigDigest> {
        self.digest
    }

    /// A write or reload is waiting for its acknowledgement.
    pub(crate) const fn view_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.overlay.is_some()
    }

    pub(crate) fn open(&mut self) {
        // The previous overlay's request, if any, is no longer waited on.
        self.pending = None;
        self.overlay = Some(Overlay {
            cursor: 0,
            draft: self.values.clone(),
        });
        self.notice = None;
    }

    pub(crate) fn close(&mut self) {
        self.overlay = None;
    }

    pub(crate) fn move_cursor(&mut self, down: bool) {
        if let Some(overlay) = self.overlay.as_mut() {
            let last = KEYS.len().saturating_sub(1);
            overlay.cursor = if down {
                overlay.cursor.saturating_add(1).min(last)
            } else {
                overlay.cursor.saturating_sub(1)
            };
        }
    }

    pub(crate) fn change(&mut self, forward: bool) {
        if let Some(overlay) = self.overlay.as_mut()
            && let (Some(key), Some(slot)) = (
                KEYS.get(overlay.cursor),
                overlay.draft.get_mut(overlay.cursor),
            )
            && let Some(next) = step(key.accepts, slot, forward)
        {
            *slot = next;
        }
    }

    /// The write the draft needs, or `None` when nothing changed.
    pub(crate) fn save_request(&self) -> Option<SettingsRequest> {
        let overlay = self.overlay.as_ref()?;
        let edits: Vec<(&'static str, Setting)> = KEYS
            .iter()
            .zip(overlay.draft.iter().zip(&self.values))
            .filter(|(_, (draft, value))| draft != value)
            .map(|(key, (draft, _))| (key.name, draft.clone()))
            .collect();
        (!edits.is_empty()).then_some(SettingsRequest::Write {
            based_on: self.digest,
            edits,
        })
    }

    pub(crate) fn begin(&mut self, ticket: JobTicket, request: SettingsRequest) {
        self.pending = Some((ticket, request));
    }

    /// The request a started job carries, for the worker.
    pub(crate) fn request(&self, ticket: JobTicket) -> Option<&SettingsRequest> {
        self.pending
            .as_ref()
            .filter(|(pending, _)| *pending == ticket)
            .map(|(_, request)| request)
    }

    /// Applies an admitted result. Returns the changed keys that apply now.
    pub(crate) fn finish(&mut self, kind: JobKind, outcome: JobOutcome) -> Vec<&'static str> {
        self.pending = None;
        match outcome {
            JobOutcome::Settings {
                values,
                digest,
                theme,
            } => {
                let before = self.values.clone();
                let previous_theme = self.value("display.theme").cloned();
                self.replace(&values, digest);
                // A theme file is used only whole (#82): an invalid one keeps
                // the current palette and theme, with one diagnostic.
                let theme_problem = match theme {
                    None => {
                        self.theme_file = None;
                        None
                    }
                    Some(Ok(colours)) => {
                        self.theme_file = Some(colours);
                        None
                    }
                    Some(Err(reason)) => {
                        if let (Some(old), Some(slot)) = (
                            previous_theme,
                            KEYS.iter()
                                .position(|key| key.name == "display.theme")
                                .and_then(|index| self.values.get_mut(index)),
                        ) {
                            *slot = old;
                        }
                        Some(reason)
                    }
                };
                let mut now = Vec::new();
                for (key, (old, new)) in KEYS.iter().zip(before.iter().zip(&self.values)) {
                    if old == new {
                        continue;
                    }
                    match effect(key.name) {
                        Some(Effect::Now) => now.push(key.name),
                        Some(Effect::NextSession) if !self.held.contains(&key.name) => {
                            self.held.push(key.name);
                        }
                        Some(Effect::NextSession) | None => {}
                    }
                }
                if let Some(reason) = theme_problem {
                    self.notice = Some(Notice::Problem(reason));
                } else {
                    match kind {
                        JobKind::ConfigWrite => {
                            self.notice = Some(Notice::Saved);
                            self.overlay = None;
                        }
                        JobKind::ConfigReload | JobKind::Ping | JobKind::History => {
                            self.notice = Some(Notice::Reloaded);
                            if let Some(overlay) = self.overlay.as_mut() {
                                overlay.draft.clone_from(&self.values);
                            }
                        }
                    }
                }
                now
            }
            JobOutcome::Failed(reason) => {
                // The previous settings stay; the draft returns to them.
                self.notice = Some(Notice::Problem(reason));
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.draft.clone_from(&self.values);
                }
                Vec::new()
            }
            JobOutcome::Succeeded | JobOutcome::History(_) => Vec::new(),
        }
    }

    /// The one line naming the last outcome, for every screen.
    pub(crate) fn notice(&self) -> Option<String> {
        let held = || -> String {
            let names: Vec<&str> = self.held.iter().map(|name| label(name)).collect();
            if names.is_empty() {
                String::new()
            } else {
                format!(" · {} applies next session", names.join(", "))
            }
        };
        match self.notice.as_ref()? {
            Notice::Saved => Some(format!("Settings saved{}", held())),
            Notice::Reloaded => Some(format!("Settings reloaded{}", held())),
            Notice::Problem(reason) => Some(format!("Settings not changed: {reason}")),
        }
    }

    pub(crate) fn view(&self) -> Option<SettingsView> {
        let overlay = self.overlay.as_ref()?;
        let rows = KEYS
            .iter()
            .zip(overlay.draft.iter().zip(&self.values))
            .map(|(key, (draft, value))| SettingRow {
                key: key.name,
                value: draft.to_string(),
                editable: step(key.accepts, draft, true).is_some(),
                changed: draft != value,
                applies_now: effect(key.name) == Some(Effect::Now),
            })
            .collect();
        Some(SettingsView {
            rows,
            cursor: overlay.cursor,
            saving: self.pending.is_some(),
            notice: self.notice(),
        })
    }
}

impl crate::Engine {
    /// Starts from the settings read at startup (#79).
    #[must_use]
    pub fn with_settings(
        mut self,
        values: &[(&'static str, Setting)],
        digest: Option<ConfigDigest>,
    ) -> Self {
        self.settings = SettingsState::loaded(values, digest);
        self
    }

    /// The theme file read at startup (#82): its colours, or why the
    /// built-in palette is used instead.
    #[must_use]
    pub fn with_theme(
        mut self,
        theme: Option<Result<cadence_core::settings::ThemeColors, String>>,
    ) -> Self {
        self.settings.start_theme(theme);
        self
    }

    /// What the worker must do for a started settings job.
    #[must_use]
    pub fn settings_request(&self, ticket: JobTicket) -> Option<SettingsRequest> {
        self.settings.request(ticket).cloned()
    }

    /// Settings open from the start screen or while paused, never while
    /// typing.
    pub(crate) fn settings_openable(&self) -> bool {
        !self.save.finishing
            && !self.invalid
            && (self.screen == crate::Screen::Today
                || self.session_state == cadence_core::session::SessionState::Paused)
    }

    pub(crate) fn settings_key(&mut self, key: crate::Key, t: &mut crate::Transition) {
        use crate::{Direction, Key};
        let waiting = self.settings.view().is_some_and(|view| view.saving);
        match key {
            Key::Arrow(Direction::Up) => self.settings.move_cursor(false),
            Key::Arrow(Direction::Down) => self.settings.move_cursor(true),
            Key::Arrow(Direction::Left) if !waiting => self.settings.change(false),
            Key::Arrow(Direction::Right) if !waiting => self.settings.change(true),
            Key::Escape => self.settings.close(),
            Key::Enter if !waiting => match self.settings.save_request() {
                Some(request) => self.start_settings_job(JobKind::ConfigWrite, request, t),
                None => self.settings.close(),
            },
            Key::Content(c) if c.get() == 'r' && !waiting => {
                self.start_settings_job(JobKind::ConfigReload, SettingsRequest::Reload, t);
            }
            Key::Arrow(_)
            | Key::Enter
            | Key::Content(_)
            | Key::Backspace
            | Key::Interrupt
            | Key::ToggleTextSize
            | Key::Tab
            | Key::BackTab
            | Key::Other => {}
        }
    }

    fn start_settings_job(
        &mut self,
        kind: JobKind,
        request: SettingsRequest,
        t: &mut crate::Transition,
    ) {
        if let Some(ticket) = self.jobs.start(kind) {
            self.settings.begin(ticket, request);
            t.job = Some(ticket);
        }
    }

    /// An admitted settings result: apply what takes effect now. Returns
    /// whether it was a successful write.
    pub(crate) fn settings_result(&mut self, kind: JobKind) -> bool {
        if !matches!(kind, JobKind::ConfigWrite | JobKind::ConfigReload) {
            return false;
        }
        let Some((_, outcome)) = self.jobs.consume(kind) else {
            return false;
        };
        let written =
            kind == JobKind::ConfigWrite && matches!(outcome, JobOutcome::Settings { .. });
        for name in self.settings.finish(kind, outcome) {
            if name == "display.text_size"
                && let Some(Setting::Text(size)) = self.settings.value(name)
            {
                self.text_size = match size.as_str() {
                    "large" => crate::view::TextSize::Large,
                    "normal" => crate::view::TextSize::Normal,
                    _ => crate::view::TextSize::Auto,
                };
            }
        }
        written
    }

    /// The expected next key when the guide applies (#82): numeric tracks
    /// only; `always`, never with `off`, and with `auto` for the learner's
    /// first [`KEY_GUIDE_SESSIONS`] numeric sessions.
    pub(crate) fn key_guide(&self, screen: &crate::view::ScreenView) -> Option<char> {
        use cadence_core::progress::Track;
        let numeric = |track: Track| matches!(track, Track::NumberRow | Track::Numpad);
        if !self.current_track().is_ok_and(numeric) {
            return None;
        }
        let on = match self.settings.value("display.key_guide") {
            Some(Setting::Text(mode)) if mode == "always" => true,
            Some(Setting::Text(mode)) if mode == "off" => false,
            Some(
                Setting::Text(_) | Setting::Integer(_) | Setting::Boolean(_) | Setting::List(_),
            )
            | None => {
                self.history
                    .recent
                    .iter()
                    .filter(|recap| {
                        recap
                            .tracks
                            .iter()
                            .any(|t| numeric(t.track) && t.first_attempted > 0)
                    })
                    .count()
                    < KEY_GUIDE_SESSIONS
            }
        };
        let crate::view::ScreenView::Practice { spans, .. } = screen else {
            return None;
        };
        on.then(|| {
            spans.iter().find_map(|span| match span {
                crate::view::UnitSpan::Cursor { expected } => Some(*expected),
                crate::view::UnitSpan::Correct { .. }
                | crate::view::UnitSpan::Wrong { .. }
                | crate::view::UnitSpan::Upcoming { .. }
                | crate::view::UnitSpan::Extra { .. } => None,
            })
        })
        .flatten()
    }

    pub(crate) fn display_view(&self) -> crate::view::DisplayView {
        let text = |name: &str, fallback: &str| match self.settings.value(name) {
            Some(Setting::Text(s)) => s.clone(),
            Some(Setting::Integer(_) | Setting::Boolean(_) | Setting::List(_)) | None => {
                fallback.to_string()
            }
        };
        let flag = |name: &str| matches!(self.settings.value(name), Some(Setting::Boolean(true)));
        crate::view::DisplayView {
            theme: text("display.theme", "dark"),
            theme_colors: self.settings.theme_file(),
            high_contrast: flag("display.high_contrast"),
            key_guide: text("display.key_guide", "auto"),
            live_pace: flag("practice.live_pace"),
        }
    }
}
