use {
    anchor_v2_testing::Signer,
    zk_shielded_pool_solana::{
        state::proof_storage::ProofStorage,
        utils::merkle_proof::MerkleProof
    },
};

mod common;
use common::constants::*;
use common::utils::*;
use common::instruction_helpers::*;
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