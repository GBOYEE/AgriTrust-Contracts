#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

mod nullifier;

#[cfg(test)]
mod test;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Verifier,
    KycStatus(Address),
}

/// Checks whether a compliance domain is registered.
fn is_domain_registered(env: &Env, domain_id: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&nullifier::NullifierDataKey::RegisteredDomain(domain_id.clone()))
        .unwrap_or(false)
}

/// Checks whether a nullifier has been consumed in a domain.
fn is_nullifier_used(env: &Env, domain_id: &BytesN<32>, nullifier: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&nullifier::NullifierDataKey::NullifierUsed(
            domain_id.clone(),
            nullifier.clone(),
        ))
        .unwrap_or(false)
}

/// Marks a nullifier as consumed in a domain.
fn consume_nullifier(env: &Env, domain_id: &BytesN<32>, nullifier: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&nullifier::NullifierDataKey::NullifierUsed(
            domain_id.clone(),
            nullifier.clone(),
        ), &true);
}

#[contract]
pub struct ZKKYCContract;

#[contractimpl]
impl ZKKYCContract {
    pub fn init(env: Env, verifier: Address) {
        if env.storage().instance().has(&DataKey::Verifier) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Verifier, &verifier);
    }

    pub fn verify_user(env: Env, user: Address) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();
        env.storage().persistent().set(&DataKey::KycStatus(user), &true);
    }

    pub fn revoke_user(env: Env, user: Address) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();
        env.storage().persistent().remove(&DataKey::KycStatus(user));
    }

    pub fn is_verified(env: Env, user: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::KycStatus(user))
            .unwrap_or(false)
    }

    /// Submits a ZK-KYC proof for verification within a specific compliance domain.
    ///
    /// Derives a jurisdiction-bound nullifier from the identity commitment and the
    /// compliance domain ID, then checks for replay within that domain.
    ///
    /// # Arguments
    /// * `identity_commitment` - The user's identity commitment (32-byte hash).
    /// * `domain_id` - The compliance domain identifier (32-byte).
    ///
    /// # Returns
    /// The derived nullifier hash on success. Panics on replay or unregistered domain.
    pub fn submit_proof(env: Env, identity_commitment: BytesN<32>, domain_id: BytesN<32>) -> BytesN<32> {
        if !is_domain_registered(&env, &domain_id) {
            panic!("compliance domain not registered");
        }

        let nullifier_hash = nullifier::derive_nullifier(&env, &identity_commitment, &domain_id);

        if is_nullifier_used(&env, &domain_id, &nullifier_hash) {
            panic!("ZK-KYC proof replay detected: nullifier already consumed in this jurisdiction");
        }

        consume_nullifier(&env, &domain_id, &nullifier_hash);

        // Emit event for nullifier consumption
        env.events().publish(
            (
                soroban_sdk::symbol_short!("nullifier"),
                domain_id.clone(),
                nullifier_hash.clone(),
            ),
            &(),
        );

        nullifier_hash
    }

    /// Registers a compliance domain as authorized to submit ZK-KYC proofs.
    pub fn register_compliance_domain(env: Env, domain_id: BytesN<32>) {
        env.storage().persistent().set(
            &nullifier::NullifierDataKey::RegisteredDomain(domain_id.clone()),
            &true,
        );
    }

    /// Checks whether a nullifier has been consumed in a given compliance domain.
    pub fn is_nullifier_consumed(
        env: Env,
        domain_id: BytesN<32>,
        nullifier_hash: BytesN<32>,
    ) -> bool {
        is_nullifier_used(&env, &domain_id, &nullifier_hash)
    }

    /// Checks whether a compliance domain is registered.
    pub fn is_registered_domain(env: Env, domain_id: BytesN<32>) -> bool {
        is_domain_registered(&env, &domain_id)
    }
}
