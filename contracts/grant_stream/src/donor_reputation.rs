//! Donor Reputation Module
//! 
//! This module implements a reputation system that tracks donor success rates based on
//! the performance of projects they fund. Higher reputation donors have increased
//! influence in matching rounds, creating incentives for quality due diligence.
//! 
//! Key Features:
//! - Success rate tracking based on milestone completion
//! - Minimum funding thresholds to prevent reputation farming
//! - Linear influence scaling in matching rounds
//! - ReputationUpdated event emissions
//! - Security measures against manipulation

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
    Vec, Symbol, String, Map, xdr::ToXdr,
};
use crate::storage_keys::StorageKey;
use crate::{Grant, GrantStatus};

// ── Constants ────────────────────────────────────────────────────────────

/// Fixed-point precision for reputation calculations (18 decimals)
pub const REPUTATION_SCALE: i128 = 10_i128.pow(18);
/// Basis points scale (100 = 1%)
pub const BASIS_POINTS: i128 = 10_000;
/// Default minimum funding threshold for reputation accrual (100 USDC in 7-decimal units)
pub const DEFAULT_MIN_FUNDING_THRESHOLD: i128 = 100 * 10_000_000;
/// Maximum reputation multiplier to prevent excessive influence
pub const MAX_REPUTATION_MULTIPLIER: i128 = 3 * REPUTATION_SCALE; // 3x max influence
/// Time window for reputation calculation (90 days in seconds)
pub const REPUTATION_WINDOW_SECS: u64 = 90 * 24 * 60 * 60;

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct DonorReputation {
    /// Donor address
    pub donor: Address,
    /// Current reputation score (scaled by REPUTATION_SCALE)
    pub reputation_score: i128,
    /// Success rate as percentage (scaled by BASIS_POINTS)
    pub success_rate: i128,
    /// Total amount funded across all qualifying projects
    pub total_funded: i128,
    /// Number of projects funded that meet minimum threshold
    pub qualifying_projects: u32,
    /// Number of successfully completed projects
    pub successful_projects: u32,
    /// Timestamp of last reputation update
    pub last_updated: u64,
    /// Reputation influence multiplier (scaled by REPUTATION_SCALE)
    pub influence_multiplier: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct ProjectSuccessMetrics {
    /// Project ID
    pub project_id: u64,
    /// Donor who funded this project
    pub donor: Address,
    /// Total milestones for this project
    pub total_milestones: u32,
    /// Successfully completed milestones
    pub completed_milestones: u32,
    /// Project status
    pub project_status: GrantStatus,
    /// Amount funded by donor
    pub funded_amount: i128,
    /// Timestamp when project was created
    pub created_at: u64,
    /// Timestamp when project was completed/failed
    pub completed_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct MilestoneRecord {
    /// Project ID
    pub project_id: u64,
    /// Milestone index
    pub milestone_index: u32,
    /// Whether milestone was successfully completed
    pub is_completed: bool,
    /// Timestamp of completion
    pub completed_at: Option<u64>,
    /// Evidence hash for milestone completion
    pub evidence_hash: Option<soroban_sdk::BytesN<32>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct ReputationConfig {
    /// Minimum funding threshold for reputation accrual
    pub min_funding_threshold: i128,
    /// Maximum reputation multiplier
    pub max_multiplier: i128,
    /// Reputation calculation window in seconds
    pub calculation_window_secs: u64,
    /// Weight given to recent projects vs historical (basis points)
    pub recency_weight: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct ReputationUpdate {
    /// Update ID (monotonically increasing)
    pub update_id: u64,
    /// Donor address
    pub donor: Address,
    /// Previous reputation score
    pub previous_score: i128,
    /// New reputation score
    pub new_score: i128,
    /// Reason for update
    pub reason: ReputationUpdateReason,
    /// Associated project ID (if applicable)
    pub project_id: Option<u64>,
    /// Timestamp of update
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ReputationUpdateReason {
    /// New project funded
    ProjectFunded,
    /// Milestone completed
    MilestoneCompleted,
    /// Project completed successfully
    ProjectCompleted,
    /// Project failed
    ProjectFailed,
    /// Manual adjustment by admin
    ManualAdjustment,
    /// Recalculation due to configuration change
    Recalculation,
}

// ── Error Types ──────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ReputationError {
    NotInitialized = 1,
    NotAuthorized = 2,
    DonorNotFound = 3,
    ProjectNotFound = 4,
    InvalidAmount = 5,
    InsufficientThreshold = 6,
    MathOverflow = 7,
    InvalidTimestamp = 8,
    ReputationTooHigh = 9,
    DuplicateMilestone = 10,
    InvalidMilestoneIndex = 11,
    ProjectNotCompleted = 12,
    ConfigurationError = 13,
}

// ── Helper Functions ────────────────────────────────────────────────────

/// Read donor reputation data
fn read_donor_reputation(env: &Env, donor: &Address) -> Result<DonorReputation, ReputationError> {
    env.storage()
        .instance()
        .get(&StorageKey::DonorReputation(donor.clone()))
        .ok_or(ReputationError::DonorNotFound)
}

/// Write donor reputation data
fn write_donor_reputation(env: &Env, donor: &Address, reputation: &DonorReputation) {
    env.storage().instance().set(&StorageKey::DonorReputation(donor.clone()), reputation);
}

/// Read project success metrics
fn read_project_metrics(env: &Env, project_id: u64) -> Result<ProjectSuccessMetrics, ReputationError> {
    env.storage()
        .instance()
        .get(&StorageKey::ProjectSuccessMetrics(project_id))
        .ok_or(ReputationError::ProjectNotFound)
}

/// Write project success metrics
fn write_project_metrics(env: &Env, project_id: u64, metrics: &ProjectSuccessMetrics) {
    env.storage().instance().set(&StorageKey::ProjectSuccessMetrics(project_id), metrics);
}

/// Read reputation configuration
fn read_reputation_config(env: &Env) -> ReputationConfig {
    env.storage()
        .instance()
        .get(&StorageKey::ReputationConfig)
        .unwrap_or(ReputationConfig {
            min_funding_threshold: DEFAULT_MIN_FUNDING_THRESHOLD,
            max_multiplier: MAX_REPUTATION_MULTIPLIER,
            calculation_window_secs: REPUTATION_WINDOW_SECS,
            recency_weight: 50 * BASIS_POINTS / 100, // 50% weight to recent
        })
}

/// Write reputation configuration
fn write_reputation_config(env: &Env, config: &ReputationConfig) {
    env.storage().instance().set(&StorageKey::ReputationConfig, config);
}

/// Get next reputation update ID
fn next_update_id(env: &Env) -> u64 {
    let id = env.storage()
        .instance()
        .get(&StorageKey::ReputationUpdateHistory(0_u64))
        .unwrap_or(0_u64)
        .saturating_add(1);
    env.storage().instance().set(&StorageKey::ReputationUpdateHistory(0_u64), &id);
    id
}

/// Record reputation update for audit trail
fn record_reputation_update(
    env: &Env,
    donor: &Address,
    previous_score: i128,
    new_score: i128,
    reason: ReputationUpdateReason,
    project_id: Option<u64>,
) {
    let update_id = next_update_id(env);
    let update = ReputationUpdate {
        update_id,
        donor: donor.clone(),
        previous_score,
        new_score,
        reason,
        project_id,
        timestamp: env.ledger().timestamp(),
    };
    
    env.storage().instance().set(&StorageKey::ReputationUpdateHistory(update_id), &update);
}

/// Calculate success rate for a donor based on their funded projects
fn calculate_success_rate(env: &Env, donor: &Address) -> Result<i128, ReputationError> {
    let funded_projects: Vec<u64> = env.storage()
        .instance()
        .get(&StorageKey::DonorFundedProjects(donor.clone()))
        .unwrap_or_else(|| Vec::new(env));

    if funded_projects.is_empty() {
        return Ok(0);
    }

    let mut successful_count = 0_u32;
    let mut total_count = 0_u32;

    for i in 0..funded_projects.len() {
        let project_id = funded_projects.get(i).unwrap();
        if let Ok(metrics) = read_project_metrics(env, project_id) {
            // Only count projects that meet minimum funding threshold
            let config = read_reputation_config(env);
            if metrics.funded_amount >= config.min_funding_threshold {
                total_count += 1;
                if metrics.project_status == GrantStatus::Completed {
                    successful_count += 1;
                }
            }
        }
    }

    if total_count == 0 {
        return Ok(0);
    }

    // Calculate success rate as basis points
    let success_rate = (successful_count as i128) * BASIS_POINTS / (total_count as i128);
    Ok(success_rate)
}

/// Calculate influence multiplier based on reputation score
fn calculate_influence_multiplier(reputation_score: i128, config: &ReputationConfig) -> i128 {
    // Linear scaling: 1x at 0 reputation, max_multiplier at full reputation
    // reputation_score is scaled by REPUTATION_SCALE
    let multiplier = REPUTATION_SCALE + 
        (reputation_score * (config.max_multiplier - REPUTATION_SCALE) / REPUTATION_SCALE);
    
    // Ensure multiplier doesn't exceed maximum
    multiplier.min(config.max_multiplier)
}

// ── Contract Implementation ──────────────────────────────────────────────

#[contract]
pub struct DonorReputationContract;

#[contractimpl]
impl DonorReputationContract {
    /// Initialize the reputation system with default configuration
    pub fn initialize(env: Env, admin: Address) -> Result<(), ReputationError> {
        admin.require_auth();

        // Check if already initialized
        if env.storage().instance().has(&StorageKey::ReputationConfig) {
            return Err(ReputationError::NotInitialized);
        }

        // Set default configuration
        let config = ReputationConfig {
            min_funding_threshold: DEFAULT_MIN_FUNDING_THRESHOLD,
            max_multiplier: MAX_REPUTATION_MULTIPLIER,
            calculation_window_secs: REPUTATION_WINDOW_SECS,
            recency_weight: 50 * BASIS_POINTS / 100,
        };
        
        write_reputation_config(&env, &config);

        // Initialize update counter
        env.storage().instance().set(&StorageKey::ReputationUpdateHistory(0_u64), &0_u64);

        env.events().publish(
            (symbol_short!("rep_init"),),
            (admin.clone(), DEFAULT_MIN_FUNDING_THRESHOLD, MAX_REPUTATION_MULTIPLIER),
        );

        Ok(())
    }

    /// Update reputation configuration (admin only)
    pub fn update_config(
        env: Env,
        admin: Address,
        min_funding_threshold: Option<i128>,
        max_multiplier: Option<i128>,
        calculation_window_secs: Option<u64>,
        recency_weight: Option<i128>,
    ) -> Result<(), ReputationError> {
        admin.require_auth();

        let mut config = read_reputation_config(&env);

        if let Some(threshold) = min_funding_threshold {
            if threshold <= 0 {
                return Err(ReputationError::InvalidAmount);
            }
            config.min_funding_threshold = threshold;
        }

        if let Some(multiplier) = max_multiplier {
            if multiplier <= REPUTATION_SCALE {
                return Err(ReputationError::InvalidAmount);
            }
            config.max_multiplier = multiplier;
        }

        if let Some(window) = calculation_window_secs {
            if window == 0 {
                return Err(ReputationError::InvalidTimestamp);
            }
            config.calculation_window_secs = window;
        }

        if let Some(weight) = recency_weight {
            if weight < 0 || weight > BASIS_POINTS {
                return Err(ReputationError::ConfigurationError);
            }
            config.recency_weight = weight;
        }

        write_reputation_config(&env, &config);

        env.events().publish(
            (symbol_short!("rep_config"),),
            (
                config.min_funding_threshold,
                config.max_multiplier,
                config.calculation_window_secs,
                config.recency_weight,
            ),
        );

        Ok(())
    }

    /// Record a new project funded by a donor
    pub fn record_project_funded(
        env: Env,
        donor: Address,
        project_id: u64,
        funded_amount: i128,
        total_milestones: u32,
    ) -> Result<(), ReputationError> {
        donor.require_auth();

        if funded_amount <= 0 || total_milestones == 0 {
            return Err(ReputationError::InvalidAmount);
        }

        let config = read_reputation_config(&env);

        // Create project success metrics
        let metrics = ProjectSuccessMetrics {
            project_id,
            donor: donor.clone(),
            total_milestones,
            completed_milestones: 0,
            project_status: GrantStatus::Active,
            funded_amount,
            created_at: env.ledger().timestamp(),
            completed_at: None,
        };

        write_project_metrics(&env, project_id, &metrics);

        // Add to donor's funded projects
        let mut funded_projects: Vec<u64> = env.storage()
            .instance()
            .get(&StorageKey::DonorFundedProjects(donor.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        
        funded_projects.push_back(project_id);
        env.storage().instance().set(&StorageKey::DonorFundedProjects(donor.clone()), &funded_projects);

        // Update donor reputation if funding meets threshold
        if funded_amount >= config.min_funding_threshold {
            Self::update_donor_reputation_internal(&env, &donor, ReputationUpdateReason::ProjectFunded, Some(project_id))?;
        }

        env.events().publish(
            (symbol_short!("proj_funded"),),
            (donor.clone(), project_id, funded_amount, total_milestones),
        );

        Ok(())
    }

    /// Record a milestone completion for a project
    pub fn record_milestone_completed(
        env: Env,
        project_id: u64,
        milestone_index: u32,
        evidence_hash: Option<soroban_sdk::BytesN<32>>,
    ) -> Result<(), ReputationError> {
        let metrics = read_project_metrics(&env, project_id)?;

        // Validate milestone index
        if milestone_index >= metrics.total_milestones {
            return Err(ReputationError::InvalidMilestoneIndex);
        }

        // Check if milestone already recorded
        let milestone_key = StorageKey::ProjectMilestoneRecord(project_id, milestone_index);
        if env.storage().instance().has(&milestone_key) {
            return Err(ReputationError::DuplicateMilestone);
        }

        // Record milestone completion
        let record = MilestoneRecord {
            project_id,
            milestone_index,
            is_completed: true,
            completed_at: Some(env.ledger().timestamp()),
            evidence_hash,
        };

        env.storage().instance().set(&milestone_key, &record);

        // Update project metrics
        let mut updated_metrics = metrics;
        updated_metrics.completed_milestones += 1;

        // Check if all milestones are completed
        if updated_metrics.completed_milestones == updated_metrics.total_milestones {
            updated_metrics.project_status = GrantStatus::Completed;
            updated_metrics.completed_at = Some(env.ledger().timestamp());
        }

        write_project_metrics(&env, project_id, &updated_metrics);

        // Update donor reputation
        Self::update_donor_reputation_internal(
            &env,
            &updated_metrics.donor,
            ReputationUpdateReason::MilestoneCompleted,
            Some(project_id),
        )?;

        // Emit ReputationUpdated event
        if let Ok(reputation) = read_donor_reputation(&env, &updated_metrics.donor) {
            env.events().publish(
                (symbol_short!("rep_updated"),),
                (
                    updated_metrics.donor.clone(),
                    reputation.reputation_score,
                    reputation.success_rate,
                    reputation.influence_multiplier,
                ),
            );
        }

        env.events().publish(
            (symbol_short!("milestone_comp"),),
            (project_id, milestone_index, updated_metrics.completed_milestones),
        );

        Ok(())
    }

    /// Mark a project as failed (for reputation calculation)
    pub fn record_project_failed(
        env: Env,
        project_id: u64,
    ) -> Result<(), ReputationError> {
        let mut metrics = read_project_metrics(&env, project_id)?;

        if metrics.project_status == GrantStatus::Completed {
            return Err(ReputationError::ProjectNotCompleted);
        }

        metrics.project_status = GrantStatus::Cancelled;
        metrics.completed_at = Some(env.ledger().timestamp());

        write_project_metrics(&env, project_id, &metrics);

        // Update donor reputation
        Self::update_donor_reputation_internal(
            &env,
            &metrics.donor,
            ReputationUpdateReason::ProjectFailed,
            Some(project_id),
        )?;

        env.events().publish(
            (symbol_short!("proj_failed"),),
            (project_id, metrics.donor.clone()),
        );

        Ok(())
    }

    /// Get donor reputation data
    pub fn get_donor_reputation(env: Env, donor: Address) -> Result<DonorReputation, ReputationError> {
        read_donor_reputation(&env, &donor)
    }

    /// Get project success metrics
    pub fn get_project_metrics(env: Env, project_id: u64) -> Result<ProjectSuccessMetrics, ReputationError> {
        read_project_metrics(&env, project_id)
    }

    /// Get current reputation configuration
    pub fn get_reputation_config(env: Env) -> ReputationConfig {
        read_reputation_config(&env)
    }

    /// Calculate influence multiplier for a donor in matching rounds
    pub fn calculate_influence(env: Env, donor: Address) -> Result<i128, ReputationError> {
        let reputation = read_donor_reputation(&env, &donor)?;
        let config = read_reputation_config(&env);
        
        Ok(calculate_influence_multiplier(reputation.reputation_score, &config))
    }

    /// Get reputation update history
    pub fn get_reputation_update(env: Env, update_id: u64) -> Result<ReputationUpdate, ReputationError> {
        env.storage()
            .instance()
            .get(&StorageKey::ReputationUpdateHistory(update_id))
            .ok_or(ReputationError::DonorNotFound)
    }

    // ── Internal Functions ────────────────────────────────────────────────────

    /// Internal function to update donor reputation
    fn update_donor_reputation_internal(
        env: &Env,
        donor: &Address,
        reason: ReputationUpdateReason,
        project_id: Option<u64>,
    ) -> Result<(), ReputationError> {
        let config = read_reputation_config(env);
        let success_rate = calculate_success_rate(env, donor)?;
        
        // Calculate new reputation score based on success rate
        // Linear scaling: 0% success = 0 reputation, 100% success = REPUTATION_SCALE
        let reputation_score = success_rate * REPUTATION_SCALE / BASIS_POINTS;
        
        // Calculate influence multiplier
        let influence_multiplier = calculate_influence_multiplier(reputation_score, &config);

        let previous_score = if let Ok(mut reputation) = read_donor_reputation(env, donor) {
            let old_score = reputation.reputation_score;
            reputation.reputation_score = reputation_score;
            reputation.success_rate = success_rate;
            reputation.influence_multiplier = influence_multiplier;
            reputation.last_updated = env.ledger().timestamp();
            
            // Update project counts
            let funded_projects: Vec<u64> = env.storage()
                .instance()
                .get(&StorageKey::DonorFundedProjects(donor.clone()))
                .unwrap_or_else(|| Vec::new(env));
            
            reputation.qualifying_projects = 0;
            reputation.successful_projects = 0;
            reputation.total_funded = 0;

            for i in 0..funded_projects.len() {
                let proj_id = funded_projects.get(i).unwrap();
                if let Ok(metrics) = read_project_metrics(env, proj_id) {
                    reputation.total_funded += metrics.funded_amount;
                    if metrics.funded_amount >= config.min_funding_threshold {
                        reputation.qualifying_projects += 1;
                        if metrics.project_status == GrantStatus::Completed {
                            reputation.successful_projects += 1;
                        }
                    }
                }
            }

            write_donor_reputation(env, donor, &reputation);
            old_score
        } else {
            // Create new reputation record
            let reputation = DonorReputation {
                donor: donor.clone(),
                reputation_score,
                success_rate,
                total_funded: project_id
                    .and_then(|id| read_project_metrics(env, id).ok())
                    .map(|m| m.funded_amount)
                    .unwrap_or(0),
                qualifying_projects: if project_id.is_some() { 1 } else { 0 },
                successful_projects: 0,
                last_updated: env.ledger().timestamp(),
                influence_multiplier,
            };

            write_donor_reputation(env, donor, &reputation);
            0
        };

        // Record update for audit trail
        record_reputation_update(env, donor, previous_score, reputation_score, reason, project_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_reputation_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);

        DonorReputationContract::initialize(env.clone(), admin.clone()).unwrap();

        let config = DonorReputationContract::get_reputation_config(env.clone());
        assert_eq!(config.min_funding_threshold, DEFAULT_MIN_FUNDING_THRESHOLD);
        assert_eq!(config.max_multiplier, MAX_REPUTATION_MULTIPLIER);
    }

    #[test]
    fn test_project_funding() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let donor = Address::generate(&env);

        DonorReputationContract::initialize(env.clone(), admin.clone()).unwrap();

        DonorReputationContract::record_project_funded(
            env.clone(),
            donor.clone(),
            1,
            DEFAULT_MIN_FUNDING_THRESHOLD,
            5,
        ).unwrap();

        let metrics = DonorReputationContract::get_project_metrics(env.clone(), 1).unwrap();
        assert_eq!(metrics.project_id, 1);
        assert_eq!(metrics.donor, donor);
        assert_eq!(metrics.funded_amount, DEFAULT_MIN_FUNDING_THRESHOLD);
        assert_eq!(metrics.total_milestones, 5);
        assert_eq!(metrics.completed_milestones, 0);
    }

    #[test]
    fn test_milestone_completion() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let donor = Address::generate(&env);

        DonorReputationContract::initialize(env.clone(), admin.clone()).unwrap();

        DonorReputationContract::record_project_funded(
            env.clone(),
            donor.clone(),
            1,
            DEFAULT_MIN_FUNDING_THRESHOLD,
            3,
        ).unwrap();

        DonorReputationContract::record_milestone_completed(
            env.clone(),
            1,
            0,
            None,
        ).unwrap();

        let metrics = DonorReputationContract::get_project_metrics(env.clone(), 1).unwrap();
        assert_eq!(metrics.completed_milestones, 1);
        assert_eq!(metrics.project_status, GrantStatus::Active);
    }

    #[test]
    fn test_influence_calculation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let donor = Address::generate(&env);

        DonorReputationContract::initialize(env.clone(), admin.clone()).unwrap();

        // Fund a project and complete all milestones
        DonorReputationContract::record_project_funded(
            env.clone(),
            donor.clone(),
            1,
            DEFAULT_MIN_FUNDING_THRESHOLD,
            2,
        ).unwrap();

        DonorReputationContract::record_milestone_completed(env.clone(), 1, 0, None).unwrap();
        DonorReputationContract::record_milestone_completed(env.clone(), 1, 1, None).unwrap();

        let reputation = DonorReputationContract::get_donor_reputation(env.clone(), donor.clone()).unwrap();
        assert_eq!(reputation.success_rate, BASIS_POINTS); // 100% success rate
        assert_eq!(reputation.reputation_score, REPUTATION_SCALE); // Full reputation

        let influence = DonorReputationContract::calculate_influence(env.clone(), donor.clone()).unwrap();
        assert_eq!(influence, MAX_REPUTATION_MULTIPLIER); // Maximum influence
    }

    #[test]
    fn test_minimum_funding_threshold() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let donor = Address::generate(&env);

        DonorReputationContract::initialize(env.clone(), admin.clone()).unwrap();

        // Fund below threshold - should not create reputation
        DonorReputationContract::record_project_funded(
            env.clone(),
            donor.clone(),
            1,
            DEFAULT_MIN_FUNDING_THRESHOLD / 2, // Below threshold
            2,
        ).unwrap();

        let result = DonorReputationContract::get_donor_reputation(env.clone(), donor.clone());
        assert!(result.is_err()); // No reputation created
    }
}
