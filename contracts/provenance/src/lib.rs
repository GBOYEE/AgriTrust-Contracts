#![no_std]

mod errors;
mod resolver;
mod types;
mod verifier;

pub use errors::Error;
pub use resolver::{get_provenance_result, resolve_provenance, write_hop_state};
pub use types::{
    HopState, ProvenanceAccessSet, ProvenanceResult, Score, StorageBudget,
    SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD, MAX_HOPS,
};

// Re-export verifier functions for external testing / integration.
pub use verifier::{validate_hop_credential, verify_hop_signature};

use soroban_sdk::{contract, contractimpl, BytesN, Env, Vec};

#[contract]
pub struct ProvenanceContract;

#[contractimpl]
impl ProvenanceContract {
    /// Resolve a provenance chain and return the aggregated result.
    ///
    /// `chain_id`  — unique identifier for this chain resolution (used as
    ///               the storage key for the written ProvenanceResult).
    /// `hop_ids`   — ordered list of hop identifiers; each must have a
    ///               corresponding HopState in persistent storage (written
    ///               by `write_hop` before calling this).
    pub fn resolve(
        env: Env,
        chain_id: BytesN<32>,
        hop_ids: Vec<BytesN<32>>,
    ) -> Result<ProvenanceResult, Error> {
        resolve_provenance(&env, chain_id, hop_ids)
    }

    /// Write a HopState into persistent storage. Called by grant_contracts,
    /// compliance, admin, and treasury hops before resolution.
    pub fn write_hop(env: Env, hop_id: BytesN<32>, state: HopState) {
        write_hop_state(&env, &hop_id, &state);
    }

    /// Retrieve a previously resolved ProvenanceResult by chain_id.
    pub fn get_result(env: Env, chain_id: BytesN<32>) -> Option<ProvenanceResult> {
        get_provenance_result(&env, &chain_id)
    }
}

#[cfg(test)]
mod test;
