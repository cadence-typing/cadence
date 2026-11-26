//! #84: `cadence import` and `cadence packs` through the binary, on a
//! temporary data directory only.
use std::path::Path;
use std::process::{Command, Output};

fn cadence(home: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cadence"))
        .args(args)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .output()
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn packs(home: &Path) -> std::path::PathBuf {
    home.join("data/cadence/packs")
}

#[test]
fn fixtures_import_with_every_refusal_located() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let file = home.path().join("notes.txt");
    let long = "x".repeat(3_000);
    let content = format!(
        "plain words here\r\nred \x1b[31mtext\x1b[0m\r\ntitle \x1b]0;owned\x07\r\nabc\u{202E}def\r\n{long}\r\nlast line\r\n"
    );
    std::fs::write(&file, content)?;
    let path = file.to_str().ok_or("path")?;
    // A dry run reports and writes nothing.
    let dry = cadence(
        home.path(),
        &["import", path, "--name", "notes", "--dry-run"],
    )?;
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let report = text(&dry);
    assert!(
        report.contains("6 lines: 2 accepted (2 passages), 4 rejected"),
        "{report}"
    );
    assert!(
        report.contains("line 2, column 5: escape character"),
        "{report}"
    );
    assert!(
        report.contains("line 3, column 7: escape character"),
        "{report}"
    );
    assert!(
        report.contains("line 4, column 4: bidirectional control"),
        "{report}"
    );
    assert!(
        report.contains("line 5, column 2001: more than 2000 characters"),
        "{report}"
    );
    assert!(!packs(home.path()).exists(), "--dry-run creates no file");
    let real = cadence(home.path(), &["import", path, "--name", "notes"])?;
    assert!(real.status.success());
    assert!(text(&real).contains("pack notes: created"));
    assert_eq!(
        std::fs::read_to_string(packs(home.path()).join("notes/passages.txt"))?,
        "plain words here\nlast line\n"
    );
    assert_eq!(
        std::fs::read(packs(home.path()).join("notes/original"))?,
        std::fs::read(&file)?
    );
    let manifest = std::fs::read_to_string(packs(home.path()).join("notes/manifest.json"))?;
    assert!(
        manifest.contains("\"source\": \"notes.txt\"")
            && !manifest.contains(home.path().to_str().ok_or("home")?)
    );
    let listed = text(&cadence(home.path(), &["packs"])?);
    assert!(listed.starts_with("notes  2 passages"), "{listed}");
    Ok(())
}

#[test]
fn an_invalid_import_leaves_existing_packs_untouched() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let good = home.path().join("good.txt");
    std::fs::write(&good, "one line\n")?;
    assert!(
        cadence(
            home.path(),
            &["import", good.to_str().ok_or("path")?, "--name", "kept"]
        )?
        .status
        .success()
    );
    let before = std::fs::read(packs(home.path()).join("kept/passages.txt"))?;
    // Too large: refused before reading it all.
    let big = home.path().join("big.txt");
    std::fs::write(&big, vec![b'a'; 11 * 1024 * 1024])?;
    let refused = cadence(
        home.path(),
        &[
            "import",
            big.to_str().ok_or("path")?,
            "--name",
            "kept",
            "--replace",
        ],
    )?;
    assert!(!refused.status.success());
    // Nothing accepted: refused too.
    let junk = home.path().join("junk.txt");
    std::fs::write(&junk, "\x1b[2J\n")?;
    assert!(
        !cadence(
            home.path(),
            &[
                "import",
                junk.to_str().ok_or("path")?,
                "--name",
                "kept",
                "--replace"
            ]
        )?
        .status
        .success()
    );
    // Other content under the same name without --replace: refused.
    let other = home.path().join("other.txt");
    std::fs::write(&other, "something else\n")?;
    let clash = cadence(
        home.path(),
        &["import", other.to_str().ok_or("path")?, "--name", "kept"],
    )?;
    assert!(!clash.status.success());
    assert_eq!(
        std::fs::read(packs(home.path()).join("kept/passages.txt"))?,
        before
    );
    let entries = std::fs::read_dir(packs(home.path()))?.count();
    assert_eq!(entries, 1, "no staging directory left behind");
    Ok(())
}
