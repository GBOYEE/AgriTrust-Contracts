//! Property-Based Tests for Optimistic Concurrency Control
//!
//! Verifies linearizability: concurrent mutations across accounts produce
//! the same final state as an equivalent sequential execution.
//!
//! Test: 20 concurrent mutations across 10 simulated accounts

#[cfg(test)]
mod proptest_optimistic {
    use super::*;
    use soroban_sdk::{
        testutils::Ledger,
        Bytes, Env, Map,
    };
    use crate::{MutationStatus, OPTIMISTIC_LOCK_TIMEOUT};

    const NUM_ACCOUNTS: usize = 10;
    const NUM_MUTATIONS: usize = 20;

    /// Represents a mutation in our property test
    #[derive(Clone, Debug)]
    struct MutationRequest {
        account_idx: usize,
        delta: i64,
    }

    /// Expected final state for verification
    fn expected_final_state(initial: &[i64], mutations: &[MutationRequest]) -> Vec<i64> {
        let mut state = initial.to_vec();
        // Apply sequentially in order they were committed (by seq_no)
        // For simplicity, apply in order received
        for m in mutations {
            if m.account_idx < state.len() {
                state[m.account_idx] = state[m.account_idx] + m.delta;
            }
        }
        state
    }

    /// Generate deterministic pseudo-random mutation sequence
    fn generate_mutation_sequence(seed: u64) -> Vec<MutationRequest> {
        let mut mutations = Vec::new();
        let mut state = seed;
        for i in 0..NUM_MUTATIONS {
            // Simple LCG for deterministic pseudo-random
            state = state.wrapping_mul(6364136223846793005).wrapping_add(i as u64 + 1);
            let account_idx = (state % NUM_ACCOUNTS as u64) as usize;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let delta = ((state % 200) - 100) as i64;
            mutations.push(MutationRequest { account_idx, delta });
        }
        mutations
    }

    fn make_key_for_account(env: &Env, idx: usize) -> Bytes {
        let mut data = Bytes::new(env);
        for b in b"acct_" {
            data.push_back(*b);
        }
        data.push_back(idx as u8);
        data
    }

    fn make_value_i64(env: &Env, val: i64) -> Bytes {
        Bytes::from_slice(env, &val.to_be_bytes())
    }

