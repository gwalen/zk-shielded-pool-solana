# ZK-Shielded Pool Solana Program

This Quasar program implements the deposit side of a shielded SOL pool. A deposit transfers lamports into a program-owned vault and inserts a Poseidon commitment into an incremental Merkle tree. The current program does not implement withdrawals.

## Major Concepts

- The **vault PDA**, derived from `b"vault"`, holds deposited SOL.
- The **root registry PDA**, derived from `b"root_registry"`, stores a depth-20 incremental Merkle tree and a 100-entry ring buffer of recent roots.
- The **proof storage PDA**, derived from `b"proof_storage"` plus the sender address and a `proof_hash`, is a fixed 1500-byte buffer with a `proof_len` field. `upload_proof` creates it with `init(idempotent)` on first use. `part == 0` writes the chunk at offset 0; any other `part` appends at the current `proof_len`. Empty chunks and writes that would exceed 1500 bytes are rejected. Each instruction carries at most 900 proof bytes so a 1264-byte fixture is two uploads.
- The `initialize` instruction handler creates the vault and root registry PDAs and initializes the empty tree.
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
