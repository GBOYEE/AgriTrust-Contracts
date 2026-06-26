# ZK-KYC Nullifier Jurisdiction Binding

## Issue #16 — ZK-KYC Nullifier Replay Across Distinct Compliance Jurisdictions

### Problem
The original nullifier derivation used only the identity commitment, allowing the same ZK proof to be replayed across different compliance jurisdictions (e.g., EU and US domains).

### Solution
Nullifiers are now derived as `SHA-256(identity_commitment || domain_id)`, binding each proof to a specific compliance domain. This ensures:

1. **Cross-jurisdiction isolation**: The same identity produces different nullifiers in different domains
2. **Replay protection within a domain**: Each nullifier can only be consumed once per domain
3. **Domain registration enforcement**: Only registered compliance domains can submit proofs

### Storage Keys
- `NullifierUsed(domain_id, nullifier_hash) → bool` — composite key scoping nullifiers per jurisdiction
- `RegisteredDomain(domain_id) → bool` — tracks authorized compliance domains

### Invariants
- `∀ identity, domain_a, domain_b: domain_a ≠ domain_b → nullifier(identity, domain_a) ≠ nullifier(identity, domain_b)`
- `∀ nullifier, domain: is_nullifier_used(domain, nullifier) → reject_replay(domain, nullifier)`

### Events
- `NullifierConsumed { domain_id, nullifier_hash }` — emitted when a nullifier is marked as used
