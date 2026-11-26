//! Transfer-probe passages (P-03, #64): purpose-written unseen text, never
//! served as practice, each with a strata class so that comparable probes
//! are alike in difficulty.

use cadence_core::alphabet::TrainingChar;
use cadence_core::learning::ProbeHistory;
use cadence_core::{ContentDigest, Layout, PassageId, validate_target};

use crate::corpus::{CorpusError, WordBand, WordPack, digest_of};

/// Fewest passages a shipped bank may have.
pub const MINIMUM_PROBES: usize = 60;
/// Fewest units in one probe passage.
pub const MINIMUM_PROBE_UNITS: usize = 1_000;

/// Strata bit: punctuation-dense (at least [`PUNCTUATION_PER_MILLE`]).
pub const STRATA_PUNCTUATION: u8 = 0b0001;
/// Strata bit: many capitals (at least [`CAPITALS_PER_MILLE`] of letters).
pub const STRATA_CAPITALS: u8 = 0b0010;
/// Strata bit: long words (mean letters per word at least
/// [`LONG_WORD_TENTHS`] / 10).
pub const STRATA_LONG_WORDS: u8 = 0b0100;
/// Strata bit: mostly common words (at least [`COMMON_PER_MILLE`] of words
/// in the core band).
pub const STRATA_COMMON: u8 = 0b1000;

/// Punctuation marks per thousand units for [`STRATA_PUNCTUATION`].
pub const PUNCTUATION_PER_MILLE: u64 = 35;
/// Capitals per thousand letters for [`STRATA_CAPITALS`].
pub const CAPITALS_PER_MILLE: u64 = 40;
/// Mean word length, in tenths of a letter, for [`STRATA_LONG_WORDS`].
pub const LONG_WORD_TENTHS: u64 = 47;
/// Core-band words per thousand words for [`STRATA_COMMON`].
pub const COMMON_PER_MILLE: u64 = 600;

/// One probe passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePassage {
    /// Content identity (digest of the text, domain-separated from prose).
    pub id: PassageId,
    /// The file's key (`probe-001`, ...).
    pub key: String,
    /// The passage's units.
    pub units: Vec<TrainingChar>,
    /// Its strata class.
    pub strata: u8,
}

/// The probe bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeBank {
    passages: Vec<ProbePassage>,
    digest: ContentDigest,
}

/// The strata class of a text: punctuation density, capitals, mean word
/// length and the share of core-band words, each against a fixed threshold.
#[must_use]
pub fn strata(text: &str, pack: &WordPack) -> u8 {
    let units = u64::try_from(text.chars().count())
        .unwrap_or(u64::MAX)
        .max(1);
    let punctuation = count(text.chars().filter(char::is_ascii_punctuation));
    let letters = count(text.chars().filter(char::is_ascii_alphabetic)).max(1);
    let capitals = count(text.chars().filter(char::is_ascii_uppercase));
    let words: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphabetic() && c != '\'')
        .filter(|word| word.chars().any(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
        .collect();
    let word_count = count(words.iter()).max(1);
    let word_letters = count(
        words
            .iter()
            .flat_map(|w| w.chars().filter(char::is_ascii_alphabetic)),
    );
    let common = count(words.iter().filter(|word| {
        pack.by_form(word)
            .is_some_and(|entry| entry.band == WordBand::Core)
    }));
    let at_least = |part: u64, whole: u64, per_mille: u64| {
        part.saturating_mul(1_000) >= per_mille.saturating_mul(whole)
    };
    let mut class = 0;
    if at_least(punctuation, units, PUNCTUATION_PER_MILLE) {
        class |= STRATA_PUNCTUATION;
    }
    if at_least(capitals, letters, CAPITALS_PER_MILLE) {
        class |= STRATA_CAPITALS;
    }
    if word_letters.saturating_mul(10) >= LONG_WORD_TENTHS.saturating_mul(word_count) {
        class |= STRATA_LONG_WORDS;
    }
    if at_least(common, word_count, COMMON_PER_MILLE) {
        class |= STRATA_COMMON;
    }
    class
}

fn count<T>(items: impl Iterator<Item = T>) -> u64 {
    u64::try_from(items.count()).unwrap_or(u64::MAX)
}

impl ProbeBank {
    /// Loads `key<TAB>text` lines.
    ///
    /// # Errors
    /// Rejects malformed lines, duplicate keys or texts, short passages,
    /// text outside the training alphabet and banks below `minimum`.
    pub fn from_tsv(text: &str, pack: &WordPack, minimum: usize) -> Result<Self, CorpusError> {
        let mut passages: Vec<ProbePassage> = Vec::new();
        for line in text.lines().filter(|line| !line.is_empty()) {
            let invalid = |reason: &str| CorpusError::InvalidWord {
                word: line.chars().take(24).collect(),
                reason: reason.to_string(),
            };
            let (key, body) = line
                .split_once('\t')
                .ok_or_else(|| invalid("expected key<TAB>text"))?;
            if body.chars().count() < MINIMUM_PROBE_UNITS {
                return Err(invalid("probe passage shorter than 1000 units"));
            }
            let units =
                validate_target(body, Layout::Us).map_err(|error| invalid(&error.to_string()))?;
            let mut domain = b"cadence-probe-v1\0".to_vec();
            domain.extend_from_slice(body.as_bytes());
            let digest = digest_of(&domain);
            let bytes: [u8; 8] = digest
                .as_bytes()
                .get(..8)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| invalid("digest"))?;
            let id = PassageId::new(u64::from_le_bytes(bytes))
                .map_err(|_zero| invalid("zero passage identity"))?;
            if passages.iter().any(|p| p.key == key || p.id == id) {
                return Err(invalid("duplicate probe key or text"));
            }
            passages.push(ProbePassage {
                id,
                key: key.to_string(),
                units,
                strata: strata(body, pack),
            });
        }
        if passages.len() < minimum {
            return Err(CorpusError::TooSmall {
                present: passages.len(),
                required: minimum,
            });
        }
        Ok(Self {
            passages,
            digest: digest_of(text.as_bytes()),
        })
    }

    /// All passages in file order.
    #[must_use]
    pub fn passages(&self) -> &[ProbePassage] {
        &self.passages
    }

    /// The bank's content digest.
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// The probe passage for this profile: the first never shown, in the
    /// previous comparable probe's strata class when one is left; with the
    /// class, whether it matches. `None` when every passage was shown.
    #[must_use]
    pub fn choose(&self, history: &ProbeHistory) -> Option<(&ProbePassage, bool)> {
        let mut unseen = self
            .passages
            .iter()
            .filter(|p| !history.shown.contains(&p.id));
        match history.last_comparable_strata {
            None => unseen.next().map(|p| (p, true)),
            Some(class) => {
                let first = unseen.clone().next();
                unseen
                    .find(|p| p.strata == class)
                    .map(|p| (p, true))
                    .or_else(|| first.map(|p| (p, false)))
            }
        }
    }
}
