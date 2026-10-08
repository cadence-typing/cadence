//! Validated trainer policy (L-04, L-05, L-06).
//!
//! Every coefficient, limit and threshold is a named, typed field
//! parsed from `policies/trainer-v1.toml`. There are no naked policy
//! numbers at call sites. Validation happens once at load; all later
//! arithmetic works with in-range values.

use std::collections::BTreeMap;

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
        crate::allocation::allocate(
            self.default_budget_seconds,
            &self.phase_minimum_seconds,
            &self.phase_seconds,
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
/// `policies/trainer-v1.toml`).
///
/// # Errors
/// Returns [`PolicyError`] for any missing, mistyped or out-of-range
/// value; no defaults are silently substituted for supplied values.
pub fn parse_policy(bytes: &[u8]) -> Result<TrainerPolicy, PolicyError> {
    let digest = digest_of(bytes);
    let document: toml::Table = toml::from_str(
        std::str::from_utf8(bytes).map_err(|_| PolicyError::WrongType("document".to_string()))?,
    )
    .map_err(|_| PolicyError::WrongType("document".to_string()))?;

    let session = parse_session(&document)?;
    let selection = parse_selection(&document)?;
    let statistics = parse_statistics(&document)?;
    let weights = parse_weights(&document)?;
    let review = parse_review(&document)?;
    let pace = parse_pace(&document)?;
    let benchmark = parse_benchmark(&document)?;

    Ok(TrainerPolicy {
        session,
        selection,
        statistics,
        weights,
        review,
        pace,
        benchmark,
        source_digest: digest,
    })
}

fn digest_of(bytes: &[u8]) -> cadence_core::ContentDigest {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut raw = [u8::MAX; 32];
    for (index, byte) in digest.iter().take(32).enumerate() {
        raw[index] = *byte;
    }
    let _ = raw;
    cadence_core::ContentDigest::from_bytes(raw)
}

fn table<'a>(
    document: &'a toml::Table,
    name: &'static str,
) -> Result<&'a toml::Table, PolicyError> {
    document
        .get(name)
        .and_then(toml::Value::as_table)
        .ok_or(PolicyError::MissingTable(name))
}

fn integer(table: &toml::Table, key: &str) -> Result<i64, PolicyError> {
    table
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| PolicyError::MissingKey(key.to_string()))
}

fn boolean(table: &toml::Table, key: &str) -> Result<bool, PolicyError> {
    table
        .get(key)
        .and_then(toml::Value::as_bool)
        .ok_or_else(|| PolicyError::MissingKey(key.to_string()))
}

fn basis_points(table: &toml::Table, key: &str) -> Result<BasisPoints, PolicyError> {
    let value = integer(table, key)?;
    let unsigned = u16::try_from(value).map_err(|_| PolicyError::OutOfRange {
        key: key.to_string(),
        reason: "basis points must fit 0..=10000",
    })?;
    BasisPoints::new(unsigned).map_err(|_| PolicyError::OutOfRange {
        key: key.to_string(),
        reason: "basis points must be 0..=10000",
    })
}

fn positive(key: &str, value: i64) -> Result<u64, PolicyError> {
    u64::try_from(value).map_err(|_| PolicyError::OutOfRange {
        key: key.to_string(),
        reason: "must be a non-negative integer",
    })
}

fn parse_session(document: &toml::Table) -> Result<SessionPolicy, PolicyError> {
    let table = table(document, "session")?;
    let order_raw = table
        .get("phase_order")
        .and_then(toml::Value::as_array)
        .ok_or(PolicyError::MissingKey("session.phase_order".into()))?;
    let mut phase_order = Vec::with_capacity(order_raw.len());
    for entry in order_raw {
        let key = entry
            .as_str()
            .ok_or(PolicyError::WrongType("session.phase_order".into()))?;
        let phase =
            phase_from_key(key).ok_or(PolicyError::WrongType("session.phase_order".into()))?;
        phase_order.push(phase);
    }
    let seconds_raw = table
        .get("phase_seconds")
        .and_then(toml::Value::as_array)
        .ok_or(PolicyError::MissingKey("session.phase_seconds".into()))?;
    let mut phase_seconds = Vec::with_capacity(seconds_raw.len());
    for entry in seconds_raw {
        let value = entry
            .as_integer()
            .ok_or(PolicyError::WrongType("session.phase_seconds".into()))?;
        phase_seconds.push(positive("session.phase_seconds", value)?);
    }
    let minimums_raw = table
        .get("phase_minimum_seconds")
        .and_then(toml::Value::as_array)
        .ok_or(PolicyError::MissingKey(
            "session.phase_minimum_seconds".into(),
        ))?;
    let mut phase_minimum_seconds = Vec::with_capacity(minimums_raw.len());
    for entry in minimums_raw {
        let value = entry.as_integer().ok_or(PolicyError::WrongType(
            "session.phase_minimum_seconds".into(),
        ))?;
        phase_minimum_seconds.push(positive("session.phase_minimum_seconds", value)?);
    }
    if phase_order.len() != phase_seconds.len() || phase_order.len() != phase_minimum_seconds.len()
    {
        return Err(PolicyError::Inconsistent {
            what: "session phase arrays must have equal lengths",
        });
    }
    if phase_order.is_empty() {
        return Err(PolicyError::Inconsistent {
            what: "session needs at least one phase",
        });
    }
    Ok(SessionPolicy {
        default_budget_seconds: positive(
            "session.default_budget_seconds",
            integer(table, "default_budget_seconds")?,
        )?,
        phase_order,
        phase_seconds,
        minimum_budget_seconds: positive(
            "session.minimum_budget_seconds",
            integer(table, "minimum_budget_seconds")?,
        )?,
        maximum_budget_seconds: positive(
            "session.maximum_budget_seconds",
            integer(table, "maximum_budget_seconds")?,
        )?,
        phase_minimum_seconds,
    })
}

