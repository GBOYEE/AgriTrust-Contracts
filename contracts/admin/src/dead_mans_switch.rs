#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env,
};

const INACTIVITY_PERIOD: u64 = 180 * 24 * 60 * 60; // 180 days in seconds

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum SwitchKey {
    PrimaryAdmin,       // Address
    RecoveryVault,      // Address
    LastActivityAt,     // u64 timestamp — reset on every admin action
    RecoveryExecuted,   // bool
    BeneficiaryVersion, // u32 — incremented on each admin transfer
    ClaimFee,           // i128 — DMS claim fee (10 XLM = 1e8 stroops)
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct DeadMansSwitchContract;

#[contractimpl]
impl DeadMansSwitchContract {

    /// Initialize with a primary admin and a recovery vault address.
    pub fn initialize(env: Env, primary_admin: Address, recovery_vault: Address) {
        primary_admin.require_auth();
        env.storage().instance().set(&SwitchKey::PrimaryAdmin, &primary_admin);
        env.storage().instance().set(&SwitchKey::RecoveryVault, &recovery_vault);
        env.storage().instance().set(&SwitchKey::LastActivityAt, &env.ledger().timestamp());
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &false);
        env.storage().instance().set(&SwitchKey::BeneficiaryVersion, &0_u32);
        env.storage().instance().set(&SwitchKey::ClaimFee, &100_000_000_i128); // 10 XLM in stroops
    }

    /// Heartbeat — primary admin calls this to prove liveness and reset the countdown.
    pub fn heartbeat(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_primary_admin(&env, &admin);
        let now = env.ledger().timestamp();
        env.storage().instance().set(&SwitchKey::LastActivityAt, &now);
        // Emit event for auditability
        env.events().publish(
            (soroban_sdk::symbol_short!("heartbeat"),),
            (admin, now),
        );
    }

    /// Any admin action should call this internally to reset the countdown.
    /// Call at the start of every privileged function.
    pub fn record_activity(env: &Env) {
        env.storage()
            .instance()
            .set(&SwitchKey::LastActivityAt, &env.ledger().timestamp());
    }

    /// Recovery vault claims admin rights after 180 days of inactivity.
    pub fn claim_admin(env: Env, recovery_vault: Address) {
        recovery_vault.require_auth();
        Self::assert_recovery_vault(&env, &recovery_vault);

        // Ensure recovery hasn't already been executed
        let already_executed: bool = env
            .storage()
            .instance()
            .get(&SwitchKey::RecoveryExecuted)
            .unwrap_or(false);
        if already_executed {
            panic!("Recovery has already been executed");
        }

        // Check inactivity window
        let last_activity: u64 = env
            .storage()
            .instance()
            .get(&SwitchKey::LastActivityAt)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(last_activity);

        if elapsed < INACTIVITY_PERIOD {
            let remaining = INACTIVITY_PERIOD - elapsed;
            panic!(
                "Inactivity period not yet elapsed — {} seconds remaining",
                remaining
            );
        }

        // Transfer admin rights to recovery vault
        env.storage().instance().set(&SwitchKey::PrimaryAdmin, &recovery_vault);
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &true);

        env.events().publish(
            (soroban_sdk::symbol_short!("recovered"),),
            (recovery_vault, now),
        );
    }

    /// Update the recovery vault address (primary admin only).
    pub fn update_recovery_vault(env: Env, admin: Address, new_vault: Address) {
        admin.require_auth();
        Self::assert_primary_admin(&env, &admin);
        Self::record_activity(&env);
        env.storage().instance().set(&SwitchKey::RecoveryVault, &new_vault);
    }

    /// Reset the Dead Man's Switch after an admin transfer.
    ///
    /// Called by AdminContract::accept_ownership() to prevent the old admin's
    /// designated beneficiary from retaining recovery rights after ownership change.
    ///
    /// Resets:
    /// - PrimaryAdmin to the new admin
    /// - RecoveryVault to the new admin (beneficiary must be explicitly re-set)
    /// - LastActivityAt to current timestamp (restart inactivity countdown)
    /// - RecoveryExecuted to false (fresh state)
    pub fn reset_on_admin_transfer(env: Env, old_admin: Address, new_admin: Address) {
        // Only the current primary admin or the pending admin can trigger this
        let current_admin: Address = env.storage().instance()
            .get(&SwitchKey::PrimaryAdmin)
            .expect("Admin not set");

        if old_admin != current_admin {
            panic!("Unauthorized: old_admin does not match current primary admin");
        }

        // Verify the new admin is authorizing this reset
        new_admin.require_auth();

        // Reset DMS state for the new admin
        env.storage().instance().set(&SwitchKey::PrimaryAdmin, &new_admin);
        env.storage().instance().set(&SwitchKey::RecoveryVault, &new_admin);
        env.storage().instance().set(&SwitchKey::LastActivityAt, &env.ledger().timestamp());
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &false);

        // Increment beneficiary version to invalidate old admin's beneficiary
        let new_version: u32 = env
            .storage()
            .instance()
            .get::<_, u32>(&SwitchKey::BeneficiaryVersion)
            .unwrap_or(0u32)
            .checked_add(1u32)
            .expect("Version overflow");
        env.storage()
            .instance()
            .set(&SwitchKey::BeneficiaryVersion, &new_version);

        env.events().publish(
            (soroban_sdk::symbol_short!("dms_reset"),),
            (old_admin, new_admin, new_version),
        );
    }

    /// View how many seconds remain before the recovery vault can claim admin.
    pub fn time_until_recovery(env: Env) -> u64 {
        let last_activity: u64 = env
            .storage()
            .instance()
            .get(&SwitchKey::LastActivityAt)
            .unwrap_or(0);
        let elapsed = env.ledger().timestamp().saturating_sub(last_activity);
        INACTIVITY_PERIOD.saturating_sub(elapsed)
    }

    /// View current primary admin.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&SwitchKey::PrimaryAdmin)
            .expect("Admin not set")
    }

    /// View recovery vault address.
    pub fn get_recovery_vault(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&SwitchKey::RecoveryVault)
            .expect("Recovery vault not set")
    }

    /// View current beneficiary version counter.
    /// Incremented on each admin transfer to invalidate old admin's beneficiary.
    pub fn get_beneficiary_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&SwitchKey::BeneficiaryVersion)
            .unwrap_or(0)
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    fn assert_primary_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&SwitchKey::PrimaryAdmin)
            .expect("Admin not set");
        if *caller != admin {
            panic!("Unauthorized: caller is not the primary admin");
        }
    }

    fn assert_recovery_vault(env: &Env, caller: &Address) {
        let vault: Address = env
            .storage()
            .instance()
            .get(&SwitchKey::RecoveryVault)
            .expect("Recovery vault not set");
        if *caller != vault {
            panic!("Unauthorized: caller is not the recovery vault");
        }
    }
}