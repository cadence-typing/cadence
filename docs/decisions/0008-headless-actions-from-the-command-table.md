# 0008: Headless actions are the command table's actions

Status: accepted (2026-11-26), issue #80. Revises
`contracts/headless-request.schema.json` (decision 0005, 0007);
`MANIFEST.sha256` records the new digest.

## Change

The `action` operation's `name` enum becomes the typed command table's
actions (`cadence_engine::commands::Action`): `Begin`, `Continue`, `Quit`,
`Pause`, `Resume`, `EndAndReview`, `SaveAndExit`, `RetrySave`, `ForceExit`,
`FinishInvalid`, `SubmitEntry`, `TextSize`, `NextDestination`,
`PreviousDestination`, `Help`, `Settings`, `NextTrack`, `PreviousTrack`,
`Longer`, `Shorter`. A request naming an action the current state's table
does not offer is refused with `InvalidArgument` and changes nothing; the
footer, help and key dispatch read the same table.

Kept as aliases, because recorded journeys use them: `StartDailyPlan` and
`StartDrill` (`Begin`), `NextPhase` (`Continue`), `OpenSettings`
(`Settings`).

Removed: `SetTheme`, `ReloadSettings` and `CloseOverlay`. The beta.2 driver
accepted them in the schema but answered every one with "this action is
not offered headless yet", so no client could depend on them; a theme is
set and settings reloaded through the settings overlay (#79).

The `track` and `theme` arguments are unchanged.
