//! `pace_decision` (#61): the alpha.6 `trainer::pace` cases restated with
//! pace values, plus each equality boundary.
use super::*;
use cadence_core::pace::Decision;
use cadence_core::progress::{SessionRecap, Track, TrackProgress};

fn policy() -> crate::policy::PacePolicy {
    crate::policy::parse_policy(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../policies/trainer-v3.toml"
        ))
        .unwrap()
        .as_slice(),
    )
    .unwrap()
    .pace
}

/// One session that typed `words` with these first-pass counts.
fn session(positions: u64, correct: u64) -> SessionRecap {
    session_with(positions, correct, positions)
}

fn session_with(positions: u64, correct: u64, content: u64) -> SessionRecap {
    SessionRecap {
        day: 1,
        completed: true,
        layout: None,
        content_class: cadence_core::progress::ContentClass::Standard,
        tracks: vec![TrackProgress {
            first_attempted: positions,
            first_correct: correct,
            content_attempts: content,
            correct_attempts: correct,
            correct_units: correct,
            active_micros: 60_000_000,
            ..TrackProgress::empty(Track::Words)
        }],
    }
}

/// Newest first, as stored.
fn decide(target: Option<u64>, oldest_first: &[SessionRecap]) -> Decision {
    let recent: Vec<SessionRecap> = oldest_first.iter().rev().cloned().collect();
    pace_decision(&policy(), Track::Words, target, &recent).unwrap()
}

#[test]
fn no_baseline_is_explicit() {
    // Was: `None` target with one perfect block → `NoBaseline`. Unchanged.
    assert_eq!(decide(None, &[session(200, 200)]), Decision::NoBaseline);
    assert_eq!(decide(Some(0), &[session(200, 200)]), Decision::NoBaseline);
}

#[test]
fn strong_blocks_across_sessions_raise_within_cap() {
    // Was: a 9,800 bp *accuracy* target raised to 10,000 (the defect). Now
    // a pace of 40 rises by ceil(40 * 3%) = 2. The old case had three
    // blocks across two sessions; with one block per stored session it is
    // three sessions.
    let blocks = [session(120, 119), session(110, 110), session(105, 105)];
    assert_eq!(decide(Some(40), &blocks), Decision::Raise(42));
}

#[test]
fn failing_block_lowers_within_cap() {
    // Was: 9,800 → 9,300 bp. Now a pace of 40 falls by ceil(40 * 5%) = 2.
    assert_eq!(decide(Some(40), &[session(150, 135)]), Decision::Lower(38));
}

#[test]
fn insufficient_evidence_stays_steady() {
    // Below the 100-position floor: cannot qualify. Unchanged.
    assert_eq!(
        decide(Some(40), &[session(40, 40), session(30, 30)]),
        Decision::Steady
    );
}

#[test]
fn one_session_never_raises() {
    // Was: three blocks in one session. A stored session has one block per
    // track, so one session is one block: never three, never two sessions.
    assert_eq!(decide(Some(40), &[session(120, 120)]), Decision::Steady);
}

#[test]
fn attempt_degradation_blocks_raise() {
    // Unchanged: attempt accuracy collapses in a later block.
    let blocks = [
        session(120, 120),
        session_with(110, 110, 220),
        session(105, 105),
    ];
    assert_eq!(decide(Some(40), &blocks), Decision::Steady);
}

#[test]
fn exactly_the_desired_accuracy_qualifies() {
    // 98.00% exactly qualifies; 97.99% does not.
    let at = [
        session(10_000, 9_800),
        session(10_000, 9_800),
        session(10_000, 9_800),
    ];
    assert_eq!(decide(Some(40), &at), Decision::Raise(42));
    let under = [
        session(10_000, 9_800),
        session(10_000, 9_799),
        session(10_000, 9_800),
    ];
    assert_eq!(decide(Some(40), &under), Decision::Steady);
}

#[test]
fn exactly_the_slow_down_band_does_not_lower() {
    // 95.00% exactly stays; 94.99% lowers.
    assert_eq!(
        decide(Some(40), &[session(10_000, 9_500)]),
        Decision::Steady
    );
    assert_eq!(
        decide(Some(40), &[session(10_000, 9_499)]),
        Decision::Lower(38)
    );
}

#[test]
fn exactly_the_position_floor_qualifies() {
    // 100 positions exactly qualify; 99 do not.
    let at = [session(100, 100), session(100, 100), session(100, 100)];
    assert_eq!(decide(Some(40), &at), Decision::Raise(42));
    let under = [session(100, 100), session(99, 99), session(100, 100)];
    assert_eq!(decide(Some(40), &under), Decision::Steady);
}

#[test]
fn exactly_three_blocks_in_two_sessions_raise() {
    // Two stored tracks of the same kind in one session are two blocks of
    // that session: 3 blocks from 2 sessions raise; 2 blocks do not.
    let mut doubled = session(120, 120);
    let again = doubled.tracks[0].clone();
    doubled.tracks.push(again);
    assert_eq!(
        decide(Some(40), &[doubled.clone(), session(120, 120)]),
        Decision::Raise(42)
    );
    assert_eq!(
        decide(Some(40), &[session(120, 120), session(120, 120)]),
        Decision::Steady
    );
    // Three blocks, one session: never.
    let mut tripled = doubled;
    let third = tripled.tracks[0].clone();
    tripled.tracks.push(third);
    assert_eq!(decide(Some(40), &[tripled]), Decision::Steady);
}

#[test]
fn a_pace_of_one_is_the_floor() {
    assert_eq!(decide(Some(1), &[session(150, 135)]), Decision::Steady);
    assert_eq!(decide(Some(2), &[session(150, 135)]), Decision::Lower(1));
}
