//! Tests for Optimistic Concurrency Control
//!
//! Unit tests verifying the 2PC protocol, compensation, and linearizability.

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Bytes, Env, Map,
    };

    fn setup() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());
        env
    }

    fn make_key(env: &Env, name: &[u8]) -> Bytes {
        Bytes::from_slice(env, name)
    }

    fn make_value(env: &Env, val: u64) -> Bytes {
        Bytes::from_slice(env, &val.to_be_bytes())
    }

    #[test]
    fn test_initialize() {
        let env = setup();
        let version = OptimisticContract::get_version(env);
        assert_eq!(version.current_version, 0);
        assert_eq!(version.current_seq_no, 0);
    }

    #[test]
    fn test_begin_assigns_seq_no() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");

        let seq1 = OptimisticContract::begin_optimistic(
            env.clone(),
            batch_id.clone(),
            Map::new(&env),
        );
        assert_eq!(seq1, 1);

        let seq2 = OptimisticContract::begin_optimistic(
            env.clone(),
            batch_id.clone(),
            Map::new(&env),
        );
        assert_eq!(seq2, 2);
    }

    #[test]
    fn test_commit_in_order() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");
        let key = make_key(&env, b"balance");
        let val1 = make_value(&env, 100);
        let val2 = make_value(&env, 200);

        // Begin two mutations
        let seq1 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val1.clone());
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates)
        };

        let seq2 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val2.clone());
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates)
        };

        // Get mutation IDs
        let version = OptimisticContract::get_version(env.clone());
        assert_eq!(version.current_version, 0);

        // Commit first (should succeed)
        let id1 = make_mutation_id(&env, &batch_id, seq1);
        let committed1 = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id1);
        assert!(committed1);

        // Verify state updated
        let stored = OptimisticContract::get_state_value(env.clone(), key.clone());
        assert_eq!(stored, val1);

        // Commit second (should succeed)
        let id2 = make_mutation_id(&env, &batch_id, seq2);
        let committed2 = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id2);
        assert!(committed2);

        let stored2 = OptimisticContract::get_state_value(env.clone(), key);
        assert_eq!(stored2, val2);

        let final_version = OptimisticContract::get_version(env);
        assert_eq!(final_version.current_version, 2);
        assert_eq!(final_version.current_seq_no, 2);
    }

    #[test]
    fn test_commit_out_of_order_compensates() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");
        let key = make_key(&env, b"balance");
        let val1 = make_value(&env, 100);
        let val2 = make_value(&env, 200);

        // Begin two mutations
        let mut updates1 = Map::new(&env);
        updates1.set(key.clone(), val1.clone());
        let seq1 = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates1);

        let mut updates2 = Map::new(&env);
        updates2.set(key.clone(), val2.clone());
        let seq2 = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates2);

        // Try to commit second first (should fail - out of order)
        let id2 = make_mutation_id(&env, &batch_id, seq2);
        let result = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id2);
        assert!(!result);

        // Now commit first (should succeed)
        let id1 = make_mutation_id(&env, &batch_id, seq1);
        let result1 = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id1);
        assert!(result1);

        // State should reflect first mutation only
        let stored = OptimisticContract::get_state_value(env.clone(), key);
        assert_eq!(stored, val1);
    }

    #[test]
    fn test_rollback_restores_state() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");
        let key = make_key(&env, b"balance");
        let initial_val = make_value(&env, 50);
        let new_val = make_value(&env, 100);

        // Set initial state directly
        env.storage().persistent().set(
            &StateKey::StateValue(key.clone()),
            &initial_val,
        );

        // Begin mutation
        let mut updates = Map::new(&env);
        updates.set(key.clone(), new_val.clone());
        let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);

        // Rollback instead of commit
        let id = make_mutation_id(&env, &batch_id, seq);
        OptimisticContract::rollback_mutation(env.clone(), batch_id.clone(), id.clone());

        // Verify state restored
        let stored = OptimisticContract::get_state_value(env.clone(), key);
        assert_eq!(stored, initial_val);

        // Verify compensation exists
        let comp_exists = verify_compensation_integrity(&env, &id);
        assert!(comp_exists);
    }

    #[test]
    fn test_expire_pending() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");
        let key = make_key(&env, b"balance");
        let val = make_value(&env, 100);

        // Begin mutation
        let mut updates = Map::new(&env);
        updates.set(key.clone(), val.clone());
        let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
        let id = make_mutation_id(&env, &batch_id, seq);

        // Cannot expire before timeout
        let expired = OptimisticContract::expire_pending(env.clone(), batch_id.clone(), id.clone());
        assert!(!expired);

        // Advance ledger past timeout
        env.ledger().set_sequence(env.ledger().sequence() + OPTIMISTIC_LOCK_TIMEOUT + 1);

        // Now expire
        let expired = OptimisticContract::expire_pending(env.clone(), batch_id.clone(), id.clone());
        assert!(expired);
    }

    #[test]
    fn test_version_linearization() {
        let env = setup();
        let batch_id = Bytes::from_slice(&env, b"batch1");
        let key = make_key(&env, b"counter");

        // Submit 5 sequential mutations
        for i in 1u64..=5 {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), make_value(&env, i * 10));
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
        }

        // Commit all in order
        for i in 1u64..=5 {
            let id = make_mutation_id(&env, &batch_id, i);
            let committed = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id);
            assert!(committed, "seq {} should commit", i);
        }

        // Verify final state is sequential
        let final_version = OptimisticContract::get_version(env.clone());
        assert_eq!(final_version.current_version, 5);
        assert_eq!(final_version.current_seq_no, 5);

        // Final value should be 50 (5 * 10)
        let stored = OptimisticContract::get_state_value(env, key);
        assert_eq!(stored, make_value(&Env::default(), 50));
    }

    // Helper to generate mutation_id (mirrors contract logic)
    fn make_mutation_id(env: &Env, batch_id: &Bytes, seq_no: u64) -> Bytes {
        let mut data = Bytes::new(env);
        for i in 0..batch_id.len() {
            data.push_back(batch_id.get(i).unwrap());
        }
        let seq_bytes = seq_no.to_be_bytes();
        for &b in &seq_bytes {
            data.push_back(b);
        }
        data
    }
}
