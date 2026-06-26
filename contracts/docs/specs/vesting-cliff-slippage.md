# Vesting Contract Cliff Violation via Slippage in Stream Capital Conversion

## Issue #17

### Problem
When capital conversion incurs delay, the actual first distribution may occur after the intended cliff date. The original cliff check only verified at schedule creation time.

### Solution: Deferred Cliff Mechanism
1. Effective cliff shifts forward when conversion completes after intended cliff
2. Adjusted total: `total * (VESTING_PERIOD - delay_past_cliff) / VESTING_PERIOD`
3. Slippage threshold: panics when bps > 500 (5%)
4. Invalidation: delay past full vesting window = refund to grantor

### Constants
- CLIFF_PERIOD: 180 days (1,555,200 ledgers)
- VESTING_PERIOD: 365 days (3,153,600 ledgers)
- CONVERSION_DELAY_MAX: 14 days (120,960 ledgers)
- MAX_SLIPPAGE_BPS: 500 bps (5%)

### Invariants
- adjusted_total <= original_total (never increases)
- effective_cliff_start >= start_time + CLIFF_PERIOD (never releases earlier)
- Slippage event emitted when threshold exceeded
