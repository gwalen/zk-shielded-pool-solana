mod common;

use {
    anchor_lang::{prelude::Address, solana_program::instruction::Instruction},
    anchor_v2_testing::{Keypair, Signer, VersionedMessage, VersionedTransaction},
    halo2_base::halo2_proofs::halo2curves::bn256::Fr,
    litesvm::{types::TransactionMetadata, LiteSVM},
    std::time::SystemTime,
    zk_shielded_pool_solana::{
        state::{nullifier::Nullifier, root_registry::RootRegistry},
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

/// The handler logs this only after the root, the proof, the recipient and the payout
/// all passed.
const WITHDRAWAL_DONE_LOG: &str = "Withdrawal done";

fn assert_withdraw_verified(
    result: Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
) -> TransactionMetadata {
    let meta = result.unwrap_or_else(|failure| {
        panic!(
            "withdraw failed: {:?}\nlogs:\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        )
    });
    assert!(
        meta.logs.iter().any(|line| line.contains(WITHDRAWAL_DONE_LOG)),
        "expected '{WITHDRAWAL_DONE_LOG}' in logs:\n{}",
        meta.logs.join("\n")
    );
    meta
}

/// Send a transaction with empty signatures. Only works on an SVM built with
/// `with_sigverify(false)`. Used to act as a fee payer whose private key the test does not have.
/// The program still sees the same accounts and signer flags as in a signed transaction.
fn send_unsigned(
    svm: &mut LiteSVM,
    fee_payer: Address,
    instructions: &[Instruction],
) -> Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let message = solana_message::v0::Message::try_compile(
        &fee_payer,
        instructions,
        &[], // LUT
        svm.latest_blockhash(),
    )
    .unwrap();
    let signature_count = message.header.num_required_signatures as usize;
    let tx = VersionedTransaction {
        signatures: vec![Default::default(); signature_count],
        message: VersionedMessage::V0(message),
    };
    svm.send_transaction(tx)
}

// TODO: do the nullfier creation and check by hand (no anchor init) to avoid this wierd testing and have verbose errors
/// A second spend of an already spent nullifier. Anchor's `init` asks the System Program
/// to create the marker account, which already exists, so the transaction fails before
/// the handler runs. It must not be refused as a duplicate transaction instead.
fn assert_replay_rejected_by_nullifier(
    result: Result<TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
) {
    let failure = result.expect_err("replay with the same nullifier must fail");
    let rendered = format!("{:?}", failure.err);
    let logs = failure.meta.logs.join("\n");
    assert!(
        !rendered.contains("AlreadyProcessed"),
        "replay was refused as a duplicate transaction, not by the nullifier: {rendered}"
    );
    assert!(
        logs.contains("already in use"),
        "expected the nullifier account creation to fail, got {rendered}\nlogs:\n{logs}"
    );
    assert!(
        !logs.contains(WITHDRAWAL_DONE_LOG),
        "replay must not withdraw, logs:\n{logs}"
    );
}

/// Rent-exempt minimum for the vault's data size. The payout must never go below it.
fn vault_rent_minimum(svm: &LiteSVM) -> u64 {
    let vault = svm.get_account(&vault_pda()).expect("vault missing");
    svm.minimum_balance_for_rent_exemption(vault.data.len())
}

/// Test-only shortcut to put the vault into a given balance.
fn set_lamports(svm: &mut LiteSVM, address: Address, lamports: u64) {
    let mut account = svm.get_account(&address).expect("account missing");
    account.lamports = lamports;
    svm.set_account(address, account).unwrap();
}

fn assert_nullifier_created(svm: &LiteSVM, public_inputs: &PublicInputs) {
    let (address, expected_bump) = nullifier_pda(&public_inputs.nullifier);
    let account = svm
        .get_account(&address)
        .unwrap_or_else(|| panic!("nullifier marker missing at {address}"));
    assert_eq!(
        account.owner,
        zk_shielded_pool_solana::id(),
        "nullifier marker must be program-owned"
    );
    let marker = read_pod::<Nullifier>(svm, address);
    assert_eq!(marker.bump, expected_bump);
}

fn assert_nullifier_missing(svm: &LiteSVM, public_inputs: &PublicInputs) {
    let (address, _) = nullifier_pda(&public_inputs.nullifier);
    assert!(
        svm.get_account(&address).is_none(),
        "failed withdraw must not leave a spent marker at {address}"
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

    // No marker before the first valid withdraw.
    assert_nullifier_missing(&svm, &public_inputs);

    // The vault holds its rent minimum plus the 9 SOL deposit. The recipient does not
    // exist yet. It is not the fee payer, so fees cannot change its balance.
    let vault_address = vault_pda();
    let rent_minimum = vault_rent_minimum(&svm);
    assert_eq!(
        account_lamports(&svm, vault_address),
        rent_minimum + FIXTURE_TOTAL_AMOUNT
    );
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), 0);

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(result);

    // First valid withdraw creates the spent marker.
    assert_nullifier_created(&svm, &public_inputs);

    // Exactly the proven 2 SOL moved from the vault to the recipient. Rent stays intact.
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), 2_000_000_000);
    assert_eq!(
        account_lamports(&svm, vault_address),
        rent_minimum + FIXTURE_TOTAL_AMOUNT - 2_000_000_000
    );
}

