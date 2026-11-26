//! Explicit profile and content selection for the feedback build.

use std::path::PathBuf;

use cadence_engine::view::TextSize;
use cadence_trainer::Phase;

/// Command selected after validating the complete argument vector.
pub enum Command {
    /// Interactive practice.
    Practice,
    /// Version only; does not open a profile.
    Version,
    /// Usage only; does not open a profile.
    Help,
    /// SQLite online backup to a new output file.
    Backup(PathBuf),
    /// Portable progress export to a new output file.
    Export(PathBuf),
    /// The headless NDJSON driver (#69): requests from stdin or a script.
    Headless {
        /// The request script; `None` reads stdin.
        script: Option<PathBuf>,
        /// Use a fresh temporary profile, removed at exit.
        ephemeral: bool,
        /// Write a trace here (#72).
        record: Option<PathBuf>,
        /// Keep target text in the trace (D-02 opt-in).
        record_content: bool,
    },
    /// Replay a trace on a fresh temporary profile (#72).
    Replay {
        /// The trace.
        trace: PathBuf,
        /// Where to write the replayed captures.
        capture_dir: Option<PathBuf>,
    },
    /// Machine-readable environment facts (#69).
    Doctor,
    /// Validate a config file and print effective values (#76).
    ConfigCheck(Option<PathBuf>),
    /// Import local text as a pack (#84).
    Import(ImportRequest),
    /// List imported packs (#84).
    Packs,
}

/// `cadence import FILE|- --name NAME [--dry-run] [--normalize-typography]
/// [--replace]` (#84).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRequest {
    /// The file, or `None` for stdin (`-`).
    pub file: Option<PathBuf>,
    /// The pack name.
    pub name: String,
    /// Report only; write nothing.
    pub dry_run: bool,
    /// Map smart quotes, dashes and non-breaking spaces.
    pub normalize_typography: bool,
    /// Replace a pack of the same name with other content.
    pub replace: bool,
}

/// Display palette (U-02); never affects targets or scoring.
#[derive(Clone, Copy, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Terminal,
}

impl Theme {
    /// The `display.theme` name.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Terminal => "terminal",
        }
    }

    pub const fn palette(self) -> cadence_tui::Palette {
        match self {
            Self::Dark => cadence_tui::Palette::cadence_dark(),
            Self::Light => cadence_tui::Palette::cadence_light(),
            Self::Terminal => cadence_tui::Palette::terminal(),
        }
    }
}

/// Validated CLI settings. Unspecified settings retain the pinned policy.
pub struct Options {
    pub command: Command,
    pub profile: Option<PathBuf>,
    pub minutes: Option<u64>,
    pub track: Option<Phase>,
    pub seed: Option<u64>,
    pub text_size: Option<TextSize>,
    pub theme: Theme,
    /// `--utc-offset +HH:MM`: overrides the detected local offset (seconds).
    pub utc_offset: Option<i32>,
    /// `--theme` was given (it then overrides the config file).
    pub theme_set: bool,
    /// The keyboard layout (config `keyboard.layout`, #77).
    pub layout: cadence_core::Layout,
    /// A numpad is present (config `keyboard.numpad`, #78).
    pub numpad: bool,
    /// Numpad operators and decimal separator (config, #83).
    pub numeric: cadence_trainer::numeric::NumericOptions,
    /// `--pack NAME`: a pack session (#85).
    pub pack: Option<String>,
}

impl Options {
    /// Practice with every setting left to the pinned policy.
    pub fn defaults() -> Self {
        Self {
            command: Command::Practice,
            profile: None,
            minutes: None,
            track: None,
            seed: None,
            text_size: None,
            theme: Theme::Dark,
            utc_offset: None,
            theme_set: false,
            layout: cadence_core::Layout::Us,
            numpad: true,
            numeric: cadence_trainer::numeric::NumericOptions::default(),
            pack: None,
        }
    }

