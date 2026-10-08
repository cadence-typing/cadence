//! Corpus pack view for the planner (D-03).
//!
//! A loaded pack exposes immutable words with frequency bands and a
//! band/length index. Packs are content-addressed
//! ([`cadence_core::CorpusVersion`]); loading validates the manifest,
//! deduplication and the training alphabet before anything reaches the
//! planner. The P2 acquisition recipe (decision 0002) produces the
//! on-disk format this loader reads.

use std::collections::HashMap;

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar, validate_target};
use cadence_core::{ContentDigest, WordId};

/// Frequency band labels (decision 0002): everyday prominence with a
/// sampled long tail (L-04 relevance input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordBand {
    /// Ranks 1–2,000: everyday core.
    Core,
    /// Ranks 2,001–10,000: common.
    Common,
    /// Ranks beyond 10,000: long tail.
    Tail,
}

impl WordBand {
    /// Parses the manifest band tag.
    ///
    /// # Errors
    /// Returns [`CorpusError::UnknownBand`] for unrecognized tags.
    pub fn parse(tag: &str) -> Result<Self, CorpusError> {
        match tag {
            "core" => Ok(Self::Core),
            "common" => Ok(Self::Common),
            "tail" => Ok(Self::Tail),
            other => Err(CorpusError::UnknownBand {
                tag: other.to_string(),
            }),
        }
    }
}

/// One vetted word form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEntry {
    /// Stable content-addressed identity.
    pub id: WordId,
    /// The word's units (validated against the training alphabet).
    pub units: Vec<TrainingChar>,
    /// Frequency band.
    pub band: WordBand,
}

/// Corpus loading/validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusError {
    /// The pack contains duplicate identities (checked, not assumed
    /// impossible at the API boundary, D-03).
    DuplicateIdentity {
        /// The duplicated numeric identity.
        id: u64,
    },
    /// A word is outside the training alphabet.
    InvalidWord {
        /// The offending word.
        word: String,
        /// The alphabet diagnostic.
        reason: String,
    },
    /// An unknown band tag.
    UnknownBand {
        /// The unrecognized tag.
        tag: String,
    },
    /// The pack is empty or too small for the required lanes.
    TooSmall {
        /// Words present.
        present: usize,
        /// Words required.
        required: usize,
    },
}

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateIdentity { id } => write!(f, "duplicate word identity {id}"),
            Self::InvalidWord { word, reason } => {
                write!(f, "word {word:?} outside the training alphabet: {reason}")
            }
            Self::UnknownBand { tag } => write!(f, "unknown frequency band {tag:?}"),
            Self::TooSmall { present, required } => {
                write!(f, "corpus too small: {present} words, {required} required")
            }
        }
    }
}

impl std::error::Error for CorpusError {}

/// An immutable in-memory word pack with band/length indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordPack {
    words: Vec<WordEntry>,
    by_id: HashMap<u64, usize>,
    by_band: HashMap<WordBand, Vec<usize>>,
    digest: ContentDigest,
}

impl WordPack {
    /// Validates and indexes raw entries; identities must be unique and
    /// every word inside the V1 training alphabet.
    ///
    /// # Errors
    /// Returns the first [`CorpusError`] violation.
    pub fn new(
        entries: Vec<(u64, String, WordBand)>,
        digest: ContentDigest,
        minimum_words: usize,
        pound: LayoutSupportsPound,
    ) -> Result<Self, CorpusError> {
        if entries.len() < minimum_words {
            return Err(CorpusError::TooSmall {
                present: entries.len(),
                required: minimum_words,
            });
        }
        let mut words = Vec::with_capacity(entries.len());
        let mut by_id = HashMap::with_capacity(entries.len());
        let mut by_band: HashMap<WordBand, Vec<usize>> = HashMap::new();
        for (index, (raw_id, text, band)) in entries.into_iter().enumerate() {
            let id = WordId::new(raw_id).map_err(|_| CorpusError::InvalidWord {
                word: text.clone(),
                reason: "identity must be nonzero".to_string(),
            })?;
            if by_id.insert(raw_id, index).is_some() {
                return Err(CorpusError::DuplicateIdentity { id: raw_id });
            }
            let units =
                validate_target(&text, pound).map_err(|error| CorpusError::InvalidWord {
                    word: text,
                    reason: error.to_string(),
                })?;
            by_band.entry(band).or_default().push(index);
            words.push(WordEntry { id, units, band });
        }
        Ok(Self {
            words,
            by_id,
            by_band,
            digest,
        })
    }

