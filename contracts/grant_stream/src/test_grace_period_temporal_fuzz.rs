#![cfg(test)]

use super::{
    Error, GrantStreamContract, GrantStreamContractClient, GRACE_PERIOD_SECONDS,
    GRACE_PERIOD_SLIPPAGE_LEDGERS, LATE_FEE_BPS, MAX_MISSED_DISTRIBUTIONS, SCALING_FACTOR,
};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const GRANT_ID: u64 = 1;
const MISSED_AMOUNT: i128 = 100 * SCALING_FACTOR;
const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_YEAR: u64 = 365 * SECONDS_PER_DAY;
const INSTANCE_TTL: u32 = 1_000_000;

fn set_ledger(env: &Env, sequence: u32, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.sequence_number = sequence;
        li.timestamp = timestamp;
    });
}

fn setup_grace_period_test(env: &Env) -> (GrantStreamContractClient, Address) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let grant_token = grant_token_addr.address();
    let native_token = native_token_addr.address();
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);
    let recipient = Address::generate(env);

    let contract_id = env.register(GrantStreamContract, ());
    let client = GrantStreamContractClient::new(env, &contract_id);
    client.initialize(
        &admin,
        &grant_token,
        &treasury,
        &oracle,
        &native_token,
    );
    env.deployer()
        .extend_ttl(client.address.clone(), INSTANCE_TTL, INSTANCE_TTL);
    env.as_contract(&client.address, || {
        env.storage().instance().extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });

    let grant_token_admin = token::StellarAssetClient::new(env, &grant_token);
    grant_token_admin.mint(&client.address, &(10_000 * SCALING_FACTOR));

    set_ledger(&env, 100, 1_000);
    client.create_grant(
        &GRANT_ID,
        &recipient,
        &(10_000 * SCALING_FACTOR),
        &SCALING_FACTOR,
        &0_u64,
        &None,
        &None,
    );

    (client, recipient)
}

fn abs_diff(left: u64, right: u64) -> u64 {
    if left >= right {
        left - right
    } else {
        right - left
    }
}

fn balanced_close_sequence(expected_close_seconds: u64, jitter_inputs: &[u64]) -> std::vec::Vec<u64> {
    let lower_room = expected_close_seconds.saturating_sub(5);
    let upper_room = 120_u64.saturating_sub(expected_close_seconds);
    let max_delta = lower_room.min(upper_room);

    if max_delta == 0 {
        return std::vec![expected_close_seconds];
    }

    let mut intervals = std::vec::Vec::new();
    for input in jitter_inputs {
        let delta = 1 + (input % max_delta);
        intervals.push(expected_close_seconds - delta);
        intervals.push(expected_close_seconds + delta);
    }
    intervals
}

fn simulated_seconds_for_ledgers(intervals: &[u64], ledgers: u32) -> u64 {
    let cycle_len = intervals.len() as u64;
    let cycle_seconds: u64 = intervals.iter().sum();
    let ledgers_u64 = ledgers as u64;
    let full_cycles = ledgers_u64 / cycle_len;
    let remainder = (ledgers_u64 % cycle_len) as usize;
    let remainder_seconds: u64 = intervals.iter().take(remainder).sum();
    full_cycles * cycle_seconds + remainder_seconds
}

#[test]
fn test_grace_period_oracle_congestion_60s_ledgers_is_30_days() {
    let env = Env::default();
    let (client, _recipient) = setup_grace_period_test(&env);
    let oracle = client.configure_grace_period_oracle(&60_u64, &GRACE_PERIOD_SLIPPAGE_LEDGERS);
    assert_eq!(oracle.grace_period_ledgers, (GRACE_PERIOD_SECONDS / 60) as u32);

    set_ledger(&env, 5_000, 2_000);
    let state = client.check_default(
        &GRANT_ID,
        &MAX_MISSED_DISTRIBUTIONS,
        &MISSED_AMOUNT,
    );

    let real_seconds = (state.grace_deadline - state.default_ledger) as u64 * 60;
    assert!(abs_diff(real_seconds, GRACE_PERIOD_SECONDS) <= 2 * SECONDS_PER_DAY);
    assert_eq!(state.late_fee, MISSED_AMOUNT * LATE_FEE_BPS / 10_000);
}