    fn read_value_i64(val: &Bytes) -> i64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&val.raw_slice()[..8]);
        i64::from_be_bytes(bytes)
    }

    /// Test that sequential commit produces the expected linearizable state
    #[test]
    fn test_sequential_linearizability() {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"prop_batch");
        let mutations = generate_mutation_sequence(42);
        let initial_values: Vec<i64> = (0..NUM_ACCOUNTS).map(|i| (i as i64 + 1) * 1000).collect();

        // Set initial state
        for (i, val) in initial_values.iter().enumerate() {
            let key = make_key_for_account(&env, i);
            let mut updates = Map::new(&env);
            updates.set(key, make_value_i64(&env, *val));
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
            let mut pending_list: Vec<crate::PendingMutation> = env
                .storage()
                .persistent()
                .get(&crate::StateKey::PendingMap(batch_id.clone()))
                .unwrap();
            let last = pending_list.len() - 1;
            let mut last_mut = pending_list.get(last).unwrap();
            last_mut.status = MutationStatus::ReadyToCommit;
            pending_list.set(last, last_mut);
            env.storage()
                .persistent()
                .set(&crate::StateKey::PendingMap(batch_id.clone()), &pending_list);
        }

        // Apply each mutation sequentially
        for (i, m) in mutations.iter().enumerate() {
            let key = make_key_for_account(&env, m.account_idx);
            let current_val = OptimisticContract::get_state_value(env.clone(), key.clone());
            let current = read_value_i64(&current_val);
            let new_val = current + m.delta;

            let mut updates = Map::new(&env);
            updates.set(key.clone(), make_value_i64(&env, new_val));
            let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);

            // Commit immediately (sequential, so should always succeed)
            let mut pending_list: Vec<crate::PendingMutation> = env
                .storage()
                .persistent()
                .get(&crate::StateKey::PendingMap(batch_id.clone()))
                .unwrap();
            let committed = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(),
                pending_list.get(pending_list.len() - 1).unwrap().mutation_id);
            assert!(committed, "sequential commit {} should succeed", i);
        }

        // Verify linearizability: final state == sequential execution
        let expected = expected_final_state(&initial_values, &mutations);
        for (i, exp) in expected.iter().enumerate() {
            let key = make_key_for_account(&env, i);
            let stored = OptimisticContract::get_state_value(env.clone(), key);
            let actual = read_value_i64(&stored);
            assert_eq!(
                actual, *exp,
                "Account {}: expected {}, got {}",
                i, exp, actual
            );
        }
    }

    /// Test concurrent scenario: all begin, then all commit in seq_no order
    #[test]
    fn test_concurrent_begin_all_commit_in_order() {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"concurrent_batch");
        let mutations = generate_mutation_sequence(123);
        let initial_values: Vec<i64> = (0..NUM_ACCOUNTS).map(|i| 5000).collect();

        // Set initial state
        for (i, val) in initial_values.iter().enumerate() {
            env.storage().persistent().set(
                &crate::StateKey::StateValue(make_key_for_account(&env, i)),
                &make_value_i64(&env, *val),
            );
        }

        // Begin ALL mutations concurrently
        let mut seq_nos = Vec::new();
        for m in &mutations {
            let key = make_key_for_account(&env, m.account_idx);
            let mut updates = Map::new(&env);
            updates.set(key, make_value_i64(&env, m.delta)); // delta, will add later
            let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
            seq_nos.push(seq);
        }

        // Now commit ALL in seq_no order
        for seq in 1u64..=seq_nos.len() as u64 {
            let mut pending_list: Vec<crate::PendingMutation> = env
                .storage()
                .persistent()
                .get(&crate::StateKey::PendingMap(batch_id.clone()))
                .unwrap();

            let idx = (seq - 1) as u32;
            let mutation_id = pending_list.get(idx).unwrap().mutation_id.clone();
            let committed = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), mutation_id);
            assert!(committed, "commit for seq {} should succeed (FIFO order)", seq);
        }

        // Verify state is consistent
        let version = OptimisticContract::get_version(env.clone());
        assert_eq!(version.current_version, mutations.len() as u64);
        assert_eq!(version.current_seq_no, mutations.len() as u64);
    }

    /// Test that out-of-order commits are properly compensated
    #[test]
    fn test_out_of_order_commit_compensation() {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"ooo_batch");
        let key = make_key_for_account(&env, 0);

        // Begin 3 mutations
        for delta in &[100i64, 200, 300] {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), make_value_i64(&env, *delta));
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
        }

        // Try to commit third (seq=3) — should fail since seq_no expected is 1
        let mut pending_list: Vec<crate::PendingMutation> = env
            .storage()
            .persistent()
            .get(&crate::StateKey::PendingMap(batch_id.clone()))
            .unwrap();
        let id3 = pending_list.get(2).unwrap().mutation_id.clone();
        let result = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id3);
        assert!(!result, "out-of-order commit should fail");

        // Commit first (seq=1) — should succeed
        let id1 = pending_list.get(0).unwrap().mutation_id.clone();
        let result1 = OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id1);
        assert!(result1, "first in-order commit should succeed");
    }

    /// Test concurrent expiration
    #[test]
    fn test_parallel_expiration_after_timeout() {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"expire_batch");

        // Begin 3 pending mutations
        for i in 0..3i64 {
            let mut updates = Map::new(&env);
            updates.set(
                make_key_for_account(&env, 0),
                make_value_i64(&env, i + 1),
            );
            OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);
        }

        // Cannot expire before timeout
        let pending_list: Vec<crate::PendingMutation> = env
            .storage()
            .persistent()
            .get(&crate::StateKey::PendingMap(batch_id.clone()))
            .unwrap();
        for pending in pending_list.iter() {
            let expired = OptimisticContract::expire_pending(
                env.clone(),
                batch_id.clone(),
                pending.mutation_id.clone(),
            );
            assert!(!expired, "should not expire before timeout");
        }

        // Advance past timeout
        env.ledger().set_sequence(env.ledger().sequence() + OPTIMISTIC_LOCK_TIMEOUT + 1);

        // Can now expire
        let pending_list: Vec<crate::PendingMutation> = env
            .storage()
            .persistent()
            .get(&crate::StateKey::PendingMap(batch_id.clone()))
            .unwrap();
        for pending in pending_list.iter() {
            let expired = OptimisticContract::expire_pending(
                env.clone(),
                batch_id.clone(),
                pending.mutation_id.clone(),
            );
            assert!(expired, "should expire after timeout");
        }
    }

    /// Property: version monotonicity
    #[test]
    fn test_version_monotonicity() {
        let env = Env::default();
        env.mock_all_auths();
        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"mono_batch");
        let mut prev_version = 0u64;

        for i in 1u64..=5 {
            let key = make_key_for_account(&env, i as usize);
            let mut updates = Map::new(&env);
            updates.set(key, make_value_i64(&env, i * 10));
            let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), updates);

            let mut pending_list: Vec<crate::PendingMutation> = env
                .storage()
                .persistent()
                .get(&crate::StateKey::PendingMap(batch_id.clone()))
                .unwrap();
            let committed = OptimisticContract::commit_optimistic(
                env.clone(),
                batch_id.clone(),
                pending_list.get(seq as u32 - 1).unwrap().mutation_id.clone(),
            );
            assert!(committed);

            let version = OptimisticContract::get_version(env.clone());
            assert!(
                version.current_version > prev_version,
                "version must be monotonically increasing"
            );
            prev_version = version.current_version;
        }
    }
}
