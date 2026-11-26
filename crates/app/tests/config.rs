//! #76: `cadence config check` through the binary, on a temporary config
//! directory (never the owner's).
use std::process::Command;

fn check(
    dir: &std::path::Path,
    file: Option<&std::path::Path>,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadence"));
    command.args(["config", "check"]);
    if let Some(file) = file {
        command.arg(file);
    }
    command
        .env("XDG_CONFIG_HOME", dir)
        .env("HOME", dir)
        .output()
}

#[test]
fn config_check_prints_effective_values_with_their_source() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let output = check(dir.path(), None)?;
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("session.minutes = 15  # default"), "{text}");
    std::fs::create_dir_all(dir.path().join("cadence"))?;
    std::fs::write(
        dir.path().join("cadence/config.toml"),
        "[display]\ntheme = \"light\"\n",
    )?;
    let text = String::from_utf8(check(dir.path(), None)?.stdout)?;
    assert!(text.contains("display.theme = \"light\"  # file"), "{text}");
    Ok(())
}

#[test]
fn config_check_fails_with_located_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("other.toml");
    std::fs::write(&file, "[session]\nminutes = 7\n")?;
    let output = check(dir.path(), Some(&file))?;
    assert!(!output.status.success());
    let text = String::from_utf8(output.stdout)?;
    assert!(
        text.contains(":2: session.minutes: expected one of 5, 10, 15, 20, 30"),
        "{text}"
    );
    Ok(())
}
