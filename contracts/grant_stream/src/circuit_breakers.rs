/// Circuit Breakers: Oracle Price Deviation Guard (#312), TVL Velocity Limit (#311), and Storage Rent Depletion Warning.
///
/// # Issue #312 — Oracle Price Deviation Guard
/// If the XLM price reported by the oracle changes by more than 50% relative to
/// the previously stored price in a single ledger update, all price-dependent
/// operations (swaps, price-dependent withdrawals) are frozen.  The freeze is
/// lifted only after a designated "sanity-check" oracle confirms the new price.
///
/// # Issue #311 — Sudden TVL Drain (Velocity Limit)
/// Tracks the total amount withdrawn from the protocol within any rolling 6-hour
/// window.  If cumulative withdrawals exceed 20% of the total protocol liquidity
/// snapshot, the contract enters `SoftPause` mode.  An admin must explicitly call
/// `resume_after_velocity_check` to resume normal operations.
///
/// # Storage Rent Depletion Warning
/// Monitors the contract's native XLM balance to ensure sufficient funds for storage rent.
/// If the balance falls below a 3-month rent buffer, non-essential functions are disabled
/// to preserve funds for storage maintenance.

use soroban_sdk::{contracttype, token, Address, Env};
use crate::storage_keys::StorageKey;
use crate::Error;

// ── Constants ─────────────────────────────────────────────────────────────────

/// 50% deviation threshold (in basis points: 5000 / 10000 = 50%).
const PRICE_DEVIATION_BPS: i128 = 5_000;
/// 20% TVL drain threshold (in basis points: 2000 / 10000 = 20%).
const TVL_DRAIN_BPS: i128 = 2_000;
/// 6-hour rolling window in seconds.
const VELOCITY_WINDOW_SECS: u64 = 6 * 60 * 60;
/// 48-hour oracle heartbeat interval.
const ORACLE_HEARTBEAT_INTERVAL_SECS: u64 = 48 * 60 * 60;
/// 24-hour dispute monitoring window.
const DISPUTE_WINDOW_SECS: u64 = 24 * 60 * 60;
/// 15% dispute threshold (in basis points: 1500 / 10000 = 15%).
const DISPUTE_THRESHOLD_BPS: i128 = 1_500;

// ── Storage Rent Depletion Constants ───────────────────────────────────────

/// Base rent reserve per month in XLM (1 XLM = 10^7 stroops).
/// This is a conservative estimate based on typical contract storage usage.
const MONTHLY_RENT_XLM: i128 = 1 * 10i128.pow(7); // 1 XLM per month
/// 3-month rent buffer threshold.
const RENT_BUFFER_MONTHS: u32 = 3;
/// Total rent buffer for 3 months in XLM (stroops).
const RENT_BUFFER_XLM: i128 = MONTHLY_RENT_XLM * RENT_BUFFER_MONTHS as i128; // 3 XLM

// ── Storage Keys ──────────────────────────────────────────────────────────────

// Legacy CircuitBreakerKey type alias preserved for backward compatibility.
// All runtime storage uses `StorageKey` directly.
type CircuitBreakerKey = StorageKey;

// ── Oracle Price Guard (Issue #312) ───────────────────────────────────────────

/// Record a new oracle price ping.  Returns `true` if the price was accepted
/// normally, or `false` if the deviation exceeded 50% and the guard was tripped.
///
/// When the guard is tripped the caller should prevent any price-dependent
/// operations until `confirm_oracle_price` is called by the sanity oracle.
pub fn record_oracle_price(env: &Env, new_price: i128) -> bool {
    // Ping the heartbeat on price update
    ping_oracle_heartbeat(env);

    let last: i128 = env
        .storage()
        .instance()
        .get(&StorageKey::LastPric)
        .unwrap_or(0);

    if last > 0 {
        // Calculate absolute deviation in basis points.
        let diff = if new_price > last { new_price - last } else { last - new_price };
        let deviation_bps = diff
            .saturating_mul(10_000)
            .checked_div(last)
            .unwrap_or(i128::MAX);

        if deviation_bps >= PRICE_DEVIATION_BPS {
            // Trip the circuit breaker — do NOT update the stored price yet.
            env.storage()
                .instance()
                .set(&StorageKey::OraFrozen, &true);
            return false; // price rejected; guard tripped
        }
    }

    // Price is within acceptable range — store it and ensure guard is clear.
    env.storage()
        .instance()
        .set(&StorageKey::LastPric, &new_price);
    env.storage()
        .instance()
        .set(&StorageKey::OraFrozen, &false);
    true
}

/// Called by the sanity-check oracle to confirm a suspicious price after the
/// guard has been tripped.  Clears the freeze and stores the confirmed price.
pub fn confirm_oracle_price(env: &Env, caller: &Address, confirmed_price: i128) -> Result<(), Error> {
    let sanity_oracle: Address = env
        .storage()
        .instance()
        .get(&StorageKey::SanityOracle)
        .ok_or(Error::NotSanityOracle)?;
    if *caller != sanity_oracle {
        return Err(Error::NotSanityOracle);
    }
    caller.require_auth();

    env.storage()
        .instance()
        .set(&StorageKey::LastPric, &confirmed_price);
    env.storage()
        .instance()
        .set(&StorageKey::OracleFrozen, &false);
    Ok(())
}

