//! Corpus gate: licensed pack manifests, count/quality/capacity
//! (B-05 `corpus` gate, P-02).
//!
//! Loads `corpus/words-en-v1.tsv` into the production
//! [`WordPack`](cadence_trainer::WordPack) type, verifies the manifest
//! digests, the promised count (≥ 20,000 vetted forms), the band
//! distribution and the offline lane capacity check. The gate reads
//! only committed files; it is deterministic and offline.

use std::path::PathBuf;

use cadence_core::{ContentDigest, LayoutSupportsPound};
use cadence_trainer::{WordBand, WordPack};

/// Minimum distinct vetted forms promised for the first release (P-04).
pub const MINIMUM_RELEASE_WORDS: usize = 20_000;

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

    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index.saturating_add(1);
        let fields: Vec<&str> = line.split('\t').collect();
        let (Some(rank_text), Some(word), Some(band_text)) = (
            fields.first().copied(),
            fields.get(1).copied(),
            fields.get(2).copied(),
        ) else {
            return Err(format!("line {line_number}: expected rank\\tword\\tband"));
        };
        let rank: u64 = rank_text
            .parse()
            .map_err(|_| format!("line {line_number}: bad rank"))?;
        let band =
            WordBand::parse(band_text).map_err(|error| format!("line {line_number}: {error}"))?;
        entries.push((rank, word.to_string(), band));
    }
    let digest = digest_hex(&bytes)?;
    WordPack::new(
        entries,
        digest,
        MINIMUM_RELEASE_WORDS,
        LayoutSupportsPound::US,
    )
    .map_err(|error| error.to_string())
}

fn digest_hex(bytes: &[u8]) -> Result<ContentDigest, String> {
    let digest = cadence_trainer::corpus::digest_of(bytes);
    Ok(digest)
}

/// Runs the full corpus gate.
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
        &std::fs::read(root.join("policies/trainer-v1.toml"))
            .map_err(|error| format!("cannot read policy: {error}"))?,
    )
    .map_err(|error| format!("policy invalid: {error}"))?;
    pack.capacity_check(
        policy.selection.block_slots,
        policy.selection.maximum_word_occurrences_per_block,
    )
    .map_err(|error| format!("capacity check failed: {error}"))?;

    // Licence notice present.
    if !root.join("corpus/NOTICE").is_file() {
        return Err("corpus/NOTICE missing (licence attribution is mandatory)".to_string());
    }

    println!(
        "corpus gate: {words} words (core {core}, common {common}, tail {tail}), digest {actual}"
    );
    Ok(())
}
