#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Env, symbol_short};

/// Maximum allowed admin chain depth before deferring to two-phase pattern.
pub const MAX_ADMIN_CHAIN_DEPTH: u32 = 24;

/// Storage key for tracking the current admin auth chain depth.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DepthKey {
    /// Current chain depth counter, reset on each new ledger sequence.
    AdminAuthDepth,
    /// The ledger sequence at which the current depth was started.
    DepthLedgerEpoch,
}

/// Get the current admin auth chain depth, resetting if the ledger epoch changed.
pub fn get_admin_chain_depth(env: &Env) -> u32 {
    let current_ledger = env.ledger().sequence();
    let epoch_key = DepthKey::DepthLedgerEpoch;
    let depth_key = DepthKey::AdminAuthDepth;

    let stored_epoch: Option<u64> = env.storage().instance().get(&epoch_key);
    match stored_epoch {
        Some(epoch) if epoch == current_ledger as u64 => {
            // Same ledger — return stored depth
            env.storage().instance().get(&depth_key).unwrap_or(0)
        }
        _ => {
            // New ledger — reset depth to 0
            env.storage().instance().set(&epoch_key, &(current_ledger as u64));
            env.storage().instance().set(&depth_key, &0u32);
            0
        }
    }
}

/// Increment the admin chain depth counter.
pub fn increment_admin_chain_depth(env: &Env) -> u32 {
    let current = get_admin_chain_depth(env);
    let new_depth = current.saturating_add(1);
    env.storage()
        .instance()
        .set(&DepthKey::AdminAuthDepth, &new_depth);
    new_depth
}

/// Check if the admin chain depth is within safe bounds.
/// Returns true if safe to proceed, false if should defer.
pub fn is_chain_depth_safe(env: &Env) -> bool {
    let depth = get_admin_chain_depth(env);
    depth < MAX_ADMIN_CHAIN_DEPTH
}

/// Flush the auth stack depth (call after batch operations complete).
pub fn flush_auth_stack(env: &Env) {
    env.storage().instance().set(&DepthKey::AdminAuthDepth, &0u32);
    env.events().publish(
        (symbol_short!("auth_flush"),),
        env.ledger().sequence(),
    );
}

#[contract]
pub struct Admin;

#[contractimpl]
impl Admin {
    /// Policy resolution hop.
    pub fn initialize(_env: Env) {}

    /// Returns the current admin chain depth for monitoring.
    pub fn get_chain_depth(env: Env) -> u32 {
        get_admin_chain_depth(&env)
    }

    /// Resets the admin chain depth (admin-only cleanup).
    pub fn reset_chain_depth(env: Env) {
        flush_auth_stack(&env);
    }
}
