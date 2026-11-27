//! Checked row decoding and canonical checkpoint integrity.

use cadence_core::learning::SessionLearning;
use cadence_core::progress::{
    Checkpoint, ChunkedEvidence, Evidence, ProgressStatus, SessionProgress, Track, TrackProgress,
};
use cadence_core::{CommitSeq, SessionId};
use rusqlite::{Connection, params};

use crate::StoreError;
use crate::error::{read_integer, sql_integer};

/// Complete latest durable checkpoint for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    /// Canonical latest checkpoint, including all exact track numerators.
    pub checkpoint: Checkpoint,
    /// The session's learning totals: a legacy session's stored snapshot,
    /// or a chunked session's derived rows (the fold of its chunks).
    pub learning: Option<SessionLearning>,
}

pub(crate) fn status_name(status: ProgressStatus) -> &'static str {
    match status {
        ProgressStatus::Running => "running",
        ProgressStatus::Completed => "completed",
        ProgressStatus::Abandoned => "abandoned",
    }
}

fn parse_status(status: &str) -> Result<ProgressStatus, StoreError> {
    match status {
        "running" => Ok(ProgressStatus::Running),
        "completed" => Ok(ProgressStatus::Completed),
        "abandoned" => Ok(ProgressStatus::Abandoned),
        _ => Err(StoreError::Corrupt(format!("unknown status {status:?}"))),
    }
}

pub(crate) fn track_name(track: Track) -> &'static str {
    match track {
        Track::WarmUp => "warm_up",
        Track::Words => "words",
        Track::NumberRow => "number_row",
        Track::Numpad => "numpad",
        Track::Prose => "prose",
        Track::Review => "review",
        Track::Benchmark => "benchmark",
    }
}

pub(crate) fn parse_track(track: &str) -> Result<Track, StoreError> {
    match track {
        "warm_up" => Ok(Track::WarmUp),
        "words" => Ok(Track::Words),
        "number_row" => Ok(Track::NumberRow),
        "numpad" => Ok(Track::Numpad),
        "prose" => Ok(Track::Prose),
        "review" => Ok(Track::Review),
        "benchmark" => Ok(Track::Benchmark),
        _ => Err(StoreError::Corrupt(format!("unknown track {track:?}"))),
    }
}

