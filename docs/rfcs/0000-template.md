---
rfc: "YYYY-MM-DD-kebab-title"
title: "Short descriptive title"
status: draft
implementation: not-started
authors:
  - handle
created: YYYY-MM-DD
updated: YYYY-MM-DD
issue: null
milestone: null
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Short descriptive title

Copy this file to `YYYY-MM-DD-kebab-title.md`, fill in the tracking issue and other metadata, and add the draft to the [registry](README.md). Replace each prompt with an answer. Leave `milestone` as `null` unless one has been assigned.

## Summary and motivation

State the decision, the problem it solves, the evidence that the problem exists, and what is outside the decision. Why is an issue alone not enough?

## Observable behaviour

Show it from the learner's point of view: the screen, command, output, saved data or refusal. Distinguish before from after. If scores, timing or what counts as an error change, say so in the first sentence.

## Proof obligations

If a kernel decision (scoring, timing, accuracy, checkpoint credit, plan arithmetic) changes, name the shipped Rust entry points, each Lean theorem over the Aeneas translation, the mutant that must break it, and the host assumptions it relies on. Name what remains tested, not proved. If no proved behaviour changes, say why.

## Design and boundaries

Name the owning crates and their tauten tiers, the dependency direction, and who can cause effects. Remember: one state owner, typed commands and events, and no I/O on the input → reduce → render path. For a protocol, specify ordering, interruption and recovery. For a persisted format, cover schema, migration and refusal of unknown versions.

## Compatibility and reversibility

What does an existing profile observe? Cover migration, downgrade, backup and export. Explain the cost of undoing this decision.

## Alternatives

Compare the strongest alternative, including leaving things as they are. What evidence would change the choice?

## Evidence and rollout

List the proof, test, PTY capture or other check that establishes each claim. Describe independently safe steps and the observable finish line. Link issues for split work; do not guess a release date.

## Unresolved questions

List what must be answered before acceptance. Name the owner decision needed and reflect blockers in `blocked_on`.

## Decision log

Record dated owner decisions, the rationale and material amendments. On implementation, note any differences from the accepted design and update the frontmatter and registry. A merged draft is not owner acceptance.
