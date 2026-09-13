use crate::common::off_chain_imt::{poseidon_hash, OffChainImt, TREE_DEPTH_MAX};

use {
    anchor_v2_testing::Signer,
    zk_shielded_pool_solana::{
        state::proof_storage::ProofStorage, utils::merkle_proof::MerkleProof,
    },
};

mod common;
use anchor_v2_testing::Keypair;
use common::constants::*;
use common::instruction_helpers::*;
use common::utils::*;
use halo2_base::halo2_proofs::halo2curves::bn256::Fr;
use litesvm::types::TransactionMetadata;
use litesvm::LiteSVM;
use zk_shielded_pool_solana::utils::public_inputs::PublicInputs;

#[test]
fn calculate_root_and_withdraw() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let proof = include_bytes!("../../../../solana-proof-generator/fixtures/proof.bin");
    assert_eq!(proof.len(), CHECKED_IN_PROOF_LEN);
    let proof_hash = calculate_proof_hash(proof);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            CHECKED_IN_PROOF_LEN as u16,
            proof[..PROOF_UPLOAD_PART0_LEN].to_vec(),
            proof_hash,
            proof_address,
        ),
    );
    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            1,
            CHECKED_IN_PROOF_LEN as u16,
            proof[PROOF_UPLOAD_PART0_LEN..].to_vec(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(
        stored.proof_current_len.get() as usize,
        CHECKED_IN_PROOF_LEN
    );
    assert_eq!(&stored.proof[..CHECKED_IN_PROOF_LEN], &proof[..]);

    let public_inputs_bytes =
        include_bytes!("../../../../solana-proof-generator/fixtures/public_inputs.bin");
    assert_eq!(public_inputs_bytes.len(), CHECKED_IN_PUBLIC_INPUTS_LEN);

    // ******** Fixtures for public inputs in plain values **********
    let total_amount = Fr::from(9);
    let chunks = [Fr::from(1), Fr::from(2), Fr::from(3)];
    let addresses = [Fr::from(1001), Fr::from(1002), Fr::from(1003)];
    let step_idx = Fr::from(0);
    let user_secret_s = Fr::from(SECRET_S);
    let nullifier = poseidon_hash(&[user_secret_s, step_idx]);

    let mt_tree = build_mt_tree(user_secret_s, chunks, addresses, total_amount);
    let root = mt_tree.root();
    // ******** Fixtures for public inputs - validation **********
    // TODO: assert that valus are same what is in the fixuture
    // ******************

    let public_inputs_byte_chunks: [[u8; 32]; PUBLIC_INPUT_COUNT] = public_inputs_bytes
        .chunks_exact(32)
        .map(|chunk| <[u8; 32]>::try_from(chunk).unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap();

    let public_inputs = PublicInputs::from_byte_chunks(&public_inputs_byte_chunks);

    let merkle_proof_mock = MerkleProof::new(proof_hash, vec![], vec![]);

    let meta = send_ok_many(
        &mut svm,
        &payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(payer.pubkey(), public_inputs, proof_hash, merkle_proof_mock),
        ],
    );
    let logs = meta.logs.join("\n");
    println!("withdraw logs: {logs}");
    assert!(
        logs.contains("Proof verified"),
        "expected the program to log Proof verified, got:\n{logs}"
    );
}

fn build_mt_tree(
    user_secret_s: Fr,
    chunks: [Fr; 3],
    addresses: [Fr; 3],
    total_amount: Fr,
) -> OffChainImt {
    let user_commitment_hash = poseidon_hash(&[
        user_secret_s,
        chunks[0],
        chunks[1],
        chunks[2],
        addresses[0],
        addresses[1],
        addresses[2],
    ]);
    let deposit_commitment_hash = poseidon_hash(&[user_commitment_hash, total_amount]);

    let mut imt_tree = OffChainImt::new(TREE_DEPTH_MAX as u32);
    imt_tree.insert_leaf_lazy(deposit_commitment_hash).unwrap();
    imt_tree.build_tree();
    imt_tree
}

fn deposit(
    svm: &mut LiteSVM,
    payer: &Keypair,
    total_amount: u64,
    user_commitment_hash: [u8; 32],
) -> TransactionMetadata {

    let deposit_result_meta = send_ok(
        svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, total_amount),
    );
    deposit_result_meta
}
