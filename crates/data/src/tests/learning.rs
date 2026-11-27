//! Schema 3 learning evidence: migration, round trip, history window and
//! regression refusal (plan 0003).
use cadence_core::learning::{
    KeyLane, PlanRecord, PlannerMode, SessionLearning, Skill, SkillKey, SkillTally, WordTally,
};
use cadence_core::progress::ProgressStatus;
use cadence_core::{Layout, PassageId, TrainingChar, WordId};
use rusqlite::{Connection, params};

use super::{TestResult, checkpoint, rekey};
use crate::{ProgressStore, SUPPORTED_SCHEMA, StoreError, paths::DATABASE_FILE};

fn learning(seed: u64, attempts: u64) -> Result<SessionLearning, Box<dyn std::error::Error>> {
    let unit = |c| TrainingChar::new(c, Layout::Us);
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
                super::chunked(&[learning(session, 10)?])?,
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
            evidence,
        )
    };
    store.apply_checkpoint(&with(1, super::chunked(&[learning(5, 10)?])?)?)?;
    // A lower watermark (fewer chunks) is a regression.
    let header =
        cadence_core::progress::Evidence::Chunked(cadence_core::progress::ChunkedEvidence {
            header: learning(5, 10)?.header(),
            through: 0,
            chain: cadence_core::chunks::GENESIS,
            chunks: Vec::new(),
        });
    assert!(matches!(
        store.apply_checkpoint(&with(2, header)?),
        Err(StoreError::Regression(_))
    ));
    // A changed plan is a regression.
    assert!(matches!(
        store.apply_checkpoint(&with(2, super::chunked(&[learning(6, 10)?])?)?),
        Err(StoreError::Regression(_))
    ));
    // Dropping the evidence is a regression.
    assert!(matches!(
        store.apply_checkpoint(&with(2, cadence_core::progress::Evidence::None)?),
        Err(StoreError::Regression(_))
    ));
    // A changed payload under a stored chunk is refused, and nothing moves.
    assert!(matches!(
        store.apply_checkpoint(&with(2, super::chunked(&[learning(5, 11)?])?)?),
        Err(StoreError::ChunkRefused(_))
    ));
    // Growth is a new chunk; the stored first one is a replay.
    store.apply_checkpoint(&with(
        2,
        super::chunked(&[learning(5, 10)?, learning(5, 11)?])?,
    )?)?;
    let totals = store
        .session_history()?
        .pop()
        .and_then(|session| session.learning);
    assert_eq!(totals, Some(learning(5, 11)?));
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

/// #46: a focus list with all three groups (letters, number-row digits,
/// numpad keys) round-trips through `plan_focus` in rank order.
#[test]
fn a_mixed_focus_list_round_trips() -> TestResult {
    let unit = |c| TrainingChar::new(c, Layout::Us);
    let focus = vec![
        SkillKey {
            lane: KeyLane::Main,
            skill: Skill::Bigram(unit('t')?, unit('h')?),
        },
        SkillKey {
            lane: KeyLane::Main,
            skill: Skill::Unit(unit('3')?),
        },
        SkillKey {
            lane: KeyLane::Numpad,
            skill: Skill::Bigram(unit('4')?, unit('8')?),
        },
        SkillKey {
            lane: KeyLane::Numpad,
            skill: Skill::Unit(unit('.')?),
        },
    ];
    let mut evidence = learning(1, 10)?;
    evidence.plan.focus.clone_from(&focus);
    let directory = tempfile::tempdir()?;
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        let base = checkpoint(1, 1, ProgressStatus::Completed)?;
        let saved = cadence_core::progress::Checkpoint::new(
            base.key.session,
            base.key.sequence,
            base.progress,
            super::chunked(&[evidence.clone()])?,
        )?;
        store.apply_checkpoint(&saved)?;
    }
    let store = ProgressStore::open_in(directory.path())?;
    let bootstrap = store.bootstrap(5)?;
    let loaded = bootstrap
        .learner
        .sessions
        .first()
        .map(|s| s.learning.plan.focus.clone());
    assert_eq!(loaded, Some(focus));
    Ok(())
}