pub(crate) fn load_session(
    connection: &Connection,
    session: SessionId,
) -> Result<Option<StoredSession>, StoreError> {
    let id = sql_integer(session.get(), "session_id")?;
    let mut statement = connection.prepare(
        "SELECT profile,day,policy_digest,corpus_digest,status,active_micros,commit_seq,utc_offset_minutes,
            layout,content_class,pack_digest
         FROM sessions WHERE session_id=?1",
    )?;
    let mut rows = statement.query([id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let mut progress = SessionProgress {
        profile: row.get(0)?,
        day: row.get(1)?,
        policy_digest: row.get(2)?,
        corpus_digest: row.get(3)?,
        status: parse_status(&row.get::<_, String>(4)?)?,
        active_micros: read_integer(row.get(5)?, "active_micros")?,
        tracks: Vec::new(),
        utc_offset_minutes: row.get(7)?,
        layout: match row.get::<_, String>(8)?.as_str() {
            "unspecified" => None,
            other => Some(
                cadence_core::Layout::parse(other)
                    .ok_or_else(|| StoreError::Corrupt("unknown session layout".into()))?,
            ),
        },
        content_class: cadence_core::progress::ContentClass::parse(&row.get::<_, String>(9)?)
            .ok_or_else(|| StoreError::Corrupt("unknown content class".into()))?,
        pack_digest: row.get(10)?,
    };
    let sequence = CommitSeq::from_raw(read_integer(row.get(6)?, "commit_seq")?);
    let mut statement = connection.prepare(
        "SELECT track,first_correct,first_attempted,correct_attempts,content_attempts,
            deletions,overflow_attempts,correct_units,correct_digits,active_micros
         FROM session_tracks WHERE session_id=?1 ORDER BY track",
    )?;
    let mut rows = statement.query([id])?;
    while let Some(row) = rows.next()? {
        progress.tracks.push(TrackProgress {
            track: parse_track(&row.get::<_, String>(0)?)?,
            first_correct: read_integer(row.get(1)?, "first_correct")?,
            first_attempted: read_integer(row.get(2)?, "first_attempted")?,
            correct_attempts: read_integer(row.get(3)?, "correct_attempts")?,
            content_attempts: read_integer(row.get(4)?, "content_attempts")?,
            deletions: read_integer(row.get(5)?, "deletions")?,
            overflow_attempts: read_integer(row.get(6)?, "overflow_attempts")?,
            correct_units: read_integer(row.get(7)?, "correct_units")?,
            correct_digits: read_integer(row.get(8)?, "correct_digits")?,
            active_micros: read_integer(row.get(9)?, "track active_micros")?,
        });
    }
    progress.tracks.sort_by_key(|track| track.track);
    let learning = crate::learning_rows::load(connection, id)?;
    let evidence = match (&learning, crate::chunk_rows::derived_state(connection, id)?) {
        (None, _) => Evidence::None,
        (Some(totals), Some((through, false))) => Evidence::Chunked(ChunkedEvidence {
            header: totals.header(),
            through,
            chain: crate::chunk_rows::chain_of(connection, id, through)?,
            chunks: Vec::new(),
        }),
        (Some(totals), Some((_, true)) | None) => Evidence::Legacy(totals.clone()),
    };
    let checkpoint = Checkpoint::new(session, sequence, progress, evidence)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let digest: String = connection.query_row(
        "SELECT digest FROM commit_receipts WHERE session_id=?1 AND commit_seq=?2",
        params![id, sql_integer(sequence.get(), "commit_seq")?],
        |row| row.get(0),
    )?;
    if digest != checkpoint.key.digest.to_hex() {
        return Err(StoreError::Corrupt(
            "latest session differs from its durable digest".into(),
        ));
    }
    Ok(Some(StoredSession {
        checkpoint,
        learning,
    }))
}

pub(crate) fn load_all(connection: &Connection) -> Result<Vec<StoredSession>, StoreError> {
    let mut statement =
        connection.prepare("SELECT session_id FROM sessions ORDER BY day,session_id")?;
    let mut rows = statement.query([])?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next()? {
        let id = SessionId::new(read_integer(row.get(0)?, "session_id")?)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        sessions.push(
            load_session(connection, id)?
                .ok_or_else(|| StoreError::Corrupt("session disappeared during read".into()))?,
        );
    }
    Ok(sessions)
}

pub(crate) fn validate_database(connection: &Connection) -> Result<Vec<StoredSession>, StoreError> {
    let sessions = load_all(connection)?;
    let mut statement = connection.prepare(
        "SELECT r.commit_seq,r.digest,r.created_at_epoch,s.commit_seq,s.day
         FROM commit_receipts r JOIN sessions s ON s.session_id=r.session_id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let sequence = read_integer(row.get(0)?, "receipt sequence")?;
        let digest: String = row.get(1)?;
        let day: i64 = row.get(2)?;
        let latest = read_integer(row.get(3)?, "latest sequence")?;
        let session_day: i64 = row.get(4)?;
        if sequence > latest
            || day != session_day
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StoreError::Corrupt("malformed commit receipt".into()));
        }
    }
    Ok(sessions)
}

