//! Pace hysteresis (L-06, #61): whether a track's suggested pace rises,
//! falls or stays, from recent blocks of typing.
//!
//! Two quantities, kept apart: a fixed desired first-pass accuracy (the
//! bar a block must meet) and the suggested pace in correct units per
//! minute (the quantity that moves). A block is one track's slice of one
//! session. The pace rises by at most `raise_bp` of itself after enough
//! qualifying blocks from enough distinct sessions with no drop in
//! attempt accuracy; a latest block strictly below the slow-down band
//! lowers it by at most `lower_bp` of itself, never below 1. Between the
//! bands it stays. All comparisons are exact integer cross-multiplications.

/// Basis points in a whole.
const FULL_BP: u64 = 10_000;
/// Added before dividing by [`FULL_BP`] to round up.
const ROUND_UP: u64 = 9_999;

/// One block's counters, in session order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// First-attempt positions typed correctly.
    pub first_correct: u64,
    /// First-attempt positions.
    pub first_attempted: u64,
    /// Correct content insertions (attempt accuracy numerator).
    pub correct_attempts: u64,
    /// Content insertions (attempt accuracy denominator).
    pub content_attempts: u64,
    /// The session the block was typed in.
    pub session: u64,
}

/// Policy bounds, validated by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// A latest block strictly below this first-pass accuracy lowers.
    pub slow_down_below_bp: u64,
    /// Qualifying blocks needed to raise.
    pub qualifying_blocks: u64,
    /// Distinct sessions among them needed to raise.
    pub minimum_sessions: u64,
    /// First-attempt positions a block needs to qualify.
    pub minimum_positions: u64,
    /// Largest raise, as a share of the pace.
    pub raise_bp: u64,
    /// Largest lowering, as a share of the pace.
    pub lower_bp: u64,
}

/// The decision for one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// No suggested pace yet: nothing to adjust.
    NoBaseline,
    /// Keep the pace.
    Steady,
    /// The new, higher pace.
    Raise(u64),
    /// The new, lower pace.
    Lower(u64),
}

/// What the blocks so far say about raising.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Qualifying blocks.
    pub qualifying: u64,
    /// Qualifying blocks whose session differs from the previous
    /// qualifying block's: distinct sessions, as blocks come in order.
    pub sessions: u64,
    /// A qualifying block was seen.
    pub has_last: bool,
    /// The last qualifying block's session.
    pub last_session: u64,
    /// Attempt accuracy dropped from one block to a later one.
    pub degraded: bool,
    /// The last block with content insertions: its correct insertions.
    pub previous_correct: u64,
    /// The last block with content insertions: its insertions (0: none).
    pub previous_total: u64,
}

/// The empty tally.
pub const EMPTY: Tally = Tally {
    qualifying: 0,
    sessions: 0,
    has_last: false,
    last_session: 0,
    degraded: false,
    previous_correct: 0,
    previous_total: 0,
};

/// Whether `first_correct / first_attempted` is strictly below
/// `band_bp / 10000`, exactly; an empty block is never below.
#[must_use]
pub fn block_accuracy_below(first_correct: u64, first_attempted: u64, band_bp: u64) -> bool {
    if first_attempted == 0 {
        return false;
    }
    // Both products fit in u128 for u64 inputs.
    match u128::from(first_correct).checked_mul(u128::from(FULL_BP)) {
        None => false,
        Some(left) => match u128::from(band_bp).checked_mul(u128::from(first_attempted)) {
            None => false,
            Some(right) => left < right,
        },
    }
}

/// Whether `correct / total` is strictly below `previous_correct /
/// previous_total`, exactly.
#[must_use]
pub fn attempt_worse(correct: u64, total: u64, previous_correct: u64, previous_total: u64) -> bool {
    match u128::from(correct).checked_mul(u128::from(previous_total)) {
        None => false,
        Some(left) => match u128::from(previous_correct).checked_mul(u128::from(total)) {
            None => false,
            Some(right) => left < right,
        },
    }
}

/// A block qualifies for a raise: enough positions, and first-pass
/// accuracy at or above the desired accuracy.
#[must_use]
pub fn qualifies(block: Block, desired_bp: u64, minimum_positions: u64) -> bool {
    block.first_attempted >= minimum_positions
        && !block_accuracy_below(block.first_correct, block.first_attempted, desired_bp)
}

