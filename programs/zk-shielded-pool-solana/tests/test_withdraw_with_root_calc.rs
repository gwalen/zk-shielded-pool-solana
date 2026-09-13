mod common;

use {
    anchor_lang::prelude::Address,
    anchor_v2_testing::{Keypair, Signer},
    halo2_base::halo2_proofs::halo2curves::bn256::Fr,
    litesvm::{types::TransactionMetadata, LiteSVM},
    std::time::SystemTime,
    zk_shielded_pool_solana::{
        state::root_registry::RootRegistry,
        utils::{
            common::reverse_byte_order, constants::EMPTY_TREE_VALUE,
            dest_address_hash::dest_address_hash_le, errors::DappError,
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

/// Off-chain version of the circuit's `convert_pubkey_32bytes_to_fr`, written with `Fr`:
/// four big-endian u64 groups of the key, hashed with Poseidon.
fn pubkey_to_fr(address: Address) -> Fr {
    let bytes = address.to_bytes();
    let limbs: Vec<Fr> = bytes
        .chunks_exact(8)
        .map(|group| Fr::from(u64::from_be_bytes(group.try_into().unwrap())))
        .collect();
    poseidon_hash(&limbs)
}

/// Poseidon over the secret, the three chunk amounts and the three destinations. This is
/// the leaf input the depositor publishes; the deposit handler hashes it with the total.
fn user_commitment_hash_from_fixture() -> Fr {
    let dest = pubkey_to_fr(FIXTURE_RECIPIENT);
    poseidon_hash(&[
        Fr::from(SECRET_S),
        Fr::from(FIXTURE_CHUNKS[0]),
        Fr::from(FIXTURE_CHUNKS[1]),
        Fr::from(FIXTURE_CHUNKS[2]),
        dest,
        dest,
        dest,
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
    recipient: Address,
    public_inputs: PublicInputs,
    proof_hash: [u8; 32],
) -> Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    send(
        svm,
        payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(payer.pubkey(), recipient, public_inputs, proof_hash),
        ],
    )
}

/// The handler logs "Proof verified" only after the root, the proof and the recipient
/// all passed.
fn assert_withdraw_verified(
    result: Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
) {
    let meta = result.unwrap_or_else(|failure| {
        panic!(
            "withdraw failed: {:?}\nlogs:\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        )
    });
    assert!(
        meta.logs.iter().any(|line| line.contains("Proof verified")),
        "expected 'Proof verified' in logs:\n{}",
        meta.logs.join("\n")
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
    let dest_address = pubkey_to_fr(FIXTURE_RECIPIENT);
    let step_idx = Fr::from(FIXTURE_STEP);
    let nullifier = hash2(Fr::from(SECRET_S), step_idx);

    // The on-chain address hash gives the same field value as the off-chain Fr version.
    assert_eq!(
        dest_address_hash_le(&FIXTURE_RECIPIENT.to_bytes()).unwrap(),
        fr_to_le_bytes(dest_address)
    );

    let user_commitment_hash = user_commitment_hash_from_fixture();
    let mt_tree = build_mt_tree(user_commitment_hash, total_amount);
    let off_chain_root = mt_tree.root();

    // ******** Fixtures for public inputs - validation **********
    assert_eq!(public_inputs.step, fr_to_be_bytes(step_idx));
    assert_eq!(public_inputs.chunk_amount, fr_to_be_bytes(chunks[0]));
    assert_eq!(public_inputs.chunk_amount_u64().unwrap(), 2_000_000_000);
    assert_eq!(public_inputs.dest_address, fr_to_be_bytes(dest_address));
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

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(result);
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
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs_from_fixture(),
        proof_hash,
    );
    assert_withdraw_verified(result);
}

/// Same valid proof and public inputs, but a different recipient account. The destination
/// check runs after verification, so this error also shows the proof itself was accepted.
#[test]
fn withdraw_to_another_recipient_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let other_recipient = Keypair::new().pubkey();
    assert_ne!(other_recipient, FIXTURE_RECIPIENT);
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        other_recipient,
        public_inputs_from_fixture(),
        proof_hash,
    );
    assert_custom_error(result, DappError::DestinationMismatch);
}

/// The payer as recipient is also refused, but earlier: `sender` and `recipient` are both
/// mutable, and Anchor rejects the same mutable account twice before the handler runs
/// (ConstraintDuplicateMutableAccount, code 2040).
#[test]
fn withdraw_to_the_payer_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        payer.pubkey(),
        public_inputs_from_fixture(),
        proof_hash,
    );
    let failure = result.expect_err("withdraw to the payer must fail");
    let rendered = format!("{:?}", failure.err);
    assert!(rendered.contains("Custom(2040)"), "got: {rendered}");
}

/// Asking for a different amount with the same proof fails verification. The new amount
/// is a real chunk of this deposit (step 1), still a valid u64, and the root is known.
#[test]
fn withdraw_with_a_changed_amount_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let mut public_inputs = public_inputs_from_fixture();
    public_inputs.chunk_amount = fr_to_be_bytes(Fr::from(FIXTURE_CHUNKS[1]));
    assert_eq!(public_inputs.chunk_amount_u64().unwrap(), 3_000_000_000);

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::InvalidProof);
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

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
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

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
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

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::InvalidProof);
}

/// A chunk amount with a nonzero byte above the u64 range is refused on chain before
/// verification. The root is valid, so the rejection comes from the amount decoding.
#[test]
fn withdraw_with_a_chunk_amount_above_u64_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let mut public_inputs = public_inputs_from_fixture();
    // Byte 23 is the lowest byte above the u64 range: this is 2^64 + the real amount.
    public_inputs.chunk_amount[23] = 1;

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::ChunkAmountTooLarge);
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
