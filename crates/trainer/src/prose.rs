//! Prose bank (plan 0003): verbatim public-domain passages plus the original
//! Cadence passages, with provenance in `corpus/NOTICE` and
//! `tools/corpus/extract_prose.py`. Passage identity is a text digest, so a
//! passage keeps its id when the bank is reordered or extended.
use std::collections::HashSet;

use cadence_core::alphabet::{Layout, TrainingChar, validate_target};
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
            let units = validate_target(text, Layout::Us).map_err(ProseError::Alphabet)?;
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

/// Splits one passage into exercises of at most `target_units`. A chunk ends
/// at the last sentence end that keeps it at least half full, else at the
/// last clause end, else at a word boundary; an overlong word is split.
/// Every unit is kept, in order; only the separating spaces move.
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
    let space = TrainingChar::new(' ', Layout::Us).map_err(ProseError::Alphabet)?;
    let minimum = target_units.div_ceil(2);
    let mut result = Vec::new();
    let mut pending: Vec<&[TrainingChar]> = Vec::new();
    for word in units
        .split(|unit| unit.get() == ' ')
        .filter(|word| !word.is_empty())
    {
        if word.len() > target_units {
            flush(&mut result, &mut pending, space, 0);
            let mut pieces = word.chunks(target_units).peekable();
            while let Some(piece) = pieces.next() {
                if pieces.peek().is_some() {
                    result.push(piece.to_vec());
                } else {
                    pending.push(piece);
                }
            }
            continue;
        }
        while !pending.is_empty()
            && joined_len(&pending)
                .saturating_add(1)
                .saturating_add(word.len())
                > target_units
        {
            let split = break_point(&pending, minimum);
            flush(&mut result, &mut pending, space, split);
        }
        pending.push(word);
    }
    flush(&mut result, &mut pending, space, 0);
    Ok(result)
}

/// Length of words joined by single spaces.
fn joined_len(words: &[&[TrainingChar]]) -> usize {
    words
        .iter()
        .map(|word| word.len())
        .fold(words.len().saturating_sub(1), usize::saturating_add)
}

/// Number of leading words to emit: the preferred boundary that leaves the
/// chunk at least `minimum` units long, or all of them.
fn break_point(words: &[&[TrainingChar]], minimum: usize) -> usize {
    let ends = |count: usize, marks: &[char]| {
        let last = words
            .get(count.saturating_sub(1))
            .and_then(|word| {
                word.iter()
                    .rev()
                    .find(|unit| !matches!(unit.get(), '"' | '\'' | ')' | ']'))
            })
            .map(|unit| unit.get());
        last.is_some_and(|c| marks.contains(&c))
            && words
                .get(..count)
                .is_some_and(|head| joined_len(head) >= minimum)
    };
    let all = words.len();
    (1..all)
        .rev()
        .find(|count| ends(*count, &['.', '!', '?']))
        .or_else(|| (1..all).rev().find(|count| ends(*count, &[',', ';', ':'])))
        .unwrap_or(all)
}

/// Emits the first `count` pending words (all when `count` is 0) as a chunk.
fn flush(
    result: &mut Vec<Vec<TrainingChar>>,
    pending: &mut Vec<&[TrainingChar]>,
    space: TrainingChar,
    count: usize,
) {
    if pending.is_empty() {
        return;
    }
    let count = if count == 0 {
        pending.len()
    } else {
        count.min(pending.len())
    };
    let rest = pending.split_off(count);
    result.push(pending.join(&space));
    *pending = rest;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCES: &str = "a\tA Work\tAn Author\nb\tB Work\tB Author\n";

    fn text(units: &[TrainingChar]) -> String {
        units.iter().map(|unit| unit.get()).collect()
    }

    #[test]
    fn chunks_respect_bounds_and_keep_every_character() {
        let units =
            validate_target("One two three. Averylongunbrokenwordthenmore!", Layout::Us).unwrap();
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
    fn chunks_prefer_sentence_then_clause_boundaries() {
        let units = validate_target(
            "The sun rose. It was cold, and the road was long; we walked on",
            Layout::Us,
        )
        .unwrap();
        let chunks: Vec<String> = chunk_passage(&units, 30)
            .unwrap()
            .iter()
            .map(|c| text(c))
            .collect();
        // "The sun rose." is under half the bound, so the clause end wins.
        assert_eq!(chunks[0], "The sun rose. It was cold,");
        let chunks: Vec<String> = chunk_passage(&units, 20)
            .unwrap()
            .iter()
            .map(|c| text(c))
            .collect();
        assert_eq!(chunks[0], "The sun rose.");
        assert!(chunks.iter().all(|c| c.len() <= 20));
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
