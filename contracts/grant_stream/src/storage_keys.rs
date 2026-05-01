/// # Unified Storage Key Organization
/// 
/// This module provides a centralized, well-documented enum for all contract storage keys.
/// It prevents key collisions by using proper namespacing and clear categorization.
/// 
/// ## Key Categories:
/// 
/// 1. **Core Contract State** - Essential contract configuration and admin data
/// 2. **Grant Management** - All grant-related storage including metadata and balances
/// 3. **User Data** - User-specific balances, permissions, and grant associations
/// 4. **Treasury & Yield** - Treasury operations and yield farming data
/// 5. **Governance** - Proposal, voting, and governance-related storage
/// 6. **Circuit Breakers** - Safety mechanisms and monitoring data
/// 7. **Audit & Reporting** - Audit logs, tax reporting, and compliance data
/// 8. **Multi-Token Operations** - Multi-token and wrapped asset storage
/// 9. **Emergency & Recovery** - Multi-signature rescue operations
/// 10. **Reentrancy Protection** - Security guards against reentrancy attacks

use soroban_sdk::{contracttype, Address, Bytes, String};

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype(export = false)]
pub enum StorageKey {
    // ── Core Contract State ──────────────────────────────────────────────────────
    
    /// Contract administrator address with full permissions
    Admin,
    /// Primary token address used for grants (e.g., USDC)
    GrantToken,
    /// Native token address (e.g., XLM) for fees and bounties
    NativeToken,
    /// Treasury address for holding and managing funds
    Treasury,
    /// Oracle address for price feeds and external data
    Oracle,
    /// Global list of all grant IDs for iteration
    GrantIds,
    /// Initialization marker flag
    Init,
    
    // ── Grant Management ────────────────────────────────────────────────────────
    
    /// Individual grant data keyed by grant ID
    Grant(u64),
    /// Lightweight cryptographic tombstone for pruned grants
    Tombstone(u64),
    /// Grant milestone data keyed by (grant_id, milestone_index)
    Milestone(u64, u32),
    /// Expected monotonic nonce for off-chain milestone proof submission
    MilestoneSubmitNonce(u64),
    /// Confidential grant amount commitment keyed by grant ID
    ConfidentialGrantCommitment(u64),
    /// Confidential grant recipient keyed by grant ID
    ConfidentialGrantRecipient(u64),
    /// Verifier key hash used for confidential claim proof checks
    ConfidentialGrantVerifierKeyHash(u64),
    /// Grant streaming metadata and configuration
    GrantStreamConfig(u64),
    /// Grant legal compliance data (hashes, signatures)
    GrantLegalData(u64),
    /// Grant validator information and rewards
    GrantValidatorData(u64),
    /// Grant performance metrics and KPIs
    GrantMetrics(u64),
    /// Grant dispute status and resolution data
    GrantDisputeData(u64),
    /// Double-approval request for high-value milestone payouts
    DoubleApprovalRequest(u64, u32),
    /// Double-approval configuration and thresholds
    DoubleApprovalConfig,
    
    // ── User Data ───────────────────────────────────────────────────────────────
    
    /// List of grant IDs associated with a specific recipient
    RecipientGrants(Address),
    /// User-specific balance and withdrawal data
    UserBalance(Address),
    /// User permissions and role assignments
    UserPermissions(Address),
    /// User voting power and governance data
    UserVotingPower(Address),
    /// User tax reporting and flow history
    UserTaxData(Address),
    /// User audit trail and compliance records
    UserAuditTrail(Address),
    
    // ── Treasury & Yield Operations ────────────────────────────────────────────
    
    /// Treasury configuration parameters
    TreasuryConfig,
    /// Current yield position and investment data
    YieldPosition,
    /// Yield farming metrics and performance data
    YieldMetrics,
    /// Reserve balance for treasury operations
    ReserveBalance,
    /// Yield token address for farming operations
    YieldToken,
    /// Yield strategy configuration and parameters
    YieldStrategy,
    /// Harvest schedule and automation data
    HarvestSchedule,
    /// Yield treasury generic config
    Config,
    /// Yield treasury generic metrics
    Metrics,
    
