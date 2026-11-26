//! Pack sessions (D-03, L-02, #85): practice over an imported text pack or a
//! shipped code pack, through the ordinary plan and reducer. A pack session
//! is a single-track plan whose exercises are the pack's passages; it never
//! serves a transfer probe and sets no pace goal, and it is stored with its
//! content class (imported or code) and the pack's digest, so it is never
//! compared with, and never feeds, standard practice.

use cadence_core::alphabet::{Layout, TrainingChar};
use cadence_core::progress::ContentClass;
use rand::seq::SliceRandom;

use crate::plan::{PlanRng, PlannedExercise, SessionPlan};
use crate::{ExerciseKind, ProbePlan};

/// A loaded pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// Its name.
    pub name: String,
    /// Imported text or code.
    pub class: ContentClass,
    /// SHA-256 of its passages, hex.
    pub digest: String,
    /// Its passages, as training units; code keeps newlines as units.
    pub passages: Vec<Vec<TrainingChar>>,
}

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn units(text: &str, layout: Layout) -> Result<Vec<TrainingChar>, String> {
    text.chars()
        .map(|c| {
            TrainingChar::new(c, layout)
                .map_err(|_alphabet| format!("U+{:04X} is not trainable", u32::from(c)))
        })
        .collect()
}

/// Decodes a code pack's snippet: `\n` is a newline, `\\` a backslash.
fn decode(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => return Err(format!("unknown escape \\{other}")),
                None => return Err("a trailing backslash".into()),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// A shipped code pack (`id<TAB>snippet` lines; see `tools/corpus/check_code.py`).
///
/// # Errors
/// A malformed line or untrainable character.
pub fn code_pack(name: &str, tsv: &str) -> Result<Pack, String> {
    let mut passages = Vec::new();
    for (index, line) in tsv.lines().enumerate() {
        let (_, snippet) = line
            .split_once('\t')
            .ok_or_else(|| format!("{name}: line {}: no tab", index.saturating_add(1)))?;
        let text = decode(snippet)
            .map_err(|e| format!("{name}: line {}: {e}", index.saturating_add(1)))?;
        passages.push(units(&text, Layout::Us)?);
    }
    if passages.is_empty() {
        return Err(format!("{name}: no snippets"));
    }
    Ok(Pack {
        name: name.to_string(),
        class: ContentClass::Code,
        digest: digest(tsv),
        passages,
    })
}

/// An imported pack's `passages.txt` (#84): one passage per line.
///
/// # Errors
/// An untrainable character (the importer never writes one) or no passage.
pub fn imported_pack(name: &str, passages_txt: &str, layout: Layout) -> Result<Pack, String> {
    let passages = passages_txt
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| units(line, layout))
        .collect::<Result<Vec<_>, _>>()?;
    if passages.is_empty() {
        return Err(format!("{name}: no passages"));
    }
    Ok(Pack {
        name: name.to_string(),
        class: ContentClass::Imported,
        digest: digest(passages_txt),
        passages,
    })
}

/// Makes a single-track `plan` a pack session: its practice phase serves the
/// pack's passages in seeded order, enough for `units_per_second` over the
/// phase; no probe, no pace goals, no focus.
///
/// # Errors
/// A plan that is not single-track.
pub fn pack_session(
    mut plan: SessionPlan,
    pack: &Pack,
    rng: &mut PlanRng,
    units_per_second: u64,
) -> Result<SessionPlan, String> {
    // The one practice phase; the closing review of the session's own
    // misses stays as it is.
    let mut practice = plan
        .phases
        .iter_mut()
        .filter(|phase| phase.phase != crate::Phase::Review);
    let (Some(phase), None) = (practice.next(), practice.next()) else {
        return Err("a pack session is a single-track plan".into());
    };
    let needed = phase
        .seconds
        .checked_mul(units_per_second)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("pack volume overflow")?;
    let mut order: Vec<usize> = (0..pack.passages.len()).collect();
    order.shuffle(rng);
    let mut prepared = 0_usize;
    let mut exercises = Vec::new();
    // Cycle the pack when it is shorter than the session needs.
    for index in order.iter().cycle().take(order.len().saturating_mul(8)) {
        if prepared >= needed {
            break;
        }
        let Some(units) = pack.passages.get(*index) else {
            continue;
        };
        prepared = prepared.saturating_add(units.len());
        let mut exercise = PlannedExercise::plain(units.clone(), ExerciseKind::Prose);
        exercise.caption = Some(format!("{} · your own text", pack.name));
        exercises.push(exercise);
    }
    phase.exercises = exercises;
    plan.probe = ProbePlan::NotDue;
    plan.pace.clear();
    plan.record.focus.clear();
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_snippets_decode_newlines_and_backslashes() {
        let pack = code_pack("t", "rust-001\tfn a() {\\n    \"x\\\\n\"\\n}\n").unwrap();
        let text: String = pack.passages[0].iter().map(|u| u.get()).collect();
        assert_eq!(text, "fn a() {\n    \"x\\n\"\n}");
        assert!(code_pack("t", "rust-001\tbad \\q\n").is_err());
        assert_eq!(pack.class, ContentClass::Code);
    }

    #[test]
    fn the_shipped_code_packs_load() {
        for (name, tsv) in [
            (
                "code-rust",
                include_str!("../../../corpus/code-rust-v1.tsv"),
            ),
            (
                "code-shell",
                include_str!("../../../corpus/code-shell-v1.tsv"),
            ),
        ] {
            let pack = code_pack(name, tsv).unwrap();
            assert!(pack.passages.len() >= 150, "{name}");
        }
    }
}
