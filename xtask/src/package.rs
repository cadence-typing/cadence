//! The `package-install` gate (B-06 "Fresh-home consumer journey", #94),
//! run in the unprivileged, network-less gate container with only the
//! candidate's bundle (`dist/`): verify the digests, install offline with
//! `install.sh --from`, launch, type a short headless drill, quit, reopen
//! the saved progress, check the example config, back up and export; then
//! reject a tampered tarball, a tampered binary inside a valid tarball and
//! a wrong-architecture bundle without moving the active link; leave the
//! working version in place when an install is interrupted; and refuse a
//! database written by a newer schema with the backup guidance. Signature
//! verification is reported as not available (#13), never as passed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = "x86_64-unknown-linux-musl";
const SIGNATURE: &str = "signature: not available: unsigned development bundle (#13)";

struct Home {
    root: PathBuf,
}

impl Home {
    fn bin(&self) -> PathBuf {
        self.root.join(".local/bin/cadence")
    }
    fn data(&self) -> PathBuf {
        self.root.join(".local/share")
    }
    fn config(&self) -> PathBuf {
        self.root.join(".config")
    }
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("HOME", &self.root)
            .env("XDG_DATA_HOME", self.data())
            .env("XDG_CONFIG_HOME", self.config())
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("TMPDIR", self.root.join("tmp"));
        command
    }
    fn link(&self) -> Option<PathBuf> {
        fs::read_link(self.bin()).ok()
    }
}

fn run(mut command: Command, what: &str) -> Result<Output, String> {
    command.output().map_err(|e| format!("{what}: {e}"))
}

fn ok(output: &Output, what: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{what} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn install(root: &Path, home: &Home, from: &Path, fault: Option<&str>) -> Result<Output, String> {
    let mut command = home.command(Path::new("/bin/sh"));
    command.arg(root.join("install.sh")).arg("--from").arg(from);
    if let Some(fault) = fault {
        command.env("CADENCE_INSTALL_FAULT", fault);
    }
    run(command, "install.sh")
}

fn sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn tar(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = Command::new("tar")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tar {args:?} failed"))
    }
}

