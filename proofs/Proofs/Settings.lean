import Extracted.CadenceKernel

/-!
# When a setting takes effect (`cadence_kernel::settings`, #79)

`cadence-engine`'s settings overlay asks `settings.takes_effect` for every
changed key when a write is acknowledged: `Now` repaints, `NextSession` is
held until the next session starts.

Trusted, not proved: that the engine maps each `config.toml` key to its
`Kind` (a total match, tested) and applies the answer.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Settings

/-- The settings that change what is practised or how it is scored, listed
independently of the implementation. -/
def scoreAffecting : settings.Kind → Bool
  | .Minutes | .Tracks | .Emphasis | .AccuracyTarget | .Breadth | .Packs
  | .NumpadOperators | .Layout | .Numpad | .DecimalSeparator => true
  | .LivePace | .Theme | .TextSize | .KeyGuide | .HighContrast => false

/-- A score-affecting setting never applies now, and every other setting
applies now: a change waits for the next session exactly when it affects
the score. Total. -/
theorem score_affecting_never_now (k : settings.Kind) :
    settings.takes_effect k ⦃ e => (e = settings.Effect.NextSession ↔ scoreAffecting k = true) ⦄ := by
  unfold settings.takes_effect
  cases k <;> simp [scoreAffecting, WP.spec_ok]

end Proofs.Settings
