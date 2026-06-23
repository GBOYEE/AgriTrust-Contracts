use soroban_sdk::{symbol_short, Bytes, BytesN, Env, Map, Vec};

use crate::errors::Error;
use crate::types::{
    HopState, ProvenanceAccessSet, ProvenanceResult, Score, StorageBudget, MAX_HOPS,
};
use crate::verifier::{validate_hop_credential, verify_hop_signature};

// ── Storage key helpers ───────────────────────────────────────────────────────

/// Compute the persistent-storage key for a HopState.
/// Key = b"HS" ++ hop_id bytes — deterministic, collision-resistant.
fn hop_key(env: &Env, hop_id: &BytesN<32>) -> Bytes {
    let mut key = Bytes::new(env);
    key.push_back(b'H');
    key.push_back(b'S');
    key.append(&hop_id.to_bytes());
    key
}

/// Storage key for the ProvenanceResult of a chain identified by chain_id.
fn result_key(env: &Env, chain_id: &BytesN<32>) -> Bytes {
    let mut key = Bytes::new(env);
    key.push_back(b'P');
    key.push_back(b'R');
    key.append(&chain_id.to_bytes());
    key
}

// ── Prefetch cache ────────────────────────────────────────────────────────────

/// Prefetch all HopState entries for a chain in a single sequential pass.
///
/// Each read charges 1 against `budget`. Returns a Map<Bytes, HopState>
/// acting as an in-memory cache so the resolver never re-reads storage.
///
/// Fails immediately with StorageBudgetExceeded if the prefetch would push
/// `budget.used` past `STORAGE_BUDGET` before any read occurs.
pub fn prefetch_hop_states(
    env: &Env,
    access_set: &ProvenanceAccessSet,
    budget: &mut StorageBudget,
) -> Result<Map<Bytes, HopState>, Error> {
    // Fail-fast: check if the entire prefetch batch would exceed the budget.
    if budget.would_exceed(access_set.hop_count) {
        return Err(Error::StorageBudgetExceeded);
    }

    let mut cache: Map<Bytes, HopState> = Map::new(env);

    for key in access_set.hop_keys.iter() {
        // Charge 1 per read — checked before each individual read.
        if budget.would_exceed_read() {
            return Err(Error::StorageBudgetExceeded);
        }
        let warned = budget.charge_read();
        if warned {
            env.events().publish(
                (symbol_short!("storage"), symbol_short!("warn")),
                budget.used,
            );
        }

        let state: HopState = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::HopNotFound)?;

        cache.set(key, state);
    }

    Ok(cache)
}

// ── Core resolver ─────────────────────────────────────────────────────────────

/// Resolve a provenance chain described by `hop_ids`, returning a
/// `ProvenanceResult` on success.
///
/// ## Storage access model
///
/// | Operation                         | Accesses |
/// |-----------------------------------|----------|
/// | prefetch read per hop             | N        |
/// | result write (ProvenanceResult)   | 1        |
/// | Total for N-hop chain             | N + 1    |
///
/// For N = 10: 11 accesses — well under STORAGE_BUDGET (120).
///
/// ## Fail-fast ordering
///
/// 1. Chain length guard (no storage touched)
/// 2. Budget pre-check on full estimated cost (no storage touched)
/// 3. Build ProvenanceAccessSet (no storage touched)
/// 4. Prefetch all hop states (reads charged one-by-one)
/// 5. Verify + accumulate scores (CPU only — no additional storage reads)
/// 6. Write ProvenanceResult (1 write charged)
pub fn resolve_provenance(
    env: &Env,
    chain_id: BytesN<32>,
    hop_ids: Vec<BytesN<32>>,
) -> Result<ProvenanceResult, Error> {
    // ── 1. Chain length guards ─────────────────────────────────────────────
    let hop_count = hop_ids.len();
    if hop_count == 0 {
        return Err(Error::EmptyChain);
    }
    if hop_count > MAX_HOPS {
        return Err(Error::ChainTooLong);
    }

    // ── 2. Budget pre-check: N reads + 1 write ────────────────────────────
    let estimated = hop_count.saturating_add(1);
    let mut budget = StorageBudget::new();
    if budget.would_exceed(estimated) {
        return Err(Error::StorageBudgetExceeded);
    }

    // ── 3. Build access set (pure computation, no storage) ────────────────
    let access_set = build_access_set(env, &hop_ids);

    // ── 4. Prefetch all hop states ─────────────────────────────────────────
    let cache = prefetch_hop_states(env, &access_set, &mut budget)?;

    // ── 5. Verify and accumulate (CPU only) ───────────────────────────────
    let mut score_sum: i128 = 0;
    let mut hops_resolved: u32 = 0;

    for hop_id in hop_ids.iter() {
        let key = hop_key(env, &hop_id);
        let state = cache.get(key).ok_or(Error::HopNotFound)?;

        if !state.score.is_valid() {
            return Err(Error::InvalidScore);
        }

        // CPU-only checks — zero storage reads.
        verify_hop_signature(env, &state)?;
        validate_hop_credential(env, &state)?;

        score_sum = score_sum.saturating_add(state.score.raw);
        hops_resolved = hops_resolved.saturating_add(1);
    }

    let final_score = Score {
        raw: score_sum / (hops_resolved as i128),
    };

    // ── 6. Write result (1 storage write) ─────────────────────────────────
    if budget.would_exceed_write() {
        return Err(Error::StorageBudgetExceeded);
    }
    let warned = budget.charge_write();
    if warned {
        env.events().publish(
            (symbol_short!("storage"), symbol_short!("warn")),
            budget.used,
        );
    }

    let result = ProvenanceResult {
        hops_resolved,
        final_score,
        storage_accesses_used: budget.used,
        resolved_at_ledger: env.ledger().sequence(),
    };

    let rkey = result_key(env, &chain_id);
    env.storage().persistent().set(&rkey, &result);

    env.events().publish(
        (symbol_short!("prov"), symbol_short!("resolved")),
        (hops_resolved, budget.used),
    );

    Ok(result)
}

// ── Access set builder ────────────────────────────────────────────────────────

fn build_access_set(env: &Env, hop_ids: &Vec<BytesN<32>>) -> ProvenanceAccessSet {
    let hop_count = hop_ids.len();
    let mut keys: Vec<Bytes> = Vec::new(env);
    for hop_id in hop_ids.iter() {
        keys.push_back(hop_key(env, &hop_id));
    }
    let estimated = ProvenanceAccessSet::estimated_accesses(hop_count);
    ProvenanceAccessSet {
        hop_keys: keys,
        hop_count,
        estimated_accesses: estimated,
    }
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Write a HopState into persistent storage. Called before resolution.
pub fn write_hop_state(env: &Env, hop_id: &BytesN<32>, state: &HopState) {
    env.storage().persistent().set(&hop_key(env, hop_id), state);
}

/// Read the ProvenanceResult for a resolved chain.
pub fn get_provenance_result(env: &Env, chain_id: &BytesN<32>) -> Option<ProvenanceResult> {
    env.storage().persistent().get(&result_key(env, chain_id))
}
