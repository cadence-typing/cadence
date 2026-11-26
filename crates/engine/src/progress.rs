//! The Progress destination (U-05, L-07, #81). Its history comes from a
//! bounded query on the store's worker, admitted by the proved
//! `ticket::admit`. A track's series runs back from the newest session only
//! across sessions the proved `comparison::same_basis` accepts against it;
//! where it stops, the reason is shown ("layout changed here"). Fewer than
//! [`MINIMUM_COMPARABLE`] sessions read as not enough. Accuracy and pace
//! are exact sums of numerators and denominators over the series, never an
//! average of percentages. No composite score, no streaks.

use cadence_core::comparison::{Basis, basis, same_basis};
use cadence_core::metrics::Ratio;
use cadence_core::progress::{ContentClass, HistoryPoint, Track, TrackProgress};
use cadence_core::{BasisPoints, CoreError};

use crate::jobs::JobTicket;
use crate::summary::SummaryMetrics;
use crate::view::RateUnit;

/// Sessions the history query reads, newest first.
pub const HISTORY_LIMIT: usize = 60;
/// A series shorter than this is not enough to read a trend from.
pub const MINIMUM_COMPARABLE: usize = 3;

/// The tracks Progress shows, in order.
const TRACKS: [Track; 5] = [
    Track::WarmUp,
    Track::Words,
    Track::NumberRow,
    Track::Numpad,
    Track::Prose,
];

/// Why a series stops before the oldest session read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    /// The session before was typed on another layout.
    LayoutChanged,
    /// The session before practised another content class.
    ContentChanged,
    /// The session before was recorded by the headless driver.
    Synthetic,
    /// The probe before was not comparable.
    ProbeNotComparable,
}

impl Break {
    /// The note shown where the series stops.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::LayoutChanged => "layout changed here",
            Self::ContentChanged => "other content (imported or code) here",
            Self::Synthetic => "a headless session here",
            Self::ProbeNotComparable => "a probe that is not comparable here",
        }
    }
}

/// One track's series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    /// The track.
    pub track: Track,
    /// Comparable sessions in the series.
    pub sessions: usize,
    /// First-pass correct positions, summed.
    pub first_correct: u64,
    /// First-pass attempted positions, summed.
    pub first_attempted: u64,
    /// The exact first-pass accuracy over the series.
    pub accuracy: Option<BasisPoints>,
    /// Correct pace over the series.
    pub speed: Option<Ratio>,
    /// Its unit.
    pub unit: RateUnit,
    /// At least [`MINIMUM_COMPARABLE`] sessions.
    pub enough: bool,
    /// Why the series stops before the oldest session read.
    pub stopped: Option<Break>,
}

/// What Progress shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressView {
    /// Waiting for the worker.
    Loading,
    /// The query failed.
    Failed(String),
    /// The series, typing tracks first, then the transfer probe.
    Ready {
        /// One per track with any session.
        tracks: Vec<Series>,
        /// The transfer probe's series, if any probe was recorded.
        benchmark: Option<Series>,
    },
}

/// Progress's query and its answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ProgressState {
    pending: Option<JobTicket>,
    answer: Option<Result<Vec<HistoryPoint>, String>>,
}

impl ProgressState {
    pub(crate) fn begin(&mut self, ticket: Option<JobTicket>) {
        self.pending = ticket;
        self.answer = None;
    }

    /// The query a started job asks for.
    pub(crate) fn request(&self, ticket: JobTicket) -> Option<usize> {
        (self.pending == Some(ticket)).then_some(HISTORY_LIMIT)
    }

    pub(crate) fn finish(&mut self, answer: Result<Vec<HistoryPoint>, String>) {
        self.pending = None;
        self.answer = Some(answer);
    }

    pub(crate) fn view(&self) -> ProgressView {
        match &self.answer {
            None => ProgressView::Loading,
            Some(Err(reason)) => ProgressView::Failed(reason.clone()),
            Some(Ok(points)) => match series_view(points) {
                Ok((tracks, benchmark)) => ProgressView::Ready { tracks, benchmark },
                Err(error) => ProgressView::Failed(error.to_string()),
            },
        }
    }
}

fn track_of(point: &HistoryPoint, track: Track) -> Option<&TrackProgress> {
    point
        .tracks
        .iter()
        .find(|t| t.track == track && t.first_attempted > 0)
}

fn point_basis(point: &HistoryPoint, track: Track) -> Basis {
    basis(point.layout, point.content_class, point.synthetic, [track])
}

fn why(newest: &HistoryPoint, other: &HistoryPoint) -> Break {
    if other.synthetic {
        Break::Synthetic
    } else if other.layout != newest.layout {
        Break::LayoutChanged
    } else if other.content_class != newest.content_class
        || newest.content_class != ContentClass::Standard
    {
        Break::ContentChanged
    } else {
        Break::ProbeNotComparable
    }
}

/// One track's series, running back from its newest session.
fn series(points: &[HistoryPoint], track: Track) -> Result<Option<Series>, CoreError> {
    let probe = track == Track::Benchmark;
    let mut with_track = points
        .iter()
        .rev()
        .filter(|point| track_of(point, track).is_some());
    let Some(newest) = with_track.next() else {
        return Ok(None);
    };
    let mut reports = Vec::new();
    let mut stopped = None;
    for point in core::iter::once(newest).chain(with_track) {
        let comparable = same_basis(point_basis(newest, track), point_basis(point, track))
            && (!probe || point.probe_comparable == Some(true));
        if !comparable {
            stopped = Some(why(newest, point));
            break;
        }
        if let Some(report) = track_of(point, track) {
            reports.push(report.clone());
        }
    }
    let metrics = SummaryMetrics::for_series(&reports)?;
    let first_correct = reports
        .iter()
        .try_fold(0_u64, |sum, r| sum.checked_add(r.first_correct));
    let first_attempted = reports
        .iter()
        .try_fold(0_u64, |sum, r| sum.checked_add(r.first_attempted));
    let (Some(first_correct), Some(first_attempted)) = (first_correct, first_attempted) else {
        return Err(CoreError::CapacityExhausted {
            limit: "progress series counts",
        });
    };
    Ok(Some(Series {
        track,
        sessions: reports.len(),
        first_correct,
        first_attempted,
        accuracy: metrics.first_pass_accuracy,
        speed: metrics.speed,
        unit: metrics.speed_unit,
        enough: reports.len() >= MINIMUM_COMPARABLE,
        stopped,
    }))
}

type Views = (Vec<Series>, Option<Series>);

fn series_view(points: &[HistoryPoint]) -> Result<Views, CoreError> {
    let mut tracks = Vec::new();
    for track in TRACKS {
        if let Some(s) = series(points, track)? {
            tracks.push(s);
        }
    }
    Ok((tracks, series(points, Track::Benchmark)?))
}

impl crate::Engine {
    /// The history query a started job asks the worker for (#81): the
    /// number of newest sessions to read.
    #[must_use]
    pub fn history_request(&self, ticket: JobTicket) -> Option<usize> {
        self.progress.request(ticket)
    }

    /// An admitted history answer.
    pub(crate) fn history_result(&mut self) {
        if let Some((_, outcome)) = self.jobs.consume(crate::jobs::JobKind::History) {
            self.progress.finish(match outcome {
                crate::jobs::JobOutcome::History(points) => Ok(points),
                crate::jobs::JobOutcome::Failed(reason) => Err(reason),
                crate::jobs::JobOutcome::Succeeded | crate::jobs::JobOutcome::Settings { .. } => {
                    Err("unexpected answer to the history query".into())
                }
            });
        }
    }
}
