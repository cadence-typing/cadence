//! Distribution packaging (B-02, B-06 — development bundle stage).
//!
//! Builds the statically linked `x86_64-unknown-linux-musl` release,
//! verifies the ELF is actually static (not just named musl), and
//! bundles binary + licences + version info under `dist/`. This is an
//! explicitly **unpromoted development bundle**: the P8 `package`
//! qualification (candidate receipts, signing, install smoke) is not
//! claimed here.

use std::path::PathBuf;
use std::process::Command;

const TARGET: &str = "x86_64-unknown-linux-musl";

fn repo_root() -> Result<PathBuf, String> {
    let mut root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| "CARGO_MANIFEST_DIR not set".to_string())?,
    );
    root.pop();
    Ok(root)
}

fn run(program: &str, args: &[&str], cwd: &PathBuf) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to spawn {program}: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Builds, verifies and bundles the static release.
pub fn build_dist() -> Result<(), String> {
    let root = repo_root()?;

    run(
        "cargo",
        &[
            "build",
            "--release",
            "--locked",
            "--target",
            TARGET,
            "-p",
            "cadence",
        ],
        &root,
    )?;

    let binary = root
        .join("target")
        .join(TARGET)
        .join("release")
        .join("cadence");
    if !binary.is_file() {
        return Err(format!("expected binary at {}", binary.display()));
    }
    // Prove actual ELF properties (B-02): file(1) reports static
    // linkage, not just the musl target name.
    let description = run("file", &[binary.to_str().unwrap_or("cadence")], &root)?;
    if !description.contains("statically linked") && !description.contains("static-pie linked") {
        return Err(format!("ELF is not statically linked: {description}"));
    }
    let ldd = run("ldd", &[binary.to_str().unwrap_or("cadence")], &root)?;
    if !ldd.contains("statically linked") && !ldd.contains("not a dynamic executable") {
        return Err(format!("ldd disagrees: {ldd}"));
    }

    // Strip a copy for the bundle.
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).map_err(|error| format!("dist dir: {error}"))?;
    let bundled = dist.join("cadence");
    std::fs::copy(&binary, &bundled).map_err(|error| format!("copy binary: {error}"))?;
    let _ = run("strip", &[bundled.to_str().unwrap_or("cadence")], &root);

    // Licences and provenance travel with the bundle.
    for (source, name) in [
        ("corpus/NOTICE", "CORPUS-NOTICE"),
        ("LICENSE-APACHE", "LICENSE-APACHE"),
        ("LICENSE-MIT", "LICENSE-MIT"),
    ] {
        let source_path = root.join(source);
        if source_path.is_file() {
            std::fs::copy(source_path, dist.join(name))
                .map_err(|error| format!("copy {source}: {error}"))?;
        }
    }

    let sha = run("sha256sum", &[bundled.to_str().unwrap_or("cadence")], &root)?;
    let digest = sha
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();
    let version = env!("CARGO_PKG_VERSION");
    let manifest = format!(
        "# cadence {version} development bundle ({TARGET})\n\
         status: UNPROMOTED DEVELOPMENT BUNDLE — not a P8-qualified owner release\n\
         binary sha256: {digest}\n\
         licences: Apache-2.0 OR MIT (code); CC-BY-SA-4.0 (word data); public domain (prose)\n\
         built: from the committed Cargo.lock; ELF verified statically linked\n"
    );
    std::fs::write(dist.join("BUNDLE.txt"), manifest)
        .map_err(|error| format!("write manifest: {error}"))?;

    println!("dist: {} (sha256 {digest})", bundled.display());
    println!("dist: this is an unpromoted development bundle; P8 packaging remains pending");
    Ok(())
}
