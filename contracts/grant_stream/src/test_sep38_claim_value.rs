#![cfg(test)]

use super::{GrantStreamContract, GrantStreamContractClient, SEP38_STALENESS_SECONDS, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

fn setup_test(env: &Env) -> (Address, Address, Address, Address, Address, GrantStreamContractClient) {
    let admin = Address::generate(env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);

    let contract_id = env.register(GrantStreamContract, ());
    let client = GrantStreamContractClient::new(env, &contract_id);

    client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address());
    let native_admin = token::StellarAssetClient::new(env, &native_token_addr.address());
    native_admin.mint(&client.address, &(100 * SCALING_FACTOR));

    (admin, grant_token_addr.address(), treasury, oracle, native_token_addr.address(), client)
}

fn set_ledger(env: &Env, timestamp: u64, sequence: u32) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
        li.sequence_number = sequence;
    });
}

fn fund_stream(
    env: &Env,
    client: &GrantStreamContractClient,
    grant_token_addr: &Address,
    recipient: &Address,
    grant_id: u64,
) {
    let grant_token_admin = token::StellarAssetClient::new(env, grant_token_addr);
    grant_token_admin.mint(&client.address, &(1_000_000 * SCALING_FACTOR));
    client.create_grant(
        &grant_id,
        recipient,
        &(1_000_000 * SCALING_FACTOR),
        &SCALING_FACTOR,
        &0,
        &None,
        &None,
    );
}

fn usd(env: &Env) -> String {
    String::from_str(env, "USD")
}

#[test]
fn fresh_sep38_claim_value_is_recorded() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);

    set_ledger(&env, 1_000, 10);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 1);
    set_ledger(&env, 1_005, 11);
    client.set_sep38_rate(&usd(&env), &(2 * SCALING_FACTOR), &SCALING_FACTOR, &1_005, &11);
    set_ledger(&env, 1_010, 12);

    client.withdraw(&1, &(5 * SCALING_FACTOR));

    assert_eq!(grant_token.balance(&recipient), 5 * SCALING_FACTOR);
    let value = client.get_claim_value(&1, &1).unwrap();
    assert_eq!(value.grant_id, 1);
    assert_eq!(value.claim_index, 1);
    assert_eq!(value.recipient, recipient);
    assert_eq!(value.token_address, grant_token_addr);
    assert_eq!(value.token_amount, 5 * SCALING_FACTOR);
    assert_eq!(value.fiat_asset, usd(&env));
    assert_eq!(value.rate, 2 * SCALING_FACTOR);
    assert_eq!(value.rate_scale, SCALING_FACTOR);
    assert_eq!(value.fiat_value, 10 * SCALING_FACTOR);
    assert_eq!(value.oracle_timestamp, 1_005);
    assert_eq!(value.oracle_ledger_sequence, 11);
    assert_eq!(value.claim_ledger_sequence, 12);
    assert_eq!(value.claim_ledger_timestamp, 1_010);
    assert!(!value.price_data_missing);
    assert_eq!(client.get_latest_claim_value(&1).unwrap(), value);
}

#[test]
fn stale_sep38_data_marks_missing_but_withdraws() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);

    set_ledger(&env, 1_000, 20);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 2);
    client.set_sep38_rate(&usd(&env), &(3 * SCALING_FACTOR), &SCALING_FACTOR, &1_000, &20);
    set_ledger(&env, 1_000 + SEP38_STALENESS_SECONDS + 1, 21);

    client.withdraw(&2, &SCALING_FACTOR);

    assert_eq!(grant_token.balance(&recipient), SCALING_FACTOR);
    let value = client.get_claim_value(&2, &1).unwrap();
    assert!(value.price_data_missing);
    assert_eq!(value.fiat_value, 0);
    assert_eq!(value.rate, 0);
    assert_eq!(value.oracle_timestamp, 0);
}

