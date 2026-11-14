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
    };
    let mut theme = None;
    let mut action = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => set_action(&mut action, "version")?,
            "--help" | "-h" => set_action(&mut action, "help")?,
            "backup" => set_action(&mut action, "backup")?,
            "export" => set_action(&mut action, "export")?,
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
                    .map_err(|_| "--minutes requires 5, 10, 15, 20 or 30")?;
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
                        .map_err(|_| "--seed requires an unsigned 64-bit integer")?,
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
        Some(_) => return Err("unsupported command".into()),
    };
    if output.is_some() {
        return Err("--output is only valid for backup/export".into());
    }
    if !matches!(options.command, Command::Practice)
        && (options.minutes.is_some()
            || options.track.is_some()
            || options.seed.is_some()
            || options.text_size.is_some()
            || theme.is_some())
    {
        return Err(
            "--minutes, --track, --seed, --text-size and --theme apply only to practice".into(),
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
        cadence [--theme dark|light|terminal]\n\
        cadence backup --output /absolute/new.sqlite3 [--profile-dir PATH]\n\
        cadence export --output /absolute/new.json [--profile-dir PATH]\n\n\
        Enter starts/continues. Esc pauses; while paused, e ends the session and shows the review.\n\
        Ctrl-C during practice also ends it and shows the review; Enter there saves and exits.\n\
        F2 cycles text size. Losing focus or pasting pauses practice; nothing is inserted.\n\
        Numpad entries require Enter; physical keypad origin is unverified.\n\
        Progress stays local. This preview is not formally qualified or signed."
    );
}
