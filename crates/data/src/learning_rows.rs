//! Learning evidence rows (plan 0003, #11): a chunked session's header is
//! written whole and its observation rows only ever gain the sums of newly
//! appended chunks, inside the checkpoint transaction; checked decoding on
//! read. Legacy sessions' rows are read as stored and never written.
use std::collections::{BTreeMap, BTreeSet};

use cadence_core::learning::{
    KeyLane, PaceMove, PaceRecord, PlanRecord, PlannerMode, ProbeReason, ProbeRecord,
    SessionLearning, Skill, SkillKey, SkillTally, WordTally,
};
use cadence_core::{PassageId, WordId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StoreError;
use crate::error::{read_integer, sql_integer};

/// The header tables: the plan, its focus, the pace records and the probe.
/// They are not observations; a checkpoint states them whole.
const HEADER_TABLES: [&str; 3] = ["pace_targets", "benchmark_probes", "plan_focus"];

/// Writes a chunked session's header (#11): the plan, focus, pace and
/// probe. The observation tables are never written here.
pub(crate) fn save_header(
    tx: &Transaction<'_>,
    session: i64,
    header: &SessionLearning,
) -> Result<(), StoreError> {
    for table in HEADER_TABLES {
        tx.prepare_cached(&format!("DELETE FROM {table} WHERE session_id=?1"))?
            .execute([session])?;
    }
    // `reinforced` is derived from the chunks: a new row starts at 0 and
    // the header never changes it.
    tx.prepare_cached(
        "INSERT INTO session_plans(session_id,seed,mode,reinforced) VALUES (?1,?2,?3,0)
         ON CONFLICT(session_id) DO UPDATE SET seed=excluded.seed, mode=excluded.mode",
    )?
    .execute(params![
        session,
        hex(header.plan.seed),
        header.plan.mode.key()
    ])?;
    let mut focus = tx.prepare_cached(
        "INSERT INTO plan_focus(session_id,rank,lane,kind,skill) VALUES (?1,?2,?3,?4,?5)",
    )?;
    for (rank, key) in header.plan.focus.iter().enumerate() {
        let rank = i64::try_from(rank)
            .map_err(|_unrepresentable| StoreError::IntegerRange { field: "rank" })?;
        focus.execute(params![
            session,
            rank,
            key.lane.key(),
            key.skill.kind_key(),
            key.skill.text()
        ])?;
    }
    let mut pace = tx.prepare_cached(
        "INSERT INTO pace_targets(session_id,track,target_cpm,decision) VALUES (?1,?2,?3,?4)",
    )?;
    for record in &header.pace {
        let target = record
            .target_cpm
            .map(|cpm| sql_integer(cpm, "pace target"))
            .transpose()?;
        pace.execute(params![
            session,
            crate::records::track_name(record.track),
            target,
            record.moved.key()
        ])?;
    }
    if let Some(probe) = header.probe {
        tx.prepare_cached(
            "INSERT INTO benchmark_probes(session_id,passage_id,strata,comparable,reason) \
             VALUES (?1,?2,?3,?4,?5)",
        )?
        .execute(params![
            session,
            hex(probe.passage.get()),
            format!("{:02x}", probe.strata),
            i64::from(probe.comparable),
            probe.reason.key()
        ])?;
    }
    Ok(())
}

/// Folds one newly appended chunk into the derived observation rows (#11):
/// sums added in place, never a delete and reinsert.
pub(crate) fn fold_chunk(
    tx: &Transaction<'_>,
    session: i64,
    chunk: &cadence_core::chunks::ObservationChunk,
) -> Result<(), StoreError> {
    let mut skills = tx.prepare_cached(
        "INSERT INTO skill_observations(session_id,lane,kind,skill,attempts,errors,timed,timed_micros)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(session_id,lane,kind,skill) DO UPDATE SET
            attempts=attempts+excluded.attempts, errors=errors+excluded.errors,
            timed=timed+excluded.timed, timed_micros=timed_micros+excluded.timed_micros",
    )?;
    for (key, tally) in &chunk.skills {
        skills.execute(params![
            session,
            key.lane.key(),
            key.skill.kind_key(),
            key.skill.text(),
            sql_integer(tally.attempts, "skill attempts")?,
            sql_integer(tally.errors, "skill errors")?,
            sql_integer(tally.timed, "skill timed")?,
            sql_integer(tally.timed_micros, "skill timed_micros")?
        ])?;
    }
    let mut words = tx.prepare_cached(
        "INSERT INTO word_outcomes(session_id,word_id,attempts,errors) VALUES (?1,?2,?3,?4)
         ON CONFLICT(session_id,word_id) DO UPDATE SET
            attempts=attempts+excluded.attempts, errors=errors+excluded.errors",
    )?;
    for (word, tally) in &chunk.words {
        words.execute(params![
            session,
            sql_integer(word.get(), "word_id")?,
            sql_integer(tally.attempts, "word attempts")?,
            sql_integer(tally.errors, "word errors")?
        ])?;
    }
    let mut passages = tx.prepare_cached(
        "INSERT INTO prose_seen(session_id,passage_id) VALUES (?1,?2) ON CONFLICT DO NOTHING",
    )?;
    for passage in &chunk.passages {
        passages.execute(params![session, hex(passage.get())])?;
    }
    tx.prepare_cached("UPDATE session_plans SET reinforced=reinforced+?2 WHERE session_id=?1")?
        .execute(params![
            session,
            sql_integer(chunk.reinforced, "reinforced")?
        ])?;
    Ok(())
}

/// Loads a session's learning evidence; `None` for sessions saved before
/// schema 3.
pub(crate) fn load(
    connection: &Connection,
    session: i64,
) -> Result<Option<SessionLearning>, StoreError> {
    let plan: Option<(String, String, i64)> = connection
        .prepare_cached("SELECT seed,mode,reinforced FROM session_plans WHERE session_id=?1")?
        .query_row([session], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .optional()?;
    let Some((seed, mode, reinforced)) = plan else {
        return Ok(None);
    };
    let mut focus = Vec::new();
    let mut statement = connection.prepare_cached(
        "SELECT lane,kind,skill FROM plan_focus WHERE session_id=?1 ORDER BY rank",
    )?;
    let mut rows = statement.query([session])?;
    while let Some(row) = rows.next()? {
        focus.push(skill_key(
            &row.get::<_, String>(0)?,
            &row.get::<_, String>(1)?,
            &row.get::<_, String>(2)?,
        )?);
    }
    let mut skills = BTreeMap::new();
    let mut statement = connection.prepare_cached(
        "SELECT lane,kind,skill,attempts,errors,timed,timed_micros
         FROM skill_observations WHERE session_id=?1",
    )?;
    let mut rows = statement.query([session])?;
    while let Some(row) = rows.next()? {
        let key = skill_key(
            &row.get::<_, String>(0)?,
            &row.get::<_, String>(1)?,
            &row.get::<_, String>(2)?,
        )?;
        let tally = SkillTally {
            attempts: read_integer(row.get(3)?, "skill attempts")?,
            errors: read_integer(row.get(4)?, "skill errors")?,
            timed: read_integer(row.get(5)?, "skill timed")?,
            timed_micros: read_integer(row.get(6)?, "skill timed_micros")?,
        };
        if skills.insert(key, tally).is_some() {
            return Err(corrupt("duplicate skill row"));
        }
    }
    let mut words = BTreeMap::new();
    let mut statement = connection
        .prepare_cached("SELECT word_id,attempts,errors FROM word_outcomes WHERE session_id=?1")?;
    let mut rows = statement.query([session])?;
    while let Some(row) = rows.next()? {
        let word = WordId::new(read_integer(row.get(0)?, "word_id")?)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        words.insert(
            word,
            WordTally {
                attempts: read_integer(row.get(1)?, "word attempts")?,
                errors: read_integer(row.get(2)?, "word errors")?,
            },
        );
    }
    let mut passages = BTreeSet::new();
    let mut statement =
        connection.prepare_cached("SELECT passage_id FROM prose_seen WHERE session_id=?1")?;
    let mut rows = statement.query([session])?;
    while let Some(row) = rows.next()? {
        let id = PassageId::new(unhex(&row.get::<_, String>(0)?)?)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        passages.insert(id);
    }
    let probe = connection
        .prepare_cached(
            "SELECT passage_id,strata,comparable,reason FROM benchmark_probes WHERE session_id=?1",
        )?
        .query_row([session], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .optional()?
        .map(|(passage, strata, comparable, reason)| {
            Ok::<_, StoreError>(ProbeRecord {
                passage: PassageId::new(unhex(&passage)?)
                    .map_err(|error| StoreError::Corrupt(error.to_string()))?,
                strata: u8::from_str_radix(&strata, 16)
                    .map_err(|error| corrupt(&format!("probe strata: {error}")))?,
                comparable: comparable != 0,
                reason: ProbeReason::parse(&reason).ok_or_else(|| corrupt("probe reason"))?,
            })
        })
        .transpose()?;
    let mut pace = Vec::new();
    let mut statement = connection.prepare_cached(
        "SELECT track,target_cpm,decision FROM pace_targets WHERE session_id=?1 ORDER BY rowid",
    )?;
    let mut rows = statement.query([session])?;
    while let Some(row) = rows.next()? {
        let target: Option<i64> = row.get(1)?;
        pace.push(PaceRecord {
            track: crate::records::parse_track(&row.get::<_, String>(0)?)?,
            target_cpm: target
                .map(|cpm| read_integer(cpm, "pace target"))
                .transpose()?,
            moved: PaceMove::parse(&row.get::<_, String>(2)?)
                .ok_or_else(|| corrupt("pace decision"))?,
        });
    }
    let learning = SessionLearning {
        plan: PlanRecord {
            seed: unhex(&seed)?,
            mode: PlannerMode::parse(&mode).ok_or_else(|| corrupt("unknown planner mode"))?,
            focus,
        },
        skills,
        words,
        passages,
        reinforced: read_integer(reinforced, "reinforced")?,
        probe,
        pace,
    };
    learning
        .validate()
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(Some(learning))
}

fn skill_key(lane: &str, kind: &str, skill: &str) -> Result<SkillKey, StoreError> {
    Ok(SkillKey {
        lane: KeyLane::parse(lane).ok_or_else(|| corrupt("unknown key lane"))?,
        skill: Skill::parse(kind, skill).map_err(|error| StoreError::Corrupt(error.to_string()))?,
    })
}

fn hex(value: u64) -> String {
    format!("{value:016x}")
}

fn unhex(text: &str) -> Result<u64, StoreError> {
    if text.len() != 16
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(corrupt("malformed 64-bit hex identity"));
    }
    u64::from_str_radix(text, 16)
        .map_err(|_unrepresentable| corrupt("malformed 64-bit hex identity"))
}

fn corrupt(message: &str) -> StoreError {
    StoreError::Corrupt(message.into())
}
