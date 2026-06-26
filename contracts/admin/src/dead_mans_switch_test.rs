#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger, LedgerInfo}, Env};

    fn setup() -> (Env, DeadMansSwitchContractClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DeadMansSwitchContract);
        let client = DeadMansSwitchContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        client.initialize(&admin, &vault);
        (env, client, admin, vault)
    }

    #[test]
    fn test_heartbeat_resets_countdown() {
        let (env, client, admin, _vault) = setup();
        // Advance 100 days
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (100 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        client.heartbeat(&admin);
        // Recovery should require another 180 days from now
        assert!(client.time_until_recovery() > INACTIVITY_PERIOD - 10);
    }

    #[test]
    #[should_panic(expected = "Inactivity period not yet elapsed")]
    fn test_claim_before_180_days_panics() {
        let (env, client, _admin, vault) = setup();
        // Only 90 days elapsed
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (90 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        client.claim_admin(&vault);
    }

    #[test]
    fn test_claim_after_180_days_succeeds() {
        let (env, client, _admin, vault) = setup();
        // Advance past 180 days
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (181 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        client.claim_admin(&vault);
        assert_eq!(client.get_admin(), vault);
    }

    #[test]
    #[should_panic(expected = "already been executed")]
    fn test_double_claim_panics() {
        let (env, client, _admin, vault) = setup();
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (181 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        client.claim_admin(&vault);
        client.claim_admin(&vault); // second claim must panic
    }
}