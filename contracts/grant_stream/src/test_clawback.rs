#![cfg(test)]

use super::{GrantStreamContract, GrantStreamContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
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

    (admin, grant_token_addr.address(), treasury, oracle, native_token_addr.address(), client)
}

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

#[test]
fn test_clawback_basic_functionality() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // Create grant with donor
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR; // 1 token per second
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    // Let some time pass for streaming
    set_timestamp(&env, 1500); // 500 seconds passed
    
    // Trigger clawback as donor
    let reason = soroban_sdk::String::from_str(&env, "Project abandoned");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    // Verify grant status is clawbacked
    let grant = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant.status, GrantStatus::Clawbacked);
    
    // Verify donor received unearned funds (should be ~500 tokens remaining)
    let donor_balance = grant_token.balance(&donor);
    assert!(donor_balance >= 490 * SCALING_FACTOR); // Allow for small rounding differences
    assert!(donor_balance <= 510 * SCALING_FACTOR);
    
    // Verify recipient can still claim vested funds
    let claimable = client.claimable(&grant_id);
    assert!(claimable >= 490 * SCALING_FACTOR); // Should have ~500 tokens vested
    assert!(claimable <= 510 * SCALING_FACTOR);
}

#[test]
fn test_clawback_with_disputed_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    set_timestamp(&env, 1500); // 500 seconds passed
    
    // Trigger contested clawback
    let reason = soroban_sdk::String::from_str(&env, "Disputed milestone completion");
    client.trigger_grant_clawback(&grant_id, &reason, &true);
    
    // Verify funds are in escrow
    let escrow_balance = client.get_dispute_escrow_balance(&grant_id).unwrap();
    assert!(escrow_balance >= 490 * SCALING_FACTOR);
    assert!(escrow_balance <= 510 * SCALING_FACTOR);
    
    // Verify donor didn't receive funds yet
    let donor_balance = grant_token.balance(&donor);
    assert_eq!(donor_balance, 0);
    
    // Resolve dispute in favor of donor
    client.resolve_disputed_clawback(&grant_id, &true);
    
    // Verify donor received funds
    let donor_balance = grant_token.balance(&donor);
    assert!(donor_balance >= 490 * SCALING_FACTOR);
    
    // Verify escrow is cleared
    let escrow_balance_after = client.try_get_dispute_escrow_balance(&grant_id);
    assert!(escrow_balance_after.is_err());
}

#[test]
fn test_clawback_proration_math_precision() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    // Test clawback at millisecond precision
    set_timestamp(&env, 1500); // Exactly 500 seconds
    
    let claimable_before = client.claimable(&grant_id);
    
    // Trigger clawback
    let reason = soroban_sdk::String::from_str(&env, "Precision test");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    let claimable_after = client.claimable(&grant_id);
    let donor_received = grant_token.balance(&donor);
    
    // Verify mathematical precision: claimable + donor_received should equal total streamed
    let total_accounted = claimable_after + donor_received;
    let expected_streamed = 500 * SCALING_FACTOR; // Exactly 500 seconds of streaming
    
    // Allow for 1 token rounding error due to precision
    assert!(total_accounted >= expected_streamed - SCALING_FACTOR);
    assert!(total_accounted <= expected_streamed + SCALING_FACTOR);
}

#[test]
fn test_clawback_access_control() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    set_timestamp(&env, 1500);
    
    // Test unauthorized access
    env.mock_auths(&[
        (&unauthorized_user, &42), // Unauthorized user
    ]);
    
    let reason = soroban_sdk::String::from_str(&env, "Unauthorized attempt");
    let result = client.try_trigger_grant_clawback(&grant_id, &reason, &false);
    assert!(result.is_err());
    
    // Test authorized access (donor)
    env.mock_auths(&[
        (&donor, &42), // Donor authorized
    ]);
    
    let result = client.try_trigger_grant_clawback(&grant_id, &reason, &false);
    assert!(result.is_ok());
}

#[test]
fn test_clawback_double_spending_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    set_timestamp(&env, 1500);
    
    // First clawback should succeed
    let reason = soroban_sdk::String::from_str(&env, "First clawback");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    // Second clawback should fail
    let reason2 = soroban_sdk::String::from_str(&env, "Second clawback");
    let result = client.try_trigger_grant_clawback(&grant_id, &reason2, &false);
    assert!(result.is_err());
    
    // Verify grant status
    let grant = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant.status, GrantStatus::Clawbacked);
}

#[test]
fn test_clawback_balance_invariants() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    
    let contract_balance_before = grant_token.balance(&client.address);
    
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    set_timestamp(&env, 1500);
    
    // Trigger clawback
    let reason = soroban_sdk::String::from_str(&env, "Invariant test");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    let contract_balance_after = grant_token.balance(&client.address);
    let donor_balance = grant_token.balance(&donor);
    let claimable = client.claimable(&grant_id);
    
    // Verify balance invariant: contract_balance_after + donor_balance should equal contract_balance_before
    let total_outside_contract = donor_balance + (contract_balance_before - contract_balance_after);
    let expected_in_contract = claimable; // Only claimable should remain in contract
    
    assert_eq!(expected_in_contract, contract_balance_after);
}

#[test]
fn test_clawback_with_validator() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &Some(validator.clone()), &Some(donor.clone()));
    
    set_timestamp(&env, 1500);
    
    // Trigger clawback
    let reason = soroban_sdk::String::from_str(&env, "Validator test");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    // Verify validator can still claim their share
    let validator_claimable = client.validator_claimable(&grant_id);
    assert!(validator_claimable > 0);
    
    // Verify donor received unearned funds minus validator's share
    let donor_balance = grant_token.balance(&donor);
    assert!(donor_balance > 0);
}

#[test]
fn test_clawback_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // Test 1: Clawback immediately after creation (no streaming)
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0u64, &None, &Some(donor.clone()));
    
    let reason = soroban_sdk::String::from_str(&env, "Immediate clawback");
    client.trigger_grant_clawback(&grant_id, &reason, &false);
    
    // Donor should receive full amount
    let donor_balance = grant_token.balance(&donor);
    assert_eq!(donor_balance, total_amount);
    
    // Test 2: Clawback after full completion
    let grant_id2 = 2;
    let total_amount2 = 100 * SCALING_FACTOR;
    let flow_rate2 = 1 * SCALING_FACTOR;
    
    grant_token_admin.mint(&client.address, &total_amount2);
    client.create_grant(&grant_id2, &recipient, &total_amount2, &flow_rate2, &0u64, &None, &Some(donor.clone()));
    
    set_timestamp(&env, 2000); // Let grant fully complete
    
    let reason2 = soroban_sdk::String::from_str(&env, "After completion");
    client.trigger_grant_clawback(&grant_id2, &reason2, &false);
    
    // Donor should receive nothing (all funds vested)
    let donor_balance_after = grant_token.balance(&donor);
    assert_eq!(donor_balance_after, total_amount); // No additional funds received
}
