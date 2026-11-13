//! Offline corpus structure checks, not linguistic or licence qualification.
//! Loads the same production TSV parser as the application and verifies
//! recorded digests/counts/bands. Vetting and benchmark inventory remain pending.

use std::path::PathBuf;

use cadence_core::LayoutSupportsPound;
use cadence_trainer::{WordBand, WordPack};

/// Minimum distinct vetted forms promised for the first release (P-04).
pub const MINIMUM_RELEASE_WORDS: usize = 20_000;
/// Shipped prose passages (plan 0003); mirrors the application lower bound.
pub const MINIMUM_PASSAGES: usize = 2_000;

fn repo_root() -> Result<PathBuf, String> {
    let mut root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| "CARGO_MANIFEST_DIR not set".to_string())?,
    );
    root.pop();
    Ok(root)
}

/// Loads the committed pack through the production loader.
pub fn load_words_pack() -> Result<WordPack, String> {
    let root = repo_root()?;
    let tsv_path = root.join("corpus/words-en-v1.tsv");
    let bytes = std::fs::read(&tsv_path)
        .map_err(|error| format!("cannot read {}: {error}", tsv_path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "pack is not UTF-8".to_string())?;

    WordPack::from_tsv(text, MINIMUM_RELEASE_WORDS, LayoutSupportsPound::US)
        .map_err(|error| error.to_string())
}

/// Runs the implemented corpus structure checks only.
pub fn check() -> Result<(), String> {
    let root = repo_root()?;
    let pack = load_words_pack()?;

    // Manifest digest match (committed file vs recorded digest).
    let manifest_path = root.join("corpus/manifest.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
        .map_err(|_| "corpus/manifest.json is not valid JSON".to_string())?;
    let recorded = manifest
        .get("digests")
        .and_then(|digests| digests.get("words-en-v1.tsv"))
        .and_then(serde_json::Value::as_str)
        .ok_or("manifest digests.words-en-v1.tsv missing")?;
    let actual = pack.digest().to_hex();
    if recorded != actual {
        return Err(format!(
            "corpus digest mismatch: manifest {recorded}, actual {actual}; rerun the acquisition recipe and update the manifest"
        ));
    }

    // Promised count.
    let words = pack.len();
    if words < MINIMUM_RELEASE_WORDS {
        return Err(format!(
            "corpus has {words} words, fewer than the promised {MINIMUM_RELEASE_WORDS}"
        ));
    }

    // Band distribution (everyday prominence: bands present, core ≥ 2000).
    let core = pack.band(WordBand::Core).len();
    let common = pack.band(WordBand::Common).len();
    let tail = pack.band(WordBand::Tail).len();
    if core < 2_000 || common == 0 || tail == 0 {
        return Err(format!(
            "band distribution too thin: core {core}, common {common}, tail {tail}"
        ));
    }

    // Offline lane capacity for the default block configuration.
    let policy = cadence_trainer::parse_policy(
        &std::fs::read(root.join("policies/trainer-v2.toml"))
            .map_err(|error| format!("cannot read policy: {error}"))?,
    )
    .map_err(|error| format!("policy invalid: {error}"))?;
    pack.capacity_check(
        policy.selection.block_slots,
        policy.selection.maximum_word_occurrences_per_block,
    )
    .map_err(|error| format!("capacity check failed: {error}"))?;

    // Prose bank (plan 0003): recorded digest, size, and enough text for the
    // policy's no-repeat window at the planning rate of the default plan.
    let prose_text = std::fs::read_to_string(root.join("corpus/prose-en-v2.tsv"))
        .map_err(|error| format!("cannot read prose bank: {error}"))?;
    let sources = std::fs::read_to_string(root.join("corpus/prose-sources-v2.tsv"))
        .map_err(|error| format!("cannot read prose sources: {error}"))?;
    let recorded_prose = manifest
        .get("prose")
        .and_then(|prose| prose.get("sha256"))
        .and_then(serde_json::Value::as_str)
        .ok_or("manifest prose.sha256 missing")?;
    let actual_prose = cadence_trainer::corpus::digest_of(prose_text.as_bytes()).to_hex();
    if recorded_prose != actual_prose {
        return Err(format!(
            "prose digest mismatch: manifest {recorded_prose}, actual {actual_prose}; rerun tools/corpus/extract_prose.py"
        ));
    }
    let bank = cadence_trainer::ProseBank::from_tsv(&prose_text, &sources, MINIMUM_PASSAGES)
        .map_err(|error| format!("prose bank invalid: {error}"))?;
    let prose_units: u64 = bank
        .passages()
        .iter()
        .map(|passage| u64::try_from(passage.units.len()).unwrap_or(u64::MAX))
        .fold(0, u64::saturating_add);
    let prose_seconds = policy
        .session
        .phase_order
        .iter()
        .zip(&policy.session.phase_seconds)
        .find(|(phase, _)| **phase == cadence_trainer::Phase::Prose)
        .map_or(0, |(_, seconds)| *seconds);
    let window_units = u64::try_from(policy.learning.prose_no_repeat_sessions)
        .unwrap_or(u64::MAX)
        .saturating_mul(prose_seconds)
        .saturating_mul(policy.learning.prose_units_per_second);
    if prose_units < window_units {
        return Err(format!(
            "prose bank has {prose_units} units; the no-repeat window needs {window_units}"
        ));
    }

    // Licence notice present.
    if !root.join("corpus/NOTICE").is_file() {
        return Err("corpus/NOTICE missing (licence attribution is mandatory)".to_string());
    }

    println!(
        "corpus gate: {words} words (core {core}, common {common}, tail {tail}), digest {actual}; prose {} passages, {prose_units} units",
        bank.passages().len()
    );
    Ok(())
}
