//! #75: ticketed worker results through `ticket::admit`.
mod support;

use cadence_engine::EventKind;
use cadence_engine::jobs::{JobKind, JobOutcome, JobTicket};
use support::{TestResult, send, words};

fn start(e: &mut cadence_engine::Engine, at: u64) -> Result<JobTicket, Box<dyn std::error::Error>> {
    send(e, at, EventKind::StartJob(JobKind::Ping))?
        .job
        .ok_or_else(|| "a ticket".into())
}

fn result(
    e: &mut cadence_engine::Engine,
    at: u64,
    ticket: JobTicket,
    outcome: JobOutcome,
) -> TestResult {
    send(e, at, EventKind::JobResult { ticket, outcome })?;
    Ok(())
}

fn accepted(e: &cadence_engine::Engine) -> u64 {
    e.jobs().slot(JobKind::Ping).map_or(0, |slot| slot.accepted)
}

#[test]
fn late_results_from_an_older_epoch_change_nothing() -> TestResult {
    let mut e = words()?;
    let old = start(&mut e, 1)?;
    send(&mut e, 2, EventKind::RestartJobs)?;
    let fresh = start(&mut e, 3)?;
    // Request ids restart with the epoch: the old ticket has the same id.
    assert_eq!(old.request, fresh.request);
    assert_ne!(old.epoch, fresh.epoch);
    result(&mut e, 4, old, JobOutcome::Succeeded)?;
    result(&mut e, 5, old, JobOutcome::Failed("late".into()))?;
    assert_eq!(accepted(&e), 0, "a late success or failure changes nothing");
    assert_eq!(
        e.jobs().slot(JobKind::Ping).and_then(|s| s.pending),
        Some(fresh.request)
    );
    result(&mut e, 6, fresh, JobOutcome::Succeeded)?;
    assert_eq!(accepted(&e), 1);
    Ok(())
}

#[test]
fn a_duplicate_result_applies_once() -> TestResult {
    let mut e = words()?;
    let ticket = start(&mut e, 1)?;
    result(&mut e, 2, ticket, JobOutcome::Succeeded)?;
    result(&mut e, 3, ticket, JobOutcome::Succeeded)?;
    result(&mut e, 4, ticket, JobOutcome::Failed("again".into()))?;
    assert_eq!(accepted(&e), 1);
    assert_eq!(
        e.jobs()
            .slot(JobKind::Ping)
            .and_then(|s| s.retained.clone()),
        Some((ticket.request, JobOutcome::Succeeded)),
        "the accepted result is retained, not replaced"
    );
    Ok(())
}

#[test]
fn a_storm_of_stale_results_leaves_the_table_bounded() -> TestResult {
    let mut e = words()?;
    let stale = start(&mut e, 1)?;
    send(&mut e, 2, EventKind::RestartJobs)?;
    let before = e.jobs().clone();
    for n in 0..10_000_u64 {
        let ticket = JobTicket {
            request: stale.request.saturating_add(n % 7),
            ..stale
        };
        result(&mut e, 3 + n, ticket, JobOutcome::Failed("stale".into()))?;
    }
    assert_eq!(e.jobs(), &before, "nothing stored, nothing grew");
    Ok(())
}
