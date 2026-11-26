//! Validated trainer policy (L-04, L-05, L-06).
//!
//! Every coefficient, limit and threshold is a named, typed field
//! parsed from `policies/trainer-v2.toml`. There are no naked policy
//! numbers at call sites. Validation happens once at load; all later
//! arithmetic works with in-range values.

use crate::policy_parse::{
    parse_benchmark, parse_pace, parse_review, parse_selection, parse_session, parse_statistics,
    parse_weights,
};

use cadence_core::{BasisPoints, CoreError};

/// Phase identifiers in policy order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Familiar words at an easy pace.
    WarmUp,
    /// Weak transitions/words, due review, unfamiliar vocabulary.
    AdaptiveWords,
    /// Digit reaches in text and neutral identifier contexts.
    NumberRow,
    /// Digit sequences, decimals, corrections, entry submission.
    Numpad,
    /// Natural sentences with matched punctuation difficulty.
    Prose,
    /// Findings and next-session focus (not extra typing).
    Review,
}

impl Phase {
    /// Stable identifier matching the policy file's `phase_order`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::WarmUp => "warmup",
            Self::AdaptiveWords => "adaptive_words",
            Self::NumberRow => "number_row",
            Self::Numpad => "numpad",
            Self::Prose => "prose",
            Self::Review => "review",
        }
    }
}

/// Validated session policy (from `[session]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPolicy {
    /// Default daily budget in seconds (900).
    pub default_budget_seconds: u64,
    /// Phase order.
    pub phase_order: Vec<Phase>,
    /// Default seconds per phase, parallel to `phase_order`.
    pub phase_seconds: Vec<u64>,
    /// Minimum legal custom budget.
    pub minimum_budget_seconds: u64,
    /// Maximum legal custom budget.
    pub maximum_budget_seconds: u64,
    /// Per-phase minimum seconds, parallel to `phase_order`.
    pub phase_minimum_seconds: Vec<u64>,
}

impl SessionPolicy {
    /// The default phase allocation for the default budget.
    ///
    /// # Errors
    /// Returns [`CoreError::CapacityExhausted`] if the pinned defaults
    /// are internally inconsistent (a policy-authoring bug).
    pub fn default_allocation(&self) -> Result<Vec<u64>, CoreError> {
        let weights = self
            .allocation_weights()
            .map_err(|_source| CoreError::InvalidIdentity {
                kind: "SessionPolicy",
                reason: "invalid allocation policy",
            })?;
        cadence_core::plan::allocate(
            self.default_budget_seconds,
            &self.phase_minimum_seconds,
            &weights,
        )
    }
}

/// Validated selection policy (from `[selection]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPolicy {
    /// Word slots per scheduled block (10).
    pub block_slots: usize,
    /// Fixed interleaved lane order.
    pub lane_order: Vec<Lane>,
    /// Targeted slots per complete block (6).
    pub target_slots: u32,
    /// Due-review slots per complete block (2).
    pub review_slots: u32,
    /// Breadth slots per complete block (2).
    pub broad_slots: u32,
    /// An exact word may appear at most this often per block (2).
    pub maximum_word_occurrences_per_block: u32,
    /// …and at most this often consecutively (2).
    pub maximum_consecutive_word_occurrences: u32,
    /// A focus unit may own at most this many targeted slots per block (3).
    pub maximum_focus_owned_target_slots: u32,
    /// Candidates materialized per slot scan (256).
    pub maximum_candidates_per_slot: usize,
}

/// Selection lanes (L-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// Targeted weak/due material.
    Target,
    /// Breadth sampling of the frequency bands.
    Broad,
    /// Oldest continuously eligible due-review item.
    Review,
}

impl Lane {
    /// Identifier matching the policy file's `lane_order`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Broad => "broad",
            Self::Review => "review",
        }
    }
}

/// Validated statistics policy (from `[statistics]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatisticsPolicy {
    /// Bounded rolling observation window per skill.
    pub recent_occurrence_window: u32,
    /// Observations below this mark label evidence sparse.
    pub minimum_observations_for_confident_label: u32,
    /// Sessions below this count label evidence sparse.
    pub minimum_sessions_for_confident_label: u32,
    /// Beta prior alpha for error probability (1).
    pub error_prior_alpha: u64,
    /// Beta prior beta for error probability (19).
    pub error_prior_beta: u64,
}

/// Validated utility weights (from `[utility_weights_basis_points]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtilityWeights {
    /// Error term weight.
    pub error: BasisPoints,
    /// Hesitation term weight.
    pub hesitation: BasisPoints,
    /// Relevance term weight.
    pub relevance: BasisPoints,
    /// Exploration/uncertainty term weight.
    pub uncertainty: BasisPoints,
}

impl UtilityWeights {
    /// Sum of the weights (for the weighted-sum denominator).
    #[must_use]
    pub fn total(self) -> u64 {
        u64::from(self.error.get())
            .saturating_add(u64::from(self.hesitation.get()))
            .saturating_add(u64::from(self.relevance.get()))
            .saturating_add(u64::from(self.uncertainty.get()))
    }
}

