## Summary

Implements storage-budget-aware provenance resolution for long cross-contract chains such as grant_contracts -> compliance -> admin -> treasury. The resolver now avoids repeated per-hop storage reads by precomputing hop keys, prefetching compact hop state once, validating from an in-memory cache, and writing only the final resolution result.

## Changes

- Add `STORAGE_BUDGET = 120`, `STORAGE_WARN_THRESHOLD = 100`, and `MAX_HOPS = 10`.
- Add `StorageBudget` read/write accounting with fail-fast `StorageBudgetExceeded` checks before storage operations.
- Add compressed `HopState` to combine metadata, audit, credential, signature, policy reference, timestamp, and score into one storage entry per hop.
- Add `ProvenanceAccessSet` and single-pass `prefetch_hop_states()` cache loading.
- Update `resolve_provenance()` to use N hop reads plus one final result write, so a 10-hop chain resolves with 11 counted accesses.
- Keep signature and credential validation CPU-only after prefetch, with no hidden `env.storage()` reads in the verifier.
- Add tests for 10-hop resolution, access-count bounds, persisted results, budget warning accounting, and error paths.

## Storage Model

| Chain length | Counted accesses | Budget remaining |
| --- | ---: | ---: |
| 1 hop | 2 | 118 |
| 6 hops | 7 | 113 |
| 10 hops | 11 | 109 |

## Testing

Not run locally: `cargo` is not installed or not available on PATH in this environment.
