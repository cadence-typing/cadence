//! Corpus pack view for the planner (D-03).
//!
//! A loaded pack exposes alphabet-validated tokens and band indexes.
//! Frequency-derived tokens are not a vetted dictionary: source quality,
//! attribution and licensing limits are recorded in `corpus/NOTICE`.
//! Identity and surface-form uniqueness are checked independently.

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

/// One alphabet-validated token (not a lexicographic quality assertion).
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
    /// Distinct IDs cannot disguise repeated content.
    DuplicateForm {
        /// The repeated surface form.
        word: String,
    },
    /// Invalid capacity-check inputs.
    InvalidCapacity,
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
            Self::DuplicateForm { word } => write!(f, "duplicate word form {word:?}"),
            Self::InvalidCapacity => {
                write!(f, "capacity slots and occurrence cap must be positive")
            }
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
    /// Loads the canonical rank, token, band TSV and hashes the original bytes.
    ///
    /// # Errors
    /// Rejects malformed rows, nonpositive or repeated IDs, duplicate forms,
    /// unsupported text and insufficient content.
    pub fn from_tsv(
        text: &str,
        minimum_words: usize,
        pound: LayoutSupportsPound,
    ) -> Result<Self, CorpusError> {
        let mut entries = Vec::new();
        for line in text.lines() {
            let mut fields = line.split('\t');
            let malformed = || CorpusError::InvalidWord {
                word: line.into(),
                reason: "expected exactly rank, token, band fields with a positive integer rank"
                    .into(),
            };
            let rank = fields
                .next()
                .ok_or_else(malformed)?
                .parse::<u64>()
                .map_err(|_malformed| malformed())?;
            if rank == 0 {
                return Err(malformed());
            }
            let word = fields.next().ok_or_else(malformed)?;
            let band = WordBand::parse(fields.next().ok_or_else(malformed)?)?;
            if fields.next().is_some() {
                return Err(malformed());
            }
            entries.push((rank, word.to_string(), band));
        }
        Self::new(entries, digest_of(text.as_bytes()), minimum_words, pound)
    }

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
        let mut forms = std::collections::HashSet::with_capacity(entries.len());
        for (index, (raw_id, text, band)) in entries.into_iter().enumerate() {
            if text.is_empty() || text.chars().any(char::is_whitespace) {
                return Err(CorpusError::InvalidWord {
                    word: text,
                    reason: "tokens must be nonempty and contain no whitespace".into(),
                });
            }
            if !forms.insert(text.clone()) {
                return Err(CorpusError::DuplicateForm { word: text });
            }
            let id = WordId::new(raw_id).map_err(|_unrepresentable| CorpusError::InvalidWord {
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
        self.by_id
            .get(&id.get())
            .and_then(|index| self.words.get(*index))
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

    /// Necessary distinct-content capacity check, not a proof of every lane,
    /// focus or consecutive-cap combination. The minimum is the ceiling of
    /// block slots divided by the per-word occurrence cap. Actual planning
    /// applies all caps before ranking.
    ///
    /// # Errors
    /// Rejects zero parameters or insufficient distinct surface forms.
    pub fn capacity_check(
        &self,
        block_slots: usize,
        occurrence_cap: u32,
    ) -> Result<(), CorpusError> {
        let cap = usize::try_from(occurrence_cap)
            .map_err(|_unrepresentable| CorpusError::InvalidCapacity)?;
        if block_slots == 0 || cap == 0 {
            return Err(CorpusError::InvalidCapacity);
        }
        let required = block_slots.div_ceil(cap);
        let distinct = self.words.len();
        if distinct < required {
            return Err(CorpusError::TooSmall {
                present: distinct,
                required,
            });
        }
        Ok(())
    }
}

/// SHA-256 digest of the original canonical pack bytes.
pub fn digest_of(bytes: &[u8]) -> ContentDigest {
    use sha2::{Digest, Sha256};
    ContentDigest::from_bytes(Sha256::digest(bytes).into())
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;

    fn pack(entries: &[(String, String)], minimum: usize) -> Result<WordPack, CorpusError> {
        let raw: Vec<(u64, String, WordBand)> = entries
            .iter()
            .enumerate()
            .map(|(index, (word, band))| {
                (
                    u64::try_from(index).unwrap().checked_add(1).unwrap(),
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
        let small = band_entries(4, "core");
        assert!(pack(&small, 1).unwrap().capacity_check(10, 2).is_err());
        let enough = band_entries(5, "tail");
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
