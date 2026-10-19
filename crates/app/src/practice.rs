//! The owner loop composes adapters; all application mutations use reduce.

use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cadence_core::progress::Checkpoint;
use cadence_core::{EventSeq, MonoTick};
use cadence_engine::view::TextSize;
use cadence_engine::{Engine, EventEnvelope, EventKind, Transition};
use cadence_runtime::{SaveWorker, TerminalGuard, WriterError, elapsed_since};
use cadence_tui::{Palette, paint};

/// Timer-only repaints are capped; semantic keys/results repaint immediately.
const TIMER_FRAME_INTERVAL: Duration = Duration::from_millis(100);

pub fn run(
    mut engine: Engine,
    profile: Option<PathBuf>,
    day: i64,
    text_size: TextSize,
) -> Result<(), Box<dyn Error>> {
    let (worker, bootstrap) = SaveWorker::open(profile)?;
    let origin = Instant::now();
    let mut seq = EventSeq::INITIAL;
    let mut dispatch = None;
    apply(
        &mut engine,
        &mut seq,
        MonoTick::ORIGIN,
        EventKind::SetTextSize(text_size),
        &mut dispatch,
    )?;
    apply(
        &mut engine,
        &mut seq,
        MonoTick::ORIGIN,
        EventKind::StorageReady {
            next_session: bootstrap.next_session,
            day,
            history: bootstrap.history,
        },
        &mut dispatch,
    )?;
    let mut terminal = TerminalGuard::enter()?;
    let (columns, rows) = terminal.viewport()?;
    apply(
        &mut engine,
        &mut seq,
        elapsed_since(origin)?,
        EventKind::Resized { columns, rows },
        &mut dispatch,
    )?;
    apply(
        &mut engine,
        &mut seq,
        elapsed_since(origin)?,
        EventKind::SetTextSupport(terminal.text_support()),
        &mut dispatch,
    )?;
    draw(&engine, &mut terminal, elapsed_since(origin)?)?;
    let mut last_frame = Instant::now();

    let outcome = loop {
        dispatch_pending(&worker, &mut dispatch)?;
        // Bound result service to one retained terminal outcome per turn.
        if let Some(result) = worker.try_result()? {
            let transition = apply(
                &mut engine,
                &mut seq,
                elapsed_since(origin)?,
                EventKind::SaveResult(result),
                &mut dispatch,
            )?;
            if transition.quit {
                break transition.exit_failed;
            }
            if transition.dirty {
                draw(&engine, &mut terminal, elapsed_since(origin)?)?;
                last_frame = Instant::now();
            }
        }
        let event = terminal.next_event()?;
        // Timestamp immediately after read, before reduction and drawing.
        let at = elapsed_since(origin)?;
        let semantic = event.is_some();
        let transition = apply(
            &mut engine,
            &mut seq,
            at,
            event.unwrap_or(EventKind::Tick),
            &mut dispatch,
        )?;
        if transition.quit {
            break transition.exit_failed;
        }
        if transition.dirty && (semantic || last_frame.elapsed() >= TIMER_FRAME_INTERVAL) {
            draw(&engine, &mut terminal, at)?;
            last_frame = Instant::now();
        }
    };
    // A successful quit is emitted by the reducer only after the final receipt.
    terminal.restore()?;
    if outcome {
        // Explicitly retire unacknowledged work without waiting forever on
        // a blocked filesystem syscall. Process exit terminates the worker.
        drop(worker);
        return Err(
            "quit without saving the unacknowledged tail; prior committed history remains safe"
                .into(),
        );
    }
    worker.finish()?;
    Ok(())
}

fn apply(
    engine: &mut Engine,
    seq: &mut EventSeq,
    at: MonoTick,
    kind: EventKind,
    pending: &mut Option<Checkpoint>,
) -> Result<Transition, Box<dyn Error>> {
    let next = seq.next()?;
    let mut transition = engine.reduce(EventEnvelope {
        seq: next,
        at,
        kind,
    });
    *seq = next;
    if let Some(checkpoint) = transition.checkpoint.take() {
        if pending.is_some() {
            return Err("invariant: an undispatched checkpoint would be overwritten".into());
        }
        *pending = Some(checkpoint);
    }
    Ok(transition)
}

fn dispatch_pending(
    worker: &SaveWorker,
    pending: &mut Option<Checkpoint>,
) -> Result<(), Box<dyn Error>> {
    match worker.try_submit(pending) {
        Ok(()) | Err(WriterError::Busy) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn draw(engine: &Engine, terminal: &mut TerminalGuard, at: MonoTick) -> Result<(), Box<dyn Error>> {
    let view = engine.prepare(at);
    terminal.draw(|buffer, rows| {
        let area = buffer.area;
        paint(buffer, area, &view, Palette::cadence_dark(), rows);
    })?;
    Ok(())
}
