//! Strict TOML decoding; semantic validation lives in `policy_validation`.
use super::policy::*;
use cadence_core::BasisPoints;

fn table<'a>(
    document: &'a toml::Table,
    name: &'static str,
) -> Result<&'a toml::Table, PolicyError> {
    document
        .get(name)
        .and_then(toml::Value::as_table)
        .ok_or(PolicyError::MissingTable(name))
}
fn value<'a>(table: &'a toml::Table, key: &str) -> Result<&'a toml::Value, PolicyError> {
    table
        .get(key)
        .ok_or_else(|| PolicyError::MissingKey(key.into()))
}
fn integer<T: TryFrom<i64>>(table: &toml::Table, key: &str) -> Result<T, PolicyError> {
    let raw = value(table, key)?
        .as_integer()
        .ok_or_else(|| PolicyError::WrongType(key.into()))?;
    T::try_from(raw).map_err(|_unrepresentable| PolicyError::OutOfRange {
        key: key.into(),
        reason: "integer outside representable range",
    })
}
fn boolean(table: &toml::Table, key: &str) -> Result<bool, PolicyError> {
    value(table, key)?
        .as_bool()
        .ok_or_else(|| PolicyError::WrongType(key.into()))
}
fn array<'a>(table: &'a toml::Table, key: &str) -> Result<&'a [toml::Value], PolicyError> {
    value(table, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| PolicyError::WrongType(key.into()))
}
fn integers<T: TryFrom<i64>>(table: &toml::Table, key: &str) -> Result<Vec<T>, PolicyError> {
    array(table, key)?
        .iter()
        .map(|item| {
            let raw = item
                .as_integer()
                .ok_or_else(|| PolicyError::WrongType(key.into()))?;
            T::try_from(raw).map_err(|_unrepresentable| PolicyError::OutOfRange {
                key: key.into(),
                reason: "integer outside representable range",
            })
        })
        .collect()
}
fn basis_points(table: &toml::Table, key: &str) -> Result<BasisPoints, PolicyError> {
    BasisPoints::new(integer(table, key)?).map_err(|_unrepresentable| PolicyError::OutOfRange {
        key: key.into(),
        reason: "basis points must be 0..=10000",
    })
}

