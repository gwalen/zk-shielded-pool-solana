# ZK-Shielded Pool Solana Program

This Quasar program implements the deposit side of a shielded SOL pool. A deposit transfers lamports into a program-owned vault and inserts a Poseidon commitment into an incremental Merkle tree. The current program does not implement withdrawals.

## Major Concepts

- The **vault PDA**, derived from `b"vault"`, holds deposited SOL.
- The **root registry PDA**, derived from `b"root_registry"`, stores a depth-20 incremental Merkle tree and a 100-entry ring buffer of recent roots.
- The `initialize` instruction handler creates both PDAs and initializes the empty tree.
- The `deposit` instruction handler accepts a BN254 scalar-field commitment and a nonzero lamport amount. It computes `Poseidon(user_commitment_hash, total_amount)`, inserts the result into the tree, transfers the lamports from the sender to the vault, and emits `DepositDone`.

## Setup

Install Rust, the Solana platform tools, and the Quasar CLI revision used by this repository:

```sh
cargo install --git https://github.com/blueshift-gg/quasar --rev 0361701 quasar-cli --locked
```

## Usage

Build the program and generate its IDL and Rust client:

```sh
quasar build
```

Use the generated `InitializeInstruction` once before sending `DepositInstruction`. Deposit amounts are expressed in lamports, and `user_commitment_hash` must be a little-endian BN254 scalar-field element.

## Testing

Run the program build and full Rust test suite:

```sh
quasar test
```

Include program logs in the test output when debugging:

```sh
quasar test --show-output
```