#[test]
fn missing_sep38_data_marks_missing_but_withdraws() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);

    set_ledger(&env, 2_000, 30);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 3);
    set_ledger(&env, 2_010, 31);

    client.withdraw(&3, &(4 * SCALING_FACTOR));

    assert_eq!(grant_token.balance(&recipient), 4 * SCALING_FACTOR);
    let value = client.get_claim_value(&3, &1).unwrap();
    assert!(value.price_data_missing);
    assert_eq!(value.fiat_value, 0);
    assert_eq!(value.rate, 0);
    assert_eq!(value.claim_ledger_sequence, 31);
    assert_eq!(value.claim_ledger_timestamp, 2_010);
}

#[test]
fn volatile_rates_anchor_each_claim_to_its_own_ledger_close() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);

    set_ledger(&env, 3_000, 40);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 4);

    set_ledger(&env, 3_010, 41);
    client.set_sep38_rate(&usd(&env), &(150 * SCALING_FACTOR / 100), &SCALING_FACTOR, &3_010, &41);
    set_ledger(&env, 3_020, 42);
    client.withdraw(&4, &(10 * SCALING_FACTOR));

    set_ledger(&env, 3_100, 43);
    client.set_sep38_rate(&usd(&env), &(55 * SCALING_FACTOR / 100), &SCALING_FACTOR, &3_100, &43);
    set_ledger(&env, 3_110, 44);
    client.withdraw(&4, &(20 * SCALING_FACTOR));

    let first = client.get_claim_value(&4, &1).unwrap();
    let second = client.get_claim_value(&4, &2).unwrap();

    assert_eq!(first.rate, 150 * SCALING_FACTOR / 100);
    assert_eq!(first.fiat_value, 15 * SCALING_FACTOR);
    assert_eq!(first.oracle_timestamp, 3_010);
    assert_eq!(first.claim_ledger_sequence, 42);
    assert_eq!(first.claim_ledger_timestamp, 3_020);
    assert!(!first.price_data_missing);

    assert_eq!(second.rate, 55 * SCALING_FACTOR / 100);
    assert_eq!(second.fiat_value, 11 * SCALING_FACTOR);
    assert_eq!(second.oracle_timestamp, 3_100);
    assert_eq!(second.claim_ledger_sequence, 44);
    assert_eq!(second.claim_ledger_timestamp, 3_110);
    assert!(!second.price_data_missing);
    assert_ne!(first.rate, second.rate);
    assert_ne!(first.claim_ledger_sequence, second.claim_ledger_sequence);
    assert_ne!(first.claim_ledger_timestamp, second.claim_ledger_timestamp);
}

#[test]
fn sep38_freshness_boundary_is_inclusive_at_300_seconds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);

    set_ledger(&env, 4_000, 50);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 5);
    client.set_sep38_rate(&usd(&env), &(4 * SCALING_FACTOR), &SCALING_FACTOR, &4_000, &50);
    set_ledger(&env, 4_000 + SEP38_STALENESS_SECONDS, 51);

    client.withdraw(&5, &SCALING_FACTOR);

    let value = client.get_claim_value(&5, &1).unwrap();
    assert!(!value.price_data_missing);
    assert_eq!(value.rate, 4 * SCALING_FACTOR);
    assert_eq!(value.fiat_value, 4 * SCALING_FACTOR);
}

#[test]
fn sep38_freshness_boundary_is_stale_after_301_seconds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);

    set_ledger(&env, 5_000, 60);
    fund_stream(&env, &client, &grant_token_addr, &recipient, 6);
    client.set_sep38_rate(&usd(&env), &(4 * SCALING_FACTOR), &SCALING_FACTOR, &5_000, &60);
    set_ledger(&env, 5_000 + SEP38_STALENESS_SECONDS + 1, 61);

    client.withdraw(&6, &SCALING_FACTOR);

    let value = client.get_claim_value(&6, &1).unwrap();
    assert!(value.price_data_missing);
    assert_eq!(value.rate, 0);
    assert_eq!(value.fiat_value, 0);
}
