use soroban_sdk::Env;

use crate::errors::Error;
use crate::types::HopState;

/// Verify the signature proof carried in `state.signature` over the
/// canonical message `state.credential_id ++ state.policy_ref`.
///
/// # Production implementation note
///
/// Replace the stub below with `env.crypto().ed25519_verify(...)` or
/// secp256k1 ECDSA recovery once the issuer public-key registry is
/// established. The function intentionally takes only immutable references
/// so it cannot perform any storage reads, keeping the call CPU-only and
/// preserving the storage budget guarantee.
pub fn verify_hop_signature(_env: &Env, state: &HopState) -> Result<(), Error> {
    // Stub: accept non-zero signatures, reject zeroed signatures (test hook).
    // A zeroed 64-byte signature signals an injected-failure in tests.
    let zeroed = state.signature.to_array() == [0u8; 64];
    if zeroed {
        return Err(Error::InvalidHopSignature);
    }
    Ok(())
}

/// Validate the credential referenced by `state.credential_id`.
///
/// Checks:
/// 1. `state.credential_verified` flag must be true (set at record time).
/// 2. `state.recorded_at` must be non-zero (credential must have a timestamp).
/// 3. Optionally, verify the credential exists in storage (read-only check).
///
/// # Production implementation note
///
/// Extend with on-chain credential registry lookups or cross-contract calls
/// to the compliance crate. Keep those lookups in a separate pre-resolution
/// phase to preserve the storage-budget model — do not call
/// `env.storage()` from this function unless performing read-only validation.
pub fn validate_hop_credential(_env: &Env, state: &HopState) -> Result<(), Error> {
    if !state.credential_verified {
        return Err(Error::InvalidHopCredential);
    }
    if state.recorded_at == 0 {
        return Err(Error::InvalidHopCredential);
    }

    Ok(())
}
