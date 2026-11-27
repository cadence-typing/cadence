//! Development bundles: verify actual ELF headers and preserve input identities.
//! This producer does not sign or claim P8 qualification.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const TARGET: &str = "x86_64-unknown-linux-musl";

pub(crate) fn root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest has no repository parent".into())
}

pub(crate) fn command(program: &str, args: &[&OsStr], cwd: &Path) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed ({}): {}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("{program} returned non-UTF8 output: {e}"))
}

fn cargo(args: &[&str], cwd: &Path) -> Result<String, String> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    command("cargo", &args, cwd)
}

fn digest_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Checks ELF class/machine and rejects both interpreter and shared-library needs.
fn verify_static(binary: &Path, cwd: &Path) -> Result<(), String> {
    let header = command("readelf", &[OsStr::new("-hW"), binary.as_os_str()], cwd)?;
    if !header.contains("ELF64") || !header.contains("X86-64") {
        return Err("bundle requires a Linux x86_64 ELF64 executable".into());
    }
    let segments = command("readelf", &[OsStr::new("-lW"), binary.as_os_str()], cwd)?;
    let dynamic = command("readelf", &[OsStr::new("-dW"), binary.as_os_str()], cwd)?;
    if segments.contains("INTERP") || dynamic.contains("(NEEDED)") {
        return Err("binary has a dynamic interpreter/shared-library dependency".into());
    }
    Ok(())
}