/// Validated review policy (from `[review]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPolicy {
    /// Promoting interval ladder in local-calendar days.
    pub interval_days: Vec<i64>,
    /// Clean completions on different sessions before promotion.
    pub independent_clean_sessions_to_promote: u32,
    /// Same-session reinforcements allowed after an error.
    pub maximum_same_session_reinforcements: u32,
}

/// Validated pace policy (from `[pace]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacePolicy {
    /// Desired first-pass accuracy (9800bp).
    pub desired_first_pass: BasisPoints,
    /// Below this first-pass accuracy (9500bp) a block lowers the target.
    pub slow_down_below: BasisPoints,
    /// Qualifying blocks needed to raise.
    pub qualifying_blocks_to_raise: u32,
    /// Distinct sessions needed to raise.
    pub minimum_sessions_to_raise: u32,
    /// First-attempt positions a block needs to qualify.
    pub minimum_first_attempt_positions_per_block: u64,
    /// Maximum raise per adjustment (300bp).
    pub maximum_raise: BasisPoints,
    /// Maximum lower per adjustment (500bp).
    pub maximum_lower: BasisPoints,
}

/// Validated benchmark policy (from `[benchmark]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkPolicy {
    /// Run a probe every N completed sessions.
    pub every_completed_sessions: u32,
    /// Probe duration in seconds.
    pub probe_seconds: u64,
    /// Prose seconds left after probe replacement.
    pub remaining_prose_seconds: u64,
    /// Probes must use unseen text.
    pub require_unseen_text: bool,
    /// Interrupted probes stay out of the comparable series.
    pub exclude_interrupted: bool,
    /// Synthetic sessions never feed benchmarks.
    pub exclude_synthetic: bool,
}

/// The complete validated policy document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainerPolicy {
    /// Session budget and phase minimums.
    pub session: SessionPolicy,
    /// Selection lanes, quotas and caps.
    pub selection: SelectionPolicy,
    /// Statistical windows and priors.
    pub statistics: StatisticsPolicy,
    /// Candidate utility weights.
    pub weights: UtilityWeights,
    /// Review intervals and promotion rules.
    pub review: ReviewPolicy,
    /// Pace hysteresis bounds.
    pub pace: PacePolicy,
    /// Benchmark cadence and comparability rules.
    pub benchmark: BenchmarkPolicy,
    /// Learner model, focus selection and content rotation (plan 0003).
    pub learning: crate::learning_policy::LearningPolicy,
    /// Digest of the source TOML bytes (PolicyVersion basis).
    pub source_digest: cadence_core::ContentDigest,
}

/// Parse failures, naming the offending key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// A required table is missing.
    MissingTable(&'static str),
    /// A required key is missing.
    MissingKey(String),
    /// A value has the wrong type.
    WrongType(String),
    /// A value is outside its legal range.
    OutOfRange {
        /// The offending key.
        key: String,
        /// Why it is invalid.
        reason: &'static str,
    },
    /// Phase/lane arrays are inconsistent.
    Inconsistent {
        /// What is inconsistent.
        what: &'static str,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTable(table) => write!(f, "policy table [{table}] missing"),
            Self::MissingKey(key) => write!(f, "policy key {key} missing"),
            Self::WrongType(key) => write!(f, "policy key {key} has the wrong type"),
            Self::OutOfRange { key, reason } => write!(f, "policy key {key}: {reason}"),
            Self::Inconsistent { what } => write!(f, "policy inconsistency: {what}"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// Parses and validates the policy TOML (bytes as shipped in
/// `policies/trainer-v3.toml`; `trainer-v2` still parses, with the fields
/// v3 added switched off).
///
/// # Errors
/// Returns [`PolicyError`] for any missing, mistyped or out-of-range
/// value; no defaults are silently substituted for supplied values.
pub fn parse_policy(bytes: &[u8]) -> Result<TrainerPolicy, PolicyError> {
    let digest = crate::corpus::digest_of(bytes);
    let document: toml::Table = toml::from_str(
        std::str::from_utf8(bytes)
            .map_err(|_not_utf8| PolicyError::WrongType("document".to_string()))?,
    )
    .map_err(|_source| PolicyError::WrongType("document".to_string()))?;
    let version = match document.get("policy_id").and_then(toml::Value::as_str) {
        Some("trainer-v2") => 2,
        Some("trainer-v3") => 3,
        _ => 0,
    };
    if version == 0
        || document
            .get("scoring_version")
            .and_then(toml::Value::as_integer)
            != Some(1)
    {
        return Err(PolicyError::Inconsistent {
            what: "unsupported trainer policy or scoring version",
        });
    }

    let session = parse_session(&document)?;
    let selection = parse_selection(&document)?;
    let statistics = parse_statistics(&document)?;
    let weights = parse_weights(&document)?;
    let review = parse_review(&document)?;
    let pace = parse_pace(&document)?;
    let benchmark = parse_benchmark(&document)?;
    let learning = crate::learning_policy::parse_learning(&document, version)?;

    let policy = TrainerPolicy {
        session,
        selection,
        statistics,
        weights,
        review,
        pace,
        benchmark,
        learning,
        source_digest: digest,
    };
    policy.validate()?;
    Ok(policy)
}