/// All three steps of the deposit, each with its own proof. Every step has its own
/// nullifier, so each one can be spent once. Together they pay out the full 9 SOL and
/// leave the vault at exactly its rent minimum.
#[test]
fn withdraw_all_three_steps_pays_the_full_deposit() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let vault_address = vault_pda();
    let rent_minimum = vault_rent_minimum(&svm);
    let mut nullifiers = Vec::new();
    let mut paid = 0;

    for (step, (proof, public_inputs_bytes)) in FIXTURE_STEP_PROOFS.iter().enumerate() {
        let public_inputs = public_inputs_from_bytes(public_inputs_bytes);
        assert_eq!(public_inputs.step, fr_to_be_bytes(Fr::from(step as u64)));
        assert_eq!(public_inputs.chunk_amount_u64().unwrap(), FIXTURE_CHUNKS[step]);
        assert!(
            !nullifiers.contains(&public_inputs.nullifier),
            "step {step} reuses a nullifier"
        );
        nullifiers.push(public_inputs.nullifier);

        let proof_hash = upload_proof(&mut svm, &payer, proof);
        let result = call_withdraw_ix(
            &mut svm,
            &payer,
            FIXTURE_RECIPIENT,
            public_inputs,
            proof_hash,
        );
        assert_withdraw_verified(result);
        assert_nullifier_created(&svm, &public_inputs);

        paid += FIXTURE_CHUNKS[step];
        assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), paid);
        assert_eq!(
            account_lamports(&svm, vault_address),
            rent_minimum + FIXTURE_TOTAL_AMOUNT - paid
        );
    }

    assert_eq!(paid, 9_000_000_000);
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), FIXTURE_TOTAL_AMOUNT);
    assert_eq!(account_lamports(&svm, vault_address), rent_minimum);
}

/// The vault holds one lamport less than rent minimum + 2 SOL. Paying 2 SOL would take it
/// below rent, so the withdrawal fails and no nullifier is spent. With exactly
/// rent minimum + 2 SOL the same withdrawal then succeeds and leaves only the rent.
#[test]
fn withdraw_fails_when_the_vault_cannot_keep_its_rent() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);
    let public_inputs = public_inputs_from_fixture();

    let vault_address = vault_pda();
    let rent_minimum = vault_rent_minimum(&svm);
    let chunk = FIXTURE_CHUNKS[0];
    set_lamports(&mut svm, vault_address, rent_minimum + chunk - 1);

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::InsufficientVaultFunds);
    assert_nullifier_missing(&svm, &public_inputs);
    assert_eq!(
        account_lamports(&svm, vault_address),
        rent_minimum + chunk - 1
    );
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), 0);

    set_lamports(&mut svm, vault_address, rent_minimum + chunk);

    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(result);
    assert_nullifier_created(&svm, &public_inputs);
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), chunk);
    assert_eq!(account_lamports(&svm, vault_address), rent_minimum);
}