pub(crate) fn validate_checkpoint(checkpoint: &Checkpoint) -> Result<(), StoreError> {
    let canonical = Checkpoint::new(
        checkpoint.key.session,
        checkpoint.key.sequence,
        checkpoint.progress.clone(),
        checkpoint.evidence.clone(),
    )
    .map_err(|error| StoreError::InvalidCheckpoint(error.to_string()))?;
    if checkpoint.key != canonical.key {
        return Err(StoreError::InvalidCheckpoint(
            "payload digest does not match save key".into(),
        ));
    }
    sql_integer(checkpoint.key.session.get(), "session_id")?;
    sql_integer(checkpoint.key.sequence.get(), "commit_seq")?;
    sql_integer(checkpoint.progress.active_micros, "active_micros")?;
    for track in &checkpoint.progress.tracks {
        for (name, value) in [
            ("first_correct", track.first_correct),
            ("first_attempted", track.first_attempted),
            ("correct_attempts", track.correct_attempts),
            ("content_attempts", track.content_attempts),
            ("deletions", track.deletions),
            ("overflow_attempts", track.overflow_attempts),
            ("correct_units", track.correct_units),
            ("correct_digits", track.correct_digits),
            ("track active_micros", track.active_micros),
        ] {
            sql_integer(value, name)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_advance(old: &Checkpoint, new: &Checkpoint) -> Result<(), StoreError> {
    let advances = match (&old.evidence, &new.evidence) {
        (Evidence::None, Evidence::None) => true,
        // Legacy sessions are never written again (#11).
        (Evidence::Legacy(_), _) | (_, Evidence::Legacy(_)) => false,
        (Evidence::Chunked(previous), Evidence::Chunked(current)) => {
            current.header.advances_from(&previous.header)
                && cadence_core::chunks::watermark_ok(previous.through, current.through)
        }
        (Evidence::None, Evidence::Chunked(_)) | (Evidence::Chunked(_), Evidence::None) => false,
    };
    if !advances {
        return Err(StoreError::Regression(
            "learning evidence changed plan, decreased, or is legacy".into(),
        ));
    }
    validate_progress_advance(&old.progress, &new.progress)
}

fn validate_progress_advance(
    old: &SessionProgress,
    new: &SessionProgress,
) -> Result<(), StoreError> {
    if old.profile != new.profile
        || old.day != new.day
        || old.policy_digest != new.policy_digest
        || old.corpus_digest != new.corpus_digest
    {
        return Err(StoreError::Regression("session identity changed".into()));
    }
    match old.status {
        ProgressStatus::Running => {}
        ProgressStatus::Abandoned if old != new => {
            return Err(StoreError::Regression("terminal session changed".into()));
        }
        ProgressStatus::Abandoned => {}
        // A completed session may still type its closing review (#49):
        // it stays completed, and only the review track and active time
        // advance; every scored track is frozen.
        ProgressStatus::Completed => {
            let scored = |progress: &SessionProgress| -> Vec<_> {
                progress
                    .tracks
                    .iter()
                    .filter(|track| track.track != Track::Review)
                    .cloned()
                    .collect()
            };
            if new.status != ProgressStatus::Completed
                || scored(old) != scored(new)
                || old.utc_offset_minutes != new.utc_offset_minutes
                || old.layout != new.layout
                || old.content_class != new.content_class
                || old.pack_digest != new.pack_digest
            {
                return Err(StoreError::Regression("terminal session changed".into()));
            }
        }
    }
    if new.active_micros < old.active_micros {
        return Err(StoreError::Regression("active time decreased".into()));
    }
    for previous in &old.tracks {
        let current = new
            .tracks
            .iter()
            .find(|track| track.track == previous.track)
            .ok_or_else(|| StoreError::Regression("track disappeared".into()))?;
        if current.first_correct < previous.first_correct
            || current.first_attempted < previous.first_attempted
            || current.correct_attempts < previous.correct_attempts
            || current.content_attempts < previous.content_attempts
            || current.deletions < previous.deletions
            || current.overflow_attempts < previous.overflow_attempts
            || current.active_micros < previous.active_micros
        {
            return Err(StoreError::Regression(
                "cumulative track counters decreased".into(),
            ));
        }
        // Visible correct units/digits may decrease on legal deletion of the partial buffer.
    }
    Ok(())
}