    // ── Governance ─────────────────────────────────────────────────────────────
    
    /// Governance proposal data keyed by proposal ID
    Proposal(u64),
    /// Vote storage keyed by (voter, proposal_id)
    Vote(VoteKey),
    /// Voter conviction and power data keyed by address
    VotePow(Address),
    /// Global list of proposal IDs
    PropIds,
    /// Governance token address
    GovTok,
    /// Voting threshold configuration
    VotingThreshold,
    /// Quorum requirements for proposals
    QuorumThreshold,
    /// Council membership list (stored as raw bytes for efficiency)
    CouncilMembers,
    /// Stake token for governance participation
    StakeToken,
    /// Required stake amount for proposals
    ProposalStakeAmount,
    /// Optimistic proposal limits
    OptimisticLimit,
    /// Challenge bond requirements
    ChallengeBond,
    /// Conviction calculation parameters (basis points)
    ConvictionAlpha,
    
    // ── Circuit Breakers & Safety ─────────────────────────────────────────────
    
    /// Last confirmed oracle price (scaled by SCALING_FACTOR)
    LastOraclePrice,
    /// Sanity-check oracle address for price verification
    SanityOracle,
    /// Oracle price freeze status
    OracleFrozen,
    /// Total liquidity snapshot for velocity calculations
    TvlSnapshot,
    /// Velocity monitoring window start timestamp
    VelocityWindowStart,
    /// Cumulative withdrawals in current velocity window
    VelocityAccumulator,
    /// Soft pause status due to velocity limit breach
    SoftPaused,
    /// Oracle last heartbeat timestamp
    OracleLastHeartbeat,
    /// Oracle freeze due to missing heartbeat
    OracleFrozenDueToNoHeartbeat,
    /// Manual exchange rate set by governance
    ManualExchangeRate,
    /// Dispute monitoring window start
    DisputeWindowStart,
    /// Dispute count in current window
    DisputeAccumulator,
    /// Active grants count at dispute window start
    ActiveGrantsSnapshot,
    /// Grant initialization halt status
    GrantInitializationHalted,
    /// Rent preservation mode status
    RentPreservationMode,
    /// Rent balance threshold for preservation
    RentBufferThreshold,
    
    // ── Audit & Reporting ───────────────────────────────────────────────────────
    
    /// Rolling transaction counter for audit trails
    AuditTxCounter,
    /// Current Merkle root for audit verification
    AuditMerkleRoot,
    /// Individual audit log entries
    AuditLogEntry(u64),
    /// Tax reporting flow history for users
    TaxFlowHistory(Address),
    /// Compliance monitoring data
    ComplianceData,
    /// Regulatory reporting snapshots
    RegulatoryReport(u64),
    /// Per-grant claim valuation counter
    ClaimValueCounter(u64),
    /// Ledger-linked fiat valuation for a specific claim
    ClaimValue(u64, u64),
    /// Default SEP-38 quote/fiat asset for grant token claims
    Sep38DefaultFiat,
    /// Latest SEP-38 rate keyed by (base token, quote/fiat asset)
    Sep38Rate(Address, String),
    
    // ── Multi-Token Operations ─────────────────────────────────────────────────
    
    /// Last oracle price recorded
    LastPric,
    /// Sanity-check oracle address
    SanityOra,
    /// Oracle freeze flag due to price deviation
    OraFrozen,
    /// TVL snapshot for velocity checks
    TvlSnap,
    /// Velocity window start timestamp
    VelWinSt,
    /// Velocity accumulator over the window
    VelAccum,
    /// Soft pause flag for velocity breaches
    SoftPa,
    /// Oracle heartbeat timestamp
    OraHeart,
    /// Oracle freeze flag due to heartbeat failure
    OraFrzHb,
    /// Manual exchange rate override
    ManRate,
    /// Dispute window start timestamp
    DispWin,
    /// Dispute count accumulator
    DispAcc,
    /// Active grants snapshot for dispute ratio
    ActGntSn,
    /// Grant initialization halt flag
    GntHalt,
    /// Rent preservation mode flag
    RentMode,
    /// Rent buffer threshold configuration
    RentThres,