/// Builds the pinned target, verifies it, and emits a versioned archive/checksums.
pub fn build_dist() -> Result<(), String> {
    let root = root()?;
    let source = command("git", &[OsStr::new("rev-parse"), OsStr::new("HEAD")], &root)?
        .trim()
        .to_string();
    let dirty = command(
        "git",
        &[OsStr::new("status"), OsStr::new("--porcelain")],
        &root,
    )?;
    // Development work can be exercised before the source freeze; the final
    // release is built from a clean source and records that fact in the manifest.
    let clean = dirty.is_empty();
    cargo(
        &[
            "build",
            "--release",
            "--locked",
            "--offline",
            "--target",
            TARGET,
            "-p",
            "cadence",
        ],
        &root,
    )?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let source_binary = target.join(TARGET).join("release/cadence");
    verify_static(&source_binary, &root)?;
    refuse_fault_hooks(&source_binary)?;

    let dist = std::env::var_os("CADENCE_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("dist"));
    fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    let bundle_name = format!("cadence-{}-{TARGET}", env!("CARGO_PKG_VERSION"));
    let bundle = dist.join(&bundle_name);
    fs::create_dir_all(&bundle).map_err(|e| e.to_string())?;
    let binary = bundle.join("cadence");
    fs::copy(&source_binary, &binary).map_err(|e| e.to_string())?;
    command("strip", &[binary.as_os_str()], &root)?;
    verify_static(&binary, &root)?;
    let binary_hash = digest_file(&binary)?;
    fs::write(
        bundle.join("cadence.sha256"),
        format!("{binary_hash}  cadence\n"),
    )
    .map_err(|e| e.to_string())?;

    for name in ["LICENSE-APACHE", "LICENSE-MIT"] {
        fs::copy(root.join(name), bundle.join(name))
            .map_err(|e| format!("mandatory {name}: {e}"))?;
    }
    let corpus = bundle.join("corpus");
    fs::create_dir_all(&corpus).map_err(|e| e.to_string())?;
    for name in [
        "NOTICE",
        "wordfreq-UPSTREAM-NOTICE.txt",
        "CC-BY-SA-4.0.txt",
        "manifest.json",
        "words-en-v2.tsv",
        "prose-en-v2.tsv",
        "prose-sources-v2.tsv",
        "benchmark-en-v1.tsv",
    ] {
        fs::copy(root.join("corpus").join(name), corpus.join(name))
            .map_err(|e| format!("mandatory corpus notice/content {name}: {e}"))?;
    }
    crate::notices::collect(&root, &bundle)?;
    // Every setting with its default (#94); the installer leaves the
    // learner's own config.toml alone.
    fs::write(bundle.join("config.example.toml"), config_example())
        .map_err(|e| format!("config.example.toml: {e}"))?;
    fs::write(
        bundle.join("ACCEPTANCE.txt"),
        crate::candidate::acceptance_summary(&root)?,
    )
    .map_err(|e| e.to_string())?;
    let compiler = command("rustc", &[OsStr::new("-Vv")], &root)?;
    let manifest = serde_json::json!({
        "schema_version": 1,
        "product": "cadence",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "unpromoted development bundle; unsigned; promotion needs #13, #14 and #98",
        "target": TARGET,
        "source_commit": source,
        "source_clean": clean,
        "compiler": compiler,
        "cargo_lock_sha256": digest_file(&root.join("Cargo.lock"))?,
        "policy_sha256": digest_file(&root.join("policies/trainer-v5.toml"))?,
        "corpus_sha256": digest_file(&root.join("corpus/words-en-v2.tsv"))?,
        "prose_sha256": digest_file(&root.join("corpus/prose-en-v2.tsv"))?,
        "probes_sha256": digest_file(&root.join("corpus/benchmark-en-v1.tsv"))?,
        "binary_sha256": binary_hash,
        "elf64_x86_64": true,
        "elf_interpreter": false,
        "elf_needed_libraries": [],
        "limitations": ["no formal qualification/signature", "adaptive word targeting and spaced review are empirical hypotheses, not proven training gains", "interrupted sessions retain acknowledged progress, not a resumed uninterrupted trial"]
    });
    fs::write(
        bundle.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(bundle.join("RUN.txt"), "Run ./cadence. Enter starts; Esc pauses/resumes; Ctrl-C saves and quits.\nUse ./cadence --help for short sessions, individual tracks, private profiles, backup and export.\nThis is an unsigned development preview, not a formally qualified release.\nVerify cadence.sha256 with sha256sum --check before running.\n").map_err(|e| e.to_string())?;

    // Reproducibility inputs (#3): the release workflow builds twice from
    // clean checkouts and compares SHA256SUMS.
    fs::write(
        bundle.join("BUNDLE.txt"),
        format!(
            "Cadence {version} for {TARGET}: unpromoted development bundle\n\
             Promotion to 0.1.0 needs: authorized signing (#13), the shared Buildbot\n\
             recovery gate (#14) and the owner's real-terminal journey (#98).\n\
             \n\
             Reproducibility inputs\n\
             source commit: {source} (clean: {clean})\n\
             Cargo.lock sha256: {lock}\n\
             toolchain: rust-toolchain.toml {toolchain}\n\
             gate image: built from docker/Dockerfile (its digest is in the release notes)\n\
             policy sha256: {policy}\n\
             words sha256: {words}\n\
             prose sha256: {prose}\n\
             archive entries: name order, mtime of the source commit, owner 0, gzip without a timestamp\n",
            version = env!("CARGO_PKG_VERSION"),
            lock = digest_file(&root.join("Cargo.lock"))?,
            toolchain = digest_file(&root.join("rust-toolchain.toml"))?,
            policy = digest_file(&root.join("policies/trainer-v5.toml"))?,
            words = digest_file(&root.join("corpus/words-en-v2.tsv"))?,
            prose = digest_file(&root.join("corpus/prose-en-v2.tsv"))?,
        ),
    )
    .map_err(|e| e.to_string())?;
    let mtime = command(
        "git",
        &[
            OsStr::new("log"),
            OsStr::new("-1"),
            OsStr::new("--format=%ct"),
        ],
        &root,
    )?;
    let mtime = format!("--mtime=@{}", mtime.trim());

    let archive = dist.join(format!("{bundle_name}.tar.gz"));
    command(
        "tar",
        &[
            OsStr::new("--sort=name"),
            OsStr::new(&mtime),
            OsStr::new("--owner=0"),
            OsStr::new("--group=0"),
            OsStr::new("--numeric-owner"),
            OsStr::new("--use-compress-program=gzip -n"),
            OsStr::new("-cf"),
            archive.as_os_str(),
            OsStr::new("-C"),
            dist.as_os_str(),
            OsStr::new(&bundle_name),
        ],
        &root,
    )?;
    // Standalone bytes are identical to the versioned archive's executable.
    let standalone = dist.join("cadence-linux-x86_64");
    fs::copy(&binary, &standalone).map_err(|e| e.to_string())?;
    let checksum = format!(
        "{}  {bundle_name}.tar.gz\n{binary_hash}  cadence-linux-x86_64\n",
        digest_file(&archive)?
    );
    fs::write(dist.join("SHA256SUMS"), checksum).map_err(|e| e.to_string())?;
    println!(
        "{}\n{}\nbinary SHA-256 {binary_hash}\nsource {source} clean={clean}",
        standalone.display(),
        archive.display()
    );
    Ok(())
}

/// A release binary never carries the test-only fault hooks (#88, #92) or
/// the performance harness (#15): their markers are present only in a
/// `test-faults` or `perf` build.
fn refuse_fault_hooks(binary: &Path) -> Result<(), String> {
    let bytes = fs::read(binary).map_err(|e| format!("{}: {e}", binary.display()))?;
    for (marker, what) in [
        (
            &b"CADENCE_FAULT_"[..],
            "a test-only fault hook (CADENCE_FAULT_*); build without `test-faults`",
        ),
        (
            &b"--calibrate-busy-us"[..],
            "the performance harness; build without `perf`",
        ),
    ] {
        if bytes.windows(marker.len()).any(|window| window == marker) {
            return Err(format!("{} contains {what}", binary.display()));
        }
    }
    Ok(())
}

/// `config.example.toml` (#94): every key of the settings catalogue with
/// its default and what it accepts, valid as written.
fn config_example() -> String {
    use cadence_core::settings::{DefaultValue, KEYS};
    let mut out = String::from(
        "# Cadence settings: every key, set to its default.\n\
         # Copy to ~/.config/cadence/config.toml (or $XDG_CONFIG_HOME/cadence/)\n\
         # and keep only what you change; `cadence config check` validates it.\n",
    );
    let mut section = "";
    for key in KEYS {
        let Some((table, name)) = key.name.split_once('.') else {
            continue;
        };
        if table != section {
            out.push_str(&format!("\n[{table}]\n"));
            section = table;
        }
        let value = match key.default {
            DefaultValue::Integer(n) => n.to_string(),
            DefaultValue::Boolean(b) => b.to_string(),
            DefaultValue::Text(t) => format!("{t:?}"),
            DefaultValue::List(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|i| format!("{i:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        out.push_str(&format!("# {}\n{name} = {value}\n", key.accepts));
    }
    out
}

#[cfg(test)]
mod config_example_tests {
    /// The example is valid as written: it parses, and every key is known.
    #[test]
    fn the_example_config_is_every_key_at_its_default() {
        let text = super::config_example();
        let parsed: toml::Value = toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        for key in cadence_core::settings::KEYS {
            let mut node = Some(&parsed);
            for part in key.name.split('.') {
                node = node.and_then(|n| n.get(part));
            }
            assert!(node.is_some(), "{} missing", key.name);
        }
    }
}