/// The vault as recipient is refused before the handler runs, even though `recipient` is
/// `unsafe(dup)`. Anchor marks both positions of a repeated account, and `vault` is plain
/// `mut`, so its bit still trips ConstraintDuplicateMutableAccount (code 2040).
/// The handler's own `RecipientIsVault` check is a second guard.
#[test]
fn withdraw_to_the_vault_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);
    let vault_address = vault_pda();
    let vault_before = account_lamports(&svm, vault_address);

    let public_inputs = public_inputs_from_fixture();
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        vault_address,
        public_inputs,
        proof_hash,
    );
    let failure = result.expect_err("withdraw to the vault must fail");
    let rendered = format!("{:?}", failure.err);
    assert!(rendered.contains("Custom(2040)"), "got: {rendered}");
    assert_nullifier_missing(&svm, &public_inputs);
    assert_eq!(account_lamports(&svm, vault_address), vault_before);
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
    let public_inputs = public_inputs_from_fixture();
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(result);
    assert_nullifier_created(&svm, &public_inputs);
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
    let public_inputs = public_inputs_from_fixture();
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        other_recipient,
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::DestinationMismatch);
    // Destination check failed, so the `init` nullifier must have been rolled back.
    assert_nullifier_missing(&svm, &public_inputs);
}

/// The payer as recipient, but this payer is not the proven destination.
/// `recipient` is `unsafe(dup)`, so Anchor no longer refuses the same writable account twice
/// (error 2040). The request reaches the handler and fails the destination check instead.
#[test]
fn withdraw_to_a_payer_that_is_not_the_proven_destination_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let public_inputs = public_inputs_from_fixture();
    let result = call_withdraw_ix(
        &mut svm,
        &payer,
        payer.pubkey(),
        public_inputs,
        proof_hash,
    );
    assert_custom_error(result, DappError::DestinationMismatch);
    assert_nullifier_missing(&svm, &public_inputs);
}

// TODO: use the saved dest private key in the test (move to test fixture dir (local for this program))
/// The fee payer withdraws to itself: `sender` and `recipient` are the same account.
///
/// The fixture proof pays `FIXTURE_RECIPIENT`, and the test has no private key for it (only
/// the public key is used). So this test turns LiteSVM signature checks off and sends the
/// upload and withdraw transactions with `FIXTURE_RECIPIENT` as fee payer and empty signatures.
/// The program sees the same accounts and signer flags as in a signed transaction.
#[test]
fn withdraw_to_the_payer_succeeds_when_the_payer_is_the_proven_destination() {
    let (svm, payer) = setup();
    let mut svm = svm.with_sigverify(false);
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    // The wallet that pays fees and rent, and also receives the payout.
    let wallet = FIXTURE_RECIPIENT;
    svm.airdrop(&wallet, 1_000_000_000).unwrap();

    let proof_hash = calculate_proof_hash(FIXTURE_STEP0_PROOF);
    let proof_address = proof_pda(&wallet, proof_hash).0;
    let proof_parts = [
        (0u8, &FIXTURE_STEP0_PROOF[..PROOF_UPLOAD_PART0_LEN]),
        (1u8, &FIXTURE_STEP0_PROOF[PROOF_UPLOAD_PART0_LEN..]),
    ];
    for (part, bytes) in proof_parts {
        send_unsigned(
            &mut svm,
            wallet,
            &[upload_proof_ix(
                wallet,
                part,
                CHECKED_IN_PROOF_LEN as u16,
                bytes.to_vec(),
                proof_hash,
                proof_address,
            )],
        )
        .unwrap_or_else(|failure| {
            panic!(
                "upload part {part} failed: {:?}\nlogs:\n{}",
                failure.err,
                failure.meta.logs.join("\n")
            )
        });
    }

    let public_inputs = public_inputs_from_fixture();
    let vault_address = vault_pda();
    let rent_minimum = vault_rent_minimum(&svm);
    let wallet_before = account_lamports(&svm, wallet);

    let result = send_unsigned(
        &mut svm,
        wallet,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            // sender == recipient
            withdraw_ix(wallet, wallet, public_inputs, proof_hash),
        ],
    );
    let meta = assert_withdraw_verified(result);
    assert_nullifier_created(&svm, &public_inputs);

    // The same wallet paid the fee and the nullifier rent, and got the 2 SOL payout.
    let nullifier_rent = account_lamports(&svm, nullifier_pda(&public_inputs.nullifier).0);
    assert_eq!(
        account_lamports(&svm, wallet),
        wallet_before - meta.fee - nullifier_rent + FIXTURE_CHUNKS[0]
    );
    assert_eq!(
        account_lamports(&svm, vault_address),
        rent_minimum + FIXTURE_TOTAL_AMOUNT - FIXTURE_CHUNKS[0]
    );
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
    assert_nullifier_missing(&svm, &public_inputs);
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
    assert_nullifier_missing(&svm, &public_inputs);
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
    assert_nullifier_missing(&svm, &public_inputs);
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
    assert_nullifier_missing(&svm, &public_inputs);
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
    assert_nullifier_missing(&svm, &public_inputs);
}

