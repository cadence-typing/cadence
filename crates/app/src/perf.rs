//! The performance harness (H-06, P6, #15), compiled only with the `perf`
//! feature: never in a release bundle (`xtask dist` refuses its marker).
//!
//! `cadence perf --output FILE [--calibrate-busy-us N]` drives the real
//! daily plan (shipped corpus, fixed seed) through the real reducer and
//! renderer, with checkpoints saved by the real background writer on a
//! temporary profile, and records:
//! - reducer time per event (keys, ticks, acknowledgements);
//! - frame preparation plus draw at 120x40 and at 60x18;
//! - input-to-frame: from the key's arrival at the reducer to its frame,
//!   including handing any checkpoint to the writer and taking its
//!   acknowledgement (the terminal's own read and write are not included);
//! - peak RSS and the job table under a storm of 10,000 worker results.
//!
//! Budgets (H-06) are named constants; a figure beyond one fails the run
//! after the receipt is written. `--calibrate-busy-us` spins inside the
//! measured reducer span: the negative calibration, which must fail.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cadence_core::{Layout, MonoTick, TrainingChar};
use cadence_engine::view::ScreenView;
use cadence_engine::{Engine, EventEnvelope, EventKind, Key, Transition, UnitSpan};
use cadence_runtime::writer::SaveWorker;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::json;

/// H-06: reducer p99.
pub const REDUCER_P99: Duration = Duration::from_millis(1);
/// H-06: frame preparation and draw p99 at 120x40.
pub const FRAME_P99: Duration = Duration::from_millis(8);
/// H-06: input arrival to frame p99.
pub const INPUT_TO_FRAME_P99: Duration = Duration::from_millis(33);
/// Peak resident memory under the storm.
pub const PEAK_RSS_BYTES: u64 = 256 * 1024 * 1024;
/// Keys typed in the measured session.
const KEYS: usize = 12_000;
/// Worker results in the storm.
const STORM: u64 = 10_000;
/// Virtual time between keys (about 150 characters a minute).
const KEY_INTERVAL_US: u64 = 80_000;
/// Every this many keys, a wrong key and its correction.
const MISTAKE_EVERY: usize = 37;
/// Minutes of the measured session (the policy maximum): enough for every key.
const SESSION_MINUTES: u64 = 30;

#[derive(Default)]
struct Series(Vec<Duration>);

impl Series {
    fn push(&mut self, value: Duration) {
        self.0.push(value);
    }
    /// The `per_mille`-th quantile (nearest rank, rounded down).
    fn quantile(&self, per_mille: usize) -> Duration {
        let mut sorted = self.0.clone();
        sorted.sort_unstable();
        let last = sorted.len().saturating_sub(1);
        let index = last
            .saturating_mul(per_mille)
            .checked_div(1_000)
            .unwrap_or(last);
        sorted.get(index.min(last)).copied().unwrap_or_default()
    }
    fn summary(&self) -> serde_json::Value {
        json!({
            "samples": self.0.len(),
            "p50_us": self.quantile(500).as_micros(),
            "p99_us": self.quantile(990).as_micros(),
            "max_us": self.0.iter().max().copied().unwrap_or_default().as_micros(),
        })
    }
}

struct Run {
    engine: Engine,
    now_us: u64,
    busy: Duration,
    reducer: Series,
    /// The checkpoint waiting for the writer, as the runtime keeps it.
    pending: Option<cadence_core::progress::Checkpoint>,
    saves: u64,
    acknowledged: u64,
    /// When the oldest unacknowledged checkpoint was handed over.
    sent: Option<Instant>,
    /// Hand-over to acknowledgement, in real time.
    round_trip: Series,
}