/// #64: a probe record round-trips through `benchmark_probes` and the
/// bootstrap derives the shown passages, the last comparable strata and the
/// completed sessions with prose since the probe.
#[test]
fn a_probe_record_round_trips_and_feeds_the_probe_history() -> TestResult {
    use cadence_core::learning::{ProbeReason, ProbeRecord};
    use cadence_core::progress::{Track, TrackProgress};
    let directory = tempfile::tempdir()?;
    let probe = ProbeRecord {
        passage: PassageId::new(9_001)?,
        strata: 0b0101,
        comparable: true,
        reason: ProbeReason::Comparable,
    };
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        for session in 1..=4 {
            let base = checkpoint(session, 1, ProgressStatus::Completed)?;
            let mut progress = base.progress.clone();
            progress.active_micros = progress.active_micros.saturating_add(1_000_000);
            progress.tracks.push(TrackProgress {
                first_correct: 10,
                first_attempted: 10,
                correct_attempts: 10,
                content_attempts: 10,
                correct_units: 10,
                active_micros: 1_000_000,
                ..TrackProgress::empty(Track::Prose)
            });
            let keyed = rekey(&base, 1, progress)?;
            let mut evidence = learning(session, 10)?;
            if session == 2 {
                evidence.probe = Some(probe);
            }
            let with = cadence_core::progress::Checkpoint::new(
                keyed.key.session,
                keyed.key.sequence,
                keyed.progress,
                super::chunked(&[evidence])?,
            )?;
            store.apply_checkpoint(&with)?;
        }
    }
    let store = ProgressStore::open_in(directory.path())?;
    let bootstrap = store.bootstrap(10)?;
    assert_eq!(
        bootstrap
            .learner
            .sessions
            .iter()
            .find(|s| s.session.get() == 2)
            .and_then(|s| s.learning.probe),
        Some(probe)
    );
    assert!(bootstrap.probes.shown.contains(&probe.passage));
    assert_eq!(bootstrap.probes.last_comparable_strata, Some(0b0101));
    assert_eq!(
        bootstrap.probes.completed_with_prose_since, 2,
        "sessions 3 and 4"
    );
    Ok(())
}

/// #77: the first chosen layout adopts the history recorded before layouts,
/// once. Switching to another layout keeps the letter skills (the same
/// keys) and starts digit and symbol skills empty; its own sessions keep
/// theirs.
#[test]
fn a_new_layout_adopts_letters_once_and_starts_symbols_empty() -> TestResult {
    let directory = tempfile::tempdir()?;
    let digit = SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Unit(TrainingChar::new('1', Layout::Us)?),
    };
    let letters = SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Bigram(
            TrainingChar::new('t', Layout::Us)?,
            TrainingChar::new('h', Layout::Us)?,
        ),
    };
    let save = |store: &mut ProgressStore, session: u64, layout| -> TestResult {
        let base = checkpoint(session, 1, ProgressStatus::Completed)?;
        let mut progress = base.progress.clone();
        progress.layout = layout;
        let mut evidence = learning(session, 10)?;
        evidence.skills.insert(
            digit,
            SkillTally {
                attempts: 4,
                errors: 2,
                timed: 0,
                timed_micros: 0,
            },
        );
        store.apply_checkpoint(&cadence_core::progress::Checkpoint::new(
            base.key.session,
            base.key.sequence,
            progress,
            super::chunked(&[evidence])?,
        )?)?;
        Ok(())
    };
    let mut store = ProgressStore::open_in(directory.path())?;
    save(&mut store, 1, None)?;
    store.adopt_unspecified(Layout::Us, cadence_core::SessionId::new(2)?)?;
    // A second adoption changes nothing: the first chosen layout keeps it.
    store.adopt_unspecified(Layout::Uk, cadence_core::SessionId::new(2)?)?;
    assert_eq!(store.adopter()?, Some(Layout::Us));
    fn skills(
        store: &ProgressStore,
        layout: Layout,
    ) -> Result<Vec<SkillKey>, Box<dyn std::error::Error>> {
        let bootstrap = store.bootstrap_for(60, Some(layout))?;
        Ok(bootstrap
            .learner
            .sessions
            .iter()
            .flat_map(|s| s.learning.skills.keys().copied())
            .collect())
    }
    let us = skills(&store, Layout::Us)?;
    assert!(us.contains(&digit) && us.contains(&letters));
    let uk = skills(&store, Layout::Uk)?;
    assert!(uk.contains(&letters), "letters history adopted");
    assert!(!uk.contains(&digit), "number-row skills start empty on UK");
    let recaps = store.bootstrap_for(60, Some(Layout::Uk))?.history.recent;
    assert_eq!(recaps.first().and_then(|r| r.layout), Some(Layout::Us));
    // A UK session's own digit skills are UK history.
    save(&mut store, 2, Some(Layout::Uk))?;
    assert!(skills(&store, Layout::Uk)?.contains(&digit));
    let audit: i64 = store.connection.query_row(
        "SELECT COUNT(*) FROM settings_audit WHERE key='layout.adopted'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(audit, 1);
    Ok(())
}

