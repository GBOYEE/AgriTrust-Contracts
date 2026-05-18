#![cfg(test)]

use super::{GrantStreamContract, GrantStreamContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec, Symbol,
};
use proptest::prelude::*;
use std::collections::HashMap;

// Test configuration for the Bank Run scenario
const NUM_USERS: usize = 150; // More than 100 users as specified
const GRANT_AMOUNT: i128 = 1_000_000 * SCALING_FACTOR;
const FLOW_RATE: i128 = 10 * SCALING_FACTOR; // 10 tokens per second
const TEST_DURATION: u64 = 3600; // 1 hour of streaming

#[derive(Debug, Clone)]
struct WithdrawScenario {
    user_id: usize,
    grant_id: u64,
    withdraw_amount: i128,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct TestState {
    env: Env,
    admin: Address,
    grant_token: Address,
    treasury: Address,
    oracle: Address,
    native_token: Address,
    client: GrantStreamContractClient,
    users: Vec<Address>,
    grant_ids: Vec<u64>,
    initial_balances: HashMap<Address, i128>,
}

impl TestState {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
        let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);

        let contract_id = env.register(GrantStreamContract, ());
        let client = GrantStreamContractClient::new(&env, &contract_id);

        client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address());

        // Generate users
        let mut users = Vec::new(&env);
        for _ in 0..NUM_USERS {
            users.push_back(Address::generate(&env));
        }

        Self {
            env,
            admin,
            grant_token: grant_token_addr.address(),
            treasury,
            oracle,
            native_token: native_token_addr.address(),
            client,
            users,
            grant_ids: Vec::new(&env),
            initial_balances: HashMap::new(),
        }
    }

    fn setup_grants(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let grant_token = token::Client::new(&self.env, &self.grant_token);
        let grant_token_admin = token::StellarAssetClient::new(&self.env, &self.grant_token);

        // Set initial timestamp
        self.set_timestamp(1000);

        // Create grants for each user
        for (i, user) in self.users.iter().enumerate() {
            let grant_id = (i + 1) as u64;
            
            // Mint tokens to contract
            grant_token_admin.mint(&self.client.address, &GRANT_AMOUNT);
            
            // Create grant
            self.client.create_grant(
                &grant_id,
                user,
                &GRANT_AMOUNT,
                &FLOW_RATE,
                &0, // no warmup
                &None, // no validator
                &None, // no donor
            );
            
            self.grant_ids.push_back(grant_id);
            
            // Record initial balance
            self.initial_balances.insert(user.clone(), 0);
        }

        Ok(())
    }

    fn set_timestamp(&self, timestamp: u64) {
        self.env.ledger().with_mut(|li| {
            li.timestamp = timestamp;
        });
    }

    fn get_contract_balance(&self) -> i128 {
        let grant_token = token::Client::new(&self.env, &self.grant_token);
        grant_token.balance(&self.client.address)
    }

    fn get_user_balance(&self, user: &Address) -> i128 {
        let grant_token = token::Client::new(&self.env, &self.grant_token);
        grant_token.balance(user)
    }

    fn verify_total_invariant(&self) -> Result<(), String> {
        let contract_balance = self.get_contract_balance();
        let mut total_user_balances = 0i128;
        
        for user in self.users.iter() {
            total_user_balances += self.get_user_balance(user);
        }
        
        let expected_total = GRANT_AMOUNT * NUM_USERS as i128;
        let actual_total = contract_balance + total_user_balances;
        
        if actual_total != expected_total {
            return Err(format!(
                "Total invariant violation: expected {}, got {} (contract: {}, users: {})",
                expected_total, actual_total, contract_balance, total_user_balances
            ));
        }
        
        Ok(())
    }

    fn verify_no_state_corruption(&self) -> Result<(), String> {
        // Verify each grant's state consistency
        for (i, grant_id) in self.grant_ids.iter().enumerate() {
            let grant = self.client.get_grant(*grant_id);
            let user = self.users.get(i).unwrap();
            
            // Check that withdrawn + claimable <= total_amount
            let total_accounted = grant.withdrawn
                .checked_add(grant.claimable)
                .ok_or("Math overflow in total_accounted")?;
            
            if total_accounted > grant.total_amount {
                return Err(format!(
                    "Grant {} state corruption: accounted {} > total {}",
                    grant_id, total_accounted, grant.total_amount
                ));
            }
            
            // Verify user balance matches withdrawn amount
            let user_balance = self.get_user_balance(user);
            if user_balance != grant.withdrawn {
                return Err(format!(
                    "Grant {} balance mismatch: user has {}, grant shows withdrawn {}",
                    grant_id, user_balance, grant.withdrawn
                ));
            }
        }
        
        Ok(())
    }
}

