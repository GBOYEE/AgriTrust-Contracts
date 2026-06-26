# Temporal Invariant Fuzz Test

## Overview

This fuzz test focuses on the `calculate_flow` logic (implemented in `calculate_accrued` function) and verifies temporal invariants for the grant streaming contract.

## Test Coverage

### 1. Random Time Jump Testing (`test_temporal_invariant_random_time_jumps`)
- **Time Range**: 1 second to 10 years (315,360,000 seconds)
- **Scenarios**: 10-50 random time jumps per test
- **Features**: 
  - Random withdrawals after each time jump
  - Configurable warmup periods
  - Optional validator rewards (5% split)
  - Multiple grant configurations

### 2. Boundary Testing (`test_stream_start_end_boundaries`)
- **Start Boundary**: Tests behavior exactly at and before stream start time
- **End Boundary**: Tests behavior around stream completion
- **Offset Range**: ±1000 seconds from boundary points
- **Verification**: No tokens should be available before start time

### 3. Maximum Duration Stress Test (`test_maximum_duration_temporal_invariant`)
- **Duration**: Full 10-year testing period
- **Purpose**: Verify no overflow or precision loss over maximum duration
- **Validation**: Total withdrawn never exceeds total allocation

### 4. Mathematical Precision Test (`test_long_term_mathematical_precision`)
- **Large Values**: Tests with near-maximum i128 values
- **High Flow Rates**: Stress tests with large flow rates
- **Multiple Checkpoints**: Verifies consistency at various time points

### 5. Loan Grace Period Temporal Testing (`test_grace_period_temporal_fuzz`)
- **Oracle Model**: `GracePeriodOracle` converts `GRACE_PERIOD_SECONDS` into ledgers using the configured `expected_ledger_secs`.
- **Stored Deadline**: `check_default()` stores `default_ledger`, `grace_deadline`, and slippage once, so later oracle changes do not stretch or shrink an already-open grace period.
- **Boundary Checks**: Catch-up at `grace_deadline - 1` and `grace_deadline` is accepted; catch-up at `grace_deadline + 1` is rejected.
- **Congestion Case**: With `expected_ledger_secs = 60`, the 30-day grace period is 43,200 ledgers.
- **Fuzz Model**: Property tests generate balanced 5-120 second ledger-close sequences and verify the simulated real-time grace window remains within 1% of 30 days.

## Key Invariants Verified

1. **Total Allocation Invariant**: `withdrawn + claimable ≤ total_amount` for each grant
2. **Global Token Invariant**: Total tokens in system never exceed initial allocation
3. **Temporal Boundary Invariant**: No tokens available before stream start time
4. **Flow Calculation Invariant**: Actual flow never exceeds expected maximum flow
5. **Mathematical Precision Invariant**: No negative values or overflow conditions
6. **Grace Deadline Invariant**: Grace checks use the stored `grace_deadline`, not a fresh computation from mutable oracle configuration
7. **Grace Real-Time Invariant**: Under the configured expected ledger close duration, the grace period represents `GRACE_PERIOD_SECONDS` within fuzz tolerance

## Grant Configurations Tested

- Standard grants (no warmup, no validator)
- Warmup grants (7-30 day warmup periods)
- Validator grants (5% ecosystem tax)
- Combined features (warmup + validator)
- Micro-streams (very small flow rates)

## Running the Tests

```bash
# Run all temporal fuzz tests
cargo test test_temporal_fuzz --lib

# Run specific test cases
cargo test test_temporal_invariant_random_time_jumps --lib
cargo test test_stream_start_end_boundaries --lib
cargo test test_maximum_duration_temporal_invariant --lib
cargo test test_long_term_mathematical_precision --lib

# Run with more test cases (slower but more thorough)
cargo test test_temporal_fuzz --lib -- --test-threads=1

# Run grace-period temporal coverage
cargo test test_grace_period_temporal_fuzz --lib
```

## Test Parameters

- **Proptest Cases**: 100 cases per fuzz test (configurable)
- **Time Jump Range**: 1 second to 10 years
- **Grant Duration**: 1 day to 1 year
- **Flow Rates**: 1 to 1000 tokens/second (scaled)
- **Withdrawal Probability**: 0.0 to 1.0 (random)

## Failure Scenarios Detected

The test will fail and report detailed errors if:

1. **Extra Token Minting**: Total withdrawn exceeds total allocation
2. **Temporal Violations**: Tokens available before start time
3. **Mathematical Errors**: Overflow, underflow, or precision loss
4. **State Corruption**: Inconsistent grant state
5. **Global Invariant Violation**: Token creation/destruction bugs
6. **Grace Drift**: Configured ledger duration produces a grace period outside the real-time tolerance
7. **Boundary Regression**: Catch-up is rejected before the stored deadline or accepted after it

## Integration with Issue #298

This fuzz test directly addresses the requirements in issue #298:

- ✅ **Temporal Invariant Focus**: Specifically targets `calculate_flow` logic
- ✅ **Random Time Jumps**: Simulates 1 second to 10 year jumps
- ✅ **Boundary Testing**: Focuses on Start and End boundaries
- ✅ **Allocation Verification**: Ensures withdrawn amount never exceeds total_allocation
- ✅ **Final Ledger Protection**: Prevents extra tokens during final grant ledger

## Grace Period Drift Compensation

The grace period is specified in wall-clock seconds (`GRACE_PERIOD_SECONDS = 30 days`) but enforced on-chain by ledger sequence. To avoid hardcoding a stale ledger count, the contract stores a `GracePeriodOracle`:

```rust
grace_period_ledgers = ceil(GRACE_PERIOD_SECONDS / expected_ledger_secs)
```

The default expected close duration is 5 seconds. Operators can configure this value for congestion scenarios before default processing. When `check_default()` opens a grace window it stores:

- `default_ledger = env.ledger().sequence()`
- `grace_deadline = default_ledger + grace_period_ledgers`
- `slippage_ledgers = oracle.slippage_ledgers`

All subsequent grace checks use this stored deadline and tolerance. This avoids a desync where changing ledger-duration assumptions after default would silently extend or shorten the catch-up window.

`GRACE_PERIOD_SLIPPAGE_LEDGERS` allows the exact deadline ledger to pass, but catch-up after `grace_deadline + 1` is rejected.

## Performance Considerations

- Tests use `proptest` for efficient property-based testing
- Time complexity is O(n) where n is number of time jumps
- Memory usage is bounded by number of grants (typically < 10)
- Each test case is independent for parallel execution

## Extending the Tests

To add new test scenarios:

1. Add new `TemporalGrantConfig` variants in `generate_grant_configs()`
2. Extend `verify_temporal_invariant()` with new checks
3. Add new proptest properties for specific edge cases
4. Update constants in the test file for different time ranges
