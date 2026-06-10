# AgriTrust-Contracts

Smart contracts for managing trust streams with milestone completion proof hashing and integrated dispute resolution system on Stellar (Soroban WASM) and Ethereum/L2s (Solidity).

## 🚀 Key Features
* **Per-Second Streaming Accrual:** High-precision streaming logic using scaling factors on Soroban.
* **Legal Anchoring & Escrow:** Restricts fund streaming until legal documents are cryptographically signed on-chain, alongside an integrated arbitration escrow.
* **Multi-Chain Smart Contracts:** Soroban-based smart contract implementation alongside a Foundry/Solidity implementation supporting ZK proof verification.

## 🛠️ Tech Stack
* **Language/Framework:** Rust / Soroban WASM, Solidity / Foundry
* **Key Dependencies:** `soroban-sdk`, `foundry-rs`

## 📦 Getting Started

### Prerequisites
Ensure you have the required toolchains installed:
* Rust toolchain (cargo, rustc)
* Stellar CLI / Soroban CLI
* Foundry (forge)

### Installation & Local Setup
```bash
# Clone the repository (if running manually)
git clone https://github.com/AgriTrust-Protocol/AgriTrust-Contracts

# Build Soroban contracts
stellar contract build

# Run cargo tests
cargo test

# Build Solidity contracts
forge build

# Run foundry tests
forge test
```

## 🤝 Contributing
Contributions are highly welcome. Please ensure your commits are cryptographically signed using GPG or SSH keys. For major structural changes, please open an issue first to discuss your proposal.