impl Run {
    fn reduce(&mut self, kind: EventKind) -> Result<Transition, Box<dyn Error>> {
        let seq = self.engine.consumed_events().next()?;
        let envelope = EventEnvelope {
            seq,
            at: MonoTick::from_us(self.now_us),
            kind,
        };
        let started = Instant::now();
        let transition = self.engine.reduce(envelope);
        if !self.busy.is_zero() {
            // The negative calibration: work inside the measured span.
            while started.elapsed() < self.busy {
                std::hint::spin_loop();
            }
        }
        self.reducer.push(started.elapsed());
        if let Some(checkpoint) = transition.checkpoint.clone() {
            self.pending = Some(checkpoint);
            self.saves = self.saves.saturating_add(1);
        }
        Ok(transition)
    }

    /// Hands any pending checkpoint to the writer and reduces every
    /// acknowledgement it has, as the live loop does after each event.
    fn pump(&mut self, worker: &SaveWorker) -> Result<(), Box<dyn Error>> {
        self.submit(worker);
        while let Some(result) = worker.try_result()? {
            self.acknowledged = self.acknowledged.saturating_add(1);
            if let Some(sent) = self.sent.take() {
                self.round_trip.push(sent.elapsed());
            }
            self.reduce(EventKind::SaveResult(result))?;
            self.submit(worker);
        }
        Ok(())
    }

    fn submit(&mut self, worker: &SaveWorker) {
        if self.pending.is_some() && worker.try_submit(&mut self.pending).is_ok() {
            self.sent = Some(Instant::now());
        }
    }

    /// Real time passes between keys (80 ms at this pace): a save in flight
    /// is acknowledged before the next key arrives. Waiting here, outside
    /// every measured span, keeps the virtual clock from outrunning the
    /// writer thread and reporting a stall that real practice never sees.
    fn settle(&mut self, worker: &SaveWorker) -> Result<(), Box<dyn Error>> {
        let deadline = deadline(Duration::from_secs(5));
        while (self.pending.is_some() || self.acknowledged < self.saves)
            && Instant::now() < deadline
        {
            self.pump(worker)?;
            std::thread::yield_now();
        }
        Ok(())
    }
}

fn frame(
    engine: &Engine,
    at: MonoTick,
    columns: u16,
    rows: u16,
    previous: &mut Buffer,
) -> Duration {
    let started = Instant::now();
    let view = engine.prepare(at);
    let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
    let area = buffer.area;
    let mut renditions = vec![cadence_engine::view::LineRendition::Normal; usize::from(rows)];
    cadence_tui::paint(
        &mut buffer,
        area,
        &view,
        cadence_tui::Palette::cadence_dark(),
        &mut renditions,
    );
    // What a terminal backend would write: the difference from the last frame.
    let changed = previous.diff(&buffer).len();
    std::hint::black_box(changed);
    *previous = buffer;
    started.elapsed()
}

fn expected(engine: &Engine, at: MonoTick) -> Option<char> {
    match engine.prepare(at).screen {
        ScreenView::Practice { spans, .. } => spans.iter().find_map(|span| match *span {
            UnitSpan::Cursor { expected } => Some(expected),
            UnitSpan::Correct { .. }
            | UnitSpan::Wrong { .. }
            | UnitSpan::Upcoming { .. }
            | UnitSpan::Extra { .. } => None,
        }),
        ScreenView::Today { .. } | ScreenView::PhaseComplete { .. } | ScreenView::Summary => None,
    }
}

/// `now + wait`, or now when the clock cannot represent it.
fn deadline(wait: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(wait).unwrap_or(now)
}

fn peak_rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .trim()
                    .strip_suffix(" kB")?
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
        })
        .map_or(0, |kib| kib.saturating_mul(1024))
}

