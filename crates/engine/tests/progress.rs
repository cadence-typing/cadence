//! #81: Progress series run only across sessions `same_basis` accepts; a
//! layout change, a synthetic session or a pack session stops them, never
//! one line through; counts are exact sums, never averaged percentages.
mod support;
use cadence_core::progress::{ContentClass, HistoryPoint, Track, TrackProgress};
use cadence_core::{BasisPoints, Layout, MonoTick, SessionId};
use cadence_engine::commands::Destination;
use cadence_engine::jobs::{JobKind, JobOutcome};
use cadence_engine::progress::{Break, ProgressView, Series};
use cadence_engine::{EventKind, Key};
use support::*;

fn point(
    id: u64,
    layout: Layout,
    class: ContentClass,
    synthetic: bool,
    (correct, attempted): (u64, u64),
) -> Result<HistoryPoint, Box<dyn std::error::Error>> {
    Ok(HistoryPoint {
        session: SessionId::new(id)?,
        day: i64::try_from(id)?,
        layout: Some(layout),
        content_class: class,
        synthetic,
        probe_comparable: None,
        tracks: vec![TrackProgress {
            first_correct: correct,
            first_attempted: attempted,
            correct_units: correct,
            active_micros: 60_000_000,
            ..TrackProgress::empty(Track::Words)
        }],
    })
}

/// Progress for `points`, through the engine's own query path.
fn progress(points: Vec<HistoryPoint>) -> Result<ProgressView, Box<dyn std::error::Error>> {
    let mut e = words()?;
    ready(&mut e)?;
    key(&mut e, 1, Key::Tab)?;
    key(&mut e, 2, Key::Tab)?;
    let t = key(&mut e, 3, Key::Tab)?;
    assert_eq!(
        e.prepare(MonoTick::from_us(3)).destination.selected,
        Destination::Progress
    );
    let ticket = t.job.ok_or("entering Progress queries history")?;
    assert_eq!(ticket.kind, JobKind::History);
    assert!(e.history_request(ticket).is_some());
    assert_eq!(
        e.prepare(MonoTick::from_us(3)).progress,
        Some(ProgressView::Loading)
    );
    send(
        &mut e,
        4,
        EventKind::JobResult {
            ticket,
            outcome: JobOutcome::History(points),
        },
    )?;
    e.prepare(MonoTick::from_us(4))
        .progress
        .ok_or_else(|| "progress view".into())
}

fn words_series(view: &ProgressView) -> Option<&Series> {
    match view {
        ProgressView::Ready { tracks, .. } => tracks.iter().find(|s| s.track == Track::Words),
        ProgressView::Loading | ProgressView::Failed(_) => None,
    }
}

const US: Layout = Layout::Us;
const UK: Layout = Layout::Uk;
const STD: ContentClass = ContentClass::Standard;

#[test]
fn a_layout_change_stops_the_series() -> TestResult {
    let history = vec![
        point(1, US, STD, false, (50, 50))?,
        point(2, US, STD, false, (50, 50))?,
        point(3, US, STD, false, (50, 50))?,
        point(4, UK, STD, false, (9, 10))?,
        point(5, UK, STD, false, (9, 10))?,
        point(6, UK, STD, false, (9, 10))?,
    ];
    let view = progress(history)?;
    let series = words_series(&view).ok_or("words")?;
    assert_eq!(
        (
            series.sessions,
            series.first_correct,
            series.first_attempted
        ),
        (3, 27, 30)
    );
    assert!(series.enough);
    assert_eq!(series.stopped, Some(Break::LayoutChanged));
    Ok(())
}

#[test]
fn a_synthetic_or_pack_session_is_never_joined() -> TestResult {
    let base = || -> Result<Vec<HistoryPoint>, Box<dyn std::error::Error>> {
        Ok(vec![
            point(1, UK, STD, false, (9, 10))?,
            point(2, UK, STD, false, (9, 10))?,
            point(3, UK, STD, false, (9, 10))?,
        ])
    };
    // A synthetic session between live ones stops the series there.
    let mut history = base()?;
    history.push(point(4, UK, STD, true, (10, 10))?);
    history.push(point(5, UK, STD, false, (1, 10))?);
    let view = progress(history)?;
    let series = words_series(&view).ok_or("words")?;
    assert_eq!((series.sessions, series.first_correct), (1, 1));
    assert_eq!(series.stopped, Some(Break::Synthetic));
    assert!(!series.enough);
    // The newest session is synthetic: no series at all.
    let mut history = base()?;
    history.push(point(4, UK, STD, true, (10, 10))?);
    let series_view = progress(history)?;
    let series = words_series(&series_view).ok_or("words")?;
    assert_eq!(series.sessions, 0);
    // A pack session stands alone.
    let mut history = base()?;
    history.push(point(4, UK, ContentClass::Imported, false, (10, 10))?);
    let pack_view = progress(history)?;
    let series = words_series(&pack_view).ok_or("words")?;
    assert_eq!(
        (series.sessions, series.stopped),
        (1, Some(Break::ContentChanged))
    );
    Ok(())
}

#[test]
fn accuracy_sums_numerators_and_denominators() -> TestResult {
    let history = vec![
        point(1, US, STD, false, (90, 100))?,
        point(2, US, STD, false, (1, 2))?,
        point(3, US, STD, false, (1, 2))?,
    ];
    let view = progress(history)?;
    let series = words_series(&view).ok_or("words")?;
    assert_eq!((series.first_correct, series.first_attempted), (92, 104));
    // 92/104 = 88.46%, not the 63.3% an average of percentages would give.
    assert_eq!(
        series.accuracy,
        Some(BasisPoints::from_ratio_truncating(92, 104)?)
    );
    Ok(())
}

#[test]
fn an_empty_profile_has_no_series() -> TestResult {
    assert_eq!(
        progress(Vec::new())?,
        ProgressView::Ready {
            tracks: Vec::new(),
            benchmark: None
        }
    );
    Ok(())
}
