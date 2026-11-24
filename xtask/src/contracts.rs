//! Handover contract verification.
//!
//! `MANIFEST.sha256` pins the exact bytes of the copied handover
//! contracts (docs, schemas, fixtures, policies, formal models). Verifying
//! it proves the editable contracts in this repository still match the
//! owner-approved revision the implementation is being built against;
//! contract edits must be deliberate decisions recorded in
//! `docs/decisions/`, not silent drift.

use std::path::PathBuf;

/// Verifies every listed file exists with its pinned digest.
pub fn verify_manifest() -> Result<(), String> {
    let mut root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_unset| "CARGO_MANIFEST_DIR not set".to_string())?,
    );
    root.pop();
    let manifest_path = root.join("MANIFEST.sha256");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((expected, path)) = line.split_once("  ") else {
            failures.push(format!("malformed manifest line: {line:?}"));
            continue;
        };
        let actual = match digest_file(&root.join(path)) {
            Ok(hex) => hex,
            Err(error) => {
                failures.push(format!("{path}: {error}"));
                continue;
            }
        };
        if actual != expected {
            failures.push(format!(
                "{path}: digest drift (manifest {expected}, actual {actual}); record contract changes in docs/decisions/"
            ));
        }
        checked = checked.saturating_add(1);
    }
    if checked == 0 {
        return Err("manifest listed no files".to_string());
    }
    if failures.is_empty() {
        println!("verified {checked} contract files against MANIFEST.sha256");
        Ok(())
    } else {
        Err(format!(
            "handover contract verification failed:\n  {}",
            failures.join("\n  ")
        ))
    }
}

/// SHA-256 of a file, lowercase hex.
fn digest_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|error| format!("unreadable: {error}"))?;
    let digest = Sha256::digest(&bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}
