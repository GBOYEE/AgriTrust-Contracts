//! Tests for the double-approval system for high-value milestone payouts

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, Env, String, testutils::{Address as _, Ledger}};
    use crate::double_approval::{
        initialize_config, get_config, update_config, requires_double_approval,
        create_request, get_request, approve_request, execute_request, cancel_request,
        ApprovalStatus, DoubleApprovalConfig, DEFAULT_HIGH_VALUE_THRESHOLD, DEFAULT_APPROVAL_WINDOW_SECS
    };
    use crate::Error::{NotInitialized, InvalidAmount, InvalidState, NotAuthorized, GrantNotFound};

    fn setup_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn setup_test_accounts(env: &Env) -> (Address, Address, Address, Address) {
        (
            Address::generate(env), // admin
            Address::generate(env), // oracle/secondary approver
            Address::generate(env), // grantee
            Address::generate(env), // random user
        )
    }

    #[test]
    fn test_initialize_double_approval_config() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Initialize configuration
        initialize_config(
            &env,
            admin.clone(),
            oracle.clone(),
            Some(50000),
            Some(86400), // 1 day
        ).unwrap();

        // Verify configuration
        let config = get_config(&env).unwrap();
        assert_eq!(config.high_value_threshold, 50000);
        assert_eq!(config.approval_window_secs, 86400);
        assert_eq!(config.primary_approver, admin);
        assert_eq!(config.secondary_approver, oracle);
        assert!(config.enabled);
    }

    #[test]
    fn test_initialize_double_approval_config_defaults() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Initialize with defaults
        initialize_config(
            &env,
            admin.clone(),
            oracle.clone(),
            None,
            None,
        ).unwrap();

        // Verify default values
        let config = get_config(&env).unwrap();
        assert_eq!(config.high_value_threshold, DEFAULT_HIGH_VALUE_THRESHOLD);
        assert_eq!(config.approval_window_secs, DEFAULT_APPROVAL_WINDOW_SECS);
        assert_eq!(config.primary_approver, admin);
        assert_eq!(config.secondary_approver, oracle);
        assert!(config.enabled);
    }

    #[test]
    fn test_initialize_double_approval_config_invalid_threshold() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Try to initialize with invalid threshold
        let result = initialize_config(
            &env,
            admin,
            oracle,
            Some(0), // Invalid: zero threshold
            None,
        );

        assert_eq!(result.unwrap_err(), InvalidAmount);
    }

    #[test]
    fn test_update_double_approval_config() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Initialize first
        initialize_config(&env, admin.clone(), oracle.clone(), None, None).unwrap();

        // Update configuration
        update_config(
            &env,
            Some(75000),
            Some(172800), // 2 days
            Some(false), // Disable
        ).unwrap();

        // Verify updates
        let config = get_config(&env).unwrap();
        assert_eq!(config.high_value_threshold, 75000);
        assert_eq!(config.approval_window_secs, 172800);
        assert!(!config.enabled);
    }

    #[test]
    fn test_requires_double_approval() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Initialize with threshold of 1000
        initialize_config(&env, admin, oracle, Some(1000), None).unwrap();

        // Test amounts below threshold
        assert!(!requires_double_approval(&env, 999).unwrap());
        assert!(!requires_double_approval(&env, 500).unwrap());

        // Test amounts at and above threshold
        assert!(requires_double_approval(&env, 1000).unwrap());
        assert!(requires_double_approval(&env, 1001).unwrap());
        assert!(requires_double_approval(&env, 5000).unwrap());
    }

    #[test]
    fn test_requires_double_approval_disabled() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        // Initialize with threshold of 1000
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Disable double approval
        update_config(&env, None, None, Some(false)).unwrap();

        // Now no amount should require double approval
        assert!(!requires_double_approval(&env, 999).unwrap());
        assert!(!requires_double_approval(&env, 1000).unwrap());
        assert!(!requires_double_approval(&env, 5000).unwrap());
    }

    #[test]
    fn test_requires_double_approval_invalid_amount() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);

        initialize_config(&env, admin, oracle, None, None).unwrap();

        // Test invalid amounts
        assert_eq!(requires_double_approval(&env, 0).unwrap_err(), InvalidAmount);
        assert_eq!(requires_double_approval(&env, -1).unwrap_err(), InvalidAmount);
    }

    #[test]
    fn test_create_double_approval_request() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create request for high-value amount
        let request_id = create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount (above threshold)
            grantee.clone(),
            token_address.clone(),
            Some(String::from_str(&env, "Test milestone")),
        ).unwrap();

        assert!(request_id > 0);

        // Verify request was created
        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.grant_id, 1);
        assert_eq!(request.milestone_index, 0);
        assert_eq!(request.amount, 5000);
        assert_eq!(request.recipient, grantee);
        assert_eq!(request.token_address, token_address);
        assert_eq!(request.status, ApprovalStatus::Pending);
        assert!(request.first_approver.is_none());
        assert!(request.second_approver.is_none());
        assert!(request.reason.is_some());
    }

    #[test]
    fn test_create_double_approval_request_low_value() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin, oracle, Some(1000), None).unwrap();

        // Try to create request for low-value amount
        let result = create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            500, // amount (below threshold)
            grantee,
            token_address,
            None,
        );

        assert_eq!(result.unwrap_err(), InvalidState);
    }

    #[test]
    fn test_approve_double_approval_request() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            grantee,
            token_address,
            None,
        ).unwrap();

        // First approval by admin
        approve_request(&env, 1, 0, admin.clone()).unwrap();

        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.status, ApprovalStatus::FirstApproved);
        assert_eq!(request.first_approver.unwrap(), admin);
        assert!(request.second_approver.is_none());

        // Second approval by oracle
        approve_request(&env, 1, 0, oracle.clone()).unwrap();

        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.status, ApprovalStatus::FullyApproved);
        assert_eq!(request.first_approver.unwrap(), admin);
        assert_eq!(request.second_approver.unwrap(), oracle);
    }

    #[test]
    fn test_approve_double_approval_request_unauthorized() {
        let env = setup_env();
        let (admin, oracle, _, random_user) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin, oracle, Some(1000), None).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            Address::generate(&env),
            token_address,
            None,
        ).unwrap();

        // Try to approve with unauthorized user
        let result = approve_request(&env, 1, 0, random_user);
        assert_eq!(result.unwrap_err(), NotAuthorized);
    }

    #[test]
    fn test_approve_double_approval_request_duplicate() {
        let env = setup_env();
        let (admin, oracle, _, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            Address::generate(&env),
            token_address,
            None,
        ).unwrap();

        // First approval
        approve_request(&env, 1, 0, admin.clone()).unwrap();

        // Try to approve again with same user
        let result = approve_request(&env, 1, 0, admin);
        assert_eq!(result.unwrap_err(), InvalidState);
    }

    #[test]
    fn test_execute_double_approval_request() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create and approve request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            grantee.clone(),
            token_address.clone(),
            None,
        ).unwrap();

        approve_request(&env, 1, 0, admin.clone()).unwrap();
        approve_request(&env, 1, 0, oracle.clone()).unwrap();

        // Execute request
        execute_request(&env, 1, 0, admin.clone()).unwrap();

        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.status, ApprovalStatus::Executed);
    }

    #[test]
    fn test_execute_double_approval_request_not_fully_approved() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create and partially approve request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            grantee,
            token_address,
            None,
        ).unwrap();

        approve_request(&env, 1, 0, admin).unwrap(); // Only first approval

        // Try to execute without full approval
        let result = execute_request(&env, 1, 0, oracle);
        assert_eq!(result.unwrap_err(), InvalidState);
    }

    #[test]
    fn test_cancel_double_approval_request() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), None).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            grantee,
            token_address,
            None,
        ).unwrap();

        // Cancel request
        cancel_request(&env, 1, 0, admin.clone()).unwrap();

        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.status, ApprovalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_double_approval_request_unauthorized() {
        let env = setup_env();
        let (admin, oracle, _, random_user) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration
        initialize_config(&env, admin, oracle, Some(1000), None).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            Address::generate(&env),
            token_address,
            None,
        ).unwrap();

        // Try to cancel with unauthorized user
        let result = cancel_request(&env, 1, 0, random_user);
        assert_eq!(result.unwrap_err(), NotAuthorized);
    }

    #[test]
    fn test_double_approval_request_expiration() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let token_address = Address::generate(&env);

        // Initialize configuration with very short window
        initialize_config(&env, admin.clone(), oracle.clone(), Some(1000), Some(1)).unwrap();

        // Create request
        create_request(
            &env,
            1, // grant_id
            0, // milestone_index
            5000, // amount
            grantee,
            token_address,
            None,
        ).unwrap();

        // Advance time beyond expiration
        env.ledger().set_timestamp(env.ledger().timestamp() + 2);

        // Try to approve expired request
        let result = approve_request(&env, 1, 0, admin);
        assert_eq!(result.unwrap_err(), InvalidState);

        // Verify request is marked as expired
        let request = get_request(&env, 1, 0).unwrap();
        assert_eq!(request.status, ApprovalStatus::Expired);
    }

    #[test]
    fn test_get_nonexistent_request() {
        let env = setup_env();

        let result = get_request(&env, 999, 0);
        assert_eq!(result.unwrap_err(), GrantNotFound);
    }

    #[test]
    fn test_double_approval_integration_with_contract() {
        let env = setup_env();
        let (admin, oracle, grantee, _) = setup_test_accounts(&env);
        let contract_id = Address::generate(&env);

        // Initialize contract
        crate::GrantStreamContract::initialize(
            &env,
            admin.clone(),
            Address::generate(&env), // grant_token
            Address::generate(&env), // treasury
            oracle.clone(),
            Address::generate(&env), // native_token
        ).unwrap();

        // Initialize double approval
        crate::GrantStreamContract::initialize_double_approval(
            &env,
            admin.clone(),
            oracle.clone(),
            Some(1000),
            None,
        ).unwrap();

        // Test requires_double_approval through contract
        assert!(!crate::GrantStreamContract::requires_double_approval(&env.clone(), 500).unwrap());
        assert!(crate::GrantStreamContract::requires_double_approval(&env.clone(), 1500).unwrap());

        // Test configuration retrieval
        let config = crate::GrantStreamContract::get_double_approval_config(&env).unwrap();
        assert_eq!(config.high_value_threshold, 1000);
        assert_eq!(config.primary_approver, admin);
        assert_eq!(config.secondary_approver, oracle);
    }
}