    /// The practice settings of these options.
    pub fn clone_for_practice(&self) -> Self {
        Self {
            command: Command::Practice,
            profile: self.profile.clone(),
            minutes: self.minutes,
            track: self.track,
            seed: self.seed,
            text_size: self.text_size,
            theme: self.theme,
            utc_offset: self.utc_offset,
            theme_set: self.theme_set,
            layout: self.layout,
            numpad: self.numpad,
            numeric: self.numeric,
            pack: self.pack.clone(),
        }
    }
}

pub fn parse() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        command: Command::Practice,
        profile: None,
        minutes: None,
        track: None,
        seed: None,
        text_size: None,
        theme: Theme::Dark,
        utc_offset: None,
        theme_set: false,
        layout: cadence_core::Layout::Us,
        numpad: true,
        numeric: cadence_trainer::numeric::NumericOptions::default(),
        pack: None,
    };
    let mut theme = None;
    let mut action = None;
    let mut output = None;
    let mut script = None;
    let mut stdio = false;
    let mut ephemeral = false;
    let mut json = false;
    let mut record = None;
    let mut record_content = false;
    let mut read_only = false;
    let mut capture_dir = None;
    let mut trace = None;
    let mut config_file = None;
    let mut import_source: Option<Option<PathBuf>> = None;
    let mut pack_name = None;
    let mut dry_run = false;
    let mut normalize_typography = false;
    let mut replace = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => set_action(&mut action, "version")?,
            "--help" | "-h" => set_action(&mut action, "help")?,
            "backup" => set_action(&mut action, "backup")?,
            "export" => set_action(&mut action, "export")?,
            "headless" => set_action(&mut action, "headless")?,
            "doctor" => set_action(&mut action, "doctor")?,
            "packs" => set_action(&mut action, "packs")?,
            "import" => {
                set_action(&mut action, "import")?;
                let source = value(&mut args, &arg)?;
                import_source = Some(if source == "-" {
                    None
                } else {
                    Some(absolute_path(source)?)
                });
            }
            "--name" => {
                if pack_name.is_some() {
                    return Err("duplicate --name".into());
                }
                pack_name = Some(value(&mut args, &arg)?);
            }
            "--dry-run" => dry_run = true,
            "--normalize-typography" => normalize_typography = true,
            "--replace" => replace = true,
            "config" => {
                set_action(&mut action, "config")?;
                if value(&mut args, &arg)? != "check" {
                    return Err("usage: cadence config check [/absolute/config.toml]".into());
                }
            }
            other
                if action == Some("config") && config_file.is_none() && other.starts_with('/') =>
            {
                config_file = Some(absolute_path(other.to_string())?);
            }
            "replay" => {
                set_action(&mut action, "replay")?;
                trace = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--read-only" => read_only = true,
            "--record-content" => record_content = true,
            "--record" => {
                if record.is_some() {
                    return Err("duplicate --record".into());
                }
                record = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--capture-dir" => {
                if capture_dir.is_some() {
                    return Err("duplicate --capture-dir".into());
                }
                capture_dir = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--stdio" => stdio = true,
            "--ephemeral-profile" => ephemeral = true,
            "--json" => json = true,
            "--script" => {
                if script.is_some() {
                    return Err("duplicate --script".into());
                }
                script = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--profile-dir" => {
                if options.profile.is_some() {
                    return Err("duplicate --profile-dir".into());
                }
                options.profile = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--output" => {
                if output.is_some() {
                    return Err("duplicate --output".into());
                }
                output = Some(absolute_path(value(&mut args, &arg)?)?);
            }
            "--minutes" => {
                if options.minutes.is_some() {
                    return Err("duplicate --minutes".into());
                }
                let minutes = value(&mut args, &arg)?
                    .parse()
                    .map_err(|_malformed| "--minutes requires 5, 10, 15, 20 or 30")?;
                if !matches!(minutes, 5 | 10 | 15 | 20 | 30) {
                    return Err("--minutes requires 5, 10, 15, 20 or 30".into());
                }
                options.minutes = Some(minutes);
            }
            "--pack" => {
                if options.pack.is_some() {
                    return Err("duplicate --pack".into());
                }
                options.pack = Some(value(&mut args, &arg)?);
            }
            "--track" => {
                if options.track.is_some() {
                    return Err("duplicate --track".into());
                }
                options.track = Some(match value(&mut args, &arg)?.as_str() {
                    "words" => Phase::AdaptiveWords,
                    "number-row" => Phase::NumberRow,
                    "numpad" => Phase::Numpad,
                    "prose" => Phase::Prose,
                    _ => return Err("--track requires words, number-row, numpad or prose".into()),
                });
            }
            "--seed" => {
                if options.seed.is_some() {
                    return Err("duplicate --seed".into());
                }
                options.seed = Some(
                    value(&mut args, &arg)?
                        .parse()
                        .map_err(|_malformed| "--seed requires an unsigned 64-bit integer")?,
                );
            }
            "--text-size" => {
                if options.text_size.is_some() {
                    return Err("duplicate --text-size".into());
                }
                options.text_size = Some(match value(&mut args, &arg)?.as_str() {
                    "auto" => TextSize::Auto,
                    "large" => TextSize::Large,
                    "normal" => TextSize::Normal,
                    _ => return Err("--text-size requires auto, large or normal".into()),
                });
            }
            "--theme" => {
                if theme.is_some() {
                    return Err("duplicate --theme".into());
                }
                theme = Some(match value(&mut args, &arg)?.as_str() {
                    "dark" => Theme::Dark,
                    "light" => Theme::Light,
                    "terminal" => Theme::Terminal,
                    _ => return Err("--theme requires dark, light or terminal".into()),
                });
            }
            "--utc-offset" => {
                if options.utc_offset.is_some() {
                    return Err("duplicate --utc-offset".into());
                }
                options.utc_offset = Some(parse_offset(&value(&mut args, &arg)?)?);
            }
            _ => return Err(format!("unknown argument {arg:?}; use --help")),
        }
    }
    options.command = match action {
        None => Command::Practice,
        Some("version") => Command::Version,
        Some("help") => Command::Help,
        Some("backup") => Command::Backup(
            output
                .take()
                .ok_or("backup requires --output /absolute/new.sqlite3")?,
        ),
        Some("export") => Command::Export(
            output
                .take()
                .ok_or("export requires --output /absolute/new.json")?,
        ),
        Some("headless") => {
            if stdio == script.is_some() {
                return Err("headless requires exactly one of --stdio or --script FILE".into());
            }
            if ephemeral == options.profile.is_some() {
                return Err(
                    "headless requires exactly one of --profile-dir PATH or --ephemeral-profile"
                        .into(),
                );
            }
            if record_content && record.is_none() {
                return Err("--record-content requires --record FILE".into());
            }
            Command::Headless {
                script: script.take(),
                ephemeral,
                record: record.take(),
                record_content,
            }
        }
        Some("replay") => {
            if !read_only {
                return Err("replay requires --read-only".into());
            }
            if options.profile.is_some() {
                return Err(
                    "replay uses a fresh temporary profile; --profile-dir is not accepted".into(),
                );
            }
            Command::Replay {
                trace: trace.take().ok_or("replay requires a trace file")?,
                capture_dir: capture_dir.take(),
            }
        }
        Some("config") => Command::ConfigCheck(config_file.take()),
        Some("packs") => Command::Packs,
        Some("import") => Command::Import(ImportRequest {
            file: import_source
                .take()
                .ok_or("usage: cadence import FILE|- --name NAME")?,
            name: pack_name.take().ok_or("import requires --name NAME")?,
            dry_run,
            normalize_typography,
            replace,
        }),
        Some("doctor") => {
            if !json {
                return Err("doctor requires --json".into());
            }
            Command::Doctor
        }
        Some(_) => return Err("unsupported command".into()),
    };
    if !matches!(options.command, Command::Headless { .. })
        && (stdio || ephemeral || script.is_some() || record.is_some() || record_content)
    {
        return Err("--stdio, --script and --ephemeral-profile are only valid for headless".into());
    }
    if json && !matches!(options.command, Command::Doctor) {
        return Err("--json is only valid for doctor".into());
    }
    if output.is_some() {
        return Err("--output is only valid for backup/export".into());
    }
    if !matches!(options.command, Command::Practice)
        && (options.minutes.is_some()
            || options.track.is_some()
            || options.seed.is_some()
            || options.text_size.is_some()
            || options.utc_offset.is_some()
            || theme.is_some())
    {
        return Err(
            "--minutes, --track, --seed, --text-size, --theme and --utc-offset apply only to practice"
                .into(),
        );
    }
    options.theme_set = theme.is_some();
    options.theme = theme.unwrap_or_default();
    Ok(options)
}

fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn absolute_path(value: String) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("path must be absolute: {}", path.display()));
    }
    Ok(path)
}

fn set_action(action: &mut Option<&'static str>, value: &'static str) -> Result<(), String> {
    if action.replace(value).is_some() {
        return Err("select only one command".into());
    }
    Ok(())
}

pub fn help() {
    println!(
        "Cadence — offline typing practice (development preview)\n\n\
        cadence [--minutes 5|10|15|20|30] [--track words|number-row|numpad|prose]\n\
        cadence [--profile-dir /absolute/private/profile] [--seed INTEGER] [--text-size auto|large|normal]\n\
        cadence [--theme dark|light|terminal] [--utc-offset +HH:MM]\n\
        cadence backup --output /absolute/new.sqlite3 [--profile-dir PATH]\n\
        cadence export --output /absolute/new.json [--profile-dir PATH]\n\
        cadence headless --stdio|--script /absolute/requests.jsonl --profile-dir PATH|--ephemeral-profile\n\
        cadence doctor --json [--profile-dir PATH]\n\
        cadence config check [/absolute/config.toml]\n\
        cadence import /absolute/file|- --name NAME [--dry-run] [--normalize-typography] [--replace]\n\
        cadence packs\n\
        cadence replay /absolute/trace.jsonl --read-only [--capture-dir /absolute/dir]\n\n\
        Enter starts/continues. Esc pauses; while paused, e ends the session and shows the review.\n\
        Ctrl-C during practice also ends it and shows the review; Enter there saves and exits.\n\
        F2 cycles text size. Losing focus or pasting pauses practice; nothing is inserted.\n\
        Numpad entries require Enter; physical keypad origin is unverified.\n\
        Progress stays local. This preview is not formally qualified or signed."
    );
}