/// Returns `true` when the oracle price circuit breaker is active (frozen).
pub fn is_oracle_frozen(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::OraFrozen)
        .unwrap_or(false) ||
    env.storage()
        .instance()
        .get(&StorageKey::OraFrozenDueToNoHeartbeat)
        .unwrap_or(false)
}

/// Set (or update) the sanity-check oracle address.  Must be called by the
/// contract admin before the guard can be cleared via `confirm_oracle_price`.
pub fn set_sanity_oracle(env: &Env, sanity_oracle: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::SanityOra, sanity_oracle);
}

// ── Oracle Heartbeat and Manual Revert (Emergency Manual Revert for Oracle Freeze) ───────────────────────────────────────────

/// Ping the oracle heartbeat, resetting the freeze if active.
pub fn ping_oracle_heartbeat(env: &Env) {
    let now = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&StorageKey::OraHeart, &now);
    env.storage()
        .instance()
        .set(&StorageKey::OraFrozenDueToNoHeartbeat, &false);
}

/// Check if oracle heartbeat is still valid. If not, freeze.
pub fn check_oracle_heartbeat(env: &Env) -> bool {
    let last: u64 = env
        .storage()
        .instance()
        .get(&StorageKey::OraHeart)
        .unwrap_or(0);
    let current = env.ledger().timestamp();
    if current.saturating_sub(last) > ORACLE_HEARTBEAT_INTERVAL_SECS {
        env.storage()
            .instance()
            .set(&StorageKey::OraFrozenDueToNoHeartbeat, &true);
        false
    } else {
        true
    }
}

/// Set manual exchange rate via DAO vote, clearing the freeze.
pub fn set_manual_exchange_rate(env: &Env, rate: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::ManRate, &rate);
    env.storage()
        .instance()
        .set(&StorageKey::OraFrozenDueToNoHeartbeat, &false);
}

/// Get the manual exchange rate if set.
pub fn get_manual_exchange_rate(env: &Env) -> Option<i128> {
    env.storage()
        .instance()
        .get(&StorageKey::ManRate)
}

// ── TVL Velocity Limit (Issue #311) ───────────────────────────────────────────

/// Update the TVL snapshot used as the denominator for velocity checks.
/// Should be called whenever the total protocol liquidity changes materially
/// (e.g., after a large deposit or at initialisation).
pub fn update_tvl_snapshot(env: &Env, total_liquidity: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TvlSnap, &total_liquidity);
}

/// Record a withdrawal of `amount` tokens and check whether the rolling 6-hour
/// velocity limit has been breached.
///
/// Returns `true` if the withdrawal is within limits, `false` if the velocity
/// limit was just breached (SoftPause has been engaged).
///
/// Panics if the contract is already in SoftPause — callers must check
/// `is_soft_paused` before calling this.
pub fn record_withdrawal_velocity(env: &Env, amount: i128) -> Result<bool, Error> {
    if is_soft_paused(env) {
        return Err(Error::SoftPaused);
    }

    assert!(amount > 0, "record_withdrawal_velocity: amount must be positive");

    let now: u64 = env.ledger().timestamp();
    let window_start: u64 = env
        .storage()
        .instance()
        .get(&StorageKey::VelWinSt)
        .unwrap_or(now);
    let mut accumulator: i128 = env
        .storage()
        .instance()
        .get(&StorageKey::VelAccum)
        .unwrap_or(0);

    // Reset window if it has expired.
    if now.saturating_sub(window_start) >= VELOCITY_WINDOW_SECS {
        env.storage()
            .instance()
            .set(&StorageKey::VelWinSt, &now);
        accumulator = 0;
    }

    accumulator = accumulator.saturating_add(amount);
    env.storage()
        .instance()
        .set(&StorageKey::VelAccum, &accumulator);

    let tvl: i128 = env
        .storage()
        .instance()
        .get(&StorageKey::TvlSnap)
        .unwrap_or(0);

    if tvl > 0 {
        let drain_bps = accumulator
            .saturating_mul(10_000)
            .checked_div(tvl)
            .unwrap_or(i128::MAX);

        if drain_bps >= TVL_DRAIN_BPS {
            env.storage()
                .instance()
                .set(&StorageKey::SoftPa, &true);
            return Ok(false); // velocity limit breached; SoftPause engaged
        }
    }

    Ok(true)
}

/// Returns `true` when the contract is in SoftPause due to a velocity breach.
pub fn is_soft_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::SoftPa)
        .unwrap_or(false)
}

