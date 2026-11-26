//! #79: acknowledged config writes refuse conflicts and keep comments;
//! reload of an invalid file reports, leaving the caller's settings.
use cadence_core::settings::{ConfigDigest, Setting};

use crate::config_write::{WriteError, reload, write};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn reloaded(path: &std::path::Path) -> Result<crate::config_write::Loaded, String> {
    reload(path).map_err(|errors| format!("{errors:?}"))
}

#[test]
fn a_write_keeps_comments_and_returns_the_new_digest() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cadence/config.toml");
    // No file yet: the edit is based on "no file".
    let first = write(&path, None, &[("session.minutes", Setting::Integer(20))])?;
    assert_eq!(first.config.integer("session.minutes"), Some(20));
    std::fs::write(&path, "# mine\n[session]\nminutes = 20 # long\n")?;
    let loaded = reloaded(&path)?;
    let next = write(
        &path,
        loaded.digest,
        &[("display.theme", Setting::Text("light".into()))],
    )?;
    let text = std::fs::read_to_string(&path)?;
    assert!(
        text.starts_with("# mine\n[session]\nminutes = 20 # long\n"),
        "{text}"
    );
    assert_eq!(next.digest, Some(ConfigDigest::of(text.as_bytes())));
    assert!(
        !dir.path()
            .join("cadence/.config.toml.cadence-write")
            .exists()
    );
    Ok(())
}

#[test]
fn an_external_edit_is_never_clobbered() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[session]\nminutes = 15\n")?;
    let based_on = reloaded(&path)?.digest;
    std::fs::write(&path, "[session]\nminutes = 30\n")?;
    let result = write(
        &path,
        based_on,
        &[("session.minutes", Setting::Integer(10))],
    );
    assert!(matches!(result, Err(WriteError::Conflict)));
    assert_eq!(std::fs::read_to_string(&path)?, "[session]\nminutes = 30\n");
    // A file that appeared after an edit based on "no file" is a conflict too.
    let fresh = dir.path().join("other.toml");
    std::fs::write(&fresh, "")?;
    assert!(matches!(
        write(&fresh, None, &[("session.minutes", Setting::Integer(10))]),
        Err(WriteError::Conflict)
    ));
    Ok(())
}

#[test]
fn an_invalid_edit_or_file_changes_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[session]\nminutes = 15\n")?;
    let based_on = reloaded(&path)?.digest;
    assert!(matches!(
        write(&path, based_on, &[("session.minutes", Setting::Integer(7))]),
        Err(WriteError::Refused(_))
    ));
    assert_eq!(std::fs::read_to_string(&path)?, "[session]\nminutes = 15\n");
    std::fs::write(&path, "[session]\nminutes = 7\n")?;
    let errors = reload(&path).err().unwrap_or_default();
    assert_eq!(errors.first().map(|e| e.line), Some(2));
    Ok(())
}