    /// Number of word forms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Whether the pack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// All entries, in identity order as loaded.
    #[must_use]
    pub fn words(&self) -> &[WordEntry] {
        &self.words
    }

    /// Lookup by identity.
    #[must_use]
    pub fn word(&self, id: WordId) -> Option<&WordEntry> {
        self.by_id.get(&id.get()).map(|index| &self.words[*index])
    }

    /// Indices of words in a band.
    #[must_use]
    pub fn band(&self, band: WordBand) -> &[usize] {
        self.by_band
            .get(&band)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The content digest this pack was loaded under.
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// The offline capacity check (L-05): every default lane
    /// configuration must find a legal candidate. With the caps from
    /// policy, a pack needs at least `distinct words ≥ block_slots ×
    /// occurrence cap` distinct candidates overall, and each lane's
    /// band must be nonempty.
    ///
    /// # Errors
    /// Returns [`CorpusError::TooSmall`] when a default lane would
    /// starve.
    pub fn capacity_check(
        &self,
        block_slots: usize,
        occurrence_cap: u32,
    ) -> Result<(), CorpusError> {
        let distinct = self.words.len();
        let required = block_slots.saturating_mul(occurrence_cap as usize).max(1);
        if distinct < required {
            return Err(CorpusError::TooSmall {
                present: distinct,
                required,
            });
        }
        // Everyday prominence requires a nonempty core band; other
        // band starvation surfaces through the planner's explicit
        // NoEligibleCandidate fallback rather than here.
        if self.band(WordBand::Core).is_empty() {
            return Err(CorpusError::TooSmall {
                present: 0,
                required: 1,
            });
        }
        Ok(())
    }
}

/// SHA-256 digest of canonical pack bytes (the acquisition recipe's
/// output file).
///
/// # Errors
/// Never fails for the SHA-256 primitive; returns [`CoreError`] on
/// none for future-proofing.
pub fn digest_of(bytes: &[u8]) -> ContentDigest {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut raw = [u8::MAX; 32];
    for (index, byte) in digest.iter().take(32).enumerate() {
        raw[index] = *byte;
    }
    ContentDigest::from_bytes(raw)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    fn pack(entries: &[(String, String)], minimum: usize) -> Result<WordPack, CorpusError> {
        let raw: Vec<(u64, String, WordBand)> = entries
            .iter()
            .enumerate()
            .map(|(index, (word, band))| {
                (
                    (index as u64) + 1,
                    word.clone(),
                    WordBand::parse(band).unwrap(),
                )
            })
            .collect();
        WordPack::new(
            raw,
            ContentDigest::from_bytes([7; 32]),
            minimum,
            LayoutSupportsPound::US,
        )
    }

    fn band_entries(count: u64, band: &str) -> Vec<(String, String)> {
        (1..=count)
            .map(|n| (format!("word{n}"), band.to_string()))
            .collect()
    }

    #[test]
    fn loads_validates_and_indexes() {
        let entries = vec![
            ("street".to_string(), "core".to_string()),
            ("structure".to_string(), "common".to_string()),
            ("xylophone".to_string(), "tail".to_string()),
        ];
        let pack = pack(&entries, 3).unwrap();
        assert_eq!(pack.len(), 3);
        assert_eq!(pack.band(WordBand::Core).len(), 1);
        let entry = pack.word(WordId::new(2).unwrap()).unwrap();
        assert_eq!(entry.units.len(), 9);
    }

    #[test]
    fn invalid_alphabet_rejected_with_word_context() {
        let raw = vec![(1u64, "café".to_string(), WordBand::Core)];
        match WordPack::new(
            raw,
            ContentDigest::from_bytes([0; 32]),
            1,
            LayoutSupportsPound::US,
        ) {
            Err(CorpusError::InvalidWord { word, .. }) => assert_eq!(word, "café"),
            other => panic!("expected invalid word, got {other:?}"),
        }
    }

    #[test]
    fn capacity_check_enforces_lane_food() {
        // 10 slots × cap 2 needs ≥ 20 distinct words.
        let small = band_entries(19, "core");
        assert!(pack(&small, 1).unwrap().capacity_check(10, 2).is_err());
        let enough = band_entries(20, "core");
        assert!(pack(&enough, 1).unwrap().capacity_check(10, 2).is_ok());
    }

    #[test]
    fn too_small_pack_fails_explicitly() {
        let entries = vec![("street".to_string(), "core".to_string())];
        match pack(&entries, 20) {
            Err(CorpusError::TooSmall { present, required }) => {
                assert_eq!((present, required), (1, 20));
            }
            other => panic!("expected too-small, got {other:?}"),
        }
    }
}