#[test]
fn test_grace_period_boundary_accepts_before_deadline_rejects_after() {
    let env = Env::default();
    let (client, _recipient) = setup_grace_period_test(&env);
    client.configure_grace_period_oracle(&60_u64, &GRACE_PERIOD_SLIPPAGE_LEDGERS);

    set_ledger(&env, 10_000, 2_000);
    let state = client.check_default(
        &GRANT_ID,
        &MAX_MISSED_DISTRIBUTIONS,
        &MISSED_AMOUNT,
    );
    let partial_payment = MISSED_AMOUNT / 4;

    set_ledger(&env, state.grace_deadline - 1, 2_000 + GRACE_PERIOD_SECONDS - 60);
    let accepted_before_deadline = client.process_catchup(&GRANT_ID, &partial_payment);
    assert_eq!(accepted_before_deadline.paid_amount, partial_payment);
    assert!(!accepted_before_deadline.resolved);

    set_ledger(&env, state.grace_deadline, 2_000 + GRACE_PERIOD_SECONDS);
    let accepted_at_deadline = client.process_catchup(&GRANT_ID, &partial_payment);
    assert_eq!(accepted_at_deadline.paid_amount, partial_payment * 2);
    assert!(!accepted_at_deadline.resolved);

    set_ledger(&env, state.grace_deadline + 1, 2_000 + GRACE_PERIOD_SECONDS + 60);
    let rejected_after_deadline = client.try_process_catchup(&GRANT_ID, &partial_payment);
    assert_eq!(rejected_after_deadline, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_grace_deadline_is_stored_not_recomputed_after_oracle_change() {
    let env = Env::default();
    let (client, _recipient) = setup_grace_period_test(&env);
    client.configure_grace_period_oracle(&60_u64, &GRACE_PERIOD_SLIPPAGE_LEDGERS);

    set_ledger(&env, 20_000, 3_000);
    let original_state = client.check_default(
        &GRANT_ID,
        &MAX_MISSED_DISTRIBUTIONS,
        &MISSED_AMOUNT,
    );

    let reconfigured = client.configure_grace_period_oracle(&5_u64, &GRACE_PERIOD_SLIPPAGE_LEDGERS);
    assert!(reconfigured.grace_period_ledgers > original_state.grace_deadline - original_state.default_ledger);

    let stored_state = client.get_grace_period_state(&GRANT_ID).unwrap();
    assert_eq!(stored_state.default_ledger, original_state.default_ledger);
    assert_eq!(stored_state.grace_deadline, original_state.grace_deadline);

    set_ledger(&env, original_state.grace_deadline + 1, 3_000 + GRACE_PERIOD_SECONDS + 60);
    assert!(!client.apply_grace_period(&GRANT_ID));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn test_grace_period_real_time_fuzz_with_variable_ledger_closes(
        expected_close_seconds in 5_u64..=120,
        jitter_inputs in prop::collection::vec(0_u64..=120, 4..=16),
        default_sequence in 1_000_u32..=50_000,
    ) {
        let env = Env::default();
        let (client, _recipient) = setup_grace_period_test(&env);
        let close_sequence = balanced_close_sequence(expected_close_seconds, &jitter_inputs);
        let one_year_ledgers =
            ((SECONDS_PER_YEAR + expected_close_seconds - 1) / expected_close_seconds) as u32;
        let simulated_year_seconds =
            simulated_seconds_for_ledgers(&close_sequence, one_year_ledgers);
        let year_drift = abs_diff(simulated_year_seconds, SECONDS_PER_YEAR);
        prop_assert!(
            year_drift * 100 <= SECONDS_PER_YEAR,
            "365-day close-time model drift {}s exceeds 1%",
            year_drift
        );

        let oracle = client.configure_grace_period_oracle(
            &expected_close_seconds,
            &GRACE_PERIOD_SLIPPAGE_LEDGERS,
        );

        set_ledger(&env, default_sequence, 4_000);
        let state = client.check_default(
            &GRANT_ID,
            &MAX_MISSED_DISTRIBUTIONS,
            &MISSED_AMOUNT,
        );

        let grace_ledgers = state.grace_deadline - state.default_ledger;
        prop_assert_eq!(grace_ledgers, oracle.grace_period_ledgers);
        let simulated_real_seconds = simulated_seconds_for_ledgers(&close_sequence, grace_ledgers);
        let drift = abs_diff(simulated_real_seconds, GRACE_PERIOD_SECONDS);

        prop_assert!(
            drift * 100 <= GRACE_PERIOD_SECONDS,
            "grace drift {}s exceeds 1% for expected close {}s over {} ledgers",
            drift,
            expected_close_seconds,
            grace_ledgers
        );
    }
}
