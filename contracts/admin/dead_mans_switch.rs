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

    /// Reset the Dead Man's Switch activity timer.
    ///
    /// Called by the admin contract during `accept_ownership` to prevent
    /// the old admin's activity timestamp from being retained after transfer.
    /// Without this, the new admin could not trigger recovery because the
    /// old admin's heartbeat is still "active" — or the old admin's last
    /// activity is carried over to the new admin's tenure.
    ///
    /// Only the current primary admin or recovery vault can call this.
    pub fn reset_dms_on_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        // Verify caller is either the current primary admin or recovery vault
        let primary_admin: Address = env
            .storage()
            .instance()
            .get(&SwitchKey::PrimaryAdmin)
            .expect("Primary admin not set");
        let recovery_vault: Address = env
            .storage()
            .instance()
            .get(&SwitchKey::RecoveryVault)
            .expect("Recovery vault not set");

        if caller != primary_admin && caller != recovery_vault {
            panic!("Unauthorized: only primary admin or recovery vault can reset DMS");
        }

        // Reset the activity timer to zero to prevent beneficiary retention
        // with the old admin's timestamp. The new admin must heartbeat
        // within the inactivity period or the recovery vault can claim.
        env.storage().instance().set(&SwitchKey::LastActivityAt, &0u64);

        // Reset the recovery executed flag so the new admin cycle starts fresh
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &false);

        env.events().publish(
            (soroban_sdk::symbol_short!("dms_reset"),),
            (caller, env.ledger().timestamp()),
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