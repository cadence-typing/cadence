# Debugging with the headless driver

A short recipe for agents and humans (H-01 to H-05). Every step uses the
same reducer and renderer as live practice; time is virtual, so nothing
sleeps and every run is repeatable.

## Reproduce from a trace

```sh
cadence replay /abs/trace.jsonl --read-only --capture-dir /abs/captures
```

Replay runs on a fresh temporary profile and prints one JSON object. On a
divergence it names the first differing line, the first differing field
(`response.result.tracks[0].first_pass.correct`) and both values, and
exits non-zero. A trace recorded without `--record-content` holds digests
in place of practice text; it still replays.

Record a trace while driving:

```sh
cadence headless --script /abs/requests.jsonl --ephemeral-profile \
  --record /abs/trace.jsonl [--record-content]
```

## Drive keys and inspect state

Requests are one JSON object per line (`contracts/headless-request.schema.json`).
Start with `hello` (a fixture `scenario` such as `scoring-cat`,
`daily-small`, `numpad-small`, `row-and-pad` or `save-error`, or none for
the real daily plan), then:

- `action` `StartDrill`, `Pause`, `Resume`, `NextPhase`;
- `text` with `inter_key_microseconds`, or single `key` events
  (`Backspace`, `Enter`, `c` with `control` for Ctrl-C);
- `advance` to move the virtual clock (deadlines fire on it);
- `snapshot` for the screen, per-track numerators and denominators,
  active and elapsed time, pending jobs, the saved watermark and the
  available actions (`include_target_text` to see the target);
- `capture` (`cells`, `text` or `both`) for the canonical frame; every
  frame is validated before it is returned.

Pass `expected_state_revision` (from the last response) to make a request
fail with `StaleState` instead of acting on a screen you did not see.

## Saves, barriers and faults

`hello` with `"fixture_delivery":"manual"` holds each save as a job named
in `snapshot.pending_jobs`; `deliver` releases it as `success`, `failure`,
`stale_success`, `stale_failure` or `duplicate`. A `barrier` (`causal` or
`durable`, through an event sequence) returns only when its scope has
drained; with manual delivery it fails at once with `BarrierTimeout` and
the pending job ids. The `save-error` scenario refuses every save.

## Journeys

`fixtures/journeys/*.jsonl` are recorded traces of the H-05 journeys;
`fixtures/journeys/requests/` holds the requests they were recorded from.
`./ci gate headless` replays them all. After an intended behaviour change,
re-record the affected journey from its request file and review the diff
of its responses before committing.
