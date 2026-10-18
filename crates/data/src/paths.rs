//! XDG data paths. Relative/empty XDG values are ignored as the specification requires.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Failure to resolve a durable profile location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathsError {
    /// Neither an absolute HOME nor a usable override/XDG directory was supplied.
    MissingHome,
}

impl std::fmt::Display for PathsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an absolute HOME or explicit profile directory is required")
    }
}
impl std::error::Error for PathsError {}

/// Resolved durable profile directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePaths {
    /// Directory holding the database and lifetime writer lock.
    pub data: PathBuf,
}

/// Resolves the profile from the environment without mutating it.
///
/// # Errors
/// Returns [`PathsError::MissingHome`] when no usable location exists.
pub fn data_dir(override_dir: Option<&Path>) -> Result<ProfilePaths, PathsError> {
    resolve_data_dir(
        override_dir,
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// Pure, injectable XDG resolver; explicit profile paths take precedence.
///
/// # Errors
/// Returns [`PathsError::MissingHome`] if both XDG and HOME are unusable.
pub fn resolve_data_dir(
    override_dir: Option<&Path>,
    xdg_data_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<ProfilePaths, PathsError> {
    let data = if let Some(path) = override_dir {
        path.to_path_buf()
    } else if let Some(path) = xdg_data_home
        .map(Path::new)
        .filter(|path| path.is_absolute())
    {
        path.join("cadence")
    } else {
        home.map(Path::new)
            .filter(|path| path.is_absolute())
            .ok_or(PathsError::MissingHome)?
            .join(".local/share/cadence")
    };
    Ok(ProfilePaths { data })
}

/// SQLite database filename.
pub const DATABASE_FILE: &str = "progress.sqlite3";
/// Adjacent advisory writer lock filename.
pub const LOCK_FILE: &str = "progress.sqlite3.writer-lock";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_resolution_obeys_xdg_without_global_mutation() -> Result<(), PathsError> {
        let home = Some(OsStr::new("/fresh/home"));
        for xdg in [None, Some(OsStr::new("relative")), Some(OsStr::new(""))] {
            assert_eq!(
                resolve_data_dir(None, xdg, home)?.data,
                Path::new("/fresh/home/.local/share/cadence")
            );
        }
        assert_eq!(
            resolve_data_dir(None, Some(OsStr::new("/xdg")), None)?.data,
            Path::new("/xdg/cadence")
        );
        assert_eq!(
            resolve_data_dir(Some(Path::new("explicit")), None, None)?.data,
            Path::new("explicit")
        );
        assert_eq!(
            resolve_data_dir(None, None, Some(OsStr::new("relative"))),
            Err(PathsError::MissingHome)
        );
        Ok(())
    }
}