/// #79: score-affecting settings are audited once per change, at the
/// session they first apply to; relaunching before that session is saved
/// updates the same row, and changing a value back removes it.
#[test]
fn settings_changes_are_audited_once_at_the_session_they_apply_to() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let at = |n| cadence_core::SessionId::new(n);
    let defaults = [("session.minutes", "15".to_string())];
    assert_eq!(
        store.audit_settings(at(1)?, &defaults)?,
        0,
        "defaults are not changes"
    );
    assert_eq!(
        store.audit_settings(at(1)?, &[("session.minutes", "20".into())])?,
        1
    );
    // Relaunch before session 1 is saved, with another value.
    assert_eq!(
        store.audit_settings(at(1)?, &[("session.minutes", "30".into())])?,
        1
    );
    let row: (String, String) = store.connection.query_row(
        "SELECT old, new FROM settings_audit WHERE at_session=1 AND key='session.minutes'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(row, ("15".into(), "30".into()));
    // Unchanged at the next session: nothing more.
    assert_eq!(
        store.audit_settings(at(2)?, &[("session.minutes", "30".into())])?,
        0
    );
    // Changed back before session 2: no row.
    assert_eq!(
        store.audit_settings(at(1)?, &[("session.minutes", "15".into())])?,
        0
    );
    Ok(())
}

/// #81: the history query is bounded, oldest first, and keeps synthetic
/// sessions visible (flagged) so Progress can stop a series at them.
#[test]
fn history_points_are_bounded_and_flag_synthetic_sessions() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    for session in 1..=5 {
        store.apply_checkpoint(&checkpoint(session, 1, ProgressStatus::Completed)?)?;
    }
    store.mark_synthetic(cadence_core::SessionId::new(4)?)?;
    let points = store.history_points(3)?;
    let ids: Vec<u64> = points.iter().map(|p| p.session.get()).collect();
    assert_eq!(ids, vec![3, 4, 5], "the newest three, oldest first");
    let synthetic: Vec<bool> = points.iter().map(|p| p.synthetic).collect();
    assert_eq!(synthetic, vec![false, true, false]);
    assert!(
        points
            .iter()
            .all(|p| p.tracks.first().is_some_and(|t| t.first_attempted == 4))
    );
    Ok(())
}

/// #85: a pack session (imported text or code) never enters the learner
/// model, the review ladder or probe due-ness, and stays readable without
/// its pack's files: the store keeps its class and digest, not the text.
#[test]
fn pack_sessions_feed_nothing_and_stay_readable() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let mut base = checkpoint(1, 1, ProgressStatus::Completed)?;
    base.progress.tracks[0].track = cadence_core::progress::Track::Prose;
    store.apply_checkpoint(&cadence_core::progress::Checkpoint::new(
        base.key.session,
        base.key.sequence,
        base.progress.clone(),
        super::chunked(&[learning(1, 10)?])?,
    )?)?;
    let before = store.bootstrap(60)?;
    let mut pack = base.progress.clone();
    pack.content_class = cadence_core::progress::ContentClass::Imported;
    pack.pack_digest = Some("ab".repeat(32));
    store.apply_checkpoint(&cadence_core::progress::Checkpoint::new(
        cadence_core::SessionId::new(2)?,
        cadence_core::CommitSeq::from_raw(1),
        pack,
        super::chunked(&[learning(2, 40)?])?,
    )?)?;
    let after = store.bootstrap(60)?;
    // Learner model and ladder (word outcomes live in learner sessions).
    assert_eq!(after.learner.sessions, before.learner.sessions);
    // Probe due-ness counts completed standard sessions with prose only.
    assert_eq!(after.probes, before.probes);
    assert_eq!(after.pace_targets, before.pace_targets);
    // The recap is kept, with its class, for comparisons to separate.
    assert_eq!(
        after.history.recent.first().map(|r| r.content_class),
        Some(cadence_core::progress::ContentClass::Imported)
    );
    // No pack file is ever read back: the history is whole without it.
    let stored = store.session_history()?;
    assert_eq!(
        stored
            .iter()
            .find(|s| s.checkpoint.key.session.get() == 2)
            .and_then(|s| s.checkpoint.progress.pack_digest.clone()),
        Some("ab".repeat(32))
    );
    Ok(())
}

