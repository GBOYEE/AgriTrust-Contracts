#![no_std]
use soroban_sdk::{contracttype, symbol_short, token, Address, Env, Vec, Symbol};

use super::{
    GrantStreamContract, GrantStatus, DataKey, Error,
    circuit_breakers::{self, CircuitBreakerKey}
};

pub struct DisputeCircuitBreakerTest;

impl DisputeCircuitBreakerTest {
    /// Test the mass dispute trigger circuit breaker functionality
    pub fn test_mass_dispute_trigger(env: Env) -> bool {
        // Initialize the contract
        let admin = Address::from_string(&env, &"admin".into_val(&env));
        let token = Address::from_string(&env, &"token".into_val(&env));
        let treasury = Address::from_string(&env, &"treasury".into_val(&env));
        let oracle = Address::from_string(&env, &"oracle".into_val(&env));
        let native_token = Address::from_string(&env, &"native".into_val(&env));
        
        GrantStreamContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            treasury.clone(),
            oracle.clone(),
            native_token.clone(),
        ).unwrap();

        // Create 10 active grants
        for i in 0u64..10u64 {
            let recipient = Address::from_string(&env, &format!("recipient_{}", i).into_val(&env));
            GrantStreamContract::create_grant(
                env.clone(),
                i,
                recipient.clone(),
                1000,
                100,
                0,
                None,
            ).unwrap();
        }

        // Verify grant initialization is not halted initially
        assert!(!circuit_breakers::is_grant_initialization_halted(&env));

        // Simulate 1 dispute (10% of active grants) - should not trigger
        let active_grants = 10u32;
        let result = circuit_breakers::record_dispute(&env, active_grants);
        assert!(result); // Should return true (threshold not breached)
        assert!(!circuit_breakers::is_grant_initialization_halted(&env));

        // Simulate 1 more dispute (total 20% of active grants) - should trigger
        let result = circuit_breakers::record_dispute(&env, active_grants);
        assert!(!result); // Should return false (threshold breached)
        assert!(circuit_breakers::is_grant_initialization_halted(&env));

        // Verify that new grant creation is now blocked
        let recipient = Address::from_string(&env, &"new_recipient".into_val(&env));
        let result = GrantStreamContract::create_grant(
            env.clone(),
            11,
            recipient.clone(),
            1000,
            100,
            0,
            None,
        );
        assert_eq!(result.unwrap_err(), Error::GrantInitializationHalted);

        // Test dispute statistics
        let (window_start, dispute_count, active_grants_snapshot, halted) = 
            circuit_breakers::get_dispute_monitoring_stats(&env);
        assert!(window_start > 0);
        assert_eq!(dispute_count, 2);
        assert_eq!(active_grants_snapshot, 10);
        assert!(halted);

        // Test admin resume functionality
        GrantStreamContract::resume_grant_initialization(env.clone()).unwrap();
        assert!(!circuit_breakers::is_grant_initialization_halted(&env));

        // Verify grant creation works again
        let result = GrantStreamContract::create_grant(
            env.clone(),
            12,
            recipient.clone(),
            1000,
            100,
            0,
            None,
        );
        assert!(result.is_ok());

        true
    }

    /// Test dispute trigger through the main contract interface
    pub fn test_dispute_trigger_interface(env: Env) -> bool {
        // Initialize the contract
        let admin = Address::from_string(&env, &"admin".into_val(&env));
        let token = Address::from_string(&env, &"token".into_val(&env));
        let treasury = Address::from_string(&env, &"treasury".into_val(&env));
        let oracle = Address::from_string(&env, &"oracle".into_val(&env));
        let native_token = Address::from_string(&env, &"native".into_val(&env));
        
        GrantStreamContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            treasury.clone(),
            oracle.clone(),
            native_token.clone(),
        ).unwrap();

        // Create 5 active grants
        for i in 0u64..5u64 {
            let recipient = Address::from_string(&env, &format!("recipient_{}", i).into_val(&env));
            GrantStreamContract::create_grant(
                env.clone(),
                i,
                recipient.clone(),
                1000,
                100,
                0,
                None,
            ).unwrap();
        }

        // Trigger disputes for 1 grant (20% of 5 active grants) - should trigger
        let result = GrantStreamContract::trigger_grant_dispute(env.clone(), 0);
        assert!(result.is_ok());
        assert!(circuit_breakers::is_grant_initialization_halted(&env));

        // Test getting dispute stats through main contract
        let (window_start, dispute_count, active_grants_snapshot, halted) = 
            GrantStreamContract::get_dispute_stats(env.clone());
        assert_eq!(dispute_count, 1);
        assert_eq!(active_grants_snapshot, 5);
        assert!(halted);

        true
    }

    /// Test window reset functionality
    pub fn test_dispute_window_reset(env: Env) -> bool {
        // Initialize the contract
        let admin = Address::from_string(&env, &"admin".into_val(&env));
        let token = Address::from_string(&env, &"token".into_val(&env));
        let treasury = Address::from_string(&env, &"treasury".into_val(&env));
        let oracle = Address::from_string(&env, &"oracle".into_val(&env));
        let native_token = Address::from_string(&env, &"native".into_val(&env));
        
        GrantStreamContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            treasury.clone(),
            oracle.clone(),
            native_token.clone(),
        ).unwrap();

        // Create 10 active grants
        for i in 0u64..10u64 {
            let recipient = Address::from_string(&env, &format!("recipient_{}", i).into_val(&env));
            GrantStreamContract::create_grant(
                env.clone(),
                i,
                recipient.clone(),
                1000,
                100,
                0,
                None,
            ).unwrap();
        }

        // Trigger disputes to reach threshold
        let active_grants = 10u32;
        circuit_breakers::record_dispute(&env, active_grants); // 10%
        circuit_breakers::record_dispute(&env, active_grants); // 20% - should trigger

        assert!(circuit_breakers::is_grant_initialization_halted(&env));

        // Simulate time passing beyond 24 hours by manually setting window start to past
        let past_timestamp = env.ledger().timestamp() - (25 * 60 * 60); // 25 hours ago
        env.storage().instance().set(&CircuitBreakerKey::DisputeWindowStart, &past_timestamp);

        // Add another dispute - should reset window and not trigger immediately
        let result = circuit_breakers::record_dispute(&env, active_grants);
        assert!(result); // Should return true (fresh window, threshold not breached yet)
        assert!(!circuit_breakers::is_grant_initialization_halted(&env));

        true
    }
}
