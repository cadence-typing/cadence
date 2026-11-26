//! Offline corpus checks (decision 0006): the vetted word pack's count,
//! case-fold uniqueness, alphabet, exclusions, bands and lane capacity,
//! recorded digests, the prose bank and the transfer probes, through the
//! production loaders. The vetting itself is the recipe's
//! (`tools/corpus/vet_words.py`); this gate checks its result.

use std::path::PathBuf;

use cadence_core::Layout;
use cadence_trainer::{WordBand, WordPack};

/// Minimum distinct vetted forms promised for the first release (P-04).
pub const MINIMUM_RELEASE_WORDS: usize = 20_000;
/// Shipped prose passages (plan 0003); mirrors the application lower bound.
pub const MINIMUM_PASSAGES: usize = 2_000;

fn repo_root() -> Result<PathBuf, String> {
    let mut root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_unset| "CARGO_MANIFEST_DIR not set".to_string())?,
    );
    root.pop();
    Ok(root)
}

/// The pack checks of the gate (#8), on a pack's text and the committed
/// reviewed exclusions: at least `MINIMUM_RELEASE_WORDS` forms, no two
/// differing only in case, the training alphabet (the production loader),
/// no reviewed exclusion (with common inflections), bands present after
/// vetting, and every lane and every letter skill with a candidate fillable.
///
/// # Errors
/// The first failed check.
pub fn check_pack(text: &str, reviewed: &str) -> Result<WordPack, String> {
    let pack = WordPack::from_tsv(text, 1, Layout::Us).map_err(|error| error.to_string())?;
    let words = pack.len();
    if words < MINIMUM_RELEASE_WORDS {
        return Err(format!(
            "corpus has {words} words, fewer than the promised {MINIMUM_RELEASE_WORDS}"
        ));
    }
    let mut folded = std::collections::HashSet::new();
    for entry in pack.words() {
        let form: String = entry
            .units
            .iter()
            .map(|u| u.get().to_ascii_lowercase())
            .collect();
        if !folded.insert(form.clone()) {
            return Err(format!("duplicate form differing only in case: {form}"));
        }
    }
    let excluded: Vec<&str> = reviewed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    for form in &folded {
        let hit = excluded.iter().any(|term| {
            form == term
                || ["'s", "s", "es", "ed", "ing", "er", "ers", "y"]
                    .iter()
                    .any(|suffix| form.strip_suffix(suffix) == Some(term))
        });
        if hit {
            return Err(format!("excluded word in the pack: {form}"));
        }
    }
    let core = pack.band(WordBand::Core).len();
    let common = pack.band(WordBand::Common).len();
    let tail = pack.band(WordBand::Tail).len();
    if core < 1_500 || common == 0 || tail == 0 {
        return Err(format!(
            "band distribution too thin: core {core}, common {common}, tail {tail}"
        ));
    }
    // Every letter and letter pair that occurs in a core or common word is
    // a focus skill with a candidate; the selector falls back to sampled
    // target words when a skill's candidates run out, so a skill is
    // feasible with one candidate and the lane with the band's words.
    let target_words = core.saturating_add(common);
    if target_words < 2 {
        return Err("no target-band words".to_string());
    }
    Ok(pack)
}

