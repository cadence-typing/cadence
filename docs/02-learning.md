# Learning, scoring and the adaptive planner

## L-01 Three separate objects

1. **Observation ledger:** what was displayed, what input arrived, when it arrived, and what was corrected. Historical facts are immutable once committed.
2. **Learner model:** derived error/timing/review summaries by skill key. It can be rebuilt or migrated without editing observations.
3. **Planner policy:** versioned decisions about what to practise next. Policy changes do not rewrite old scores.

`SkillKey` includes track, layout revision, content class and pattern identity. Numpad `12` and number-row `12` are different skills even when the terminal reports the same characters. Annotate physical-source evidence as `VerifiedKeypad`, `VerifiedMain`, or `Unverified`; do not infer it from content. Symbols/case and code/prose retain separate comparability metadata.

## L-02 Input and scoring semantics

V1 training alphabet: printable ASCII, explicit newline, and `£` where the chosen layout supports it. Imported prose may opt into a documented typography-to-ASCII normalization (smart quotes, em/en dashes and non-breaking spaces); show a preview and preserve the original. Code defaults to exact bytes within the supported UTF-8 alphabet. Reject unsupported training characters before a scored session with line/column diagnostics. UI text itself remains Unicode-aware; this limited training alphabet avoids pretending IME composition has single-keystroke timing. Wider language/input support is a future contract change.

`TextUnitIndex` identifies an expected logical training unit, not a UTF-8 byte or terminal cell. One ASCII character/£/newline is one unit. Wraps introduced by the renderer do not become expected input. In ordinary word/prose practice, edit only at the tail: append and Backspace (optional explicit delete-current-word); arrows never reposition the typing caret. This makes alignment unambiguous. Mistyped input advances the attempted position and remains highlighted; correct it with Backspace. Do not silently skip a wrong character or automatically resynchronize on a space.

The target is immutable for an exercise. Maintain:

- `first_outcome[position]: Option<Correct | Incorrect>`: assigned on the first content insertion at that expected position and never replaced.
- `attempt_count`, `correct_attempt_count`, `deletion_count`, `overflow_attempt_count`: cumulative event facts.
- A bounded current typed buffer, and a committed frontier of already completed exercises.
- Per-word/entry flags for any error, deletion, interruption or paste in that attempt.

First-pass accuracy = first-correct positions / first-attempted expected positions. A later correction never changes either historical position outcome. Deleting a correct character and retyping incorrectly leaves that first outcome correct, but reduces attempt accuracy and marks the word non-clean. Show attempt accuracy in detailed results so this distinction is visible. Attempt accuracy = correct content insertions / all content insertions, including overflow. Backspace is a correction event, not a content insertion in that denominator. Never average per-exercise percentages: aggregate numerators and denominators.

Example target `cat`, input `c x Backspace a t`: first-pass = 2/3; attempt accuracy = 3/4; one deletion; current buffer exact; clean-word false. A successful correction is useful behaviour, but it is not an error-free first attempt. If the user abandons after `c x`, first-pass is 1/2 and no completed word is awarded. Restart creates a new attempt linked to the abandoned one; it does not delete the prior facts or grant a clean spaced review.

Content beyond the target length has no expected slot, counts as incorrect attempt/overflow, and prevents completion until deleted. Buffer length is bounded by target units plus a named extra-input allowance. Exceeding that bound visibly rejects further content and marks measurement interrupted; never silently discard keys while claiming an ordinary score. Empty attempts and zero-duration observations yield `None`, not infinity, 0% accuracy, or a made-up baseline.

Correct WPM = `correct_visible_or_committed_units × 60 / (5 × active_seconds)`. Correct units at an unfinished slice are positionally matching units, not all keys pressed. Completed slices retain their contribution exactly once. Editing/repeating does not increase the numerator. Spaces, punctuation and explicit newlines are units; renderer line wraps and submit-only numpad Enter are not. Use exact rational/count-duration arithmetic internally; format at the UI edge. The conventional five-unit definition belongs in one named constant.

Numpad: correct digits/min counts matching digits only; decimal/operator units contribute to accuracy, not the digit numerator. Exact-entry rate measures submitted exact strings (useful in free-flow assessments). **Clean-entry rate** measures entries that were exact without any error/deletion in their first attempt (the primary numeric precision measure). In correction-required drills, exact submitted entries will normally be 100%; do not present that as evidence of improved first-pass skill. Enter submits only; it must not auto-advance a wrong value. Leading zeros and decimal places are significant.

