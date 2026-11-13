# 0003 — Learn from mistakes; vary natural prose (alpha.4)

Status: accepted by the owner's request of 2026-11-13 ("review the training
algorithms… they don't learn from my mistakes… natural writing is basically
the same text; plan, execute, release and install").

## Findings that motivate this change (independent review, `0f9e006`)

- Nothing typed influences what is shown next, within or across sessions.
  `build_session_plan` runs before the store opens and takes no history;
  every word candidate has all-zero `UtilityInputs`, so "adaptive words"
  was a seeded random draw (one third from the unvetted tail band).
- The seed is the UTC day, so same-day sessions are identical.
- Prose is a 2,832-unit bank, chunked in file order and served from the
  first paragraph every session: the owner has only ever seen ~6 of 16
  paragraphs.
- Persistence keeps only per-track totals; no per-key, per-bigram or
  per-word error or timing evidence exists. `review.rs`/`pace.rs` and the
  `[statistics]`/`[review]` policy sections have no production caller.
- The "review" phase is the summary screen.

## Scope of alpha.4

1. **Observation capture (reducer, bounded).** At each exercise boundary
   the engine folds the ledger into a per-session `SessionLearning`:
   per-key and per-bigram first-attempt counts/errors, clean inter-key
   intervals (adjacent, first-attempt-correct pair; no deletion between;
   not across a pause; capped by a policy hesitation ceiling — the cap
   applies to the learner model only, never to displayed scores), per-word
   attempt/error outcomes, and prose passages served. Main keyboard and
   numpad are separate skill lanes (L-01).
2. **Schema 3 (additive).** New tables `session_plans`, `skill_observations`,
   `word_outcomes`, `prose_seen`. A populated schema-2 database migrates in
   one transaction; its new tables start empty — per-key history cannot be
   reconstructed from track totals and is not backfilled. Checkpoints with
   no learning payload keep the exact v1 digest so existing receipts verify;
   a learning payload extends the canonical encoding with a tagged section.
   Export gains `learning` and `schema_version: 3`.
3. **Learner model (startup only).** From the most recent sessions (policy
   window) with per-session exponential decay: Beta-prior posterior error
   rate per skill, hesitation as the skill's mean clean interval relative to
   the lane median, and uncertainty for sparse evidence. Sparse is never
   treated as mastered.
4. **Weakness-weighted words.** A startup inverted index from bigram to
   core/common words. The planner chooses a small set of focus skills and
   fills Target slots from their posting lists with real utility inputs
   (error, hesitation, frequency relevance, uncertainty) via the existing
   bounded `scan_select`. Broad slots keep breadth. Review slots serve
   due missed words (spaced repetition: 1/3/7/14/30-day ladder,
   promotion needs clean typing in different sessions, an error resets).
5. **In-session reinforcement.** Words missed in this session re-enter a
   later exercise's review slots (bounded FIFO, each word at most the
   policy's same-session reinforcement count). Substitution is O(block)
   at an exercise boundary and is recorded.
6. **Prose variety.** A much larger verbatim public-domain prose bank with
   a recorded extraction recipe and digests. Passages are chosen by seeded
   shuffle excluding passages served within a no-repeat window, falling
   back to least-recently-seen; a true repeat is labelled. Prose stays
   unadapted: it is the transfer check.
7. **Per-session seed** from (session id, day), persisted; `--seed` still
   overrides.
8. **Honest labelling.** The planner mode (cold start vs adaptive) and
   focus skills are persisted per session and shown on the start and
   summary screens ("Focus: th · ou · w"). Scores are never adjusted;
   targeted words are harder by design and are labelled as such.
9. **Hygiene.** New `policies/trainer-v2.toml` (v1 is a manifest-pinned
   handover contract and stays unchanged) carrying the new `[learning]`
   section plus the previously naked prose-chunk and words-per-exercise
   numbers.

## Out of scope (stated limits)

- Numpad/number-row content stays cold-start random; their observations
  are recorded so a later change can adapt them.
- Pace hysteresis and benchmark probes remain unwired.
- No formal proofs are claimed for the new code; it has behavioural tests.
- Adaptation starts from the first session after the upgrade; the owner's
  ten existing sessions contain no per-key evidence.

## Verification

Unit tests at the cheapest real layer: ledger→observation folding,
including the L-02 corrected `cat` example and timing exclusions; learner
posterior, decay and sparse uncertainty; planner (synthetic learner weak on
`th` receives more `th` targets than cold start; empty history is
deterministic; bounds hold); prose no-repeat window and fallback; engine
reinforcement; schema 2→3 migration of a populated database with digests
intact; learning round-trip. Canonical offline Docker Compose quality,
static musl dist, GitHub prerelease, checksum-verified atomic install with
a pre-upgrade backup of the owner's profile.

## Integration evidence (alpha.4 delivery)

- Clean source `123b044bbb04594010c7c63c34ee23e34c08c6a9`; canonical offline
  Compose quality passed (fmt, strict Clippy, 180 tests, boundaries, 48
  contract hashes). Static musl bundle: binary SHA-256
  `e78f91685974fe3c32e29628be6b5df01aeeed1c1d145775ade5ada9886ba318`.
- Offline container (`--network none`, read-only root, all capabilities
  dropped) migrated a copy of the owner's 10-session schema-2 profile to
  schema 3 with every session and its 2,474 receipts intact and no learning
  evidence invented.
- Real PTY runs of the release binary on that copy: session 1, which fumbled
  `th`/`ou`, re-served missed words mid-session ("missed earlier, again");
  session 2 opened with "Today targets th · ou" and labelled its exercises.
  Prose sessions started on different passages, each attributed.
- An earlier smoke showed noise (unerring, jitter-slow `pe`/`he`) and
  redundant keys (`n`, `r`) in the focus list; explicit focus thresholds and
  the explained-key filter were added with regression tests before release.
- The uploaded assets were downloaded, checksum-verified (tarball binary
  identical to the standalone asset), installed at
  `~/.local/lib/cadence/0.1.0-alpha.4-123b044bbb04/` and activated by an
  atomic symlink swap. The owner's profile was backed up with alpha.3 first
  (`~/.local/share/cadence-backups/`); the installed alpha.4 then opened and
  exported it at schema 3 with all 10 sessions. Alpha.3 remains installed.