fn environment() -> serde_json::Value {
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    let cpu = read("/proc/cpuinfo")
        .lines()
        .find_map(|line| {
            line.strip_prefix("model name")
                .map(|v| v.trim_start_matches([' ', '\t', ':']).to_string())
        })
        .unwrap_or_default();
    json!({
        "cpu": cpu,
        "cpus": std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        "kernel": read("/proc/sys/kernel/osrelease").trim(),
        "load": read("/proc/loadavg").trim(),
        "container": Path::new("/.dockerenv").exists() || read("/proc/1/cgroup").contains("docker"),
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// Runs the harness.
///
/// # Errors
/// A setup failure, or a measured figure beyond its budget (after the
/// receipt is written).
pub fn main(args: &[String]) -> Result<(), Box<dyn Error>> {
    let mut output = None;
    let mut busy = Duration::ZERO;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--output" => output = rest.next().map(PathBuf::from),
            "--calibrate-busy-us" => {
                busy = Duration::from_micros(rest.next().ok_or("--calibrate-busy-us N")?.parse()?);
            }
            other => return Err(format!("unknown perf argument {other:?}").into()),
        }
    }
    let output = output.ok_or("usage: cadence perf --output FILE [--calibrate-busy-us N]")?;
    let home = std::env::temp_dir().join(format!("cadence-perf-{}", std::process::id()));
    std::fs::create_dir_all(&home)?;
    let result = measure(&home, &output, busy);
    std::fs::remove_dir_all(&home)?;
    result
}

fn measure(home: &Path, output: &Path, busy: Duration) -> Result<(), Box<dyn Error>> {
    let policy = cadence_trainer::parse_policy(crate::POLICY.as_bytes())?;
    let (worker, bootstrap) = SaveWorker::open(
        Some(home.to_path_buf()),
        policy.learning.history_window(),
        None,
        Vec::new(),
    )?;
    let mut options = crate::cli::Options::defaults();
    options.minutes = Some(SESSION_MINUTES);
    let day = 20_500;
    let engine = crate::build_engine(&options, &policy, &bootstrap, day)?;
    let mut run = Run {
        engine,
        now_us: 0,
        busy,
        reducer: Series::default(),
        pending: None,
        saves: 0,
        acknowledged: 0,
        sent: None,
        round_trip: Series::default(),
    };
    run.reduce(EventKind::StorageReady {
        next_session: bootstrap.next_session,
        day,
        history: bootstrap.history.clone(),
    })?;
    run.reduce(EventKind::Command(cadence_engine::commands::Action::Begin))?;
    let (mut frames, mut small, mut input) =
        (Series::default(), Series::default(), Series::default());
    let (mut large_buffer, mut small_buffer) = (
        Buffer::empty(Rect::new(0, 0, 120, 40)),
        Buffer::empty(Rect::new(0, 0, 60, 18)),
    );
    let mut typed = 0_usize;
    while typed < KEYS {
        run.settle(&worker)?;
        run.now_us = run.now_us.saturating_add(KEY_INTERVAL_US);
        let at = MonoTick::from_us(run.now_us);
        let Some(next) = expected(&run.engine, at) else {
            // Between phases: continue; at the summary, the session is done.
            match run.engine.prepare(at).screen {
                ScreenView::PhaseComplete { .. } => {
                    run.now_us = run.now_us.saturating_add(2_000_000);
                    run.reduce(EventKind::Command(
                        cadence_engine::commands::Action::Continue,
                    ))?;
                    run.pump(&worker)?;
                    continue;
                }
                ScreenView::Today { .. } | ScreenView::Summary | ScreenView::Practice { .. } => {
                    break;
                }
            }
        };
        let wrong = typed.checked_rem(MISTAKE_EVERY) == Some(MISTAKE_EVERY.saturating_sub(1));
        let keys: Vec<Key> = if wrong {
            let other = if next == 'x' { 'z' } else { 'x' };
            vec![
                Key::Content(TrainingChar::new(other, Layout::Us)?),
                Key::Backspace,
                Key::Content(TrainingChar::new(next, Layout::Us)?),
            ]
        } else if next == '\n' {
            vec![Key::Enter]
        } else {
            vec![Key::Content(TrainingChar::new(next, Layout::Us)?)]
        };
        for key in keys {
            let arrived = Instant::now();
            run.reduce(EventKind::Key(key))?;
            run.pump(&worker)?;
            frames.push(frame(&run.engine, at, 120, 40, &mut large_buffer));
            input.push(arrived.elapsed());
            small.push(frame(&run.engine, at, 60, 18, &mut small_buffer));
            typed = typed.saturating_add(1);
        }
        if typed.is_multiple_of(3) {
            run.reduce(EventKind::Tick)?;
            run.pump(&worker)?;
        }
    }
    // The final flush: every checkpoint handed over is acknowledged.
    let deadline = deadline(Duration::from_secs(30));
    while (run.pending.is_some() || run.acknowledged < run.saves) && Instant::now() < deadline {
        run.pump(&worker)?;
        std::thread::yield_now();
    }
    let (saves, acknowledged) = (run.saves, run.acknowledged);
    let round_trip = run.round_trip.summary();
    // The storm: 10,000 worker results for a restarted job's old epoch.
    let ticket = run
        .reduce(EventKind::StartJob(cadence_engine::jobs::JobKind::Ping))?
        .job
        .ok_or("a job ticket")?;
    run.reduce(EventKind::RestartJobs)?;
    let before = run.engine.jobs().clone();
    let mut consumed = 0_u64;
    for n in 0..STORM {
        let stale = cadence_engine::jobs::JobTicket {
            request: ticket.request.saturating_add(n % 7),
            ..ticket
        };
        run.reduce(EventKind::JobResult {
            ticket: stale,
            outcome: cadence_engine::jobs::JobOutcome::Failed("stale".into()),
        })?;
        consumed = consumed.saturating_add(1);
    }
    let table_bounded = run.engine.jobs() == &before;
    worker.finish()?;
    let rss = peak_rss();
    let receipt = json!({
        "harness": "cadence perf (#15)",
        "environment": environment(),
        "corpus": {"words": crate::WORDS.lines().count(), "prose": crate::PROSE.lines().count()},
        "keys": typed,
        "calibration_busy_us": busy.as_micros(),
        "reducer": run.reducer.summary(),
        "frame_120x40": frames.summary(),
        "frame_60x18": small.summary(),
        "input_to_frame": input.summary(),
        "saves": {"dispatched": saves, "acknowledged": acknowledged, "round_trip": round_trip},
        "storm": {"results": STORM, "consumed": consumed, "job_table_unchanged": table_bounded},
        "peak_rss_bytes": rss,
        "budgets": {
            "reducer_p99_us": REDUCER_P99.as_micros(),
            "frame_120x40_p99_us": FRAME_P99.as_micros(),
            "input_to_frame_p99_us": INPUT_TO_FRAME_P99.as_micros(),
            "peak_rss_bytes": PEAK_RSS_BYTES,
        },
    });
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_string_pretty(&receipt)?)?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    let mut misses = Vec::new();
    for (name, series, budget) in [
        ("reducer", &run.reducer, REDUCER_P99),
        ("frame at 120x40", &frames, FRAME_P99),
        ("input to frame", &input, INPUT_TO_FRAME_P99),
    ] {
        let p99 = series.quantile(990);
        if p99 > budget {
            misses.push(format!("{name} p99 {p99:?} > {budget:?}"));
        }
    }
    if rss > PEAK_RSS_BYTES {
        misses.push(format!("peak RSS {rss} > {PEAK_RSS_BYTES}"));
    }
    if typed < KEYS {
        misses.push(format!("the session ended after {typed} of {KEYS} keys"));
    }
    if acknowledged < saves {
        misses.push(format!(
            "{} checkpoints were never acknowledged",
            saves.saturating_sub(acknowledged)
        ));
    }
    if consumed != STORM || !table_bounded {
        misses.push("the storm dropped a result or grew the job table".into());
    }
    if misses.is_empty() {
        Ok(())
    } else {
        Err(format!("performance budget missed: {}", misses.join("; ")).into())
    }
}
