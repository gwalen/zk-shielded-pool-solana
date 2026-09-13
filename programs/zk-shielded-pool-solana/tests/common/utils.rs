use {
    anchor_lang::{
        bytemuck,
        prelude::Address,
        solana_program::instruction::Instruction,
        Discriminator,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer, VersionedTransaction},
    litesvm::types::{FailedTransactionMetadata, TransactionMetadata},
    solana_message::{v0, VersionedMessage},
    zk_shielded_pool_solana::{
        state::proof_storage::ProofStorage,
        utils::{errors::DappError, public_inputs::PublicInputs},
    },
};


use super::constants::*;
use super::instruction_helpers::{proof_pda, upload_proof_ix};

/// The checked-in GWC proof the withdrawal tests replay.
pub const FIXTURE_PROOF: &[u8] =
    include_bytes!("../../../../../solana-proof-generator/fixtures/proof.bin");

/// The five 32-byte big-endian public inputs that proof was generated for.
pub const FIXTURE_PUBLIC_INPUTS: &[u8] =
    include_bytes!("../../../../../solana-proof-generator/fixtures/public_inputs.bin");

/// Read the checked-in public inputs into the struct the program takes.
pub fn public_inputs_from_fixture() -> PublicInputs {
    assert_eq!(FIXTURE_PUBLIC_INPUTS.len(), CHECKED_IN_PUBLIC_INPUTS_LEN);

    let byte_chunks: [[u8; 32]; PUBLIC_INPUT_COUNT] = FIXTURE_PUBLIC_INPUTS
        .chunks_exact(32)
        .map(|chunk| <[u8; 32]>::try_from(chunk).unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap();

    PublicInputs::from_byte_chunks(&byte_chunks)
}

/// Upload the checked-in proof, split over the two instructions the packet budget
/// forces, and return its hash. Also checks that the bytes landed in the buffer.
pub fn upload_fixture_proof(svm: &mut LiteSVM, payer: &Keypair) -> [u8; 32] {
    assert_eq!(FIXTURE_PROOF.len(), CHECKED_IN_PROOF_LEN);
    let proof_hash = calculate_proof_hash(FIXTURE_PROOF);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    send_ok(
        svm,
        payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            CHECKED_IN_PROOF_LEN as u16,
            FIXTURE_PROOF[..PROOF_UPLOAD_PART0_LEN].to_vec(),
            proof_hash,
            proof_address,
        ),
    );
    send_ok(
        svm,
        payer,
        upload_proof_ix(
            payer.pubkey(),
            1,
            CHECKED_IN_PROOF_LEN as u16,
            FIXTURE_PROOF[PROOF_UPLOAD_PART0_LEN..].to_vec(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(svm, proof_address);
    assert_eq!(
        stored.proof_current_len.get() as usize,
        CHECKED_IN_PROOF_LEN
    );
    assert_eq!(&stored.proof[..CHECKED_IN_PROOF_LEN], FIXTURE_PROOF);

    proof_hash
}


/// Same Keccak256 as on-chain `sol_keccak256`. Host tests cannot call that
/// syscall, so this uses the Solana hasher crate with its `sha3` feature.
pub fn calculate_proof_hash(proof: &[u8]) -> [u8; 32] {
    solana_keccak_hasher::hash(proof).to_bytes()
}

pub fn setup() -> (LiteSVM, Keypair) {
    let mut feature_set = LiteSVM::mainnet_feature_set();
    feature_set.activate(&ENABLE_BIG_MOD_EXP_SYSCALL_ID, 0);

    // Set the feature before rebuilding the runtime. That puts
    // sol_big_mod_exp in the syscall table used by the loaded program.
    // `svm()` is LiteSVM::new(), plus tracing when `--features profile` is on.
    let mut svm = anchor_v2_testing::svm()
        .with_feature_set(feature_set)
        .with_builtins();
    let zk_shieleded_pool_binary =
        include_bytes!("../../../../target/deploy/zk_shielded_pool_solana.so");
    svm.add_program(zk_shielded_pool_solana::id(), zk_shieleded_pool_binary)
        .unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), AIRDROP_LAMPORTS).unwrap();
    (svm, payer)
}

#[allow(clippy::result_large_err)]
pub fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ixs: &[Instruction],
) -> Result<TransactionMetadata, FailedTransactionMetadata> {
    let msg = v0::Message::try_compile(
        &payer.pubkey(),
        ixs,
        &[], // LUT
        svm.latest_blockhash(),
    )
    .unwrap();
    let tx = VersionedTransaction::try_new(VersionedMessage::V0(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
}

pub fn send_ok(svm: &mut LiteSVM, payer: &Keypair, instruction: Instruction) -> TransactionMetadata {
    send_ok_many(svm, payer, &[instruction])
}

pub fn send_ok_many(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
) -> TransactionMetadata {
    send(svm, payer, instructions).unwrap_or_else(|failure| {
        panic!(
            "transaction failed: {:?}\nlogs:\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        )
    })
}

pub fn dapp_error_code(error: DappError) -> u32 {
    error as u32 + ANCHOR_V2_ERROR_CODE_OFFSET
}

pub fn assert_custom_error(
    result: Result<TransactionMetadata, FailedTransactionMetadata>,
    error: DappError,
) {
    let expected = dapp_error_code(error);
    let failure = match result {
        Ok(_) => panic!("expected Custom({expected}), got success"),
        Err(failure) => failure,
    };
    let rendered = format!("{:?}", failure.err);
    assert!(
        rendered.contains(&format!("Custom({expected})")),
        "expected Custom({expected}), got: {rendered}"
    );
}

pub fn account_lamports(svm: &LiteSVM, address: Address) -> u64 {
    svm.get_account(&address)
        .map(|account| account.lamports)
        .unwrap_or(0)
}

pub fn read_pod<T: Discriminator + bytemuck::Pod>(svm: &LiteSVM, address: Address) -> T {
    let account = svm.get_account(&address).expect("account missing");
    let disc_len = T::DISCRIMINATOR.len();
    // skip discriminator and read the rest of the data
    let payload = &account.data[disc_len..disc_len + core::mem::size_of::<T>()];
    // from_bytes gives &T, so we copy and dereference it to get T (T is Copy)
    *bytemuck::from_bytes(payload)
}