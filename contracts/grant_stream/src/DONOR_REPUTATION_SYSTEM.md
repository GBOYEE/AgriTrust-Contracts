# Donor Reputation System

## Overview

The Donor Reputation System implements a sophisticated mechanism to track and reward donors who demonstrate high-quality due diligence in their funding decisions. The system measures the "Success Rate" of projects funded by each donor and adjusts their influence in future "Matching Round" votes accordingly.

## Key Features

### 1. Success Rate Tracking
- **Metric**: Percentage of milestones successfully completed across all funded projects
- **Calculation**: `(Successful Projects / Qualifying Projects) * 100%`
- **Scale**: Measured in basis points (10,000 = 100%)

### 2. Reputation-Based Influence Scaling
- **Linear Scaling**: 1x influence at 0% reputation, up to configurable maximum at 100%
- **Default Range**: 1x to 3x influence multiplier
- **Formula**: `Influence = 1x + (Success_Rate * (Max_Influence - 1x))`

### 3. Anti-Farming Protection
- **Minimum Funding Threshold**: Configurable minimum amount for reputation accrual
- **Default Threshold**: 100 USDC (in 7-decimal units)
- **Influence Capping**: Maximum multiplier prevents excessive advantage

### 4. Event Emissions
- **ReputationUpdated**: Emitted when donor reputation changes
- **ProjectFunded**: Emitted when new project funding is recorded
- **MilestoneCompleted**: Emitted when project milestone is completed

## Architecture

### Data Structures

#### DonorReputation
```rust
pub struct DonorReputation {
    pub donor: Address,
    pub reputation_score: i128,        // Scaled by REPUTATION_SCALE
    pub success_rate: i128,             // In basis points
    pub total_funded: i128,
    pub qualifying_projects: u32,
    pub successful_projects: u32,
    pub last_updated: u64,
    pub influence_multiplier: i128,    // Scaled by REPUTATION_SCALE
}
```

#### ProjectSuccessMetrics
```rust
pub struct ProjectSuccessMetrics {
    pub project_id: u64,
    pub donor: Address,
    pub total_milestones: u32,
    pub completed_milestones: u32,
    pub project_status: GrantStatus,
    pub funded_amount: i128,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}
```

### Storage Layout

| Storage Key | Purpose | Namespace |
|-------------|---------|-----------|
| `DonorReputation(address)` | Individual donor reputation data | `donor_reputation` |
| `ProjectSuccessMetrics(project_id)` | Project success tracking | `donor_reputation` |
| `DonorFundedProjects(address)` | Donor's project history | `donor_reputation` |
| `ProjectMilestoneRecord(project_id, milestone_index)` | Milestone completion records | `donor_reputation` |
| `ReputationConfig` | System configuration | `donor_reputation` |
| `ReputationUpdateHistory(update_id)` | Audit trail | `donor_reputation` |

## Integration Points

### 1. Grant Stream Integration
The reputation system integrates with the grant stream contract to track:
- Project funding events
- Milestone completion events
- Project failure/cancellation events

### 2. Matching Pool Integration
The matching pool uses reputation scores to:
- Calculate influence multipliers for donations
- Apply reputation-weighted contributions to quadratic funding
- Emit enhanced donation events with influence data

### 3. Event System
Events are emitted for:
- Reputation score changes
- Project funding and completion
- Milestone achievements

## Security Measures

### 1. Reputation Farming Prevention

#### Financial Barriers
- **Minimum Funding Threshold**: Only projects above minimum funding count toward reputation
- **Configurable Threshold**: Can be adjusted by admin to increase/decrease barriers
- **Influence Capping**: Maximum multiplier prevents unlimited advantage

#### Mathematical Fairness
- **Linear Scaling**: Prevents exponential growth of influence
- **Basis Point Precision**: Ensures accurate calculations
- **Overflow Protection**: All math operations include overflow checks

### 2. Anti-Manipulation Measures

#### Duplicate Prevention
- **Milestone Deduplication**: Each milestone can only be recorded once
- **Project Uniqueness**: Each project ID can only be funded once per donor
- **Update Auditing**: All reputation changes are logged

#### Access Control
- **Admin-Only Configuration**: Only authorized addresses can update system parameters
- **Donor Authentication**: Donors must authenticate funding and milestone operations
- **Read-Only Queries**: Reputation data is publicly readable but only writable through authorized operations

