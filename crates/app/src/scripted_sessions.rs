//! The beta.1 scripted run (#66): eight real 5-minute daily plans played
//! through the engine into a temporary profile, with virtual time. Sessions
//! 1 to 6 are typed cleanly, session 7 with frequent slips. The words pace
//! goal rises once enough clean blocks exist, falls after the sloppy
//! session, and the eighth session (after seven completed sessions with
//! prose) carries the transfer probe. With `CADENCE_EVIDENCE_OUT` set, the
//! per-session summary is written there.

use cadence_core::learning::PaceMove;
use cadence_core::progress::{SaveResult, Track};
use cadence_core::{Layout, MonoTick, TrainingChar};
use cadence_data::ProgressStore;
use cadence_engine::{Engine, EventEnvelope, EventKind, Key, ScreenView, UnitSpan};

struct Run {
    engine: Engine,
    now: u64,
    store: ProgressStore,
}

impl Run {
    fn send(&mut self, kind: EventKind) -> bool {
        let Ok(seq) = self.engine.consumed_events().next() else {
            return true;
        };
        let at = MonoTick::from_us(self.now);
        let mut transition = self.engine.reduce(EventEnvelope { seq, at, kind });
        let mut quit = transition.quit;
        while let Some(checkpoint) = transition.checkpoint.take() {
            let result = self
                .store
                .apply_checkpoint(&checkpoint)
                .map(|_| ())
                .map_err(|e| e.to_string());
            let Ok(seq) = self.engine.consumed_events().next() else {
                return true;
            };
            transition = self.engine.reduce(EventEnvelope {
                seq,
                at,
                kind: EventKind::SaveResult(SaveResult {
                    key: checkpoint.key,
                    result,
                }),
            });
            quit |= transition.quit;
        }
        quit
    }
}

fn unit(c: char) -> Result<TrainingChar, Box<dyn std::error::Error>> {
    Ok(TrainingChar::new(c, Layout::Us)?)
}

/// Plays one session to its end; `slip_every` > 0 mistypes (then corrects)
/// every n-th key.
fn play(run: &mut Run, slip_every: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut keys = 0_u64;
    for _ in 0..200_000 {
        let view = run.engine.prepare(MonoTick::from_us(run.now));
        let quit = match &view.screen {
            ScreenView::Today { .. } | ScreenView::Summary => run.send(EventKind::Key(Key::Enter)),
            ScreenView::PhaseComplete { .. } => {
                run.now += 1_000_000;
                run.send(EventKind::Key(Key::Enter))
            }
            ScreenView::Practice { spans, .. } => {
                run.now += 200_000;
                let next = spans.iter().find_map(|span| match span {
                    UnitSpan::Cursor { expected } => Some(*expected),
                    _ => None,
                });
                match next {
                    None => run.send(EventKind::Key(Key::Enter)),
                    Some(expected) => {
                        keys += 1;
                        if slip_every > 0 && keys.is_multiple_of(slip_every) {
                            let wrong = if expected == 'x' { 'q' } else { 'x' };
                            run.send(EventKind::Key(Key::Content(unit(wrong)?)));
                            run.now += 200_000;
                            run.send(EventKind::Key(Key::Backspace));
                            run.now += 200_000;
                        }
                        let typed = if expected == '\n' {
                            Key::Enter
                        } else {
                            Key::Content(unit(expected)?)
                        };
                        run.send(EventKind::Key(typed))
                    }
                }
            }
        };
        if quit {
            return Ok(());
        }
    }
    Err("the session did not finish".into())
}

#[test]
fn eight_sessions_show_one_probe_and_a_raised_and_lowered_goal()
-> Result<(), Box<dyn std::error::Error>> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cadence-scripted-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir)?;
    let result = sessions(&dir);
    std::fs::remove_dir_all(&dir)?;
    result
}

fn sessions(profile: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let policy = cadence_trainer::parse_policy(crate::POLICY.as_bytes())?;
    let mut options = crate::cli::Options::defaults();
    options.minutes = Some(5);
    let mut summary = Vec::new();
    let mut probes = 0;
    let mut raised = false;
    let mut lowered = false;
    for session in 1..=8_u64 {
        let store = ProgressStore::open_in(profile)?;
        let bootstrap = store.bootstrap(policy.learning.history_window())?;
        let day = 20_400;
        options.seed = Some(session);
        let engine = crate::build_engine(&options, &policy, &bootstrap, day)?;
        let mut run = Run {
            engine,
            now: 1,
            store,
        };
        run.send(EventKind::StorageReady {
            next_session: bootstrap.next_session,
            day,
            history: bootstrap.history.clone(),
        });
        let view = run.engine.prepare(MonoTick::from_us(run.now));
        let words = view
            .learning
            .goals
            .iter()
            .find(|g| g.track == Track::Words)
            .copied();
        if view.learning.probe_today {
            probes += 1;
        }
        raised |= words.is_some_and(|g| g.moved == PaceMove::Raised);
        lowered |= words.is_some_and(|g| g.moved == PaceMove::Lowered);
        summary.push(format!(
            "session {session}: words goal {:?} cpm {:?}; probe today: {}",
            words.map(|g| g.moved),
            words.and_then(|g| g.target_cpm),
            view.learning.probe_today
        ));
        play(&mut run, if session == 7 { 8 } else { 0 })?;
    }
    if let Some(out) = std::env::var_os("CADENCE_EVIDENCE_OUT") {
        std::fs::write(out, summary.join("\n") + "\n")?;
    }
    assert_eq!(probes, 1, "{summary:#?}");
    assert!(raised && lowered, "{summary:#?}");
    Ok(())
}
