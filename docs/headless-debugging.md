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
`daily-small`, `numpad-small`, `numpad-full` (`12+7`, `-4.50`, `6*3/2`: the
whole keypad, #101), `row-and-pad` or `save-error`, or none for
the real daily plan), then:

- `action` with any command the current state offers (the command table:
  `Begin`, `Continue`, `Pause`, `Resume`, `Help`, `Settings`,
  `NextDestination`, ...; decision 0008); one it does not offer is refused;
- `text` with `inter_key_microseconds`, or single `key` events
  (`Backspace`, `Enter`, `c` with `control` for Ctrl-C);
- `advance` to move the virtual clock (deadlines fire on it);
- `snapshot` for the screen, per-track numerators and denominators,
  active and elapsed time, pending jobs, the saved watermark and the
  available actions (`include_target_text` to see the target);
- `capture` (`cells`, `text` or `both`) for the canonical frame; every
  frame is validated before it is returned.

The profile's own `config.toml` holds the settings (never yours). With no
scenario and no settings file, `hello` opens first-run setup, exactly as
live. The calendar day is virtual (fixed), like the clock, so the daily
plan's content replays identically on any date. `snapshot` also reports
the destination, whether setup or settings are open, the notice, the
theme, held settings jobs and the settings file's text (sealed in traces
unless `--record-content`). Settings scenarios: `commented-config` (a
commented settings file), `config-conflict` (an external edit before the
first write) and `config-changes` (an invalid external edit before the
first reload). `contamination` starts with a transfer probe over a history
of standard, synthetic and imported-text sessions.

Pass `expected_state_revision` (from the last response) to make a request
fail with `StaleState` instead of acting on a screen you did not see.

## Saves, barriers and faults

`hello` with `"fixture_delivery":"manual"` holds each save as a job named
in `snapshot.pending_jobs`; `deliver` releases it as `success`, `failure`,
`stale_success`, `stale_failure` or `duplicate`. A `barrier` (`causal` or
`durable`, through an event sequence) returns only when its scope has
drained; with manual delivery it fails at once with `BarrierTimeout` and
the pending job ids. The `save-error` scenario refuses every save. With
manual delivery a settings write or reload is held the same way
(`snapshot.pending_settings_jobs`) and `deliver` answers it as `success`
(the real write) or `failure`; reopening the settings overlay starts a new
epoch, so a late answer to the earlier overlay changes nothing.

## Journeys

`fixtures/journeys/*.jsonl` are recorded traces of the H-05 journeys;
`fixtures/journeys/requests/` holds the requests they were recorded from.
They are the twelve of H-05: 01 first run, setup and the daily plan; 02
mistype, Backspace and ending early; 03 numpad zeros, decimal and Enter; 04
number-row and numpad histories; 05 deadlines, pause and the boundary key;
06 stale settings results; 07 lost acknowledgement and retry; 08 parent and
child saves under barriers; 09 a settings update, an invalid file and a
conflict; 10 a tiny terminal; 11 a full trace (the gate also replays a copy
with one counter changed and one with an input missing: each must fail at
its first divergence); 12 a transfer probe and contamination exclusions.
`./ci gate headless` replays them all. After an intended behaviour change,
re-record the affected journey from its request file and review the diff
of its responses before committing.
