//! The owner loop composes adapters; all application mutations use reduce.

use std::error::Error;
use std::time::{Duration, Instant};

use cadence_core::progress::Checkpoint;
use cadence_core::{EventSeq, MonoTick};
use cadence_data::StoreBootstrap;
use cadence_engine::view::TextSize;
use cadence_engine::{DomainOutcome, Engine, EventEnvelope, EventKind, Transition};
use cadence_runtime::{ConfigWorker, SaveWorker, TerminalGuard, WriterError, elapsed_since};
use cadence_tui::{Palette, paint};

/// Timer-only repaints are capped; semantic keys/results repaint immediately.
const TIMER_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// An engine with its store, ready for the loop.
pub struct Prepared {
    /// The reducer.
    pub engine: Engine,
    /// The retained save writer.
    pub worker: SaveWorker,
    /// What the store held at startup.
    pub bootstrap: StoreBootstrap,
}

/// The engine, its event sequence and its undispatched checkpoint.
struct Owner {
    engine: Engine,
    seq: EventSeq,
    dispatch: Option<Checkpoint>,
}

impl Owner {
    const fn new(engine: Engine) -> Self {
        Self {
            engine,
            seq: EventSeq::INITIAL,
            dispatch: None,
        }
    }

    fn apply(
        &mut self,
        at: MonoTick,
        kind: EventKind,
        config: &ConfigWorker,
    ) -> Result<Transition, Box<dyn Error>> {
        let next = self.seq.next()?;
        let mut transition = self.engine.reduce(EventEnvelope {
            seq: next,
            at,
            kind,
        });
        self.seq = next;
        if let Some(checkpoint) = transition.checkpoint.take() {
            if self.dispatch.is_some() {
                return Err("invariant: an undispatched checkpoint would be overwritten".into());
            }
            self.dispatch = Some(checkpoint);
        }
        // A started settings job goes to the config worker (#79).
        if let Some(ticket) = transition.job
            && let Some(request) = self.engine.settings_request(ticket)
        {
            config.submit(ticket, request)?;
        }
        Ok(transition)
    }
}

/// The start-up facts every engine receives before its first frame.
fn begin(
    owner: &mut Owner,
    bootstrap: StoreBootstrap,
    (day, text_size): (i64, TextSize),
    (terminal, origin): (&TerminalGuard, Instant),
    config: &ConfigWorker,
) -> Result<(), Box<dyn Error>> {
    owner.apply(MonoTick::ORIGIN, EventKind::SetTextSize(text_size), config)?;
    owner.apply(
        MonoTick::ORIGIN,
        EventKind::StorageReady {
            next_session: bootstrap.next_session,
            day,
            history: bootstrap.history,
        },
        config,
    )?;
    let (columns, rows) = terminal.viewport()?;
    owner.apply(
        elapsed_since(origin)?,
        EventKind::Resized { columns, rows },
        config,
    )?;
    owner.apply(
        elapsed_since(origin)?,
        EventKind::SetTextSupport(terminal.text_support()),
        config,
    )?;
    Ok(())
}

pub fn run(
    first: Prepared,
    config: ConfigWorker,
    (day, text_size): (i64, TextSize),
    palette: Palette,
    mut rebuild: impl FnMut() -> Result<Prepared, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let origin = Instant::now();
    let Prepared {
        engine,
        mut worker,
        bootstrap,
    } = first;
    let mut owner = Owner::new(engine);
    let mut terminal = TerminalGuard::enter()?;
    begin(
        &mut owner,
        bootstrap,
        (day, text_size),
        (&terminal, origin),
        &config,
    )?;
    draw(
        &owner.engine,
        &mut terminal,
        elapsed_since(origin)?,
        palette,
    )?;
    let mut last_frame = Instant::now();

    let outcome = loop {
        dispatch_pending(&worker, &mut owner.dispatch)?;
        // Bound result service to one retained terminal outcome per turn.
        if let Some(result) = worker.try_result()? {
            let transition = owner.apply(
                elapsed_since(origin)?,
                EventKind::SaveResult(result),
                &config,
            )?;
            if transition.quit {
                break transition.exit_failed;
            }
            if transition.dirty {
                draw(
                    &owner.engine,
                    &mut terminal,
                    elapsed_since(origin)?,
                    palette,
                )?;
                last_frame = Instant::now();
            }
        }
        // A settings answer is a fact for the reducer, which admits it
        // through the proved ticket decision (#79).
        if let Some((ticket, outcome)) = config.try_result()? {
            let transition = owner.apply(
                elapsed_since(origin)?,
                EventKind::JobResult { ticket, outcome },
                &config,
            )?;
            if transition.outcome == DomainOutcome::SetupComplete {
                // Setup is written (#78) and nothing was typed or saved: the
                // store and plan are built again from the file.
                worker.finish()?;
                let next = rebuild()?;
                worker = next.worker;
                owner = Owner::new(next.engine);
                begin(
                    &mut owner,
                    next.bootstrap,
                    (day, text_size),
                    (&terminal, origin),
                    &config,
                )?;
            }
            if transition.dirty {
                draw(
                    &owner.engine,
                    &mut terminal,
                    elapsed_since(origin)?,
                    palette,
                )?;
                last_frame = Instant::now();
            }
        }
        let event = terminal.next_event()?;
        // Timestamp immediately after read, before reduction and drawing.
        let at = elapsed_since(origin)?;
        let semantic = event.is_some();
        let transition = owner.apply(at, event.unwrap_or(EventKind::Tick), &config)?;
        if transition.quit {
            break transition.exit_failed;
        }
        if transition.dirty && (semantic || last_frame.elapsed() >= TIMER_FRAME_INTERVAL) {
            draw(&owner.engine, &mut terminal, at, palette)?;
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
    config.finish()?;
    if let Some(line) = owner.engine.recap() {
        println!("{line}");
    }
    Ok(())
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

fn draw(
    engine: &Engine,
    terminal: &mut TerminalGuard,
    at: MonoTick,
    palette: Palette,
) -> Result<(), Box<dyn Error>> {
    let view = engine.prepare(at);
    // The theme applies at once (#79); a theme file falls back to the
    // palette chosen at startup.
    let palette = Palette::named(&view.display.theme).unwrap_or(palette);
    terminal.draw(|buffer, rows| {
        let area = buffer.area;
        paint(buffer, area, &view, palette, rows);
    })?;
    Ok(())
}