fn phase_from_key(key: &str) -> Option<Phase> {
    match key {
        "warmup" => Some(Phase::WarmUp),
        "adaptive_words" => Some(Phase::AdaptiveWords),
        "number_row" => Some(Phase::NumberRow),
        "numpad" => Some(Phase::Numpad),
        "prose" => Some(Phase::Prose),
        "review" => Some(Phase::Review),
        _ => None,
    }
}

fn lane_from_key(key: &str) -> Option<Lane> {
    match key {
        "target" => Some(Lane::Target),
        "broad" => Some(Lane::Broad),
        "review" => Some(Lane::Review),
        _ => None,
    }
}

fn parse_selection(document: &toml::Table) -> Result<SelectionPolicy, PolicyError> {
    let table = table(document, "selection")?;
    let order_raw = table
        .get("lane_order")
        .and_then(toml::Value::as_array)
        .ok_or(PolicyError::MissingKey("selection.lane_order".into()))?;
    let mut lane_order = Vec::with_capacity(order_raw.len());
    for entry in order_raw {
        let key = entry
            .as_str()
            .ok_or(PolicyError::WrongType("selection.lane_order".into()))?;
        let lane =
            lane_from_key(key).ok_or(PolicyError::WrongType("selection.lane_order".into()))?;
        lane_order.push(lane);
    }
    let block_slots = positive("selection.block_slots", integer(table, "block_slots")?)?
        .min(usize::MAX as u64) as usize;
    if block_slots == 0 {
        return Err(PolicyError::OutOfRange {
            key: "selection.block_slots".into(),
            reason: "must be positive",
        });
    }
    if lane_order.len() != block_slots {
        return Err(PolicyError::Inconsistent {
            what: "lane_order length must equal block_slots",
        });
    }
    Ok(SelectionPolicy {
        block_slots,
        lane_order,
        target_slots: positive("selection.target_slots", integer(table, "target_slots")?)?
            .min(u32::MAX as u64) as u32,
        review_slots: positive("selection.review_slots", integer(table, "review_slots")?)?
            .min(u32::MAX as u64) as u32,
        broad_slots: positive("selection.broad_slots", integer(table, "broad_slots")?)?
            .min(u32::MAX as u64) as u32,
        maximum_word_occurrences_per_block: positive(
            "selection.maximum_word_occurrences_per_block",
            integer(table, "maximum_word_occurrences_per_block")?,
        )? as u32,
        maximum_consecutive_word_occurrences: positive(
            "selection.maximum_consecutive_word_occurrences",
            integer(table, "maximum_consecutive_word_occurrences")?,
        )? as u32,
        maximum_focus_owned_target_slots: positive(
            "selection.maximum_focus_owned_target_slots",
            integer(table, "maximum_focus_owned_target_slots")?,
        )? as u32,
        maximum_candidates_per_slot: positive(
            "selection.maximum_candidates_per_slot",
            integer(table, "maximum_candidates_per_slot")?,
        )? as usize,
    })
}

