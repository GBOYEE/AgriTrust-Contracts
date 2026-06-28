//! Property-Based Tests for Optimistic Concurrency Control
//!
//! Simple tests verifying the core linearizability property:
//! sequential commits produce correct final state.

#[cfg(test)]
mod proptest_optimistic {
    use crate::*;
    use soroban_sdk::{
        Bytes, Env, Map,
    };

    const NUM_ROUNDS: usize = 5;

    /// Helper: generate mutation_id the same way the contract does
    fn make_mutation_id(env: &Env, batch_id: &Bytes, seq_no: u64) -> Bytes {
        let mut data = Bytes::new(env);
        for i in 0..batch_id.len() {
            data.push_back(batch_id.get(i).unwrap());
        }
        let seq_bytes = seq_no.to_be_bytes();
        for b in seq_bytes.iter() {
            data.push_back(*b);
        }
        data
    }

    /// Test sequential commits accumulate correctly
    #[test]
    fn test_sequential_accumulation() {
        let env = Env::default();
        env.mock_all_auths();

        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"seq_batch");
        let key = Bytes::from_slice(&env, b"counter");

        for i in 0..NUM_ROUNDS {
            let delta = 10_i64;
            let mut state_updates: Map<Bytes, Bytes> = Map::new(&env);
            state_updates.set(key.clone(), Bytes::from_slice(&env, &delta.to_be_bytes()));

            let seq = OptimisticContract::begin_optimistic(
                env.clone(),
                batch_id.clone(),
                state_updates,
            );

            let id = make_mutation_id(&env, &batch_id, seq);
            let committed =
                OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id);
            assert!(committed, "Commit {} should succeed", i);
        }

        // Final value should be NUM_ROUNDS * 10
        let stored = OptimisticContract::get_state_value(env.clone(), key);
        let mut bytes = [0u8; 8];
        for j in 0..8_u32 {
            bytes[j as usize] = stored.get(j).unwrap();
        }
        assert_eq!(
            i64::from_be_bytes(bytes),
            (NUM_ROUNDS as i64) * 10,
            "Final value should be {}",
            (NUM_ROUNDS as i64) * 10
        );
    }

    /// Test that rollback clears state and creates compensation
    #[test]
    fn test_rollback_creates_compensation() {
        let env = Env::default();
        env.mock_all_auths();

        OptimisticContract::initialize(env.clone());

        let batch_id = Bytes::from_slice(&env, b"rb_batch");
        let key = Bytes::from_slice(&env, b"val");

        // Set initial value first
        let mut su: Map<Bytes, Bytes> = Map::new(&env);
        su.set(key.clone(), Bytes::from_slice(&env, &42_i64.to_be_bytes()));
        let init_seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), su);
        let init_id = make_mutation_id(&env, &batch_id, init_seq);
        OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), init_id);

        // Now begin a mutation and rollback
        let mut su2: Map<Bytes, Bytes> = Map::new(&env);
        su2.set(key.clone(), Bytes::from_slice(&env, &99_i64.to_be_bytes()));
        let seq2 = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), su2);
        let id2 = make_mutation_id(&env, &batch_id, seq2);
        OptimisticContract::rollback_mutation(env.clone(), batch_id.clone(), id2);

        // Value should be restored to 42
        let stored = OptimisticContract::get_state_value(env.clone(), key);
        let mut bytes = [0u8; 8];
        for j in 0..8_u32 {
            bytes[j as usize] = stored.get(j).unwrap();
        }
        assert_eq!(
            i64::from_be_bytes(bytes),
            42,
            "Value should be restored to 42 after rollback"
        );
    }

    /// Test version tracking
    #[test]
    fn test_version_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        OptimisticContract::initialize(env.clone());

        let v0 = OptimisticContract::get_version(env.clone());
        assert_eq!(v0.current_version, 0, "Initial version should be 0");

        let batch_id = Bytes::from_slice(&env, b"ver_batch");
        let mut su: Map<Bytes, Bytes> = Map::new(&env);
        let key = Bytes::from_slice(&env, b"k");
        su.set(key.clone(), Bytes::from_slice(&env, &1_i64.to_be_bytes()));

        let seq = OptimisticContract::begin_optimistic(env.clone(), batch_id.clone(), su);
        let id = make_mutation_id(&env, &batch_id, seq);
        OptimisticContract::commit_optimistic(env.clone(), batch_id.clone(), id);

        let v1 = OptimisticContract::get_version(env.clone());
        assert_eq!(
            v1.current_version, 1,
            "Version should increment after commit"
        );
    }
}
