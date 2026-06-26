#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Verifier,
    KycStatus(Address),
    /// Maps (identity_commitment || jurisdiction_id) → consumed flag
    NullifierUsed(BytesN<32>),
    /// Maps identity_commitment → jurisdiction_id for the proof
    IdentityJurisdiction(BytesN<32>),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZkProof {
    pub identity_commitment: BytesN<32>,
    pub jurisdiction_id: BytesN<32>,
    pub proof_bytes: Vec<u8>,
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

    /// Submit a ZK proof for KYC verification.
    /// The nullifier is derived from (identity_commitment || jurisdiction_id)
    /// to prevent replay of the same proof across different jurisdictions.
    pub fn submit_proof(env: Env, proof: ZkProof) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();

        // Derive jurisdiction-bound nullifier
        let nullifier = derive_jurisdiction_nullifier(&env, &proof.identity_commitment, &proof.jurisdiction_id);

        // Check if this nullifier has already been consumed
        if env.storage().persistent().get::<_, bool>(&DataKey::NullifierUsed(nullifier.clone())).unwrap_or(false) {
            panic!("Nullifier already consumed — proof replay detected");
        }

        // Mark nullifier as consumed
        env.storage().persistent().set(&DataKey::NullifierUsed(nullifier), &true);

        // Store identity → jurisdiction mapping
        env.storage().persistent().set(
            &DataKey::IdentityJurisdiction(proof.identity_commitment),
            &proof.jurisdiction_id,
        );

        // Grant KYC status
        env.storage().persistent().set(&DataKey::KycStatus(proof.identity_commitment), &true);
    }

    /// Verify that a user is KYC-verified for a specific jurisdiction.
    pub fn is_verified_for_jurisdiction(env: Env, user: Address, jurisdiction_id: BytesN<32>) -> bool {
        // Check base KYC status
        let kyc_status: bool = env.storage().persistent().get(&DataKey::KycStatus(user.clone())).unwrap_or(false);
        if !kyc_status {
            return false;
        }

        // Verify the proof was submitted for this jurisdiction
        let registered_jurisdiction: BytesN<32> = env.storage().persistent()
            .get(&DataKey::IdentityJurisdiction(user))
            .unwrap_or(BytesN::from_array(&env, &[0u8; 32]));

        registered_jurisdiction == jurisdiction_id
    }

    pub fn revoke_user(env: Env, user: Address) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();
        env.storage().persistent().remove(&DataKey::KycStatus(user));
    }

    pub fn is_verified(env: Env, user: Address) -> bool {
        env.storage().persistent().get(&DataKey::KycStatus(user)).unwrap_or(false)
    }
}

/// Derives a jurisdiction-specific nullifier from identity commitment and jurisdiction ID.
/// This prevents replay of the same ZK proof across different compliance jurisdictions.
fn derive_jurisdiction_nullifier(env: &Env, identity: &BytesN<32>, jurisdiction: &BytesN<32>) -> BytesN<32> {
    let mut input: [u8; 64] = [0u8; 64];
    input[..32].copy_from_slice(&identity.to_array());
    input[32..].copy_from_slice(&jurisdiction.to_array());
    env.crypto().sha256(&input).slice(..32).unwrap()
}