    // ── Audit & Reporting ────────────────────────────────────────────────────────

    /// Audit transaction counter
    AudTxCnt,
    /// Audit merkle root for log verification
    AudRoot,
    /// Individual audit log entry keyed by index
    AudLog(u64),
    /// Tax flow history keyed by recipient address
    TaxHist(Address),
    /// Compliance metadata
    ComplDat,
    /// Regulatory report keyed by report ID
    RegRep(u64),

    // ── Multi-Token Operations ────────────────────────────────────────────────────

    /// Wrapped asset configuration keyed by token address
    WrapAst(Address),
    /// Multi-token bridge configuration
    BridgeConfig,
    /// Cross-chain transaction tracking
    CrossChainTx(u64),
    /// Token oracle price feeds
    TokenPriceFeed(Address),
    
    // ── Emergency & Recovery ─────────────────────────────────────────────────
    
    /// Registered emergency signers for multi-sig operations
    EmergencySigners,
    /// Emergency rescue proposals keyed by proposal ID
    RescueProposal(u64),
    /// Emergency execution logs
    EmergencyExecutionLog(u64),
    /// Circuit breaker trigger events
    CircuitBreakerTrigger(u64),
    
    // ── Reentrancy Protection ───────────────────────────────────────────────────
    
    /// Global reentrancy guard lock
    ReentrancyGuard,
    /// Function-specific reentrancy locks
    FunctionReentrancyLock(Bytes),
    /// Operation timeout tracking
    OperationTimeout(Bytes),
    
    // ── Matching Pool (Quadratic Funding) ────────────────────────────────────
    
    /// Matching pool vault data keyed by pool ID
    MatchingPool(u64),
    /// Donation record keyed by (pool_id, project_id, donor_address)
    Donation(u64, u64, Address),
    /// Project contributions aggregate keyed by (pool_id, project_id)
    ProjectContributions(u64, u64),
    /// All donors in a pool keyed by pool_id
    PoolDonors(u64),
    /// All projects in a pool keyed by pool_id
    PoolProjects(u64),
    /// SEP-12 identity verification status keyed by address
    Sep12Identity(Address),
    /// Donation matched amount keyed by (pool_id, project_id)
    ProjectMatched(u64, u64),
    /// Pool matching round metadata keyed by pool_id
    MatchingRound(u64),
    
    // ── Public Dashboard & Monitoring ──────────────────────────────────────────
    
    /// Last heartbeat timestamp for monitoring
    LastHeartbeat,
    /// Last TVL snapshot for dashboard
    LastTvl,
    /// Dashboard configuration parameters
    DashboardConfig,
    /// Health check metrics
    HealthMetrics,
    
    // ── Donor Reputation System ─────────────────────────────────────────────────────
    
    /// Donor reputation data keyed by donor address
    DonorReputation(Address),
    /// Project success metrics for reputation calculation keyed by project ID
    ProjectSuccessMetrics(u64),
    /// Donor's funded projects history keyed by donor address
    DonorFundedProjects(Address),
    /// Project milestones completion record keyed by (project_id, milestone_index)
    ProjectMilestoneRecord(u64, u32),
    /// Minimum funding threshold configuration for reputation accrual
    ReputationMinFundingThreshold,
    /// Reputation system configuration parameters
    ReputationConfig,
    /// Global reputation statistics and analytics
    ReputationStats,
    /// Reputation update history for audit trail
    ReputationUpdateHistory(u64),
    
    // ── Miscellaneous & Future Extensions ───────────────────────────────────────
    
