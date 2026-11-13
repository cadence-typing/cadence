use super::*;
fn progress() -> SessionProgress {
    SessionProgress {
        profile: "default".to_string(),
        day: 1,
        policy_digest: "policy".to_string(),
        corpus_digest: "corpus".to_string(),
        status: ProgressStatus::Running,
        tracks: vec![
            TrackProgress::empty(Track::Words),
            TrackProgress::empty(Track::Numpad),
        ],
        active_micros: 0,
    }
}
#[test]
fn digest_is_order_canonical_and_field_sensitive() -> Result<(), CoreError> {
    let session = SessionId::new(1)?;
    let original = Checkpoint::new(session, CommitSeq::INITIAL, progress(), None)?;
    let mut reversed = progress();
    reversed.tracks.reverse();
    assert_eq!(
        Checkpoint::new(session, CommitSeq::INITIAL, reversed, None)?.key,
        original.key
    );
    let mut changed = progress();
    changed.status = ProgressStatus::Abandoned;
    assert_ne!(
        Checkpoint::new(session, CommitSeq::INITIAL, changed, None)?
            .key
            .digest,
        original.key.digest
    );
    let mut changed = progress();
    changed.profile = "other".to_string();
    assert_ne!(
        Checkpoint::new(session, CommitSeq::INITIAL, changed, None)?
            .key
            .digest,
        original.key.digest
    );
    Ok(())
}
#[test]
fn invalid_counts_duplicates_and_duration_mismatch_are_refused() {
    let mut value = progress();
    value.tracks.push(TrackProgress::empty(Track::Words));
    assert!(value.validate().is_err());
    let mut value = progress();
    value.active_micros = 1;
    assert!(value.validate().is_err());
    let mut value = progress();
    if let Some(track) = value.tracks.first_mut() {
        track.correct_attempts = 1;
    }
    assert!(value.validate().is_err());
}
#[test]
fn track_accumulation_exhaustion_is_atomic() {
    let mut value = TrackProgress::empty(Track::Words);
    value.correct_units = u64::MAX;
    let before = value.clone();
    let mut other = TrackProgress::empty(Track::Words);
    other.first_attempted = 1;
    other.correct_units = 1;
    assert!(value.add(&other).is_err());
    assert_eq!(value, before);
}
#[test]
fn length_prefix_prevents_identity_concatenation_collision() -> Result<(), CoreError> {
    let mut first = progress();
    first.profile = "ab".to_string();
    first.policy_digest = "c".to_string();
    let mut second = progress();
    second.profile = "a".to_string();
    second.policy_digest = "bc".to_string();
    let session = SessionId::new(1)?;
    assert_ne!(
        Checkpoint::new(session, CommitSeq::INITIAL, first, None)?
            .key
            .digest,
        Checkpoint::new(session, CommitSeq::INITIAL, second, None)?
            .key
            .digest
    );
    Ok(())
}