/// Runs the corpus checks.
pub fn check() -> Result<(), String> {
    let root = repo_root()?;
    let text = std::fs::read_to_string(root.join("corpus/words-en-v2.tsv"))
        .map_err(|error| format!("cannot read the word pack: {error}"))?;
    let reviewed = std::fs::read_to_string(root.join("corpus/vetting/excluded-reviewed.txt"))
        .map_err(|error| format!("cannot read the reviewed exclusions: {error}"))?;
    let pack = check_pack(&text, &reviewed)?;

    // Manifest digest match (committed file vs recorded digest).
    let manifest_path = root.join("corpus/manifest.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
        .map_err(|_source| "corpus/manifest.json is not valid JSON".to_string())?;
    let recorded = manifest
        .get("vetted")
        .and_then(|vetted| vetted.get("sha256"))
        .and_then(serde_json::Value::as_str)
        .ok_or("manifest vetted.sha256 missing")?;
    let actual = pack.digest().to_hex();
    if recorded != actual {
        return Err(format!(
            "corpus digest mismatch: manifest {recorded}, actual {actual}; rerun the acquisition recipe and update the manifest"
        ));
    }

    let words = pack.len();
    let core = pack.band(WordBand::Core).len();
    let common = pack.band(WordBand::Common).len();
    let tail = pack.band(WordBand::Tail).len();

    // Offline lane capacity for the default block configuration.
    let policy = cadence_trainer::parse_policy(
        &std::fs::read(root.join("policies/trainer-v4.toml"))
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

    // Transfer probes (#64): digest, count, alphabet, length, unique keys,
    // and no sentence shared with the practice prose bank.
    let probe_text = std::fs::read_to_string(root.join("corpus/benchmark-en-v1.tsv"))
        .map_err(|error| format!("cannot read probe bank: {error}"))?;
    let recorded_probes = manifest
        .get("benchmark")
        .and_then(|benchmark| benchmark.get("sha256"))
        .and_then(serde_json::Value::as_str)
        .ok_or("manifest benchmark.sha256 missing")?;
    let actual_probes = cadence_trainer::corpus::digest_of(probe_text.as_bytes()).to_hex();
    if recorded_probes != actual_probes {
        return Err(format!(
            "probe digest mismatch: manifest {recorded_probes}, actual {actual_probes}"
        ));
    }
    let probes = cadence_trainer::probe::ProbeBank::from_tsv(
        &probe_text,
        &pack,
        cadence_trainer::probe::MINIMUM_PROBES,
    )
    .map_err(|error| format!("probe bank invalid: {error}"))?;
    let sentences = |text: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut current = String::new();
        for c in text.chars() {
            current.push(c);
            if matches!(c, '.' | '!' | '?') {
                let sentence = current.trim().to_lowercase();
                if sentence.len() > 20 {
                    out.push(sentence);
                }
                current.clear();
            }
        }
        out
    };
    let practice: std::collections::HashSet<String> = prose_text
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(_, text)| text))
        .flat_map(sentences)
        .collect();
    for line in probe_text.lines() {
        let (key, text) = line.split_once('\t').unwrap_or((line, ""));
        if let Some(shared) = sentences(text).into_iter().find(|s| practice.contains(s)) {
            return Err(format!(
                "probe {key} shares a sentence with the practice bank: {shared}"
            ));
        }
    }

    // Licence notice present.
    if !root.join("corpus/NOTICE").is_file() {
        return Err("corpus/NOTICE missing (licence attribution is mandatory)".to_string());
    }

    println!(
        "corpus gate: {words} words (core {core}, common {common}, tail {tail}), digest {actual}; prose {} passages, {prose_units} units; {} transfer probes",
        bank.passages().len(),
        probes.passages().len()
    );
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small bounded counts; overflow is impossible by construction"
)]
mod tests {
    use super::check_pack;

    /// `count` distinct four-letter forms with rank bands.
    fn pack(count: usize) -> Vec<String> {
        (0..count)
            .map(|n| {
                let mut word = String::new();
                let mut rest = n;
                for _ in 0..4 {
                    word.push(char::from(b'a' + u8::try_from(rest % 26).unwrap_or(0)));
                    rest /= 26;
                }
                let rank = n + 1;
                let band = if rank <= 2_000 {
                    "core"
                } else if rank <= 10_000 {
                    "common"
                } else {
                    "tail"
                };
                format!("{rank}\t{word}\t{band}")
            })
            .collect()
    }

    const REVIEWED: &str = "# reviewed\nretard\n";

    #[test]
    fn the_gate_passes_twenty_thousand_vetted_forms() {
        assert!(check_pack(&pack(20_000).join("\n"), REVIEWED).is_ok());
    }

    #[test]
    fn the_gate_refuses_nineteen_thousand_nine_hundred_ninety_nine() {
        let error = check_pack(&pack(19_999).join("\n"), REVIEWED).unwrap_err();
        assert!(error.contains("fewer than the promised"), "{error}");
    }

    #[test]
    fn the_gate_refuses_a_duplicate_differing_only_in_case() {
        let mut lines = pack(20_000);
        lines.push("20001\tAaaa\ttail".to_string());
        let error = check_pack(&lines.join("\n"), REVIEWED).unwrap_err();
        assert!(error.contains("only in case"), "{error}");
    }

    #[test]
    fn the_gate_refuses_an_excluded_word_and_its_inflections() {
        for word in ["retard", "retards", "retarded"] {
            let mut lines = pack(20_000);
            lines.push(format!("20001\t{word}\ttail"));
            let error = check_pack(&lines.join("\n"), REVIEWED).unwrap_err();
            assert!(error.contains("excluded word"), "{word}: {error}");
        }
    }
}
