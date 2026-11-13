//! Prose bank (plan 0003): verbatim public-domain passages plus the original
//! Cadence passages, with provenance in `corpus/NOTICE` and
//! `tools/corpus/extract_prose.py`. Passage identity is a text digest, so a
//! passage keeps its id when the bank is reordered or extended.
use std::collections::HashSet;

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar, validate_target};
use cadence_core::{ContentDigest, PassageId};

use crate::corpus::digest_of;

/// Prose preparation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProseError {
    /// A zero-size exercise is not meaningful.
    InvalidBound,
    /// The bank contains unsupported text.
    Alphabet(cadence_core::AlphabetError),
    /// A row is not `source<TAB>text` or names an unknown source.
    Malformed {
        /// One-based line number.
        line: usize,
    },
    /// Two passages share text or digest identity.
    Duplicate {
        /// One-based line number of the repeat.
        line: usize,
    },
    /// Fewer passages than the caller requires.
    TooSmall {
        /// Passages present.
        present: usize,
        /// Passages required.
        required: usize,
    },
}
impl std::fmt::Display for ProseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBound => write!(f, "prose target bound must be positive"),
            Self::Alphabet(error) => error.fmt(f),
            Self::Malformed { line } => write!(f, "malformed prose row at line {line}"),
            Self::Duplicate { line } => write!(f, "duplicate prose passage at line {line}"),
            Self::TooSmall { present, required } => {
                write!(f, "prose bank has {present} passages, {required} required")
            }
        }
    }
}
impl std::error::Error for ProseError {}

/// Where a passage comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseSource {
    /// Stable key used in the passage file.
    pub key: String,
    /// Work title.
    pub title: String,
    /// Author.
    pub author: String,
}

/// One validated passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProsePassage {
    /// Digest-derived identity.
    pub id: PassageId,
    /// Index into the bank's sources.
    pub source: usize,
    /// Validated units, single-spaced.
    pub units: Vec<TrainingChar>,
}

/// The immutable prose bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseBank {
    passages: Vec<ProsePassage>,
    sources: Vec<ProseSource>,
    digest: ContentDigest,
}

