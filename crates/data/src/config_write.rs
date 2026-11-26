//! Acknowledged config writes and reloads (U-04, #79). A write applies the
//! edits to the file's current text with [`crate::config::set`] (comments
//! and formatting kept), validates the result, and replaces the file
//! atomically: a temporary file in the same directory, `fsync`, `rename`,
//! then `fsync` of the directory. It refuses when the file's digest is not
//! the one the edit was based on, so an external edit is never clobbered.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use cadence_core::settings::{ConfigDigest, Setting};

use crate::config::{Config, ConfigError, set};

/// The temporary file's name next to the config file.
const TEMPORARY_NAME: &str = ".config.toml.cadence-write";

/// A config file as read: its validated values and its digest (`None`
/// when there is no file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The effective values.
    pub config: Config,
    /// The digest of the bytes read.
    pub digest: Option<ConfigDigest>,
}

/// Why a write did not happen. The file is unchanged in every case.
#[derive(Debug)]
pub enum WriteError {
    /// The file changed since the edit began (or appeared or vanished).
    Conflict,
    /// The edited file would not be valid.
    Invalid(Vec<ConfigError>),
    /// An edit names an unknown key or an unaccepted value.
    Refused(String),
    /// Reading, writing or renaming failed.
    Io(io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => f.write_str(
                "the settings file changed on disk since you opened settings; nothing was written (Reload settings to see it)",
            ),
            Self::Invalid(errors) => match errors.first() {
                Some(first) => write!(f, "{first}"),
                None => f.write_str("the edited settings are not valid"),
            },
            Self::Refused(reason) => f.write_str(reason),
            Self::Io(error) => write!(f, "cannot write the settings file: {error}"),
        }
    }
}

impl std::error::Error for WriteError {}

fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Reads and validates the file with its digest.
///
/// # Errors
/// An unreadable or invalid file: the caller keeps its last valid settings.
pub fn reload(path: &Path) -> Result<Loaded, Vec<ConfigError>> {
    let unreadable = |error: io::Error| {
        vec![ConfigError {
            file: path.to_path_buf(),
            line: 0,
            key: String::new(),
            message: format!("cannot read: {error}"),
        }]
    };
    let Some(bytes) = read(path).map_err(unreadable)? else {
        return Ok(Loaded {
            config: Config::default(),
            digest: None,
        });
    };
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        vec![ConfigError {
            file: path.to_path_buf(),
            line: 0,
            key: String::new(),
            message: format!("not UTF-8: {error}"),
        }]
    })?;
    Ok(Loaded {
        config: Config::parse(&text, path)?,
        digest: Some(ConfigDigest::of(&bytes)),
    })
}

/// Applies `edits` to the file at `path`, which must still have digest
/// `based_on`, and replaces it atomically.
///
/// # Errors
/// [`WriteError`]; the file is unchanged.
pub fn write(
    path: &Path,
    based_on: Option<ConfigDigest>,
    edits: &[(&str, Setting)],
) -> Result<Loaded, WriteError> {
    let current = read(path).map_err(WriteError::Io)?;
    if current.as_deref().map(ConfigDigest::of) != based_on {
        return Err(WriteError::Conflict);
    }
    let mut text = match current {
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|error| WriteError::Refused(format!("not UTF-8: {error}")))?,
        None => String::new(),
    };
    for (key, value) in edits {
        text = set(&text, key, &value.to_string()).map_err(WriteError::Refused)?;
    }
    let config = Config::parse(&text, path).map_err(WriteError::Invalid)?;
    let directory = path
        .parent()
        .ok_or_else(|| WriteError::Refused("the settings path has no directory".into()))?;
    fs::create_dir_all(directory).map_err(WriteError::Io)?;
    let temporary = directory.join(TEMPORARY_NAME);
    replace(&temporary, path, text.as_bytes()).map_err(WriteError::Io)?;
    Ok(Loaded {
        config,
        digest: Some(ConfigDigest::of(text.as_bytes())),
    })
}

fn replace(temporary: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    // A leftover from an interrupted write is ours to replace.
    match fs::remove_file(temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    if let Some(directory) = path.parent() {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}
