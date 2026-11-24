use super::*;
use crate::{LayoutSupportsPound, validate_target};

fn sample() -> Result<AttemptLedger, Box<dyn std::error::Error>> {
    Ok(AttemptLedger::new(
        validate_target("cat", LayoutSupportsPound::US)?,
        ExtraInputAllowance::new(2)?,
    )?)
}
#[test]
fn correct_counter_exhaustion_does_not_partially_insert() -> Result<(), Box<dyn std::error::Error>>
{
    let mut ledger = sample()?;
    ledger.counters.inner.correct = u64::MAX;
    let before = ledger.clone();
    assert!(
        ledger
            .apply(LedgerEvent::Content(TrainingChar::new(
                'c',
                LayoutSupportsPound::US
            )?))
            .is_err()
    );
    assert_eq!(ledger, before);
    Ok(())
}
#[test]
fn overflow_counter_exhaustion_does_not_change_buffer_or_content_count()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = sample()?;
    for c in "cat".chars() {
        ledger.apply(LedgerEvent::Content(TrainingChar::new(
            c,
            LayoutSupportsPound::US,
        )?))?;
    }
    ledger.counters.inner.overflow = u64::MAX;
    let before = ledger.clone();
    assert!(
        ledger
            .apply(LedgerEvent::Content(TrainingChar::new(
                'x',
                LayoutSupportsPound::US
            )?))
            .is_err()
    );
    assert_eq!(ledger, before);
    Ok(())
}
#[test]
fn counted_deletion_overflow_is_atomic() -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = sample()?;
    for c in "cat".chars() {
        ledger.apply(LedgerEvent::Content(TrainingChar::new(
            c,
            LayoutSupportsPound::US,
        )?))?;
    }
    ledger.counters.inner.deletions = u64::MAX.checked_sub(1).ok_or("fixture underflow")?;
    let before = ledger.clone();
    assert!(ledger.apply(LedgerEvent::DeleteWord(3)).is_err());
    assert_eq!(ledger, before);
    Ok(())
}
