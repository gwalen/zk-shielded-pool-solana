# Handoff prompt: Quasar → Anchor v2 (paste into a new agent)

Copy everything below the line into a new chat.

---

You are continuing a rewrite of a Solana shielded-pool program from **Quasar** to **Anchor v2**.

## Goal

Make `pool-anchor-v2-rewrite` compile and then pass tests as an Anchor v2 program. Match the working stub at:

`/Users/whale/development/workshop/crypto/solana/anchor-v2-counter`

The original Quasar program (reference only, do not edit it):

`/Users/whale/development/workshop/rust/zk-shielded-pool/zk-shielded-pool-solana`

Work in:

`/Users/whale/development/workshop/rust/zk-shielded-pool/pool-anchor-v2-rewrite`

## How to work

Go **step by step**. After each fix: say what you changed, stop, wait for me to say go on the next item. Do not batch the rest of the migration.

- For Anchor CLI: use the `anchor2` alias (`anchor2 build`, `anchor2 test`). Do not run `anchor2 build` unless I ask.
- Do not change the program ID (`declare_id!` / `Anchor.toml`) unless I ask. Current ID: `FCrymxYUTEnXDJXdDn2E71KPxB9sBjXZCh1ezBwmUhvp` (matches `target/deploy/zk_shielded_pool_solana-keypair.json`).
- Read `AGENTS.md`: simple words, explain clearly, do not be a smartass.
- Read `docs/anchorV2-quasar-zero-copy-byte-layout.md` before touching account field types.
- Follow the solana-quicknode skill in the original repo if you edit program Rust. Call `program_autofixer` on Solana program edits.

## What is already done

Workspace compiles far enough to hit Rust errors (Cargo.toml workspace edition / rust-version, no nested `[workspace]`, `unexpected_cfgs` lint, program ID synced).

Quasar crate is gone from compiled code (`quasar_lang`, `quasar_enum!`, `macros.rs` deleted).

Logging uses `msg!` (not Quasar `log`). Hello logs `crate::ID`.

**No `ImtTreeZc`.** Tree methods live on `ImtTree`. Integer fields that sit next to a `u8` bump (or that would give the struct alignment 4 and trailing pad) use Anchor's `PodU32` / `PodU16` from prelude. Read with `.get()`, write with `PodU32::from` / `PodU16::from`. Do not add `_pad` arrays for those. We tried `_pad` and rejected it.

`ImtTree::new` and several handlers already return `Result<T>` (one type arg). Some functions still use `Result<T, ProgramError>` (two args). That is `E0107` because `prelude::*` aliases `Result<T> = Result<T, Error>`.

## What is left (do in this order unless I say otherwise)

1. **Finish `Result<T>` everywhere** (`E0107`). Drop `, ProgramError`. Still two-arg in: `lib.rs` handlers, `hello.rs`, `deposit.rs`, `upload_proof.rs` (handle), `poseidon_hash.rs`. `initialize.rs` handle is already `Result<()>`. `imt_tree.rs` insert is already `Result<[u8; 32]>`.

2. **Account constraint structs for Anchor v2** (`Bumps` / `TryAccounts` missing on `Initialize` and `UploadProof`; `Deposit` and `HelloAccountConstraints` already implement them). Stub pattern:

   - `Program<System>` not `Program<SystemProgram>`
   - `init, payer = ...` not Quasar `init(idempotent)` + `address = Foo::seeds()`
   - `Account<T>` for program accounts (Deposit currently uses raw `Vault` / `RootRegistry`)

   `RootRegistry`'s `#[account]` / `#[seeds]` are commented out. Uncomment/fix when you wire `Account<RootRegistry>`. Compare `Vault` (`#[account(discriminator = 1, set_inner)]`, `#[seeds(b"vault")]`) and the stub `Counter`.

3. **`hello` wiring** (`E0308`). `lib.rs` passes `ctx.accounts`; `handle_hello` expects `&mut Context<...>`. Pass `ctx`, same as initialize/deposit.

4. **Rename** `#[program] mod quasar_hello_solana` to match `zk_shielded_pool_solana` / `Anchor.toml`.

5. **Then, after it compiles**, leftover Quasar-shaped API (do not start these until build gets past the errors above):

   - `Vec<u8, 900>` in `upload_proof` (Anchor v2 instruction args)
   - Flattened `[u8; 32 * N]` vs `[[u8; 32]; N]` (there is a TODO on `ImtTree` asking if flattening is still required)
   - Events: `Address` vs `[u8; 32]` (`events.rs` still talks about Quasar)
   - `VaultStatus` unused; if it goes in an account it needs `#[pod_wrapper]`, not a plain enum
   - `rust-toolchain.toml` has `channel` commented out; stub pins `1.89.0`
   - Stale Quasar comments in `Cargo.toml`
   - Tests: `tests.rs.old` is not compiled. Stub uses LiteSVM under `programs/<name>/tests/` and `Anchor.toml` `test = "cargo test"`. Restore real tests after the program builds.
   - `halo2-solana-verifier` is commented out in Cargo.toml

## Compile errors last seen (from `anchor2 build`)

54 errors, including: `E0107` Result two-args; `Initialize` / `UploadProof` missing `Bumps` and `TryAccounts`; hello `ctx.accounts` type mismatch. `log` is already fixed. `ImtTreeZc` / `quasar_lang` are already fixed.

## Do not

Do not run `anchor2 build` until I ask.
Do not invent a new program ID.
Do not copy `_pad: [u8; N]` for integers next to `u8`; use `PodU16` / `PodU32`.
Do not reintroduce `ImtTreeZc` or `quasar_lang`.
Do not "finish the whole rewrite" in one turn.
