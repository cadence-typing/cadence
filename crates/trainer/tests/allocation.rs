//! The default policy's phase split and custom budgets, through the proved
//! kernel allocation (#34).
use cadence_trainer::{allocate, policy::parse_policy};

#[test]
fn default_policy_allocates_documented_split() {
    let policy = parse_policy(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../policies/trainer-v2.toml"
        ))
        .unwrap()
        .as_slice(),
    )
    .unwrap();
    assert_eq!(
        policy.session.default_allocation().unwrap(),
        policy.session.phase_seconds
    );
    // The pinned default table is exact for the default budget.
    assert_eq!(
        policy.session.phase_seconds,
        vec![60, 300, 120, 180, 180, 60]
    );
    assert_eq!(
        policy.session.phase_seconds.iter().copied().sum::<u64>(),
        policy.session.default_budget_seconds
    );
    for (seconds, minimum) in policy
        .session
        .phase_seconds
        .iter()
        .zip(&policy.session.phase_minimum_seconds)
    {
        assert!(seconds >= minimum);
    }
    // Custom budgets apportion exactly (P-02 presets 5..30 minutes).
    for budget in (300..1800).step_by(11) {
        let allocation = allocate(
            budget,
            &policy.session.phase_minimum_seconds,
            &policy.session.allocation_weights().unwrap(),
        )
        .unwrap();
        assert_eq!(allocation.iter().copied().sum::<u64>(), budget);
    }
}
