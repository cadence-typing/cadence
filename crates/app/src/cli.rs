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
    },
    /// Machine-readable environment facts (#69).
    Doctor,
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
    };
    let mut theme = None;
    let mut action = None;
    let mut output = None;
    let mut script = None;
    let mut stdio = false;
    let mut ephemeral = false;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => set_action(&mut action, "version")?,
            "--help" | "-h" => set_action(&mut action, "help")?,
            "backup" => set_action(&mut action, "backup")?,
            "export" => set_action(&mut action, "export")?,
            "headless" => set_action(&mut action, "headless")?,
            "doctor" => set_action(&mut action, "doctor")?,
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
            Command::Headless {
                script: script.take(),
                ephemeral,
            }
        }
        Some("doctor") => {
            if !json {
                return Err("doctor requires --json".into());
            }
            Command::Doctor
        }
        Some(_) => return Err("unsupported command".into()),
    };
    if !matches!(options.command, Command::Headless { .. })
        && (stdio || ephemeral || script.is_some())
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
        cadence doctor --json [--profile-dir PATH]\n\n\
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