pub(super) fn parse_session(document: &toml::Table) -> Result<SessionPolicy, PolicyError> {
    let t = table(document, "session")?;
    let phase_order = array(t, "phase_order")?
        .iter()
        .map(|item| match item.as_str() {
            Some("warmup") => Ok(Phase::WarmUp),
            Some("adaptive_words") => Ok(Phase::AdaptiveWords),
            Some("number_row") => Ok(Phase::NumberRow),
            Some("numpad") => Ok(Phase::Numpad),
            Some("prose") => Ok(Phase::Prose),
            Some("review") => Ok(Phase::Review),
            _ => Err(PolicyError::WrongType("phase_order".into())),
        })
        .collect::<Result<_, _>>()?;
    Ok(SessionPolicy {
        default_budget_seconds: integer(t, "default_budget_seconds")?,
        phase_order,
        phase_seconds: integers(t, "phase_seconds")?,
        minimum_budget_seconds: integer(t, "minimum_budget_seconds")?,
        maximum_budget_seconds: integer(t, "maximum_budget_seconds")?,
        phase_minimum_seconds: integers(t, "phase_minimum_seconds")?,
    })
}
pub(super) fn parse_selection(document: &toml::Table) -> Result<SelectionPolicy, PolicyError> {
    let t = table(document, "selection")?;
    let lane_order = array(t, "lane_order")?
        .iter()
        .map(|item| match item.as_str() {
            Some("target") => Ok(Lane::Target),
            Some("review") => Ok(Lane::Review),
            Some("broad") => Ok(Lane::Broad),
            _ => Err(PolicyError::WrongType("lane_order".into())),
        })
        .collect::<Result<_, _>>()?;
    Ok(SelectionPolicy {
        block_slots: integer(t, "block_slots")?,
        lane_order,
        target_slots: integer(t, "target_slots")?,
        review_slots: integer(t, "review_slots")?,
        broad_slots: integer(t, "broad_slots")?,
        maximum_word_occurrences_per_block: integer(t, "maximum_word_occurrences_per_block")?,
        maximum_consecutive_word_occurrences: integer(t, "maximum_consecutive_word_occurrences")?,
        maximum_focus_owned_target_slots: integer(t, "maximum_focus_owned_target_slots")?,
        maximum_candidates_per_slot: integer(t, "maximum_candidates_per_slot")?,
    })
}
pub(super) fn parse_statistics(document: &toml::Table) -> Result<StatisticsPolicy, PolicyError> {
    let t = table(document, "statistics")?;
    Ok(StatisticsPolicy {
        recent_occurrence_window: integer(t, "recent_occurrence_window")?,
        minimum_observations_for_confident_label: integer(
            t,
            "minimum_observations_for_confident_label",
        )?,
        minimum_sessions_for_confident_label: integer(t, "minimum_sessions_for_confident_label")?,
        error_prior_alpha: integer(t, "error_prior_alpha")?,
        error_prior_beta: integer(t, "error_prior_beta")?,
    })
}
pub(super) fn parse_weights(document: &toml::Table) -> Result<UtilityWeights, PolicyError> {
    let t = table(document, "utility_weights_basis_points")?;
    Ok(UtilityWeights {
        error: basis_points(t, "error")?,
        hesitation: basis_points(t, "hesitation")?,
        relevance: basis_points(t, "relevance")?,
        uncertainty: basis_points(t, "uncertainty")?,
    })
}
pub(super) fn parse_review(document: &toml::Table) -> Result<ReviewPolicy, PolicyError> {
    let t = table(document, "review")?;
    if value(t, "admission_order")?.as_str() != Some("fifo_when_first_due") {
        return Err(PolicyError::Inconsistent {
            what: "review admission must be fifo_when_first_due",
        });
    }
    Ok(ReviewPolicy {
        interval_days: integers(t, "interval_days")?,
        independent_clean_sessions_to_promote: integer(t, "independent_clean_sessions_to_promote")?,
        maximum_same_session_reinforcements: integer(t, "maximum_same_session_reinforcements")?,
    })
}
pub(super) fn parse_pace(document: &toml::Table, version: u8) -> Result<PacePolicy, PolicyError> {
    let t = table(document, "pace")?;
    let familiar = if version >= 4 {
        let share = t
            .get("familiar_target_share_when_lowered")
            .and_then(toml::Value::as_array)
            .filter(|share| share.len() == 2)
            .ok_or_else(|| PolicyError::WrongType("familiar_target_share_when_lowered".into()))?;
        let part = |index: usize| -> Result<u32, PolicyError> {
            share
                .get(index)
                .and_then(toml::Value::as_integer)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| PolicyError::WrongType("familiar_target_share_when_lowered".into()))
        };
        let (taken, of) = (part(0)?, part(1)?);
        if of == 0 || taken > of || of > 10 {
            return Err(PolicyError::Inconsistent {
                what: "familiar_target_share_when_lowered must be a share at most 1, denominator at most 10",
            });
        }
        (taken, of)
    } else {
        (0, 1)
    };
    Ok(PacePolicy {
        desired_first_pass: basis_points(t, "desired_first_pass_basis_points")?,
        slow_down_below: basis_points(t, "slow_down_below_basis_points")?,
        qualifying_blocks_to_raise: integer(t, "qualifying_blocks_to_raise")?,
        minimum_sessions_to_raise: integer(t, "minimum_sessions_to_raise")?,
        minimum_first_attempt_positions_per_block: integer(
            t,
            "minimum_first_attempt_positions_per_block",
        )?,
        maximum_raise: basis_points(t, "maximum_raise_basis_points")?,
        maximum_lower: basis_points(t, "maximum_lower_basis_points")?,
        familiar_target_share_when_lowered: familiar,
    })
}
pub(super) fn parse_benchmark(document: &toml::Table) -> Result<BenchmarkPolicy, PolicyError> {
    let t = table(document, "benchmark")?;
    Ok(BenchmarkPolicy {
        every_completed_sessions: integer(t, "every_completed_sessions")?,
        probe_seconds: integer(t, "probe_seconds")?,
        remaining_prose_seconds: integer(t, "remaining_prose_seconds")?,
        require_unseen_text: boolean(t, "require_unseen_text")?,
        exclude_interrupted: boolean(t, "exclude_interrupted")?,
        exclude_synthetic: boolean(t, "exclude_synthetic")?,
    })
}
