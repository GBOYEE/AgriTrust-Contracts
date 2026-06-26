//! # Nullifier Module
//!
//! Implements jurisdiction-bound nullifier derivation for the ZK-KYC compliance module.
//! Nullifiers are derived from `identity_commitment + domain_id` to prevent replay
//! across distinct compliance jurisdictions.

use soroban_sdk::{contracttype, Bytes, BytesN, Env, IntoVal};

/// A 32-byte hash representing a nullifier that has been consumed.
pub type NullifierHash = BytesN<32>;

/// A 32-byte identifier for a compliance domain (e.g., contract hash of compliance contract).
pub type ComplianceDomainId = BytesN<32>;

/// Storage key for tracking consumed nullifiers, scoped per compliance domain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NullifierDataKey {
    /// Whether a nullifier has been used within a specific compliance domain.
    /// Composite key: (ComplianceDomainId, NullifierHash) → bool
    NullifierUsed(ComplianceDomainId, NullifierHash),
    /// Registered compliance domains that are allowed to submit proofs.
    /// ComplianceDomainId → bool
    RegisteredDomain(ComplianceDomainId),
}

/// Derives a jurisdiction-bound nullifier from an identity commitment and a compliance domain ID.
///
/// The nullifier is computed as `SHA-256(identity_commitment || domain_id)`, ensuring
/// that the same identity commitment produces distinct nullifiers for different jurisdictions.
///
/// # Arguments
/// * `env` - The Soroban environment providing crypto primitives.
/// * `identity_commitment` - The user's identity commitment (32-byte hash).
/// * `domain_id` - The compliance domain identifier (32-byte).
///
/// # Returns
/// A 32-byte `NullifierHash` bound to the given identity and domain.
pub fn derive_nullifier(
    env: &Env,
    identity_commitment: &BytesN<32>,
    domain_id: &BytesN<32>,
) -> BytesN<32> {
    let mut input: Bytes = Bytes::new(env);
    input.append(&identity_commitment.clone().into_val(env));
    input.append(&domain_id.clone().into_val(env));
    env.crypto().sha256(&input).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::BytesN as _, Env};

    fn setup_env() -> Env {
        Env::default()
    }

    fn random_bytes32(env: &Env) -> BytesN<32> {
        BytesN::random(env)
    }

    #[test]
    fn test_derive_nullifier_is_deterministic() {
        let env = setup_env();
        let identity = random_bytes32(&env);
        let domain = random_bytes32(&env);

        let n1 = derive_nullifier(&env, &identity, &domain);
        let n2 = derive_nullifier(&env, &identity, &domain);

        assert_eq!(n1, n2, "nullifier derivation must be deterministic");
    }

    #[test]
    fn test_same_identity_different_domains_produce_different_nullifiers() {
        let env = setup_env();
        let identity = random_bytes32(&env);
        let domain_a = random_bytes32(&env);
        let domain_b = random_bytes32(&env);

        let n_a = derive_nullifier(&env, &identity, &domain_a);
        let n_b = derive_nullifier(&env, &identity, &domain_b);

        assert_ne!(
            n_a, n_b,
            "same identity in different domains must produce different nullifiers"
        );
    }

    #[test]
    fn test_different_identities_same_domain_produce_different_nullifiers() {
        let env = setup_env();
        let identity_a = random_bytes32(&env);
        let identity_b = random_bytes32(&env);
        let domain = random_bytes32(&env);

        let n_a = derive_nullifier(&env, &identity_a, &domain);
        let n_b = derive_nullifier(&env, &identity_b, &domain);

        assert_ne!(
            n_a, n_b,
            "different identities in same domain must produce different nullifiers"
        );
    }
}