/// Second spend of the same nullifier fails, even in a fresh transaction with the same
/// valid proof. The `init` nullifier account already exists, so account creation fails
/// before verification. The marker stays spent and nothing is paid twice.
#[test]
fn withdraw_replay_with_same_nullifier_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let public_inputs = public_inputs_from_fixture();
    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let first = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(first);
    assert_nullifier_created(&svm, &public_inputs);

    let vault_after_first = account_lamports(&svm, vault_pda());
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), FIXTURE_CHUNKS[0]);

    let replay = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_replay_rejected_by_nullifier(replay);

    // Still spent after the failed replay, and no second payout.
    assert_nullifier_created(&svm, &public_inputs);
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), FIXTURE_CHUNKS[0]);
    assert_eq!(account_lamports(&svm, vault_pda()), vault_after_first);
}

/// Same nullifier, but a different fee payer with their own uploaded proof account.
/// Proof storage is per-sender, the nullifier is global, so the second spend still fails.
#[test]
fn withdraw_replay_with_another_sender_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    deposit_with_fixture_values(&mut svm, &payer);

    let public_inputs = public_inputs_from_fixture();
    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    let first = call_withdraw_ix(
        &mut svm,
        &payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        proof_hash,
    );
    assert_withdraw_verified(first);
    assert_nullifier_created(&svm, &public_inputs);

    // Second sender funds their own fee/rent and uploads the same proof bytes under
    // their own proof PDA (seeds include the sender).
    let other_payer = Keypair::new();
    svm.airdrop(&other_payer.pubkey(), AIRDROP_LAMPORTS).unwrap();
    let other_proof_hash = upload_fixture_proof(&mut svm, &other_payer);
    assert_eq!(other_proof_hash, proof_hash);
    let (first_proof_pda, _) = proof_pda(&payer.pubkey(), proof_hash);
    let (other_proof_pda, _) = proof_pda(&other_payer.pubkey(), proof_hash);
    assert_ne!(first_proof_pda, other_proof_pda);

    // Same nullifier bytes, so the same global marker PDA.
    let (marker, _) = nullifier_pda(&public_inputs.nullifier);
    assert!(svm.get_account(&marker).is_some());

    let vault_after_first = account_lamports(&svm, vault_pda());

    let replay = call_withdraw_ix(
        &mut svm,
        &other_payer,
        FIXTURE_RECIPIENT,
        public_inputs,
        other_proof_hash,
    );
    assert_replay_rejected_by_nullifier(replay);
    assert_nullifier_created(&svm, &public_inputs);
    assert_eq!(account_lamports(&svm, FIXTURE_RECIPIENT), FIXTURE_CHUNKS[0]);
    assert_eq!(account_lamports(&svm, vault_pda()), vault_after_first);
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
