mod common;

use {
    anchor_v2_testing::{Keypair, Signer},
    halo2_base::halo2_proofs::halo2curves::bn256::Fr,
    litesvm::{types::TransactionMetadata, LiteSVM},
    std::time::SystemTime,
    zk_shielded_pool_solana::{
        state::root_registry::RootRegistry,
        utils::{
            common::reverse_byte_order, constants::EMPTY_TREE_VALUE, errors::DappError,
            flatten_array::get_array_element, public_inputs::PublicInputs,
        },
    },
};

use common::{
    constants::*,
    instruction_helpers::*,
    off_chain_imt::{
        fr_to_be_bytes, fr_to_le_bytes, hash1, hash2, poseidon_hash, OffChainImt, TREE_DEPTH_MAX,
    },
    utils::*,
};

// ******** Plain values the checked-in proof was generated from ********
const FIXTURE_TOTAL_AMOUNT: u64 = 9;
const FIXTURE_CHUNKS: [u64; 3] = [2, 3, 4];
const FIXTURE_ADDRESSES: [u64; 3] = [1001, 1002, 1003];
const FIXTURE_STEP: u64 = 0;

/// Poseidon over the secret, the three chunk amounts and the three destinations. This is
/// the leaf input the depositor publishes; the deposit handler hashes it with the total.
fn user_commitment_hash_from_fixture() -> Fr {
    poseidon_hash(&[
        Fr::from(SECRET_S),
        Fr::from(FIXTURE_CHUNKS[0]),
        Fr::from(FIXTURE_CHUNKS[1]),
        Fr::from(FIXTURE_CHUNKS[2]),
        Fr::from(FIXTURE_ADDRESSES[0]),
        Fr::from(FIXTURE_ADDRESSES[1]),
        Fr::from(FIXTURE_ADDRESSES[2]),
    ])
}

/// The deposit that puts the fixture's commitment into the pool, so the pool records the
/// root the fixture proof was built against.
fn deposit_with_fixture_values(svm: &mut LiteSVM, payer: &Keypair) -> TransactionMetadata {
    deposit(
        svm,
        payer,
        FIXTURE_TOTAL_AMOUNT,
        fr_to_le_bytes(user_commitment_hash_from_fixture()),
    )
}

fn root_registry(svm: &LiteSVM) -> RootRegistry {
    read_pod::<RootRegistry>(svm, root_registry_pda().0)
}

fn call_withdraw_ix(
    svm: &mut LiteSVM,
    payer: &Keypair,
    public_inputs: PublicInputs,
    proof_hash: [u8; 32],
) -> Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    send(
        svm,
        payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(payer.pubkey(), public_inputs, proof_hash),
        ],
    )
}

fn assert_proof_verified(meta: &TransactionMetadata) {
    let logs = meta.logs.join("\n");
    println!("withdraw logs: {logs}");
    assert!(
        logs.contains("Proof verified"),
        "expected the program to log Proof verified, got:\n{logs}"
    );
}

/// The full positive path: deposit the fixture's commitment, then withdraw against the
/// root that deposit produced. Also pins the three ways that root is expressed against
/// each other - the off-chain tree, the on-chain registry, and the fixture's public input.
#[test]
fn calculate_root_and_withdraw() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let public_inputs = public_inputs_from_fixture();

    // ******** Fixtures for public inputs in plain values **********
    let total_amount = Fr::from(FIXTURE_TOTAL_AMOUNT);
    let chunks = FIXTURE_CHUNKS.map(Fr::from);
    let addresses = FIXTURE_ADDRESSES.map(Fr::from);
    let step_idx = Fr::from(FIXTURE_STEP);
    let nullifier = hash2(Fr::from(SECRET_S), step_idx);

    let user_commitment_hash = user_commitment_hash_from_fixture();
    let mt_tree = build_mt_tree(user_commitment_hash, total_amount);
    let off_chain_root = mt_tree.root();

    // ******** Fixtures for public inputs - validation **********
    assert_eq!(public_inputs.step, fr_to_be_bytes(step_idx));
    assert_eq!(public_inputs.chunk_amount, fr_to_be_bytes(chunks[0]));
    assert_eq!(public_inputs.dest_address, fr_to_be_bytes(addresses[0]));
    assert_eq!(public_inputs.nullifier, fr_to_be_bytes(nullifier));
    assert_eq!(public_inputs.root, fr_to_be_bytes(off_chain_root));
    // ******************

    // The matching deposit. Without it the pool has never seen this root.
    deposit_with_fixture_values(&mut svm, &payer);

    // The registry stores roots little-endian; the public input carries the same value
    // big-endian. Flip it so both sides of the comparison are in the same byte order.
    let fixture_root_le = reverse_byte_order(public_inputs.root);

    let registry = root_registry(&svm);
    assert_eq!(registry.imt.root, fr_to_le_bytes(off_chain_root));
    assert_eq!(registry.imt.root, fixture_root_le);
    // First deposit, so the new root went into ring buffer slot 1 (slot 0 is the empty tree).
    assert_eq!(registry.last_root_idx.get(), 1);
    assert_eq!(
        get_array_element(&registry.roots_history, 1),
        fixture_root_le
    );

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let meta = send_ok_many(
        &mut svm,
        &payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(payer.pubkey(), public_inputs, proof_hash),
        ],
    );
    assert_proof_verified(&meta);
}