/// A copy of the bundle directory whose archive's binary is changed by
/// `change`, with `cadence.sha256` and the sums rewritten as a tamperer
/// that controls the archive would (`fix_inner`, `fix_standalone`).
fn tampered_binary(
    good: &Path,
    out: &Path,
    name: &str,
    change: impl Fn(&mut Vec<u8>),
    fix_inner: bool,
    fix_standalone: bool,
) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let unpack = out.join("unpack");
    fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
    let archive = good.join(format!("{name}.tar.gz"));
    tar(&[
        "-xzf".as_ref(),
        archive.as_os_str(),
        "-C".as_ref(),
        unpack.as_os_str(),
    ])?;
    let binary = unpack.join(name).join("cadence");
    let mut bytes = fs::read(&binary).map_err(|e| e.to_string())?;
    change(&mut bytes);
    fs::write(&binary, &bytes).map_err(|e| e.to_string())?;
    let new_hash = sha256(&binary)?;
    if fix_inner {
        fs::write(
            unpack.join(name).join("cadence.sha256"),
            format!("{new_hash}  cadence\n"),
        )
        .map_err(|e| e.to_string())?;
    }
    let repacked = out.join(format!("{name}.tar.gz"));
    tar(&[
        "-czf".as_ref(),
        repacked.as_os_str(),
        "-C".as_ref(),
        unpack.as_os_str(),
        name.as_ref(),
    ])?;
    fs::remove_dir_all(&unpack).map_err(|e| e.to_string())?;
    let sums = fs::read_to_string(good.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    let archive_hash = sha256(&repacked)?;
    let rewritten: String = sums
        .lines()
        .map(|line| {
            if line.ends_with(".tar.gz") {
                format!("{archive_hash}  {name}.tar.gz\n")
            } else if fix_standalone && line.ends_with("cadence-linux-x86_64") {
                format!("{new_hash}  cadence-linux-x86_64\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(out.join("SHA256SUMS"), rewritten).map_err(|e| e.to_string())
}

fn refused(
    root: &Path,
    home: &Home,
    bundle: &Path,
    before: Option<&PathBuf>,
    case: &str,
    says: &str,
) -> Result<(), String> {
    let output = install(root, home, bundle, None)?;
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        return Err(format!("{case}: the installer accepted it"));
    }
    if !text.contains(says) {
        return Err(format!("{case}: refused for another reason: {text}"));
    }
    if home.link().as_ref() != before {
        return Err(format!("{case}: the active link moved"));
    }
    ok(
        &run(
            {
                let mut c = home.command(&home.bin());
                c.arg("--version");
                c
            },
            "cadence --version",
        )?,
        case,
    )?;
    println!(
        "package-install: {case}: refused ({}) and the working version still runs",
        text.trim().lines().last().unwrap_or("")
    );
    Ok(())
}

/// Runs the gate.
///
/// # Errors
/// Any step of the journey failing, or a tamper case accepted.
pub fn check(root: &Path) -> Result<(), String> {
    let version = env!("CARGO_PKG_VERSION");
    let name = format!("cadence-{version}-{TARGET}");
    let dist =
        std::env::var_os("CADENCE_DIST_DIR").map_or_else(|| root.join("dist"), PathBuf::from);
    if !dist.join(format!("{name}.tar.gz")).is_file() {
        crate::dist::build_dist()?;
    }
    // The bundle's own digests.
    let mut verify = Command::new("sha256sum");
    verify.args(["--check", "SHA256SUMS"]).current_dir(&dist);
    ok(&run(verify, "sha256sum")?, "the bundle digests")?;
    let work = std::env::temp_dir().join(format!("cadence-package-{}", std::process::id()));
    let result = journey(root, &dist, &work, &name, version);
    let _cleanup = fs::remove_dir_all(&work);
    result
}

fn journey(root: &Path, dist: &Path, work: &Path, name: &str, version: &str) -> Result<(), String> {
    // Only the candidate's bundle: its archive and sums, nothing else.
    let good = work.join("bundle");
    fs::create_dir_all(&good).map_err(|e| e.to_string())?;
    for file in [format!("{name}.tar.gz"), "SHA256SUMS".to_string()] {
        fs::copy(dist.join(&file), good.join(&file)).map_err(|e| format!("{file}: {e}"))?;
    }
    let home = Home {
        root: work.join("home"),
    };
    fs::create_dir_all(home.root.join("tmp")).map_err(|e| e.to_string())?;
    ok(&install(root, &home, &good, None)?, "install.sh --from")?;
    let bin = home.bin();
    let shown = run(
        {
            let mut c = home.command(&bin);
            c.arg("--version");
            c
        },
        "cadence --version",
    )?;
    ok(&shown, "cadence --version")?;
    if !String::from_utf8_lossy(&shown.stdout).contains(version) {
        return Err("the installed binary reports another version".into());
    }
    println!("package-install: installed offline; cadence --version reports {version}");
    // A short headless drill on the default profile, then reopen it.
    let profile = home.data().join("cadence");
    let drill = work.join("drill.jsonl");
    fs::write(
        &drill,
        concat!(
            r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}"#,
            "\n",
            r#"{"v":1,"id":"2","op":"action","args":{"name":"StartDrill"}}"#,
            "\n",
            r#"{"v":1,"id":"3","op":"text","args":{"text":"cat","inter_key_microseconds":300000}}"#,
            "\n",
            r#"{"v":1,"id":"4","op":"quit","args":{"flush":true}}"#,
            "\n",
        ),
    )
    .map_err(|e| e.to_string())?;
    let typed = run(
        {
            let mut c = home.command(&bin);
            c.arg("headless")
                .arg("--script")
                .arg(&drill)
                .arg("--profile-dir")
                .arg(&profile);
            c
        },
        "headless drill",
    )?;
    ok(&typed, "the headless drill")?;
    if !String::from_utf8_lossy(&typed.stdout).contains(r#""saved":true"#) {
        return Err("the drill was not saved".into());
    }
    let doctor = run(
        {
            let mut c = home.command(&bin);
            c.args(["doctor", "--json"]);
            c
        },
        "doctor",
    )?;
    ok(&doctor, "cadence doctor --json")?;
    let report: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).map_err(|e| e.to_string())?;
    let commit = report
        .pointer("/store/sessions/0/commit_watermark")
        .and_then(serde_json::Value::as_u64);
    if commit.is_none_or(|c| c == 0) {
        return Err(format!("the saved drill did not reopen: {report}"));
    }
    println!("package-install: a headless drill saved and reopened (commit watermark {commit:?})");
    // The example config is valid as shipped.
    let installed = home
        .root
        .join(".local/lib/cadence")
        .join(format!("v{version}"));
    let config = home.config().join("cadence");
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    fs::copy(
        installed.join("config.example.toml"),
        config.join("config.toml"),
    )
    .map_err(|e| format!("config.example.toml: {e}"))?;
    ok(
        &run(
            {
                let mut c = home.command(&bin);
                c.args(["config", "check"]);
                c
            },
            "config check",
        )?,
        "config check of config.example.toml",
    )?;
    for (command, file) in [("backup", "backup.sqlite3"), ("export", "export.json")] {
        let out = work.join(file);
        ok(
            &run(
                {
                    let mut c = home.command(&bin);
                    c.arg(command).arg("--output").arg(&out);
                    c
                },
                command,
            )?,
            command,
        )?;
        if !out.is_file() {
            return Err(format!("{command} wrote nothing"));
        }
    }
    println!("package-install: config.example.toml passes config check; backup and export written");
    let acceptance = fs::read_to_string(installed.join("ACCEPTANCE.txt"))
        .map_err(|e| format!("ACCEPTANCE.txt: {e}"))?;
    if !acceptance.contains(SIGNATURE) {
        return Err("ACCEPTANCE.txt does not state the signature".into());
    }

    let before = home.link();
    // A tampered tarball: one byte changed, the sums left as published.
    let flipped = work.join("tampered-tarball");
    fs::create_dir_all(&flipped).map_err(|e| e.to_string())?;
    fs::copy(good.join("SHA256SUMS"), flipped.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    let mut bytes = fs::read(good.join(format!("{name}.tar.gz"))).map_err(|e| e.to_string())?;
    let middle = bytes.len().checked_div(2).unwrap_or(0);
    if let Some(byte) = bytes.get_mut(middle) {
        *byte ^= 0x5a;
    }
    fs::write(flipped.join(format!("{name}.tar.gz")), bytes).map_err(|e| e.to_string())?;
    refused(
        root,
        &home,
        &flipped,
        before.as_ref(),
        "a tampered tarball",
        "checksum mismatch",
    )?;
    // A tampered binary inside a tarball whose sum is valid.
    let inner = work.join("tampered-binary");
    tampered_binary(&good, &inner, name, |b| b.push(0), false, false)?;
    refused(
        root,
        &home,
        &inner,
        before.as_ref(),
        "a tampered binary in a valid tarball",
        "does not match its cadence.sha256",
    )?;
    // A wrong-architecture binary with every digest made consistent.
    let arm = work.join("wrong-architecture");
    tampered_binary(
        &good,
        &arm,
        name,
        |b| {
            if let Some(machine) = b.get_mut(18..20) {
                machine.copy_from_slice(&[0xb7, 0x00]);
            }
        },
        true,
        true,
    )?;
    refused(
        root,
        &home,
        &arm,
        before.as_ref(),
        "a wrong-architecture bundle",
        "not for x86_64",
    )?;
    // An install interrupted before the link: the working version stays.
    let interrupted = install(root, &home, &good, Some("before-link"))?;
    if interrupted.status.success() {
        return Err("the interrupted install reported success".into());
    }
    ok(
        &run(
            {
                let mut c = home.command(&bin);
                c.arg("--version");
                c
            },
            "cadence --version",
        )?,
        "the working version after an interrupted install",
    )?;
    println!("package-install: an interrupted install leaves the working version running");
    // A rollback against a newer schema is refused with the backup guidance.
    let database = profile.join("progress.sqlite3");
    let mut bump = Command::new("python3");
    bump.arg("-c")
        .arg("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('UPDATE schema_info SET version=version+1'); c.commit()")
        .arg(&database);
    ok(&run(bump, "python3 sqlite3")?, "writing a newer schema")?;
    let rollback = run(
        {
            let mut c = home.command(&bin);
            c.arg("export").arg("--output").arg(work.join("again.json"));
            c
        },
        "export",
    )?;
    let says = String::from_utf8_lossy(&rollback.stderr).into_owned();
    if rollback.status.success()
        || !says.contains("newer than supported")
        || !says.contains("restore a backup")
    {
        return Err(format!(
            "a newer schema was not refused with the backup guidance: {says}"
        ));
    }
    println!("package-install: a newer schema is refused with the backup guidance");
    println!("package-install: {SIGNATURE}");
    Ok(())
}