Expected-pattern observations are anchored to target positions. A bigram/trigram occurrence is first-correct only if all constituent expected positions were first-correct; it is counted once when its final position is first attempted. Re-typing an occurrence does not add a new training success. Overlapping patterns may each get one observation, but the overall accuracy ledger still counts positions once. Word observations require a completed boundary; incomplete words are retained as partial evidence only. Keep error and timing evidence denominators separate.

## L-03 Time and measurement quality

Use `MonoTick` (checked microseconds since the run's timing origin) for intra-run timing and injected UTC/calendar values for review scheduling. Stamp terminal input when read at ingress, before the engine queue. Record reducer consumption order separately. This measures terminal-to-app arrival, not electrical switch actuation. Input from a stalled pipe, paste, recorded macro or invalid ordering cannot produce credible physical cadence evidence.

The stage clock starts on the first valid content key, stops at its allocated deadline, and excludes only explicit pause intervals. Ordinary reading/thinking/hesitation remains in active time. No silent trimming of long gaps. Focus loss may pause when the terminal actually reports it; unavailable focus reporting is not fabricated. Any paused or interrupted benchmark is ineligible for comparison, though its results remain visible. Manual practice can show active WPM with a pause label. On crash/restart, a resumed exercise is an interrupted new timing segment; never subtract wall-clock downtime from a claimed uninterrupted trial.

Inter-key intervals for model timing use adjacent first-attempt, correct content input within a contiguous clean segment. Exclude pairs crossing a correction, pause, service interruption, paste, stage/exercise boundary or known transport degradation. Keep word-start latency separate from within-word transitions. Do not divide a word's total duration evenly among its keys. Reject negative/backward timestamps; equal timestamps are possible at the transport and must not create infinite speed. Count them as insufficient timing resolution. Store named reasons for excluded timing samples.

For resource overload, loss of ingress ordering, a gap in sequence numbering, or a corrupt trace, pause/unqualify measurement rather than produce a normal score. Auto-repeat events are ignored as held-key repeats when reliably identified; do not classify repeated-letter words as repeats on legacy terminals where no distinction is available. Bracketed paste is rejected in practice and cannot become keystroke evidence. Explicit headless scripted input is tagged synthetic and stored only in its sandbox profile.

## L-04 Statistical model: transparent and conservative

V1 uses a bounded rolling observation window per skill (last 128 valid occurrences with contributing session/day identities), all-time aggregate counts, and robust recent timing summaries. This window is a policy choice, not a published optimum. Unit, bigram, trigram and frequent whole-word models share the same evidence rules. Do not instantiate a dense Cartesian product of all possible words/patterns.

Error estimate: Beta(1,19) prior for error probability, with posterior mean `(recent_errors + 1) / (recent_observations + 20)`. This smooths sparse estimates. It is not an independently validated psychological model. Mark evidence “sparse” below 20 observations or fewer than two separate sessions; show counts, not a spurious precise confidence percentage. Shrink sparse whole-word timing toward its constituent pattern class and fall back to broad sampling. Unknown does not mean mastered.

Timing: median and interquartile range of valid within-class intervals; no unstable global mean from a few samples. Derive a hesitation score relative to the user's clean timing in the same track and comparable transition class. Do not demand the same latency for every key reach. Lack of sufficient clean timing means no timing score, with weights re-normalized over available evidence. A slow unfamiliar word can reflect reading/planning as well as motor skill; explanations must not diagnose finger or cognitive causes.

Candidate utility is bounded fixed-point arithmetic (`BasisPoints`, 0..10,000). Initial weights in `policies/trainer-v1.toml`: error 5,000; hesitation 2,500; relevance 1,500; exploration/uncertainty 1,000. Weighted sum divided by the sum of available weights is in range; no floating NaN/order instability. Relevance comes from corpus frequency and explicit pack selection, never hidden monitoring of the user's work. The score ranks within a lane, while lane quotas protect review and breadth. Preserve the score components and a reason code for explanation.

No estimate or coefficient is a guaranteed optimal learning algorithm. Include a policy-comparison mode in developer simulations: fixed mixed practice versus adaptive practice on synthetic learner models. This detects pathological allocation, not human efficacy. Actual effectiveness is judged through repeated comparable transfer probes and the owner's experience.

## L-05 Selection lanes and termination

Adaptive words use blocks of 10 scheduled word slots: 6 targeted, 2 due review, 2 breadth. A fixed interleaved lane order is `[target, broad, target, review, target, target, broad, target, review, target]`. Prefixes therefore remain mixed when the stage ends mid-block. A focus unit may own at most 3 targeted slots per block. An exact word may appear at most twice per block and at most twice consecutively, allowing a short useful repetition without a drill trap. Incidental occurrences of that unit elsewhere are not claimed to satisfy this exposure cap; the cap governs the planned focus label.

Review takes the oldest continuously eligible item from a FIFO due-admission queue. Admission order is monotonic when an item first becomes due; a newly imported overdue item joins at the tail rather than jumping ahead. An item cannot be queued twice. Following review, remove it and compute its next due date. Paused/disabled tracks preserve their queue but have no service guarantee while disabled. A bounded backlog must expose a diagnostic if at capacity; never silently drop review work. Review is a budgeted opportunity, not a demand to clear all due items today.

For each slot, query indexed eligible candidates, bounded to 256 materialized entries. The bounded scan selects the highest utility eligible candidate with seeded tie-breaking. Use an established pinned deterministic PRNG and record its algorithm/version/seed; never depend on `HashMap` iteration. The same inputs produce the same plan on supported builds. Category and word/focus eligibility checks are shared functions, not duplicated per lane.

Fallback order is explicit: targeted → due-review → breadth; due-review → breadth → targeted; breadth → due-review → targeted. Keep repetition/cap/content safety constraints during fallback. A tiny corpus with no legal candidate returns `NoEligibleCandidate`, with a plain explanation and an offer to broaden content. No unbounded resampling loop, hidden constraint relaxation or arithmetic divide-by-zero. The release corpus must pass an offline capacity check for all default lane configurations.

Fairness claim is limited and precise: when due work is continuously eligible and at least one review slot is served per block, a queued item with `q` eligible predecessors is served within at most `q+1` served review slots, assuming no insertion ahead, completion of those slots and bounded worker progress. Two reserved slots per full block sharpen the block bound. Do not claim finite-time coverage of every word in an expanding corpus; breadth is a lane quota, not a guarantee that random selection visits every item.

## L-06 Review and pace policy

Review intervals begin at 1, 3, 7, 14 and 30 local-calendar days. Two clean completions on different sessions/days are required before promoting an item beyond its current interval. Same-session repetitions count as practice, not independent retention checks. An error returns the item to the 1-day level; it may get one short reinforcement later in the current session, within repetition caps. Timing alone never labels a word forgotten. Check additions for overflow and record calendar/timezone identity; wall-clock rollback cannot make all items instantly overdue or grant future success.

Default desired first-pass accuracy is 98%. Pace is a soft goal, not a moving forced cursor. Raise a track's suggested target by at most 3% after three qualifying blocks across at least two sessions, each with at least 100 first-attempt positions and accuracy at/above the threshold. Also require no material attempt-accuracy degradation. A block below 95% lowers the suggested target by up to 5% and increases familiar practice within targeted slots. Values between bands keep the pace steady. Insufficient evidence keeps the current target. The initial target is the robust baseline of clean samples; it is absent until known. Bounds and hysteresis belong in policy, with integer-rational comparisons tested at equality boundaries.

No progression inference crosses layout changes, incompatible corpus difficulty classes, paused benchmarks or synthetic sessions. A theme change does not reset learning. A changed layout starts a separate profile partition while preserving earlier history. A minor display fix never silently becomes a new scoring version.

## L-07 Required correctness properties

Scoring: first outcomes are write-once; counters are bounded; corrections cannot improve old first-pass accuracy; completed contributions are committed once; fractional aggregation is exact; timing exclusions are explicit.

Planner: exact time budget, legal candidates only, fixed finite selection work, deterministic replay, lane quotas when feasible, explicit failure when infeasible, repetition/focus bounds, no duplicate due entries, and fairness under stated eligibility assumptions.

Progress: observations retain policy/corpus/layout/measurement identity; benchmark trends compare like-for-like; inability to prove comparability displays an unavailable comparison. These software properties are the subject of tests/formal work. Human learning benefit is not a theorem in this project.
