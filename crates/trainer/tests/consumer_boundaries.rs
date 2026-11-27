//! Consumer-boundary coverage for the distributed policy and corpus.
use cadence_core::{ContentDigest, Layout};
use cadence_trainer::{
    ExerciseKind, LearnerModel, Phase, PlanInputs, ProseBank, TrainerPolicy, WordBand, WordPack,
    build_session_plan, parse_policy,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const POLICY: &str = include_str!("../../../policies/trainer-v2.toml");
const WORDS: &str = include_str!("../../../corpus/words-en-v2.tsv");
const PROSE: &str = include_str!("../../../corpus/prose-en-v2.tsv");
const SOURCES: &str = include_str!("../../../corpus/prose-sources-v2.tsv");
fn bank() -> Result<ProseBank, cadence_trainer::ProseError> {
    ProseBank::from_tsv(PROSE, SOURCES, 1)
}
fn policy() -> Result<TrainerPolicy, cadence_trainer::PolicyError> {
    parse_policy(POLICY.as_bytes())
}

#[test]
fn shipped_startup_replays_many_seeds_and_obeys_caps() -> TestResult {
    let policy = policy()?;
    let pack = WordPack::from_tsv(WORDS, 20_000, Layout::Us)?;
    let (prose, learner) = (bank()?, LearnerModel::cold(&policy));
    for seed in 0..32 {
        let plan = build_session_plan(
            &policy,
            PlanInputs {
                pack: &pack,
                prose: &prose,
                learner: &learner,
                probes: None,
                pace: None,
                numeric: cadence_trainer::numeric::NumericOptions::default(),
            },
            seed,
            &[],
            900,
        )?;
        assert_eq!(
            plan,
            build_session_plan(
                &policy,
                PlanInputs {
                    pack: &pack,
                    prose: &prose,
                    learner: &learner,
                    probes: None,
                    pace: None,
                    numeric: cadence_trainer::numeric::NumericOptions::default(),
                },
                seed,
                &[],
                900
            )?
        );
        assert_eq!(
            plan.phases
                .iter()
                .map(|phase| phase.seconds)
                .collect::<Vec<_>>(),
            policy.session.default_allocation()?
        );
        for phase in &plan.phases {
            for exercise in &phase.exercises {
                assert!(!exercise.units.is_empty());
                let text: String = exercise.units.iter().map(|unit| unit.get()).collect();
                if matches!(exercise.kind, ExerciseKind::Words | ExerciseKind::WarmUp) {
                    let mut occurrences = std::collections::HashMap::new();
                    let mut previous = None;
                    let mut run = 0u32;
                    for word in text.split(' ') {
                        let count = occurrences.entry(word).or_insert(0u32);
                        *count = count.checked_add(1).ok_or("count overflow")?;
                        assert!(*count <= policy.selection.maximum_word_occurrences_per_block);
                        run = if previous == Some(word) {
                            run.checked_add(1).ok_or("run overflow")?
                        } else {
                            1
                        };
                        assert!(run <= policy.selection.maximum_consecutive_word_occurrences);
                        previous = Some(word);
                    }
                }
                if exercise.kind == ExerciseKind::Numpad {
                    assert!(text.chars().all(|c| c.is_ascii_digit() || c == '.'));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn caps_are_filtered_before_ranking_with_smallest_legal_scan() -> TestResult {
    let mut policy = policy()?;
    policy.selection.maximum_word_occurrences_per_block = 1;
    policy.selection.maximum_consecutive_word_occurrences = 1;
    policy.selection.maximum_candidates_per_slot = policy.selection.block_slots;
    let entries = (1..=300)
        .map(|id| {
            (
                id,
                format!("token{id}"),
                match id {
                    1..=100 => WordBand::Core,
                    101..=200 => WordBand::Common,
                    _ => WordBand::Tail,
                },
            )
        })
        .collect();
    let pack = WordPack::new(entries, ContentDigest::from_bytes([1; 32]), 1, Layout::Us)?;
    let (prose, learner) = (bank()?, LearnerModel::cold(&policy));
    for seed in 0..32 {
        let plan = build_session_plan(
            &policy,
            PlanInputs {
                pack: &pack,
                prose: &prose,
                learner: &learner,
                probes: None,
                pace: None,
                numeric: cadence_trainer::numeric::NumericOptions::default(),
            },
            seed,
            &[Phase::Numpad, Phase::NumberRow, Phase::Prose],
            300,
        )?;
        assert_eq!(
            plan.phases.iter().map(|phase| phase.seconds).sum::<u64>(),
            300
        );
    }
    Ok(())
}

#[test]
fn all_cli_track_presets_keep_exact_budget() -> TestResult {
    let policy = policy()?;
    let pack = WordPack::from_tsv(WORDS, 1, Layout::Us)?;
    let (prose, learner) = (bank()?, LearnerModel::cold(&policy));
    for selected in [
        Phase::AdaptiveWords,
        Phase::NumberRow,
        Phase::Numpad,
        Phase::Prose,
    ] {
        let disabled: Vec<_> = policy
            .session
            .phase_order
            .iter()
            .copied()
            .filter(|phase| *phase != selected && *phase != Phase::Review)
            .collect();
        for budget in [300, 600, 900, 1200, 1800] {
            let plan = build_session_plan(
                &policy,
                PlanInputs {
                    pack: &pack,
                    prose: &prose,
                    learner: &learner,
                    probes: None,
                    pace: None,
                    numeric: cadence_trainer::numeric::NumericOptions::default(),
                },
                7,
                &disabled,
                budget,
            )?;
            assert_eq!(
                plan.phases.iter().map(|phase| phase.seconds).sum::<u64>(),
                budget
            );
            assert!(
                plan.phases
                    .iter()
                    .any(|phase| phase.phase == selected && !phase.exercises.is_empty())
            );
        }
    }
    Ok(())
}

#[test]
fn malformed_policy_is_rejected_not_clamped() {
    for (from, to) in [
        ("block_slots = 10", "block_slots = 4294967296"),
        ("target_slots = 6", "target_slots = 5"),
        (
            "maximum_candidates_per_slot = 256",
            "maximum_candidates_per_slot = 0",
        ),
        (
            "maximum_word_occurrences_per_block = 1",
            "maximum_word_occurrences_per_block = 0",
        ),
        (
            "phase_seconds = [60, 300, 120, 180, 180, 60]",
            "phase_seconds = [60, 300]",
        ),
        (
            "phase_minimum_seconds = [20, 60, 30, 45, 60, 15]",
            "phase_minimum_seconds = [0, 60, 30, 45, 60, 15]",
        ),
        (
            "default_budget_seconds = 900",
            "default_budget_seconds = 901",
        ),
        (
            "maximum_budget_seconds = 1800",
            "maximum_budget_seconds = 299",
        ),
        (
            "recent_occurrence_window = 128",
            "recent_occurrence_window = 4294967296",
        ),
        (
            "interval_days = [1, 3, 7, 14, 30]",
            "interval_days = [1, 7, 3]",
        ),
        (
            "qualifying_blocks_to_raise = 3",
            "qualifying_blocks_to_raise = 0",
        ),
        ("error_prior_beta = 19", "error_prior_beta = 0"),
        ("error = 5000", "error = 10001"),
    ] {
        assert!(POLICY.contains(from));
        assert!(
            parse_policy(POLICY.replace(from, to).as_bytes()).is_err(),
            "accepted {to}"
        );
    }
}

#[test]
fn tsv_boundary_rejects_identity_content_and_shape_errors() -> TestResult {
    for text in [
        "0\tword\tcore",
        "-1\tword\tcore",
        "1\tword\tcore\textra",
        "1\tword",
        "1\t\tcore",
        "1\ttwo words\tcore",
        "1\tword\tcore\n1\tother\tcore",
        "1\tword\tcore\n2\tword\ttail",
    ] {
        assert!(WordPack::from_tsv(text, 1, Layout::Us).is_err());
    }
    let text = "1\tword\tcore\n2\tother\ttail\n";
    let pack = WordPack::from_tsv(text, 2, Layout::Us)?;
    assert_eq!(
        pack.digest(),
        cadence_trainer::corpus::digest_of(text.as_bytes())
    );
    assert_eq!(pack.len(), 2);
    Ok(())
}

#[test]
fn pace_rejects_invented_counts_and_handles_full_width_evidence() -> TestResult {
    use cadence_core::pace::Decision;
    use cadence_core::progress::{SessionRecap, Track, TrackProgress};
    let policy = policy()?;
    let recap = |track: TrackProgress| SessionRecap {
        day: 1,
        completed: true,
        tracks: vec![track],
        layout: None,
        content_class: cadence_core::progress::ContentClass::Standard,
    };
    // More first-correct positions than first attempts: refused.
    let invalid = TrackProgress {
        first_attempted: 1,
        first_correct: 2,
        content_attempts: 1,
        correct_attempts: 1,
        ..TrackProgress::empty(Track::Words)
    };
    assert!(
        cadence_trainer::pace_decision(
            &policy.pace,
            Track::Words,
            Some(u64::MAX),
            &[recap(invalid)]
        )
        .is_err()
    );
    assert_eq!(
        cadence_trainer::pace_decision(&policy.pace, Track::Words, None, &[])?,
        Decision::NoBaseline
    );
    // Full-width stored counters (the persisted maximum) never overflow the
    // exact comparison.
    let top = u64::try_from(i64::MAX)?;
    let full = TrackProgress {
        first_attempted: top,
        first_correct: top,
        content_attempts: top,
        correct_attempts: top,
        correct_units: top,
        ..TrackProgress::empty(Track::Words)
    };
    assert_eq!(
        cadence_trainer::pace_decision(
            &policy.pace,
            Track::Words,
            Some(u64::MAX),
            &[recap(full.clone()), recap(full)]
        )?,
        Decision::Steady
    );
    Ok(())
}

/// #61, #63: the pace and benchmark decisions live only in the kernel; the
/// trainer calls them and keeps no second copy.
#[test]
fn pace_has_no_second_copy_in_the_trainer() -> TestResult {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let text = std::fs::read_to_string(&path)?;
        let name = path.display().to_string();
        assert!(!name.ends_with("/pace.rs"), "trainer::pace is gone: {name}");
        for copy in [
            "fn below_band",
            "PaceDecision",
            "fn pace_decision_inner",
            "fn comparable(",
            "fn due(",
        ] {
            assert!(!text.contains(copy), "{name} keeps {copy}");
        }
        let policy_file = [
            "/plan.rs",
            "/policy.rs",
            "/policy_parse.rs",
            "/policy_validation.rs",
        ]
        .iter()
        .any(|allowed| name.ends_with(allowed));
        if !policy_file {
            assert!(
                !text.contains("slow_down_below"),
                "{name} reads the pace bands"
            );
        }
    }
    Ok(())
}

/// #77: whether two sessions compare is decided only by the kernel's
/// `same_basis`; the engine's review keeps no track-set equality of its own.
#[test]
fn comparison_has_no_second_copy_in_the_engine() -> TestResult {
    let review = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/src/review.rs"
    ))?;
    assert!(review.contains("same_basis("), "review calls same_basis");
    for copy in ["shared(&recap.tracks) ==", "== tracks)", "recap.layout =="] {
        assert!(!review.contains(copy), "review keeps {copy}");
    }
    Ok(())
}

/// #79: when a setting applies is decided only by the kernel's
/// `takes_effect`; the engine's overlay classifies no keys itself.
#[test]
fn settings_effect_has_no_second_copy_in_the_engine() -> TestResult {
    let settings = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/src/settings.rs"
    ))?;
    assert!(
        settings.contains("effect(key.name)"),
        "the overlay asks the kernel"
    );
    for copy in [
        "starts_with(\"display",
        "Kind::Theme",
        "Kind::Layout",
        "fn takes_effect",
    ] {
        assert!(!settings.contains(copy), "engine/settings.rs keeps {copy}");
    }
    Ok(())
}

/// #90: which due item is admitted and served is decided only by the
/// kernel's `review::fifo_*`; the trainer's queue keeps no deque or member
/// set of its own.
#[test]
fn the_due_queue_has_no_second_copy_in_the_trainer() -> TestResult {
    let review = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/review.rs"))?;
    for call in ["fifo_admit(", "fifo_serve(", "fifo_contains("] {
        assert!(review.contains(call), "review.rs calls {call}");
    }
    for copy in ["VecDeque", "HashSet", "push_back", "pop_front"] {
        assert!(!review.contains(copy), "review.rs keeps {copy}");
    }
    Ok(())
}

/// #89: every flag and sequence of the save protocol is the kernel
/// machine's (`save::step`); the engine's storage driver keeps payloads and
/// timestamps only, and no second copy of the flags.
#[test]
fn the_save_protocol_has_no_second_copy_in_the_engine() -> TestResult {
    let storage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/src/storage.rs"
    ))?;
    let engine =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../engine/src/lib.rs"))?;
    assert!(
        storage.contains("cadence_core::save::step("),
        "the driver steps the kernel"
    );
    for copy in [
        "failed: Option<Checkpoint>",
        "sequence: CommitSeq,",
        "storage.sequence",
        "checkpoint_due",
        "self.finishing",
    ] {
        assert!(!storage.contains(copy), "engine/storage.rs keeps {copy}");
    }
    for copy in ["checkpoint_due: bool", "finishing: bool"] {
        assert!(!engine.contains(copy), "engine/lib.rs keeps {copy}");
    }
    Ok(())
}