/// The tally after one more block.
#[must_use]
pub fn absorb(tally: Tally, block: Block, desired_bp: u64, minimum_positions: u64) -> Tally {
    let qualified = qualifies(block, desired_bp, minimum_positions);
    let new_session = qualified && (!tally.has_last || tally.last_session != block.session);
    let worse = tally.previous_total != 0
        && block.content_attempts != 0
        && attempt_worse(
            block.correct_attempts,
            block.content_attempts,
            tally.previous_correct,
            tally.previous_total,
        );
    let has_content = block.content_attempts != 0;
    Tally {
        qualifying: if qualified {
            tally.qualifying.saturating_add(1)
        } else {
            tally.qualifying
        },
        sessions: if new_session {
            tally.sessions.saturating_add(1)
        } else {
            tally.sessions
        },
        has_last: qualified || tally.has_last,
        last_session: if qualified {
            block.session
        } else {
            tally.last_session
        },
        degraded: tally.degraded || worse,
        previous_correct: if has_content {
            block.correct_attempts
        } else {
            tally.previous_correct
        },
        previous_total: if has_content {
            block.content_attempts
        } else {
            tally.previous_total
        },
    }
}

/// The tally over all blocks, in order.
#[must_use]
pub fn tally(blocks: &[Block], desired_bp: u64, minimum_positions: u64) -> Tally {
    let mut acc = EMPTY;
    let mut i = 0;
    while i < blocks.len() {
        if let Some(block) = blocks.get(i) {
            acc = absorb(acc, *block, desired_bp, minimum_positions);
        }
        i = i.saturating_add(1);
    }
    acc
}

/// `ceil(target * share_bp / 10000)`, if it fits.
#[must_use]
pub fn share(target: u64, share_bp: u64) -> Option<u64> {
    match target.checked_mul(share_bp) {
        None => None,
        Some(product) => match product.checked_add(ROUND_UP) {
            None => None,
            Some(rounded) => rounded.checked_div(FULL_BP),
        },
    }
}

/// The raised pace, or `Steady` when no raise fits.
#[must_use]
pub fn raise(target: u64, raise_bp: u64) -> Decision {
    let Some(delta) = share(target, raise_bp) else {
        return Decision::Steady;
    };
    if delta == 0 {
        return Decision::Steady;
    }
    match target.checked_add(delta) {
        None => Decision::Steady,
        Some(raised) => Decision::Raise(raised),
    }
}

/// The lowered pace, never below 1, or `Steady` when it cannot fall.
#[must_use]
pub fn lower(target: u64, lower_bp: u64) -> Decision {
    let Some(delta) = share(target, lower_bp) else {
        return Decision::Steady;
    };
    if delta == 0 {
        return Decision::Steady;
    }
    if delta < target {
        return match target.checked_sub(delta) {
            None => Decision::Steady,
            Some(lowered) => Decision::Lower(lowered),
        };
    }
    // The share reaches the whole pace: the floor is 1.
    if target > 1 {
        Decision::Lower(1)
    } else {
        Decision::Steady
    }
}

/// Whether the latest block is strictly below the slow-down band.
#[must_use]
pub fn latest_below(blocks: &[Block], band_bp: u64) -> bool {
    match blocks.len().checked_sub(1) {
        None => false,
        Some(last) => match blocks.get(last) {
            None => false,
            Some(block) => {
                block_accuracy_below(block.first_correct, block.first_attempted, band_bp)
            }
        },
    }
}

/// The pace decision for one track. `target` is the suggested pace in
/// correct units per minute (absent or 0: no baseline); `blocks` are in
/// session order, latest last.
#[must_use]
pub fn decide(target: Option<u64>, desired_bp: u64, blocks: &[Block], policy: Policy) -> Decision {
    let Some(current) = target else {
        return Decision::NoBaseline;
    };
    if current == 0 {
        return Decision::NoBaseline;
    }
    if latest_below(blocks, policy.slow_down_below_bp) {
        return lower(current, policy.lower_bp);
    }
    let seen = tally(blocks, desired_bp, policy.minimum_positions);
    if !seen.degraded
        && seen.qualifying >= policy.qualifying_blocks
        && seen.sessions >= policy.minimum_sessions
    {
        return raise(current, policy.raise_bp);
    }
    Decision::Steady
}