// Property-based test for concurrent withdrawals
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn test_concurrent_withdraw_bank_run(
        withdraw_scenarios in prop::collection::vec(
            (
                0usize..NUM_USERS, // user_id
                1i128..=1000i128, // withdraw_amount (in SCALING_FACTOR units)
                100u64..(1000 + TEST_DURATION), // timestamp
            ),
            50..200 // number of withdraw operations
        )
    ) {
        let mut state = TestState::new();
        state.setup_grants().expect("Failed to setup grants");
        
        let mut total_withdrawn = 0i128;
        let mut operations_per_user: HashMap<usize, u32> = HashMap::new();
        
        // Process withdraw scenarios in order (simulating ledger sequence)
        for (user_id, withdraw_amount_raw, timestamp) in withdraw_scenarios {
            let withdraw_amount = withdraw_amount_raw * SCALING_FACTOR;
            let grant_id = (user_id + 1) as u64;
            let user = state.users.get(user_id).unwrap();
            
            // Set timestamp for this operation
            state.set_timestamp(timestamp);
            
            // Check current claimable amount
            let claimable_before = state.client.claimable(grant_id);
            
            // Only withdraw if there's enough claimable
            if claimable_before >= withdraw_amount {
                // Perform withdrawal
                state.client.withdraw(&grant_id, &withdraw_amount);
                total_withdrawn += withdraw_amount;
                
                // Track operations per user
                *operations_per_user.entry(user_id).or_insert(0) += 1;
                
                // Verify invariants after each withdrawal
                state.verify_total_invariant()
                    .unwrap_or_else(|e| panic!("Total invariant violation after withdrawal: {}", e));
                
                state.verify_no_state_corruption()
                    .unwrap_or_else(|e| panic!("State corruption detected after withdrawal: {}", e));
            }
        }
        
        // Final verification
        state.verify_total_invariant()
            .unwrap_or_else(|e| panic!("Final total invariant violation: {}", e));
        
        state.verify_no_state_corruption()
            .unwrap_or_else(|e| panic!("Final state corruption detected: {}", e));
        
        // Verify that users with more operations didn't get blocked
        let max_ops = operations_per_user.values().max().unwrap_or(&0);
        let min_ops = operations_per_user.values().min().unwrap_or(&0);
        
        // Ensure that later withdrawers weren't blocked (should have reasonable distribution)
        if *max_ops > 0 && *min_ops > 0 {
            let ratio = *max_ops as f64 / *min_ops as f64;
            prop_assert!(ratio < 10.0, "Operations too skewed: max={}, min={}, ratio={}", max_ops, min_ops, ratio);
        }
    }
}

// Stress test with maximum concurrent users
#[test]
fn test_maximum_concurrent_withdrawals() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register(GrantStreamContract, ());
    let client = GrantStreamContractClient::new(&env, &contract_id);

    client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address());

    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    // Create 200 users for stress testing
    let mut users = Vec::new(&env);
    for _ in 0..200 {
        users.push_back(Address::generate(&env));
    }

    // Setup timestamp
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // Create grants for all users
    for (i, user) in users.iter().enumerate() {
        let grant_id = (i + 1) as u64;
        grant_token_admin.mint(&client.address, &GRANT_AMOUNT);
        client.create_grant(&grant_id, user, &GRANT_AMOUNT, &FLOW_RATE, &0, &None, &None);
    }

    // Advance time to allow accrual
    env.ledger().with_mut(|li| {
        li.timestamp = 2000; // 1000 seconds later
    });

    // Simulate all users withdrawing at once
    let mut successful_withdrawals = 0;
    for (i, user) in users.iter().enumerate() {
        let grant_id = (i + 1) as u64;
        let claimable = client.claimable(grant_id);
        
        if claimable > 0 {
            let withdraw_amount = claimable / 2; // Withdraw half of claimable
            client.withdraw(&grant_id, &withdraw_amount);
            successful_withdrawals += 1;
        }
    }

    // Verify that most withdrawals succeeded
    assert!(successful_withdrawals > 150, "Too many failed withdrawals: {}", successful_withdrawals);

    // Verify final state consistency
    let mut total_balance = 0i128;
    for user in users.iter() {
        total_balance += grant_token.balance(user);
    }
    let contract_balance = grant_token.balance(&client.address);
    let expected_total = GRANT_AMOUNT * 200;
    
    assert_eq!(total_balance + contract_balance, expected_total);
}

// Test gas consumption doesn't increase with position in sequence
#[test]
fn test_gas_consumption_consistency() {
    let mut state = TestState::new();
    state.setup_grants().expect("Failed to setup grants");
    
    // Advance time to allow accrual
    state.set_timestamp(2000);
    
    let mut gas_costs = Vec::new();
    
    // Measure gas for withdrawals at different positions
    for i in 0..50 {
        let grant_id = (i + 1) as u64;
        let claimable = state.client.claimable(grant_id);
        
        if claimable > 0 {
            let withdraw_amount = claimable.min(100 * SCALING_FACTOR);
            
            // Record gas before
            let gas_before = state.env.budget().last_ledger_fee();
            
            state.client.withdraw(&grant_id, &withdraw_amount);
            
            // Record gas after
            let gas_after = state.env.budget().last_ledger_fee();
            let gas_used = gas_after.saturating_sub(gas_before);
            
            gas_costs.push(gas_used);
        }
    }
    
    // Verify gas consumption is relatively consistent
    if !gas_costs.is_empty() {
        let max_gas = *gas_costs.iter().max().unwrap();
        let min_gas = *gas_costs.iter().min().unwrap();
        
        // Gas should not vary by more than 50%
        let ratio = max_gas as f64 / min_gas as f64;
        assert!(ratio < 1.5, "Gas consumption too variable: max={}, min={}, ratio={}", max_gas, min_gas, ratio);
    }
}
