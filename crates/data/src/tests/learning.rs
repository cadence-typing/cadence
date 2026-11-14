//! Schema 3 learning evidence: migration, round trip, history window and
//! regression refusal (plan 0003).
use cadence_core::learning::{
    KeyLane, PlanRecord, PlannerMode, SessionLearning, Skill, SkillKey, SkillTally, WordTally,
};
use cadence_core::progress::ProgressStatus;
use cadence_core::{LayoutSupportsPound, PassageId, TrainingChar, WordId};
use rusqlite::{Connection, params};

use super::{TestResult, checkpoint, rekey};
use crate::{ProgressStore, SUPPORTED_SCHEMA, StoreError, paths::DATABASE_FILE};

fn learning(seed: u64, attempts: u64) -> Result<SessionLearning, Box<dyn std::error::Error>> {
    let unit = |c| TrainingChar::new(c, LayoutSupportsPound::US);
    let key = SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Bigram(unit('t')?, unit('h')?),
    };
    let mut evidence = SessionLearning::new(PlanRecord {
        seed,
        mode: PlannerMode::Adaptive,
        focus: vec![key],
    });
    evidence.skills.insert(
        key,
        SkillTally {
            attempts,
            errors: 1,
            timed: 1,
            timed_micros: 180_000,
        },
    );
    evidence.words.insert(
        WordId::new(42)?,
        WordTally {
            attempts: 1,
            errors: 1,
        },
    );
    evidence.passages.insert(PassageId::new(u64::MAX)?);
    evidence.reinforced = 1;
    Ok(evidence)
}

#[test]
fn populated_schema_two_migrates_with_receipts_intact_and_no_invented_evidence() -> TestResult {
    let directory = tempfile::tempdir()?;
    let saved = checkpoint(1, 3, ProgressStatus::Completed)?;
    {
        let connection = Connection::open(directory.path().join(DATABASE_FILE))?;
        connection.execute_batch(crate::schema::MIGRATION_V1)?;
        connection.execute_batch(crate::schema::MIGRATION_V2)?;
        let progress = &saved.progress;
        let track = progress.tracks.first().ok_or("fixture track")?;
        connection.execute(
            "INSERT INTO profiles(name) VALUES (?1)",
            [&progress.profile],
        )?;
        connection.execute(
            "INSERT INTO sessions VALUES (1,?1,?2,?3,?4,'completed',?5,3)",
            params![
                progress.profile,
                progress.day,
                progress.policy_digest,
                progress.corpus_digest,
                i64::try_from(progress.active_micros)?
            ],
        )?;
        connection.execute(
            "INSERT INTO session_tracks(session_id,track,first_correct,first_attempted,correct_attempts,
                content_attempts,correct_units,deletions,overflow_attempts,correct_digits,active_micros)
             VALUES (1,'number_row',3,4,5,8,3,2,1,2,?1)",
            [i64::try_from(track.active_micros)?],
        )?;
        connection.execute(
            "INSERT INTO commit_receipts VALUES (1,3,?1,?2)",
            params![saved.key.digest.to_hex(), progress.day],
        )?;
    }
    let store = ProgressStore::open_in(directory.path())?;
    let version: i64 =
        store
            .connection
            .query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, SUPPORTED_SCHEMA);
    let history = store.session_history()?;
    assert_eq!(history.len(), 1);
    assert_eq!(history.first().map(|s| &s.checkpoint), Some(&saved));
    let bootstrap = store.bootstrap(10)?;
    assert!(bootstrap.learner.sessions.is_empty());
    assert_eq!(bootstrap.history.completed_sessions, 1);
    Ok(())
}

#[test]
fn learning_round_trips_and_history_returns_the_newest_window() -> TestResult {
    let directory = tempfile::tempdir()?;
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        for session in 1..=3 {
            let base = checkpoint(session, 1, ProgressStatus::Completed)?;
            let with = rekey(&base, 1, base.progress.clone())?;
            let with = cadence_core::progress::Checkpoint::new(
                with.key.session,
                with.key.sequence,
                with.progress,
                Some(learning(session, 10)?),
            )?;
            store.apply_checkpoint(&with)?;
        }
        // A pre-schema-3 style session without evidence is skipped.
        store.apply_checkpoint(&checkpoint(4, 1, ProgressStatus::Completed)?)?;
    }
    let store = ProgressStore::open_in(directory.path())?;
    let bootstrap = store.bootstrap(2)?;
    let sessions: Vec<u64> = bootstrap
        .learner
        .sessions
        .iter()
        .map(|s| s.session.get())
        .collect();
    assert_eq!(sessions, vec![2, 3]);
    assert_eq!(
        bootstrap.learner.sessions.last().map(|s| &s.learning),
        Some(&learning(3, 10)?)
    );
    assert_eq!(bootstrap.next_session.get(), 5);
    Ok(())
}

#[test]
fn decreasing_or_replanned_learning_is_a_regression() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let base = checkpoint(1, 1, ProgressStatus::Running)?;
    let with = |sequence, evidence| {
        cadence_core::progress::Checkpoint::new(
            base.key.session,
            cadence_core::CommitSeq::from_raw(sequence),
            base.progress.clone(),
            Some(evidence),
        )
    };
    store.apply_checkpoint(&with(1, learning(5, 10)?)?)?;
    assert!(matches!(
        store.apply_checkpoint(&with(2, learning(5, 9)?)?),
        Err(StoreError::Regression(_))
    ));
    assert!(matches!(
        store.apply_checkpoint(&with(2, learning(6, 10)?)?),
        Err(StoreError::Regression(_))
    ));
    let dropped = cadence_core::progress::Checkpoint::new(
        base.key.session,
        cadence_core::CommitSeq::from_raw(2),
        base.progress.clone(),
        None,
    )?;
    assert!(matches!(
        store.apply_checkpoint(&dropped),
        Err(StoreError::Regression(_))
    ));
    store.apply_checkpoint(&with(2, learning(5, 11)?)?)?;
    Ok(())
}

#[test]
fn recent_history_is_newest_first_with_status_and_skips_untyped_sessions() -> TestResult {
    let directory = tempfile::tempdir()?;
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        store.apply_checkpoint(&checkpoint(1, 1, ProgressStatus::Completed)?)?;
        store.apply_checkpoint(&checkpoint(2, 1, ProgressStatus::Abandoned)?)?;
        // A session that never recorded a first attempt has nothing to compare.
        let mut empty = checkpoint(3, 1, ProgressStatus::Abandoned)?;
        for track in &mut empty.progress.tracks {
            *track = cadence_core::progress::TrackProgress {
                active_micros: track.active_micros,
                ..cadence_core::progress::TrackProgress::empty(track.track)
            };
        }
        let empty = rekey(&empty, 1, empty.progress.clone())?;
        store.apply_checkpoint(&empty)?;
    }
    let store = ProgressStore::open_in(directory.path())?;
    let recent = store.bootstrap(2)?.history.recent;
    let seen: Vec<bool> = recent.iter().map(|recap| recap.completed).collect();
    assert_eq!(seen, vec![false, true]);
    assert!(
        recent
            .iter()
            .all(|recap| recap.day == 20_000 && recap.tracks.len() == 1)
    );
    Ok(())
}