    /// Contract version information
    ContractVersion,
    /// Feature flags for gradual rollouts
    FeatureFlag(Bytes),
    /// Temporary data (should be cleaned up)
    TemporaryData(Bytes),
    /// Migration status for contract upgrades
    MigrationStatus,
    /// Protocol-wide pause reason
    ProtocolPauseReason,
}

pub type StorageKey = Key;

impl Key {
    /// Returns the namespace category for this storage key
    /// Useful for debugging and storage analysis
    pub fn namespace(&self) -> &'static str {
        match self {
            // Core Contract State
            StorageKey::Admin
            | StorageKey::GrantToken
            | StorageKey::NativeToken
            | StorageKey::Treasury
            | StorageKey::Oracle
            | StorageKey::GrantIds
            | StorageKey::ContractInitialized => "core",
            
            // Grant Management
            StorageKey::Grant(_)
            | StorageKey::Tombstone(_)
            | StorageKey::Milestone(_, _)
            | StorageKey::MilestoneSubmitNonce(_)
            | StorageKey::ConfidentialGrantCommitment(_)
            | StorageKey::ConfidentialGrantRecipient(_)
            | StorageKey::ConfidentialGrantVerifierKeyHash(_)
            | StorageKey::GrantStreamConfig(_)
            | StorageKey::GrantLegalData(_)
            | StorageKey::GrantValidatorData(_)
            | StorageKey::GrantMetrics(_)
            | StorageKey::GrantDisputeData(_)
            | StorageKey::DoubleApprovalRequest(_, _)
            | StorageKey::DoubleApprovalConfig => "grant",
            
            // User Data
            StorageKey::RecipientGrants(_)
            | StorageKey::UserBalance(_)
            | StorageKey::UserPermissions(_)
            | StorageKey::UserVotingPower(_)
            | StorageKey::UserTaxData(_)
            | StorageKey::UserAuditTrail(_) => "user",
            
            // Treasury & Yield
            StorageKey::TreasuryConfig
            | StorageKey::YieldPosition
            | StorageKey::YieldMetrics
            | StorageKey::ReserveBalance
            | StorageKey::YieldToken
            | StorageKey::YieldStrategy
            | StorageKey::HarvestSchedule
            | StorageKey::Config
            | StorageKey::Metrics => "treasury",
            
            // Governance
            StorageKey::Proposal(_)
            | StorageKey::Vote(_, _)
            | StorageKey::VotingPower(_)
            | StorageKey::ProposalIds
            | StorageKey::GovernanceToken
            | StorageKey::VotingThreshold
            | StorageKey::QuorumThreshold
            | StorageKey::CouncilMembers
            | StorageKey::StakeToken
            | StorageKey::ProposalStakeAmount
            | StorageKey::OptimisticLimit
            | StorageKey::ChallengeBond
            | StorageKey::ConvictionAlpha => "governance",
            
            // Circuit Breakers
            StorageKey::LastOraclePrice
            | StorageKey::SanityOracle
            | StorageKey::OracleFrozen
            | StorageKey::TvlSnapshot
            | StorageKey::VelocityWindowStart
            | StorageKey::VelocityAccumulator
            | StorageKey::SoftPaused
            | StorageKey::OracleLastHeartbeat
            | StorageKey::OracleFrozenDueToNoHeartbeat
            | StorageKey::ManualExchangeRate
            | StorageKey::DisputeWindowStart
            | StorageKey::DisputeAccumulator
            | StorageKey::ActiveGrantsSnapshot
            | StorageKey::GrantInitializationHalted
            | StorageKey::RentPreservationMode
            | StorageKey::RentBufferThreshold => "circuit_breaker",
            
            // Audit & Reporting
            StorageKey::AuditTxCounter
            | StorageKey::AuditMerkleRoot
            | StorageKey::AuditLogEntry(_)
            | StorageKey::TaxFlowHistory(_)
            | StorageKey::ComplianceData
            | StorageKey::RegulatoryReport(_)
            | StorageKey::ClaimValueCounter(_)
            | StorageKey::ClaimValue(_, _)
            | StorageKey::Sep38DefaultFiat
            | StorageKey::Sep38Rate(_, _) => "audit",
            
            // Multi-Token
            StorageKey::WrappedAsset(_)
            | StorageKey::BridgeConfig
            | StorageKey::CrossChainTx(_)
            | StorageKey::TokenPriceFeed(_) => "multi_token",
            
            // Emergency & Recovery
            StorageKey::EmergencySigners
            | StorageKey::RescueProposal(_)
            | StorageKey::EmergencyExecutionLog(_)
            | StorageKey::CircuitBreakerTrigger(_) => "emergency",
            
            // Reentrancy Protection
            StorageKey::ReentrancyGuard
            | StorageKey::FunctionReentrancyLock(_)
            | StorageKey::OperationTimeout(_) => "security",
            
            // Matching Pool
            StorageKey::MatchingPool(_)
            | StorageKey::Donation(_, _, _)
            | StorageKey::ProjectContributions(_, _)
            | StorageKey::PoolDonors(_)
            | StorageKey::PoolProjects(_)
            | StorageKey::Sep12Identity(_)
            | StorageKey::ProjectMatched(_, _)
            | StorageKey::MatchingRound(_) => "matching_pool",
            
            // Dashboard & Monitoring
            StorageKey::LastHeartbeat
            | StorageKey::LastTvl
            | StorageKey::DashboardConfig
            | StorageKey::HealthMetrics => "monitoring",
            
            // Donor Reputation
            StorageKey::DonorReputation(_)
            | StorageKey::ProjectSuccessMetrics(_)
            | StorageKey::DonorFundedProjects(_)
            | StorageKey::ProjectMilestoneRecord(_, _)
            | StorageKey::ReputationMinFundingThreshold
            | StorageKey::ReputationConfig
            | StorageKey::ReputationStats
            | StorageKey::ReputationUpdateHistory(_) => "donor_reputation",
            
            // Miscellaneous
            StorageKey::ContractVersion
            | StorageKey::FeatureFlag(_)
            | StorageKey::TemporaryData(_)
            | StorageKey::MigrationStatus
            | StorageKey::ProtocolPauseReason => "misc",
        }
    }

    /// Returns a human-readable description of the storage key
    /// Useful for debugging and documentation
    pub fn description(&self) -> &'static str {
        match self {
            StorageKey::Admin => "Contract administrator address",
            StorageKey::GrantToken => "Primary token for grant operations",
            StorageKey::NativeToken => "Native token for fees and bounties",
            StorageKey::Treasury => "Treasury address for fund management",
            StorageKey::Oracle => "Oracle address for price feeds",
            StorageKey::GrantIds => "Global list of all grant IDs",
            StorageKey::ContractInitialized => "Contract initialization status",
            
            StorageKey::Grant(_) => "Individual grant data and metadata",
            StorageKey::Tombstone(_) => "Cryptographic proof of a pruned grant",
            StorageKey::Milestone(_, _) => "Grant milestone information",
            StorageKey::MilestoneSubmitNonce(_) => "Expected nonce for milestone proof submission",
            StorageKey::ConfidentialGrantCommitment(_) => "Commitment for confidential grant amount",
            StorageKey::ConfidentialGrantRecipient(_) => "Recipient authorized for confidential grant claims",
            StorageKey::ConfidentialGrantVerifierKeyHash(_) => "Verifier key hash for confidential claim proofs",
            StorageKey::GrantStreamConfig(_) => "Grant streaming configuration",
            StorageKey::GrantLegalData(_) => "Grant legal compliance data",
            StorageKey::GrantValidatorData(_) => "Grant validator rewards data",
            StorageKey::GrantMetrics(_) => "Grant performance metrics",
            StorageKey::GrantDisputeData(_) => "Grant dispute status",
            StorageKey::DoubleApprovalRequest(_, _) => "Double-approval request for milestone",
            StorageKey::DoubleApprovalConfig => "Double-approval configuration",
            
            StorageKey::RecipientGrants(_) => "Grants associated with recipient",
            StorageKey::UserBalance(_) => "User balance information",
            StorageKey::UserPermissions(_) => "User permissions and roles",
            StorageKey::UserVotingPower(_) => "User voting power allocation",
            StorageKey::UserTaxData(_) => "User tax reporting data",
            StorageKey::UserAuditTrail(_) => "User audit trail records",
            
            StorageKey::TreasuryConfig => "Treasury configuration parameters",
            StorageKey::YieldPosition => "Current yield farming position",
            StorageKey::YieldMetrics => "Yield farming performance metrics",
            StorageKey::ReserveBalance => "Treasury reserve balance",
            StorageKey::YieldToken => "Token used for yield farming",
            StorageKey::YieldStrategy => "Yield farming strategy config",
            StorageKey::HarvestSchedule => "Automated harvest schedule",
            StorageKey::Config => "Yield treasury generic configuration",
            StorageKey::Metrics => "Yield treasury generic metrics",
            
            StorageKey::Proposal(_) => "Governance proposal data",
            StorageKey::Vote(_, _) => "Individual vote records",
            StorageKey::VotingPower(_) => "Voting power allocation",
            StorageKey::ProposalIds => "List of all proposal IDs",
            StorageKey::GovernanceToken => "Token used for governance",
            StorageKey::VotingThreshold => "Voting threshold configuration",
            StorageKey::QuorumThreshold => "Quorum requirements",
            StorageKey::CouncilMembers => "Council membership list",
            StorageKey::StakeToken => "Token for governance staking",
            StorageKey::ProposalStakeAmount => "Required stake for proposals",
            StorageKey::OptimisticLimit => "Optimistic proposal limits",
            StorageKey::ChallengeBond => "Challenge bond requirements",
            StorageKey::ConvictionAlpha => "Conviction calculation parameters",
            
            StorageKey::LastOraclePrice => "Last confirmed oracle price",
            StorageKey::SanityOracle => "Sanity-check oracle address",
            StorageKey::OracleFrozen => "Oracle price freeze status",
            StorageKey::TvlSnapshot => "Total liquidity snapshot",
            StorageKey::VelocityWindowStart => "Velocity monitoring start",
            StorageKey::VelocityAccumulator => "Cumulative withdrawals",
            StorageKey::SoftPaused => "Soft pause due to velocity",
            StorageKey::OracleLastHeartbeat => "Oracle last heartbeat",
            StorageKey::OracleFrozenDueToNoHeartbeat => "Oracle freeze (no heartbeat)",
            StorageKey::ManualExchangeRate => "Manual exchange rate",
            StorageKey::DisputeWindowStart => "Dispute monitoring start",
            StorageKey::DisputeAccumulator => "Dispute count in window",
            StorageKey::ActiveGrantsSnapshot => "Active grants count",
            StorageKey::GrantInitializationHalted => "Grant init halt status",
            StorageKey::RentPreservationMode => "Rent preservation mode",
            StorageKey::RentBufferThreshold => "Rent buffer threshold",
            
            StorageKey::AuditTxCounter => "Audit transaction counter",
            StorageKey::AuditMerkleRoot => "Current audit Merkle root",
            StorageKey::AuditLogEntry(_) => "Individual audit log entry",
            StorageKey::TaxFlowHistory(_) => "User tax flow history",
            StorageKey::ComplianceData => "Compliance monitoring data",
            StorageKey::RegulatoryReport(_) => "Regulatory report snapshot",
            StorageKey::ClaimValueCounter(_) => "Per-grant claim valuation counter",
            StorageKey::ClaimValue(_, _) => "Ledger-linked claim fiat valuation",
            StorageKey::Sep38DefaultFiat => "Default SEP-38 fiat quote asset",
            StorageKey::Sep38Rate(_, _) => "SEP-38 grant-token fiat rate",
            
            StorageKey::WrappedAsset(_) => "Wrapped asset data",
            StorageKey::BridgeConfig => "Multi-token bridge config",
            StorageKey::CrossChainTx(_) => "Cross-chain transaction",
            StorageKey::TokenPriceFeed(_) => "Token price feed data",
            
            StorageKey::EmergencySigners => "Emergency signer set",
            StorageKey::RescueProposal(_) => "Emergency rescue proposal",
            StorageKey::EmergencyExecutionLog(_) => "Emergency execution log",
            StorageKey::CircuitBreakerTrigger(_) => "Circuit breaker trigger",
            
            StorageKey::ReentrancyGuard => "Global reentrancy guard",
            StorageKey::FunctionReentrancyLock(_) => "Function-specific lock",
            StorageKey::OperationTimeout(_) => "Operation timeout tracking",
            
            StorageKey::MatchingPool(_) => "Matching pool vault data",
            StorageKey::Donation(_, _, _) => "Individual donation record",
            StorageKey::ProjectContributions(_, _) => "Project contribution aggregate",
            StorageKey::PoolDonors(_) => "All donors in matching pool",
            StorageKey::PoolProjects(_) => "All projects in matching pool",
            StorageKey::Sep12Identity(_) => "SEP-12 identity verification status",
            StorageKey::ProjectMatched(_, _) => "Project matched amount",
            StorageKey::MatchingRound(_) => "Matching round metadata",
            
            StorageKey::LastHeartbeat => "Last monitoring heartbeat",
            StorageKey::LastTvl => "Last TVL snapshot",
            StorageKey::DashboardConfig => "Dashboard configuration",
            StorageKey::HealthMetrics => "Health check metrics",
            
            StorageKey::DonorReputation(_) => "Donor reputation score and metrics",
            StorageKey::ProjectSuccessMetrics(_) => "Project success metrics for reputation calculation",
            StorageKey::DonorFundedProjects(_) => "History of projects funded by donor",
            StorageKey::ProjectMilestoneRecord(_, _) => "Milestone completion record for projects",
            StorageKey::ReputationMinFundingThreshold => "Minimum funding threshold for reputation accrual",
            StorageKey::ReputationConfig => "Reputation system configuration parameters",
            StorageKey::ReputationStats => "Global reputation statistics and analytics",
            StorageKey::ReputationUpdateHistory(_) => "Reputation update history for audit trail",
            
            StorageKey::ContractVersion => "Contract version info",
            StorageKey::FeatureFlag(_) => "Feature flag configuration",
            StorageKey::TemporaryData(_) => "Temporary storage data",
            StorageKey::MigrationStatus => "Contract migration status",
            StorageKey::ProtocolPauseReason => "Protocol-wide emergency pause reason",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn address() -> Address {
        Address::generate(&soroban_sdk::Env::default())
    }

    #[test]
    fn test_storage_key_namespace() {
        assert_eq!(StorageKey::Admin.namespace(), "core");
        assert_eq!(StorageKey::Grant(123).namespace(), "grant");
        assert_eq!(StorageKey::RecipientGrants(address()).namespace(), "user");
        assert_eq!(StorageKey::TreasuryConfig.namespace(), "treasury");
        assert_eq!(StorageKey::Proposal(456).namespace(), "governance");
        assert_eq!(StorageKey::OracleFrozen.namespace(), "circuit_breaker");
        assert_eq!(StorageKey::AuditTxCounter.namespace(), "audit");
        assert_eq!(StorageKey::WrappedAsset(address()).namespace(), "multi_token");
        assert_eq!(StorageKey::EmergencySigners.namespace(), "emergency");
        assert_eq!(StorageKey::ReentrancyGuard.namespace(), "security");
        assert_eq!(StorageKey::LastHeartbeat.namespace(), "monitoring");
        assert_eq!(StorageKey::ContractVersion.namespace(), "misc");
    }

    #[test]
    fn test_storage_key_description() {
        assert!(!StorageKey::Admin.description().is_empty());
        assert!(!StorageKey::Grant(123).description().is_empty());
        assert!(!StorageKey::RecipientGrants(address()).description().is_empty());
    }
}