/// A later deposit moves the current root on. The fixture's root is now history, and
/// history is still accepted.
#[test]
fn withdraw_accepts_a_recorded_historical_root() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    deposit_with_fixture_values(&mut svm, &payer);
    let fixture_root_le = root_registry(&svm).imt.root;

    // Somebody else deposits. The current root changes, slot 1 still holds ours.
    deposit(&mut svm, &payer, 5, fr_to_le_bytes(hash1(7)));

    let registry = root_registry(&svm);
    assert_ne!(registry.imt.root, fixture_root_le);
    assert_eq!(registry.last_root_idx.get(), 2);
    assert_eq!(
        get_array_element(&registry.roots_history, 1),
        fixture_root_le
    );

    let proof_hash = upload_fixture_proof(&mut svm, &payer);
    let result = call_withdraw_ix(&mut svm, &payer, public_inputs_from_fixture(), proof_hash)
        .expect("a root still in the history must be accepted");
    assert_proof_verified(&result);
}

/// A root this pool never recorded is refused, even with the matching deposit present.
#[test]
fn withdraw_with_an_unknown_root_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let mut public_inputs = public_inputs_from_fixture();
    public_inputs.root = fr_to_be_bytes(hash1(99));

    let result = call_withdraw_ix(&mut svm, &payer, public_inputs, proof_hash);
    assert_custom_error(result, DappError::UnknownRoot);
}

/// Slots 2..99 of the ring buffer still hold `EMPTY_TREE_VALUE`, the "nothing here yet"
/// marker. Passing root == EMPTY_TREE_VALUE must not find a match.
#[test]
fn withdraw_with_the_unused_history_marker_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    // The marker really is sitting in the account we are about to query.
    assert_eq!(
        get_array_element(&root_registry(&svm).roots_history, 2),
        EMPTY_TREE_VALUE
    );

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    // The program flips the public input back to little-endian before the lookup.
    let mut public_inputs = public_inputs_from_fixture();
    public_inputs.root = reverse_byte_order(EMPTY_TREE_VALUE);

    let result = call_withdraw_ix(&mut svm, &payer, public_inputs, proof_hash);
    assert_custom_error(result, DappError::UnknownRoot);
}

/// Swapping in a different root that the pool *did* record gets past the registry lookup
/// and then fails verification: the proof is bound to the root it was generated for.
#[test]
fn withdraw_with_a_recorded_but_different_root_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    // A second deposit records a second, unrelated root.
    deposit(&mut svm, &payer, 5, fr_to_le_bytes(hash1(7)));
    let other_root_be = reverse_byte_order(root_registry(&svm).imt.root);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let mut public_inputs = public_inputs_from_fixture();
    assert_ne!(public_inputs.root, other_root_be);
    public_inputs.root = other_root_be;

    let result = call_withdraw_ix(&mut svm, &payer, public_inputs, proof_hash);
    assert_custom_error(result, DappError::InvalidProof);
}

fn build_mt_tree(user_commitment_hash: Fr, total_amount: Fr) -> OffChainImt {
    let deposit_commitment_hash = hash2(user_commitment_hash, total_amount);

    let empty_tree_start = SystemTime::now();
    let mut imt_tree = OffChainImt::new(TREE_DEPTH_MAX as u32);
    let empty_tree_duration = SystemTime::now().duration_since(empty_tree_start).unwrap();
    println!("empty tree build duration: {:?}", empty_tree_duration);

    imt_tree.insert_leaf_lazy(deposit_commitment_hash).unwrap();

    let leaf_tree_start = SystemTime::now();
    imt_tree.build_tree();
    let leaf_tree_duration = SystemTime::now().duration_since(leaf_tree_start).unwrap();
    println!("tree rebuild after leaf duration: {:?}", leaf_tree_duration);

    imt_tree
}

fn deposit(
    svm: &mut LiteSVM,
    payer: &Keypair,
    total_amount: u64,
    user_commitment_hash: [u8; 32],
) -> TransactionMetadata {
    send_ok(
        svm,
        payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, total_amount),
    )
}
