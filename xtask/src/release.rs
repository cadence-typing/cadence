//! `cargo xtask release-check --tag TAG` (#3): the rule a release tag must
//! satisfy, kept in the repository rather than in workflow YAML. The tag is
//! `v` + the workspace version, `CHANGELOG.md` has that version's section,
//! and the tagged commit is on `main`. Prints the section as release notes.

use std::ffi::OsStr;
use std::fs;

use crate::dist::{command, root};

/// Checks a tag against the workspace and prints its release notes.
pub fn check(args: &[String]) -> Result<(), String> {
    let tag = match args {
        [flag, tag] if flag == "--tag" => tag.as_str(),
        _ => return Err("usage: cargo xtask release-check --tag vVERSION".into()),
    };
    let root = root()?;
    let manifest =
        fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let changelog =
        fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| format!("CHANGELOG.md: {e}"))?;
    let version = workspace_version(&manifest)?;
    let notes = release_notes(tag, &version, &changelog)?;
    let on_main = command(
        "git",
        &[
            OsStr::new("merge-base"),
            OsStr::new("--is-ancestor"),
            OsStr::new("HEAD"),
            OsStr::new("origin/main"),
        ],
        &root,
    );
    if on_main.is_err() {
        return Err(format!("{tag}: the tagged commit is not on origin/main"));
    }
    println!("{notes}");
    Ok(())
}

/// The `[workspace.package] version`.
pub(crate) fn workspace_version(manifest: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
            continue;
        }
        if in_package
            && let Some(rest) = line.strip_prefix("version")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_string());
        }
    }
    Err("Cargo.toml has no [workspace.package] version".into())
}

/// The changelog section for `version`, when `tag` is `v` + `version`.
pub(crate) fn release_notes(tag: &str, version: &str, changelog: &str) -> Result<String, String> {
    if tag.strip_prefix('v') != Some(version) {
        return Err(format!(
            "tag {tag} does not match the workspace version {version} (expected v{version})"
        ));
    }
    let heading = format!("## [{version}]");
    let mut section = Vec::new();
    let mut inside = false;
    for line in changelog.lines() {
        if line.starts_with("## [") {
            if inside {
                break;
            }
            inside = line.starts_with(&heading);
        }
        if inside {
            section.push(line);
        }
    }
    if section.is_empty() {
        return Err(format!("CHANGELOG.md has no section {heading}"));
    }
    Ok(section.join("\n").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[workspace]\nmembers = []\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.1.0-alpha.6\"\n\n[workspace.dependencies]\nversion = \"9\"\n";
    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0-alpha.6] — Proved decisions (2026-11-27)\n\n- One.\n- Two.\n\n## [0.1.0-alpha.5] — Earlier\n\n- Old.\n";

    #[test]
    fn version_comes_from_the_workspace_package() {
        assert_eq!(workspace_version(MANIFEST).as_deref(), Ok("0.1.0-alpha.6"));
        assert!(workspace_version("[package]\nversion = \"1\"\n").is_err());
    }

    #[test]
    fn a_matching_tag_prints_its_section() {
        let notes = release_notes("v0.1.0-alpha.6", "0.1.0-alpha.6", CHANGELOG).unwrap();
        assert!(notes.starts_with("## [0.1.0-alpha.6]"));
        assert!(notes.contains("- Two."));
        assert!(!notes.contains("Old."));
    }

    #[test]
    fn a_tag_that_does_not_match_the_version_is_refused() {
        let error = release_notes("v0.1.0-alpha.7", "0.1.0-alpha.6", CHANGELOG).unwrap_err();
        assert!(error.contains("does not match"));
        assert!(release_notes("0.1.0-alpha.6", "0.1.0-alpha.6", CHANGELOG).is_err());
    }

    #[test]
    fn a_missing_changelog_section_is_refused() {
        let error = release_notes("v0.1.0-alpha.8", "0.1.0-alpha.8", CHANGELOG).unwrap_err();
        assert!(error.contains("no section"));
    }
}
