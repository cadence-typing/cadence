//! Observation chunks on disk (L-01, D-02, #11). Each incoming chunk is
//! decided by the proved `cadence_kernel::chunks::admit` against what is
//! stored: the next one is inserted and folded into the derived rows, a
//! stored one with the same payload is a replay, anything else refuses the
//! whole transaction. The table's triggers abort any `UPDATE` or `DELETE`.

use cadence_core::ContentDigest;
use cadence_core::chunks::{Admit, GENESIS, ObservationChunk, admit, chain};
use cadence_core::progress::ChunkedEvidence;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StoreError;
use crate::error::{read_integer, sql_integer};

fn parse_digest(hex: &str) -> Result<ContentDigest, StoreError> {
    let bad = || StoreError::Corrupt("malformed chunk digest".into());
    if hex.len() != 64 {
        return Err(bad());
    }
    let mut bytes = [0_u8; 32];
    for (slot, index) in bytes.iter_mut().zip((0_usize..64).step_by(2)) {
        let pair = hex.get(index..index.saturating_add(2)).ok_or_else(bad)?;
        *slot = u8::from_str_radix(pair, 16).map_err(|_digit| bad())?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}

/// The stored last chunk of a session, if any.
fn stored_last(connection: &Connection, session: i64) -> Result<Option<u64>, StoreError> {
    let last: Option<i64> = connection
        .prepare_cached("SELECT MAX(chunk_seq) FROM observation_chunks WHERE session_id=?1")?
        .query_row([session], |row| row.get(0))?;
    last.map(|seq| read_integer(seq, "chunk_seq")).transpose()
}

fn stored_digest(
    connection: &Connection,
    session: i64,
    seq: u64,
) -> Result<Option<String>, StoreError> {
    Ok(connection
        .prepare_cached(
            "SELECT digest FROM observation_chunks WHERE session_id=?1 AND chunk_seq=?2",
        )?
        .query_row(params![session, sql_integer(seq, "chunk_seq")?], |row| {
            row.get(0)
        })
        .optional()?)
}

/// The chain over a session's stored chunk digests 1 through `through`.
///
/// # Errors
/// A missing chunk or a malformed digest.
pub(crate) fn chain_of(
    connection: &Connection,
    session: i64,
    through: u64,
) -> Result<ContentDigest, StoreError> {
    let mut statement = connection.prepare_cached(
        "SELECT chunk_seq, digest FROM observation_chunks WHERE session_id=?1 AND chunk_seq<=?2
         ORDER BY chunk_seq",
    )?;
    let mut rows = statement.query(params![session, sql_integer(through, "chunk_seq")?])?;
    let mut value = GENESIS;
    let mut expected = 1_u64;
    while let Some(row) = rows.next()? {
        let seq = read_integer(row.get(0)?, "chunk_seq")?;
        if seq != expected {
            return Err(StoreError::Corrupt("a stored chunk is missing".into()));
        }
        value = chain(value, parse_digest(&row.get::<_, String>(1)?)?);
        expected = expected.saturating_add(1);
    }
    if expected.saturating_sub(1) != through {
        return Err(StoreError::Corrupt(
            "fewer chunks stored than the watermark".into(),
        ));
    }
    Ok(value)
}

/// A session's stored chunks, in order (export, rebuild).
///
/// # Errors
/// A malformed payload.
pub(crate) fn load(
    connection: &Connection,
    session: i64,
) -> Result<Vec<ObservationChunk>, StoreError> {
    let mut statement = connection.prepare_cached(
        "SELECT payload, digest FROM observation_chunks WHERE session_id=?1 ORDER BY chunk_seq",
    )?;
    let mut rows = statement.query([session])?;
    let mut chunks = Vec::new();
    while let Some(row) = rows.next()? {
        let payload: Vec<u8> = row.get(0)?;
        let chunk = ObservationChunk::decode(&payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if chunk.digest().to_hex() != row.get::<_, String>(1)? {
            return Err(StoreError::Corrupt(
                "a chunk differs from its digest".into(),
            ));
        }
        chunks.push(chunk);
    }
    Ok(chunks)
}

/// Appends a checkpoint's chunks and folds the new ones into the derived
/// rows, then checks that the stored chunks are exactly what the checkpoint
/// names (its watermark and chain) and records the derived watermark.
///
/// # Errors
/// [`StoreError::ChunkRefused`] for a changed payload or a gap, and a
/// mismatch between the stored chunks and the checkpoint's chain; the
/// caller's transaction then rolls back whole.
pub(crate) fn append(
    tx: &Transaction<'_>,
    session: i64,
    evidence: &ChunkedEvidence,
) -> Result<(), StoreError> {
    let mut last = stored_last(tx, session)?;
    for chunk in &evidence.chunks {
        let digest = chunk.digest().to_hex();
        let same = stored_digest(tx, session, chunk.seq)?.is_some_and(|stored| stored == digest);
        match admit(last.unwrap_or(0), last.is_some(), chunk.seq, same) {
            Admit::Append => {
                tx.prepare_cached(
                    "INSERT INTO observation_chunks(session_id,chunk_seq,track,payload,digest)
                     VALUES (?1,?2,?3,?4,?5)",
                )?
                .execute(params![
                    session,
                    sql_integer(chunk.seq, "chunk_seq")?,
                    chunk.track.key(),
                    chunk.encode(),
                    digest
                ])?;
                crate::learning_rows::fold_chunk(tx, session, chunk)?;
                last = Some(chunk.seq);
            }
            Admit::Replay => {}
            Admit::Conflict => {
                return Err(StoreError::ChunkRefused(format!(
                    "chunk {} is stored with a different payload",
                    chunk.seq
                )));
            }
            Admit::Gap => {
                return Err(StoreError::ChunkRefused(format!(
                    "chunk {} does not follow the stored chunks",
                    chunk.seq
                )));
            }
        }
    }
    if last.unwrap_or(0) != evidence.through
        || chain_of(tx, session, evidence.through)? != evidence.chain
    {
        return Err(StoreError::ChunkRefused(
            "the stored chunks differ from the checkpoint's chain".into(),
        ));
    }
    tx.prepare_cached(
        "INSERT INTO derived_state(session_id,source_chunk_seq,legacy) VALUES (?1,?2,0)
         ON CONFLICT(session_id) DO UPDATE SET source_chunk_seq=excluded.source_chunk_seq",
    )?
    .execute(params![
        session,
        sql_integer(evidence.through, "chunk_seq")?
    ])?;
    Ok(())
}

/// The derived watermark and whether the session is legacy.
///
/// # Errors
/// SQL failures.
pub(crate) fn derived_state(
    connection: &Connection,
    session: i64,
) -> Result<Option<(u64, bool)>, StoreError> {
    let row: Option<(i64, i64)> = connection
        .prepare_cached("SELECT source_chunk_seq, legacy FROM derived_state WHERE session_id=?1")?
        .query_row([session], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    row.map(|(seq, legacy)| Ok((read_integer(seq, "source_chunk_seq")?, legacy == 1)))
        .transpose()
}
