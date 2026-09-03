# ZK-Shielded Pool Solana Program

This Anchor v2 program implements the deposit side of a shielded SOL pool. A deposit transfers lamports into a program-owned vault and inserts a Poseidon commitment into an incremental Merkle tree. The current program does not implement withdrawals.

## Major Concepts

- The **vault PDA**, derived from `b"vault"`, holds deposited SOL.
- The **root registry PDA**, derived from `b"root_registry"`, stores a depth-20 incremental Merkle tree and a 100-entry ring buffer of recent roots.
- The **proof storage PDA**, derived from `b"proof_storage"` plus the sender address and a `proof_hash`, is a fixed 1500-byte buffer with a `proof_len` field. It holds raw GWC proof bytes (the checked-in `proof.bin` is 1088 bytes). `upload_proof` creates it with `init_if_needed` on first use. `part == 0` writes the chunk at offset 0; any other `part` appends at the current `proof_len`. Empty chunks and writes that would exceed 1500 bytes are rejected. A 1088-byte proof does not fit in one instruction (about 995 bytes leftover after headers), so the client splits it across two `upload_proof` calls.
- The `initialize` instruction handler creates the vault and root registry PDAs and initializes the empty tree.
- The `deposit` instruction handler accepts a BN254 scalar-field commitment and a nonzero lamport amount. It computes `Poseidon(user_commitment_hash, total_amount)`, inserts the result into the tree, transfers the lamports from the sender to the vault, and emits `DepositDone`.

## Setup

Install Rust, the Solana platform tools, and the Anchor v2 CLI used by this repository (`anchor2` alias).

## Usage

Build the program and generate its IDL:

```sh
anchor2 build
```

Use `initialize` once before sending `deposit`. Deposit amounts are expressed in lamports, and `user_commitment_hash` must be a little-endian BN254 scalar-field element.

## Testing

LiteSVM tests live under `programs/zk-shielded-pool-solana/tests/`. Build first so `target/deploy/zk_shielded_pool_solana.so` exists, then run:

```sh
anchor2 build
anchor2 test
```

Include program logs in the test output when debugging:

```sh
anchor2 test -- --show-output
```