fn parse_statistics(document: &toml::Table) -> Result<StatisticsPolicy, PolicyError> {
    let table = table(document, "statistics")?;
    Ok(StatisticsPolicy {
        recent_occurrence_window: positive(
            "statistics.recent_occurrence_window",
            integer(table, "recent_occurrence_window")?,
        )? as u32,
        minimum_observations_for_confident_label: positive(
            "statistics.minimum_observations_for_confident_label",
            integer(table, "minimum_observations_for_confident_label")?,
        )? as u32,
        minimum_sessions_for_confident_label: positive(
            "statistics.minimum_sessions_for_confident_label",
            integer(table, "minimum_sessions_for_confident_label")?,
        )? as u32,
        error_prior_alpha: positive(
            "statistics.error_prior_alpha",
            integer(table, "error_prior_alpha")?,
        )?,
        error_prior_beta: positive(
            "statistics.error_prior_beta",
            integer(table, "error_prior_beta")?,
        )?,
    })
}

fn parse_weights(document: &toml::Table) -> Result<UtilityWeights, PolicyError> {
    let table = table(document, "utility_weights_basis_points")?;
    Ok(UtilityWeights {
        error: basis_points(table, "error")?,
        hesitation: basis_points(table, "hesitation")?,
        relevance: basis_points(table, "relevance")?,
        uncertainty: basis_points(table, "uncertainty")?,
    })
}

fn parse_review(document: &toml::Table) -> Result<ReviewPolicy, PolicyError> {
    let table = table(document, "review")?;
    let intervals_raw = table
        .get("interval_days")
        .and_then(toml::Value::as_array)
        .ok_or(PolicyError::MissingKey("review.interval_days".into()))?;
    let mut interval_days = Vec::with_capacity(intervals_raw.len());
    for entry in intervals_raw {
        let value = entry
            .as_integer()
            .ok_or(PolicyError::WrongType("review.interval_days".into()))?;
        if value <= 0 {
            return Err(PolicyError::OutOfRange {
                key: "review.interval_days".into(),
                reason: "intervals must be positive",
            });
        }
        interval_days.push(value);
    }
    if interval_days.is_empty() {
        return Err(PolicyError::OutOfRange {
            key: "review.interval_days".into(),
            reason: "needs at least the 1-day level",
        });
    }
    Ok(ReviewPolicy {
        interval_days,
        independent_clean_sessions_to_promote: positive(
            "review.independent_clean_sessions_to_promote",
            integer(table, "independent_clean_sessions_to_promote")?,
        )? as u32,
        maximum_same_session_reinforcements: positive(
            "review.maximum_same_session_reinforcements",
            integer(table, "maximum_same_session_reinforcements")?,
        )? as u32,
    })
}

fn parse_pace(document: &toml::Table) -> Result<PacePolicy, PolicyError> {
    let table = table(document, "pace")?;
    Ok(PacePolicy {
        desired_first_pass: basis_points(table, "desired_first_pass_basis_points")?,
        slow_down_below: basis_points(table, "slow_down_below_basis_points")?,
        qualifying_blocks_to_raise: positive(
            "pace.qualifying_blocks_to_raise",
            integer(table, "qualifying_blocks_to_raise")?,
        )? as u32,
        minimum_sessions_to_raise: positive(
            "pace.minimum_sessions_to_raise",
            integer(table, "minimum_sessions_to_raise")?,
        )? as u32,
        minimum_first_attempt_positions_per_block: positive(
            "pace.minimum_first_attempt_positions_per_block",
            integer(table, "minimum_first_attempt_positions_per_block")?,
        )?,
        maximum_raise: basis_points(table, "maximum_raise_basis_points")?,
        maximum_lower: basis_points(table, "maximum_lower_basis_points")?,
    })
}

fn parse_benchmark(document: &toml::Table) -> Result<BenchmarkPolicy, PolicyError> {
    let table = table(document, "benchmark")?;
    Ok(BenchmarkPolicy {
        every_completed_sessions: positive(
            "benchmark.every_completed_sessions",
            integer(table, "every_completed_sessions")?,
        )? as u32,
        probe_seconds: positive("benchmark.probe_seconds", integer(table, "probe_seconds")?)?,
        remaining_prose_seconds: positive(
            "benchmark.remaining_prose_seconds",
            integer(table, "remaining_prose_seconds")?,
        )?,
        require_unseen_text: boolean(table, "require_unseen_text")?,
        exclude_interrupted: boolean(table, "exclude_interrupted")?,
        exclude_synthetic: boolean(table, "exclude_synthetic")?,
    })
}

/// Convenient lookup for per-phase weights (unused keys are ignored).
#[allow(dead_code)]
fn map_of(table: &toml::Table) -> BTreeMap<String, i64> {
    let mut map = BTreeMap::new();
    for (key, value) in table {
        if let Some(integer) = value.as_integer() {
            map.insert(key.clone(), integer);
        }
    }
    map
}
