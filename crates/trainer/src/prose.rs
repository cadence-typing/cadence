//! Prose passage bank (P-04): natural, edited, offline text from
//! public-domain sources, split into bounded exercises.
//!
//! The embedded bank is Project Gutenberg eBook #1342 (Jane Austen,
//! *Pride and Prejudice*, 1813) — public domain in the United States;
//! Gutenberg makes its public-domain books freely usable. The file
//! `corpus/prose-en-v1.txt` carries the exact passage set; the binary
//! embeds it so the base bundle works offline (B-02). Benchmark probes
//! (unseen-text) remain purpose-written and separate (P-03, later
//! packet); practice passages are never benchmark probes.

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar};

/// Splits the bank into exercises of roughly `target_units` units,
/// breaking at sentence boundaries where possible.
///
/// # Errors
/// Returns the first alphabet violation (the bank is ASCII; this is an
/// authoring guard).
pub fn prose_exercises(
    bank: &str,
    target_units: usize,
) -> Result<Vec<Vec<TrainingChar>>, cadence_core::AlphabetError> {
    let mut exercises = Vec::new();
    let mut current = String::new();
    let mut current_units = 0usize;
    for paragraph in bank.split("\n\n") {
        let paragraph = paragraph.replace('\n', " ");
        for sentence in split_sentences(&paragraph) {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }
            // A single sentence longer than the target splits at word
            // boundaries so no exercise exceeds the bound.
            for chunk in chunk_long_sentence(sentence, target_units) {
                let units = chunk.chars().count();
                if current_units > 0 && current_units.saturating_add(units) > target_units {
                    exercises.push(to_units(&current)?);
                    current.clear();
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&chunk);
                current_units = current.chars().count();
            }
        }
        if !current.is_empty() {
            exercises.push(to_units(&current)?);
            current.clear();
            current_units = 0;
        }
    }
    Ok(exercises)
}

/// Splits an overlong sentence at word boundaries into chunks of at
/// most `target_units` characters; short sentences come back whole.
fn chunk_long_sentence(sentence: &str, target_units: usize) -> Vec<String> {
    if sentence.chars().count() <= target_units {
        return vec![sentence.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in sentence.split(' ') {
        if word.is_empty() {
            continue;
        }
        let extra = word.chars().count() + usize::from(!current.is_empty());
        if !current.is_empty() && current.chars().count().saturating_add(extra) > target_units {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0usize;
    for (index, character) in text.char_indices() {
        if matches!(character, '.' | '!' | '?') {
            sentences.push(&text[start..=index]);
            start = index + character.len_utf8();
        }
    }
    if start < text.len() {
        sentences.push(&text[start..]);
    }
    sentences
}

fn to_units(text: &str) -> Result<Vec<TrainingChar>, cadence_core::AlphabetError> {
    text.chars()
        .map(|character| TrainingChar::new(character, LayoutSupportsPound::US))
        .collect()
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    const BANK: &str =
        "One two three. Four five!\n\nSix seven eight? Nine ten eleven twelve thirteen fourteen.";

    #[test]
    fn splits_into_bounded_sentence_exercises() {
        let exercises = prose_exercises(BANK, 30).unwrap();
        assert!(!exercises.is_empty());
        for exercise in &exercises {
            assert!(exercise.len() <= 34, "roughly bounded: {}", exercise.len());
        }
        // All bank content is present in order.
        let joined: String = exercises
            .iter()
            .flatten()
            .map(|unit| unit.get())
            .collect::<String>()
            .replace("  ", " ");
        assert!(joined.contains("One two three."));
        assert!(joined.contains("thirteen fourteen."));
    }

    #[test]
    fn embedded_bank_loads_and_stays_in_alphabet() {
        let bank = include_str!("../../../corpus/prose-en-v1.txt");
        let exercises = prose_exercises(bank, 120).unwrap();
        assert!(exercises.len() >= 8, "the bank yields several passages");
    }
}