/// `+HH:MM` or `-HH:MM` (at most 18 hours) as seconds east of UTC.
fn parse_offset(text: &str) -> Result<i32, String> {
    let error = || "--utc-offset requires +HH:MM or -HH:MM, at most 18:00".to_string();
    let (sign, rest) = match text.split_at_checked(1) {
        Some(("+", rest)) => (1, rest),
        Some(("-", rest)) => (-1, rest),
        _ => return Err(error()),
    };
    let (hours, minutes) = rest.split_once(':').ok_or_else(error)?;
    if hours.len() != 2 || minutes.len() != 2 {
        return Err(error());
    }
    let hours: i32 = hours.parse().map_err(|_malformed| error())?;
    let minutes: i32 = minutes.parse().map_err(|_malformed| error())?;
    if minutes >= 60
        || hours
            .checked_mul(60)
            .and_then(|h| h.checked_add(minutes))
            .is_none_or(|t| t > 18 * 60)
    {
        return Err(error());
    }
    hours
        .checked_mul(3600)
        .and_then(|h| h.checked_add(minutes.checked_mul(60)?))
        .and_then(|seconds| seconds.checked_mul(sign))
        .ok_or_else(error)
}

#[cfg(test)]
mod tests {
    use super::parse_offset;

    #[test]
    fn offsets_parse_with_sign_and_bound() {
        assert_eq!(parse_offset("+10:00"), Ok(36_000));
        assert_eq!(parse_offset("-08:00"), Ok(-28_800));
        assert_eq!(parse_offset("+05:30"), Ok(19_800));
        assert_eq!(parse_offset("+18:00"), Ok(64_800));
        for bad in ["10:00", "+18:01", "+1:00", "+10:60", "+aa:00", ""] {
            assert!(parse_offset(bad).is_err(), "{bad}");
        }
    }
}