/// #11: a session saved before schema 7 is `legacy`: it reads back with its
/// original digest and totals, and is never written again; the chunk table
/// refuses any update or delete.
#[test]
fn legacy_sessions_stay_readable_and_chunks_are_append_only() -> TestResult {
    let directory = tempfile::tempdir()?;
    let base = checkpoint(1, 1, ProgressStatus::Running)?;
    let legacy = cadence_core::progress::Checkpoint::new(
        base.key.session,
        base.key.sequence,
        base.progress.clone(),
        cadence_core::progress::Evidence::Legacy(learning(1, 10)?),
    )?;
    {
        // A schema-6 file: the legacy rows written as beta.3 wrote them.
        let store = ProgressStore::open_in(directory.path())?;
        store.connection.execute_batch(
            "DROP TABLE derived_state; DROP TABLE observation_chunks;
             UPDATE schema_info SET version=6 WHERE id=1;",
        )?;
    }
    // The legacy rows, as beta.3 wrote them.
    {
        let connection = Connection::open(directory.path().join(DATABASE_FILE))?;
        let progress = &legacy.progress;
        connection.execute(
            "INSERT INTO profiles(name) VALUES (?1)",
            [&progress.profile],
        )?;
        connection.execute(
            "INSERT INTO sessions(session_id,profile,day,policy_digest,corpus_digest,status,active_micros,commit_seq)
             VALUES (1,?1,?2,?3,?4,'running',?5,1)",
            params![progress.profile, progress.day, progress.policy_digest, progress.corpus_digest,
                i64::try_from(progress.active_micros)?],
        )?;
        connection.execute(
            "INSERT INTO session_tracks(session_id,track,first_correct,first_attempted,correct_attempts,
                content_attempts,correct_units,deletions,overflow_attempts,correct_digits,active_micros)
             VALUES (1,'number_row',3,4,5,8,3,2,1,2,900000)",
            [],
        )?;
        let evidence = learning(1, 10)?;
        let tx = rusqlite::Transaction::new_unchecked(
            &connection,
            rusqlite::TransactionBehavior::Deferred,
        )?;
        crate::learning_rows::save_header(&tx, 1, &evidence)?;
        tx.execute(
            "INSERT INTO skill_observations(session_id,lane,kind,skill,attempts,errors,timed,timed_micros)
             VALUES (1,'main','bigram','th',10,1,1,180000)",
            [],
        )?;
        tx.execute("INSERT INTO word_outcomes VALUES (1,42,1,1)", [])?;
        tx.execute(
            "INSERT INTO prose_seen VALUES (1,?1)",
            [format!("{:016x}", u64::MAX)],
        )?;
        tx.execute(
            "UPDATE session_plans SET reinforced=1 WHERE session_id=1",
            [],
        )?;
        tx.execute(
            "INSERT INTO commit_receipts VALUES (1,1,?1,?2)",
            params![legacy.key.digest.to_hex(), progress.day],
        )?;
        tx.commit()?;
    }
    let mut store = ProgressStore::open_in(directory.path())?;
    let stored = store.session_history()?.pop().ok_or("stored session")?;
    assert_eq!(stored.checkpoint, legacy, "the original digest reads back");
    assert_eq!(stored.learning, Some(learning(1, 10)?));
    // Legacy evidence is never written again.
    let next = rekey(&legacy, 2, legacy.progress.clone())?;
    assert!(store.apply_checkpoint(&next).is_err());
    // Chunked sessions append; their chunks refuse UPDATE and DELETE.
    let chunked = cadence_core::progress::Checkpoint::new(
        cadence_core::SessionId::new(2)?,
        cadence_core::CommitSeq::from_raw(1),
        base.progress.clone(),
        super::chunked(&[learning(2, 3)?, learning(2, 5)?])?,
    )?;
    store.apply_checkpoint(&chunked)?;
    assert_eq!(
        store
            .session_chunks(cadence_core::SessionId::new(2)?)?
            .len(),
        2
    );
    for sql in [
        "UPDATE observation_chunks SET payload=x'00' WHERE session_id=2",
        "DELETE FROM observation_chunks WHERE session_id=2",
    ] {
        assert!(store.connection.execute(sql, []).is_err(), "{sql}");
    }
    let reread = store
        .session_history()?
        .into_iter()
        .find(|s| s.checkpoint.key.session.get() == 2)
        .ok_or("chunked session")?;
    assert_eq!(
        reread.checkpoint.key, chunked.key,
        "identity from stored chunks"
    );
    assert_eq!(reread.learning, Some(learning(2, 5)?));
    Ok(())
}