impl ProseBank {
    /// Loads `source<TAB>text` passages and `key<TAB>title<TAB>author` sources.
    ///
    /// # Errors
    /// Malformed rows, unknown sources, unsupported characters, duplicates
    /// and banks smaller than `minimum_passages`.
    pub fn from_tsv(
        passages: &str,
        sources: &str,
        minimum_passages: usize,
    ) -> Result<Self, ProseError> {
        let sources = sources
            .lines()
            .enumerate()
            .map(|(index, row)| {
                let mut fields = row.split('\t');
                match (fields.next(), fields.next(), fields.next(), fields.next()) {
                    (Some(key), Some(title), Some(author), None)
                        if !key.is_empty() && !title.is_empty() =>
                    {
                        Ok(ProseSource {
                            key: key.into(),
                            title: title.into(),
                            author: author.into(),
                        })
                    }
                    _ => Err(ProseError::Malformed {
                        line: index.saturating_add(1),
                    }),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for (index, row) in passages.lines().enumerate() {
            let line = index.saturating_add(1);
            let (key, text) = row.split_once('\t').ok_or(ProseError::Malformed { line })?;
            let source = sources
                .iter()
                .position(|candidate| candidate.key == key)
                .ok_or(ProseError::Malformed { line })?;
            if text.is_empty() || text.contains("  ") || text.trim() != text {
                return Err(ProseError::Malformed { line });
            }
            let units =
                validate_target(text, LayoutSupportsPound::US).map_err(ProseError::Alphabet)?;
            let id = passage_id(text).ok_or(ProseError::Duplicate { line })?;
            if !seen.insert(id) {
                return Err(ProseError::Duplicate { line });
            }
            result.push(ProsePassage { id, source, units });
        }
        if result.len() < minimum_passages {
            return Err(ProseError::TooSmall {
                present: result.len(),
                required: minimum_passages,
            });
        }
        let mut bytes = passages.as_bytes().to_vec();
        bytes.extend_from_slice(sources_digest_input(&sources).as_bytes());
        Ok(Self {
            passages: result,
            sources,
            digest: digest_of(&bytes),
        })
    }

    /// All passages in file order.
    #[must_use]
    pub fn passages(&self) -> &[ProsePassage] {
        &self.passages
    }

    /// The source of a passage.
    #[must_use]
    pub fn source(&self, passage: &ProsePassage) -> Option<&ProseSource> {
        self.sources.get(passage.source)
    }

    /// Digest over passage and source bytes.
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

fn sources_digest_input(sources: &[ProseSource]) -> String {
    sources
        .iter()
        .map(|source| format!("{}\t{}\t{}\n", source.key, source.title, source.author))
        .collect()
}

/// First eight digest bytes of the passage text; zero is not an identity.
fn passage_id(text: &str) -> Option<PassageId> {
    let digest = digest_of(text.as_bytes());
    let bytes: [u8; 8] = digest.as_bytes().get(..8)?.try_into().ok()?;
    PassageId::new(u64::from_le_bytes(bytes)).ok()
}

/// Splits one passage into exercises of at most `target_units`, preferring
/// word boundaries; an overlong word is split.
///
/// # Errors
/// Rejects a zero bound.
pub fn chunk_passage(
    units: &[TrainingChar],
    target_units: usize,
) -> Result<Vec<Vec<TrainingChar>>, ProseError> {
    if target_units == 0 {
        return Err(ProseError::InvalidBound);
    }
    let space = TrainingChar::new(' ', LayoutSupportsPound::US).map_err(ProseError::Alphabet)?;
    let mut result = Vec::new();
    let mut current: Vec<TrainingChar> = Vec::new();
    for word in units
        .split(|unit| unit.get() == ' ')
        .filter(|word| !word.is_empty())
    {
        let extra = word
            .len()
            .checked_add(usize::from(!current.is_empty()))
            .ok_or(ProseError::InvalidBound)?;
        if !current.is_empty()
            && current
                .len()
                .checked_add(extra)
                .is_none_or(|length| length > target_units)
        {
            result.push(std::mem::take(&mut current));
        }
        for chunk in word.chunks(target_units) {
            if !current.is_empty() {
                current.push(space);
            }
            current.extend_from_slice(chunk);
            if current.len() >= target_units {
                result.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    const SOURCES: &str = "a\tA Work\tAn Author\nb\tB Work\tB Author\n";

    fn text(units: &[TrainingChar]) -> String {
        units.iter().map(|unit| unit.get()).collect()
    }

    #[test]
    fn chunks_respect_bounds_and_keep_every_character() {
        let units = validate_target(
            "One two three. Averylongunbrokenwordthenmore!",
            LayoutSupportsPound::US,
        )
        .unwrap();
        for bound in 1..=30 {
            let chunks = chunk_passage(&units, bound).unwrap();
            assert!(chunks.iter().all(|c| !c.is_empty() && c.len() <= bound));
            let joined: String = chunks.iter().map(|c| text(c)).collect::<String>();
            assert_eq!(
                joined.replace(' ', ""),
                "Onetwothree.Averylongunbrokenwordthenmore!"
            );
        }
        assert!(chunk_passage(&units, 0).is_err());
    }

    #[test]
    fn bank_validates_sources_duplicates_and_size() {
        let bank =
            ProseBank::from_tsv("a\tFirst passage.\nb\tSecond passage.\n", SOURCES, 2).unwrap();
        assert_eq!(bank.passages().len(), 2);
        assert_eq!(bank.source(&bank.passages()[1]).unwrap().title, "B Work");
        assert_ne!(bank.passages()[0].id, bank.passages()[1].id);
        assert!(matches!(
            ProseBank::from_tsv("a\tSame.\nb\tSame.\n", SOURCES, 1),
            Err(ProseError::Duplicate { line: 2 })
        ));
        assert!(matches!(
            ProseBank::from_tsv("z\tUnknown source.\n", SOURCES, 1),
            Err(ProseError::Malformed { line: 1 })
        ));
        assert!(matches!(
            ProseBank::from_tsv("a\tOnly one.\n", SOURCES, 2),
            Err(ProseError::TooSmall { .. })
        ));
    }

    #[test]
    fn identity_survives_reordering() {
        let forward = ProseBank::from_tsv("a\tFirst.\nb\tSecond.\n", SOURCES, 1).unwrap();
        let reverse = ProseBank::from_tsv("b\tSecond.\na\tFirst.\n", SOURCES, 1).unwrap();
        assert_eq!(forward.passages()[0].id, reverse.passages()[1].id);
        assert_ne!(forward.digest(), reverse.digest());
    }

    #[test]
    fn shipped_bank_is_valid_and_large() {
        let bank = ProseBank::from_tsv(
            include_str!("../../../corpus/prose-en-v2.tsv"),
            include_str!("../../../corpus/prose-sources-v2.tsv"),
            2_000,
        )
        .unwrap();
        let units: usize = bank.passages().iter().map(|p| p.units.len()).sum();
        assert!(units > 500_000, "bank holds {units} units");
    }
}