## Usage Examples

### 1. Basic Reputation Building

```rust
// Initialize reputation system
DonorReputationContract::initialize(env, admin)?;

// Fund a qualifying project
DonorReputationContract::record_project_funded(
    env,
    donor,
    project_id,
    funding_amount, // Must meet minimum threshold
    total_milestones,
)?;

// Complete milestones
for milestone in 0..total_milestones {
    DonorReputationContract::record_milestone_completed(
        env,
        project_id,
        milestone,
        Some(evidence_hash),
    )?;
}

// Check reputation
let reputation = DonorReputationContract::get_donor_reputation(env, donor)?;
println!("Success Rate: {}%", reputation.success_rate / 100);
```

### 2. Matching Pool with Reputation

```rust
// Donate to matching pool (reputation automatically applied)
MatchingPoolContract::donate(
    env,
    pool_id,
    project_id,
    donor,
    donation_amount,
)?;

// Calculate matching (reputation influences quadratic funding)
let projects = vec![project_id];
let total_matched = MatchingPoolContract::calculate_matching(env, pool_id, projects)?;

// Get influence multiplier
let influence = DonorReputationContract::calculate_influence(env, donor)?;
println!("Influence Multiplier: {}x", influence as f64 / REPUTATION_SCALE as f64);
```

## Configuration

### Default Parameters
- **Minimum Funding Threshold**: 100 USDC (1,000,000,000 in 7-decimal units)
- **Maximum Influence Multiplier**: 3x (3 * REPUTATION_SCALE)
- **Calculation Window**: 90 days (7,776,000 seconds)
- **Recency Weight**: 50% (5,000 basis points)

### Configuration Updates
```rust
DonorReputationContract::update_config(
    env,
    admin,
    Some(new_threshold),      // Optional new minimum funding
    Some(new_max_multiplier), // Optional new max influence
    Some(new_window),         // Optional new time window
    Some(new_recency_weight), // Optional new recency weight
)?;
```

## Testing

### Test Coverage
The system includes comprehensive tests covering:
- **Linear Influence Scaling**: Verifies mathematical fairness across donor tiers
- **Reputation Farming Resistance**: Tests anti-manipulation measures
- **Integration Testing**: Validates matching pool integration
- **Edge Cases**: Handles error conditions and boundary values
- **Security Testing**: Verifies access controls and data integrity

### Test Categories
1. **Unit Tests** (`test_donor_reputation.rs`): Core functionality
2. **Integration Tests** (`test_reputation_matching_integration.rs`): End-to-end scenarios
3. **Security Tests**: Anti-farming and manipulation prevention

## Economic Impact

### 1. Incentive Alignment
- **High-Quality Due Diligence**: Donors with good track records get more influence
- **Capital Efficiency**: Matching funds flow to more successful projects
- **Self-Optimization**: System naturally steers capital toward successful teams

### 2. Network Effects
- **Reputation Building**: Early successful donors accumulate influence over time
- **Quality Signaling**: High reputation becomes a signal of reliability
- **Market Efficiency**: Better information leads to better capital allocation

### 3. Risk Mitigation
- **Diversified Risk**: Multiple reputable donors reduce single-point failures
- **Track Record Validation**: Historical performance predicts future success
- **Adaptive Learning**: System learns from collective funding decisions

## Future Enhancements

### Potential Improvements
1. **Time-Based Weighting**: Recent projects could count more heavily
2. **Project Category Specialization**: Donors could build reputation in specific domains
3. **Collaborative Reputation**: Multi-donor projects with shared reputation credit
4. **Dynamic Thresholds**: Adaptive minimum funding based on market conditions
5. **Reputation Decay**: Gradual reputation reduction for inactivity

### Extensibility
The system is designed to be:
- **Modular**: Easy to add new reputation factors
- **Configurable**: Parameters can be adjusted without contract upgrades
- **Composable**: Can integrate with other governance and funding mechanisms

## Conclusion

The Donor Reputation System creates a powerful incentive mechanism that rewards high-quality curation and due diligence while protecting against manipulation. By linking reputation to measurable project success and applying it through matching rounds, the system creates a self-optimizing funding ecosystem that naturally steers capital toward the most capable teams.

The combination of mathematical fairness, security measures, and economic incentives makes this system a robust foundation for decentralized grant allocation and community-driven funding decisions.
