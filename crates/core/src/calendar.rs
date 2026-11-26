//! Local-calendar days (L-06, #52). The decisions are the proved
//! `cadence_kernel::calendar` functions (Lean: the day is the floor of the
//! local time over a day; a later time never gives an earlier day; an
//! offset beyond 18 hours is refused; a day count is never negative).
pub use cadence_kernel::calendar::{Day, days_ago, local_day};

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small constant offsets; overflow is impossible by construction"
)]
mod tests {
    use super::*;

    /// 2026-11-26T00:00:00Z.
    const MIDNIGHT_UTC: i64 = 1_795_651_200;
    const HOUR: i64 = 3_600;
    const HALF_HOUR: i64 = 1_800;

    fn day(unix: i64, offset_hours: i32) -> i64 {
        match local_day(unix, offset_hours * 3_600) {
            Day::Number(d) => d,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn local_midnight_separates_days_at_minus_eight_and_plus_ten() {
        for offset in [-8, 10] {
            // 23:30 and 00:30 local, an hour apart.
            let local_midnight = MIDNIGHT_UTC - i64::from(offset) * HOUR;
            let before = local_midnight - HALF_HOUR;
            let after = local_midnight + HALF_HOUR;
            assert_eq!(day(after, offset), day(before, offset) + 1, "UTC{offset:+}");
        }
    }

    #[test]
    fn utc_midnight_inside_one_local_day_is_one_day() {
        // 23:30Z and 00:30Z are 15:30 and 16:30 at UTC-8: the same local day,
        // so a pair of sessions there cannot promote a word.
        let before = MIDNIGHT_UTC - HALF_HOUR;
        let after = MIDNIGHT_UTC + HALF_HOUR;
        assert_eq!(day(before, -8), day(after, -8));
        assert_ne!(day(before, 0), day(after, 0));
    }

    #[test]
    fn offsets_beyond_eighteen_hours_are_refused() {
        assert_eq!(
            local_day(MIDNIGHT_UTC, 18 * 3_600 + 1),
            Day::OffsetOutOfRange
        );
        assert_eq!(
            local_day(MIDNIGHT_UTC, -18 * 3_600 - 1),
            Day::OffsetOutOfRange
        );
        assert_eq!(local_day(0, -3_600), Day::Overflow);
    }

    #[test]
    fn days_ago_is_never_negative() {
        assert_eq!(days_ago(20_000, 19_990), 10);
        assert_eq!(days_ago(20_000, 20_000), 0);
        // After travel west the local day can step back: still 0, not -1.
        assert_eq!(days_ago(20_000, 20_001), 0);
    }
}
