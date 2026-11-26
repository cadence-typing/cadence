//! Imported text packs on disk (D-03, #84):
//! `$XDG_DATA_HOME/cadence/packs/<name>/` holds `manifest.json` (name,
//! digest, normaliser version, counts and the source file's name only),
//! `passages.txt` (the accepted passages, one per line) and `original`
//! (the bytes as given). A pack is written into a temporary directory,
//! each file synced, then renamed into place; replacing renames the old
//! pack aside first, so a reader never sees half a pack. The same name with
//! different content is refused unless replacing. Nothing is executed.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The manifest file.
const MANIFEST: &str = "manifest.json";
/// The accepted passages.
const PASSAGES: &str = "passages.txt";
/// The bytes as given.
const ORIGINAL: &str = "original";

/// A pack's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    /// The pack's name (`[a-z0-9-]`, 1 to 64).
    pub name: String,
    /// SHA-256 of `passages.txt`, hex.
    pub digest: String,
    /// The normaliser version that produced it.
    pub normalizer_version: u32,
    /// Lines read.
    pub lines: u64,
    /// Passages accepted.
    pub passages: u64,
    /// Training units accepted.
    pub units: u64,
    /// Lines rejected.
    pub rejected: u64,
    /// The source file's name, without its directory (`-` for stdin).
    pub source: String,
}

/// What a write did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// A new pack.
    Created,
    /// An existing pack replaced.
    Replaced,
    /// The same content already existed under the name.
    Unchanged,
}

/// Why a pack was not written. Existing packs are untouched in every case.
#[derive(Debug)]
pub enum PackError {
    /// The name is not `[a-z0-9-]`, 1 to 64.
    InvalidName(String),
    /// A pack of that name with other content exists.
    Exists(String),
    /// Reading or writing failed.
    Io(io::Error),
    /// A manifest could not be read.
    Manifest(String),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "pack name {name:?}: use a-z, 0-9 and -, 1 to 64"),
            Self::Exists(name) => write!(
                f,
                "a pack named {name} with other content exists; --replace replaces it"
            ),
            Self::Io(error) => write!(f, "cannot write the pack: {error}"),
            Self::Manifest(reason) => write!(f, "unreadable pack manifest: {reason}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<io::Error> for PackError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Whether `name` is a valid pack name.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The packs directory under a profile's data directory.
#[must_use]
pub fn directory(data: &Path) -> PathBuf {
    data.join("packs")
}

fn synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn read_manifest(pack: &Path) -> Result<PackManifest, PackError> {
    let text = fs::read_to_string(pack.join(MANIFEST))?;
    serde_json::from_str(&text).map_err(|error| PackError::Manifest(error.to_string()))
}

/// Writes a pack atomically.
///
/// # Errors
/// [`PackError`]; existing packs are untouched.
pub fn write(
    packs: &Path,
    manifest: &PackManifest,
    passages: &str,
    original: &[u8],
    replace: bool,
) -> Result<Written, PackError> {
    if !valid_name(&manifest.name) {
        return Err(PackError::InvalidName(manifest.name.clone()));
    }
    fs::create_dir_all(packs)?;
    let target = packs.join(&manifest.name);
    let existing = match read_manifest(&target) {
        Ok(existing) => Some(existing),
        Err(PackError::Io(error)) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(existing) = &existing {
        if existing.digest == manifest.digest {
            return Ok(Written::Unchanged);
        }
        if !replace {
            return Err(PackError::Exists(manifest.name.clone()));
        }
    }
    let staging = packs.join(format!(".staging-{}", manifest.name));
    let aside = packs.join(format!(".replaced-{}", manifest.name));
    for leftover in [&staging, &aside] {
        match fs::remove_dir_all(leftover) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::create_dir(&staging)?;
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| PackError::Manifest(error.to_string()))?;
    synced(&staging.join(MANIFEST), &json)?;
    synced(&staging.join(PASSAGES), passages.as_bytes())?;
    synced(&staging.join(ORIGINAL), original)?;
    File::open(&staging)?.sync_all()?;
    let written = if existing.is_some() {
        fs::rename(&target, &aside)?;
        fs::rename(&staging, &target)?;
        fs::remove_dir_all(&aside)?;
        Written::Replaced
    } else {
        fs::rename(&staging, &target)?;
        Written::Created
    };
    File::open(packs)?.sync_all()?;
    Ok(written)
}

/// Every pack's manifest, by name.
///
/// # Errors
/// An unreadable directory or manifest.
pub fn list(packs: &Path) -> Result<Vec<PackManifest>, PackError> {
    let entries = match fs::read_dir(packs) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_str().is_some_and(valid_name) && entry.file_type()?.is_dir() {
            manifests.push(read_manifest(&entry.path())?);
        }
    }
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, digest: &str) -> PackManifest {
        PackManifest {
            name: name.into(),
            digest: digest.into(),
            normalizer_version: 1,
            lines: 2,
            passages: 2,
            units: 8,
            rejected: 0,
            source: "notes.txt".into(),
        }
    }

    #[test]
    fn packs_are_written_whole_and_never_silently_replaced()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let packs = directory(dir.path());
        assert_eq!(
            write(
                &packs,
                &manifest("notes", "aa"),
                "one\ntwo\n",
                b"one\ntwo\n",
                false
            )?,
            Written::Created
        );
        assert_eq!(
            write(&packs, &manifest("notes", "aa"), "one\ntwo\n", b"x", false)?,
            Written::Unchanged
        );
        assert!(matches!(
            write(
                &packs,
                &manifest("notes", "bb"),
                "three\n",
                b"three\n",
                false
            ),
            Err(PackError::Exists(_))
        ));
        assert_eq!(
            fs::read_to_string(packs.join("notes/passages.txt"))?,
            "one\ntwo\n"
        );
        assert_eq!(
            write(
                &packs,
                &manifest("notes", "bb"),
                "three\n",
                b"three\n",
                true
            )?,
            Written::Replaced
        );
        assert_eq!(
            fs::read_to_string(packs.join("notes/passages.txt"))?,
            "three\n"
        );
        assert!(matches!(
            write(&packs, &manifest("Bad Name", "cc"), "x\n", b"x", false),
            Err(PackError::InvalidName(_))
        ));
        let names: Vec<String> = list(&packs)?.into_iter().map(|m| m.name).collect();
        assert_eq!(names, ["notes"]);
        // No staging or replaced directory is left behind.
        assert_eq!(fs::read_dir(&packs)?.count(), 1);
        Ok(())
    }
}
