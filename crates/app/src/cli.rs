//! Explicit profile and content selection for the feedback build.

use std::path::PathBuf;

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

/// Validated CLI settings. Unspecified settings retain the pinned policy.
pub struct Options {
    pub command: Command,
    pub profile: Option<PathBuf>,
    pub minutes: Option<u64>,
    pub track: Option<Phase>,
    pub seed: Option<u64>,
}

pub fn parse() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        command: Command::Practice,
        profile: None,
        minutes: None,
        track: None,
        seed: None,
    };
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
        && (options.minutes.is_some() || options.track.is_some() || options.seed.is_some())
    {
        return Err("--minutes, --track and --seed apply only to practice".into());
    }
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
        cadence [--profile-dir /absolute/private/profile] [--seed INTEGER]\n\
        cadence backup --output /absolute/new.sqlite3 [--profile-dir PATH]\n\
        cadence export --output /absolute/new.json [--profile-dir PATH]\n\n\
        Enter starts/continues. Esc pauses/resumes. Ctrl-C saves and quits.\n\
        Numpad entries require Enter; physical keypad origin is unverified.\n\
        Progress stays local. This preview is not formally qualified or signed."
    );
}
