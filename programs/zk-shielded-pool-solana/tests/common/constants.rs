use anchor_lang::prelude::*;
use halo2_base::halo2_proofs::halo2curves::bn256::Fr;

/// Default first `#[error_code]` value. Matches Anchor v2's offset.
pub const ANCHOR_V2_ERROR_CODE_OFFSET: u32 = 6000;

/// 20 SOL covers the 9 SOL fixture deposit (plus a small second deposit in some tests),
/// rent for the vault, root registry and proof account, and transaction fees.
pub const AIRDROP_LAMPORTS: u64 = 20_000_000_000;

pub const DEPOSIT_LAMPORTS: u64 = 1_250_000_000;

/// Checked-in GWC proof (`solana-proof-generator/fixtures/step0/proof.bin`).
pub const CHECKED_IN_PROOF_LEN: usize = 1088;
/// First `upload_proof` chunk. One instruction has about 971 bytes leftover after headers.
pub const PROOF_UPLOAD_PART0_LEN: usize = 800;
/// Five 32-byte public inputs (`solana-proof-generator/fixtures/step0/public_inputs.bin`).
pub const CHECKED_IN_PUBLIC_INPUTS_LEN: usize = 160;
pub const PUBLIC_INPUT_COUNT: usize = 5;
/// Same CU cap the Mollusk verifier harness uses (`SOLANA_TRANSACTION_CU_LIMIT`).
pub const VERIFY_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;
/// Same heap the Mollusk verifier harness uses. Default 32 KiB overflows in `verify_gwc`.
pub const VERIFY_HEAP_FRAME_BYTES: u32 = 64 * 1024;
pub const COMPUTE_BUDGET_PROGRAM_ID: Address =
    anchor_lang::address!("ComputeBudget111111111111111111111111111111");
// This is agave_feature_set::enable_big_mod_exp_syscall::ID. LiteSVM 0.13.1's
// mainnet snapshot does not include it yet.
pub const ENABLE_BIG_MOD_EXP_SYSCALL_ID: Address =
    anchor_lang::address!("EBq48m8irRKuE7ZnMTLvLg2UuGSqhe8s8oMqnmja1fJw");


//**** Proof fixture constants ****
// Same values as solana-proof-generator/circuits/shielded-pool/src/circuit/prover.rs
// (`build_fixture_input_for_step`). The checked-in step 0, 1 and 2 proofs were generated from them.

pub const SECRET_S: u64 = 1_234_567_890;
/// All three chunks go to this key. Only the public key is used, no keypair file.
pub const FIXTURE_RECIPIENT: Address =
    anchor_lang::address!("dstH17g8RBGdUo3YeYhSFHDdFzHrWkAzNCKSveAchyD");
pub const FIXTURE_TOTAL_AMOUNT: u64 = 9_000_000_000;
pub const FIXTURE_CHUNKS: [u64; 3] = [2_000_000_000, 3_000_000_000, 4_000_000_000];
pub const FIXTURE_STEP: u64 = 0;