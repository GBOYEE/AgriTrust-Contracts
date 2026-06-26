//! Vesting Schedule Scheduler
//!
//! Handles linear vesting with cliff period enforcement and slippage-aware
//! capital conversion timing.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, BytesN, Env, Symbol,
};

use crate::{
    VestingSchedule, VestingError, vesting_schedule_key, legacy_vesting_schedule_key,
    VESTING_SCHEDULE_VARIANT, VESTING_TTL_LEDGERS,
};

pub const CLIFF_PERIOD: u32 = 1_555_200; // 180 days in ledgers
pub const VESTING_PERIOD: u32 = 3_153_600; // 365 days in ledgers
pub const MAX_CONVERSION_DELAY: u32 = 100; // Max ledgers for capital conversion

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlippageAdjustedSchedule {
    pub base_schedule: VestingSchedule,
    pub conversion_start_ledger: u32,
    pub actual_cliff_met: bool,
    pub slippage_forfeiture_amount: i128,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SchedulerError {
    CliffViolation = 1,
    ConversionPending = 2,
    SlippageThresholdExceeded = 3,
}

/// Validates that the cliff period has been genuinely met, accounting for
/// capital conversion slippage that may delay the first distribution.
///
/// # Invariants
/// - If conversion takes longer than MAX_CONVERSION_DELAY ledgers, the cliff
///   is considered violated and cliff-accrued tokens are forfeited.
/// - The forfeited amount is proportional to the number of ledgers missed
///   beyond the cliff period.
pub fn validate_cliff_with_slippage(
    env: &Env,
    schedule: &VestingSchedule,
    conversion_delay_ledgers: u32,
) -> Result<i128, SchedulerError> {
    let current_ledger = env.ledger().sequence();
    let cliff_end = schedule.start_time + CLIFF_PERIOD as u64;

    // If conversion is still pending beyond max delay, cliff is violated
    if conversion_delay_ledgers > MAX_CONVERSION_DELAY {
        return Err(SchedulerError::SlippageThresholdExceeded);
    }

    // If the cliff hasn't elapsed yet, no distribution should be possible
    if current_ledger < cliff_end as u32 {
        return Err(SchedulerError::CliffViolation);
    }

    // Calculate slippage forfeiture: tokens that accrued during the delay
    // should be forfeited because the cliff wasn't genuinely met
    if conversion_delay_ledgers > 0 {
        let total_vested = schedule.total_amount - schedule.released_amount;
        let total_ledgers = (schedule.end_time - schedule.start_time) as u128;
        if total_ledgers == 0 {
            return Err(SchedulerError::CliffViolation);
        }
        let cliff_amount = (total_vested * CLIFF_PERIOD as i128) / total_ledgers as i128;
        let forfeited = (cliff_amount * conversion_delay_ledgers as i128) / CLIFF_PERIOD as i128;
        return Ok(schedule.released_amount + forfeited);
    }

    Ok(schedule.released_amount)
}

/// Releases vested tokens with cliff validation and slippage adjustment.
/// If the cliff was violated due to conversion slippage, the cliff-accrued
/// portion is forfeited and the grantee receives only the post-cliff amount.
pub fn release_vested_with_slippage(
    env: Env,
    grant_id: BytesN<32>,
    conversion_delay_ledgers: u32,
) -> Result<i128, VestingError> {
    let key = vesting_schedule_key(&env, &grant_id);
    let schedule: VestingSchedule = env.storage().persistent().get(&key)
        .ok_or(VestingError::ScheduleNotFound)?;

    let adjusted_amount = validate_cliff_with_slippage(&env, &schedule, conversion_delay_ledgers)
        .map_err(|e| match e {
            SchedulerError::CliffViolation => VestingError::InvalidSchedule,
            SchedulerError::SlippageThresholdExceeded => VestingError::InvalidSchedule,
            _ => VestingError::InvalidSchedule,
        })?;

    let releasable = adjusted_amount - schedule.released_amount;
    if releasable <= 0 {
        return Ok(0);
    }

    let updated = VestingSchedule {
        released_amount: schedule.released_amount + releasable,
        ..schedule
    };
    env.storage().persistent().set(&key, &updated);

    env.events().publish(
        (Symbol::short("vest_rel"), grant_id),
        (releasable, adjusted_amount),
    );

    Ok(releasable)
}