/// Admin-only: clear SoftPause after manual verification.
pub fn resume_after_velocity_check(env: &Env, admin: &Address) {
    admin.require_auth();
    env.storage()
        .instance()
        .set(&StorageKey::SoftPa, &false);
    // Reset the velocity window so the 6-hour clock starts fresh.
    let now: u64 = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&StorageKey::VelWinSt, &now);
    env.storage()
        .instance()
        .set(&StorageKey::VelAccum, &0_i128);
}

// ── Mass Milestone Dispute Trigger (Sybil-Dispute Attack Protection) ─────────

/// Record a new dispute and check whether the mass dispute threshold has been breached.
///
/// Returns `true` if the dispute is recorded normally, `false` if the threshold
/// was just breached and grant initialization has been halted.
///
/// This function should be called whenever a grant is placed into "Dispute" status.
pub fn record_dispute(env: &Env, active_grants_count: u32) -> bool {
    let now: u64 = env.ledger().timestamp();
    let window_start: u64 = env
        .storage()
        .instance()
        .get(&StorageKey::DispWin)
        .unwrap_or(now);
    let mut accumulator: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::DispAcc)
        .unwrap_or(0);

    // Reset window if it has expired (24 hours).
    if now.saturating_sub(window_start) >= DISPUTE_WINDOW_SECS {
        env.storage()
            .instance()
            .set(&StorageKey::DispWin, &now);
        env.storage()
            .instance()
            .set(&StorageKey::ActGntSn, &active_grants_count);
        accumulator = 0;
    }

    // Update the active grants snapshot if this is the first dispute in the window
    if accumulator == 0 {
        env.storage()
            .instance()
            .set(&StorageKey::ActGntSn, &active_grants_count);
    }

    accumulator = accumulator.saturating_add(1);
    env.storage()
        .instance()
        .set(&StorageKey::DispAcc, &accumulator);

    // Check if disputes exceed 15% of active grants
    let snapshot_grants: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::ActGntSn)
        .unwrap_or(active_grants_count);

    if snapshot_grants > 0 {
        let dispute_percentage_bps = (accumulator as i128)
            .saturating_mul(10_000)
            .checked_div(snapshot_grants as i128)
            .unwrap_or(i128::MAX);

        if dispute_percentage_bps >= DISPUTE_THRESHOLD_BPS {
            // Trip the circuit breaker - halt new grant initializations
            env.storage()
                .instance()
                .set(&StorageKey::GntHalt, &true);
            return false; // threshold breached; grant initialization halted
        }
    }

    true
}

/// Returns `true` when new grant initialization is halted due to mass dispute trigger.
pub fn is_grant_initialization_halted(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::GntHalt)
        .unwrap_or(false)
}

/// Admin-only: resume grant initialization after manual verification of dispute activity.
/// This should only be called after the admin has investigated the dispute pattern
/// and determined it was not a coordinated Sybil attack.
pub fn resume_grant_initialization(env: &Env, admin: &Address) {
    admin.require_auth();
    env.storage()
        .instance()
        .set(&StorageKey::GntHalt, &false);
    // Reset the dispute window so the 24-hour clock starts fresh.
    let now: u64 = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&StorageKey::DispWin, &now);
    env.storage()
        .instance()
        .set(&StorageKey::DispAcc, &0_u32);
    env.storage()
        .instance()
        .set(&StorageKey::ActGntSn, &0_u32);
}

/// Get current dispute monitoring statistics for transparency.
pub fn get_dispute_monitoring_stats(env: &Env) -> (u64, u32, u32, bool) {
    let window_start: u64 = env
        .storage()
        .instance()
        .get(&StorageKey::DispWin)
        .unwrap_or(0);
    let dispute_count: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::DispAcc)
        .unwrap_or(0);
    let active_grants_snapshot: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::ActGntSn)
        .unwrap_or(0);
    let halted: bool = is_grant_initialization_halted(env);

    (window_start, dispute_count, active_grants_snapshot, halted)
}

pub fn get_rent_buffer_threshold(_env: &Env) -> i128 {
    RENT_BUFFER_XLM
}

pub fn get_current_xlm_balance(env: &Env) -> i128 {
    if let Some(native_token) = env.storage().instance().get::<_, Address>(&StorageKey::NativeToken) {
        let client = token::Client::new(env, &native_token);
        client.balance(&env.current_contract_address())
    } else {
        0
    }
}

pub fn is_rent_preservation_mode(env: &Env) -> bool {
    env.storage().instance().get(&StorageKey::RentPreservationMode).unwrap_or(false)
}

pub fn check_rent_balance(env: &Env) -> bool {
    let below_threshold = get_current_xlm_balance(env) < RENT_BUFFER_XLM;
    env.storage().instance().set(&StorageKey::RentPreservationMode, &below_threshold);
    below_threshold
}

pub fn is_function_allowed(env: &Env, essential: bool) -> bool {
    essential || !is_rent_preservation_mode(env)
}

pub fn disable_rent_preservation_mode(env: &Env, admin: &Address) {
    admin.require_auth();
    env.storage().instance().set(&StorageKey::RentPreservationMode, &false);
}
