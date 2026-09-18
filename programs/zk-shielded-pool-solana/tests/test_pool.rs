use {
    anchor_v2_testing::{Keypair, Signer},
    zk_shielded_pool_solana::{
        state::{program_config::ProgramConfig, proof_storage::ProofStorage, root_registry::RootRegistry},
        utils::{
            constants::{DEFAULT_MIN_DEPOSIT_LAMPORTS, EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
            errors::DappError,
            flatten_array::get_array_element,
            imt_tree::{u64_to_32bytes_le, ImtTree},
            poseidon_hash,
        },
    },
};

mod common;
use common::constants::*;
use common::utils::*;
use common::instruction_helpers::*;


#[test]
fn initialize_writes_root_registry_in_place() {
    let (mut svm, payer) = setup();
    let (root_registry_address, root_registry_bump) = root_registry_pda();

    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let expected_imt = ImtTree::new().unwrap();

    let root_registry = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(root_registry.imt.root, expected_imt.root);
    assert_eq!(root_registry.imt.frontiers, expected_imt.frontiers);
    assert_eq!(root_registry.imt.zero_values, expected_imt.zero_values);
    assert_eq!(root_registry.imt.next_leaf_idx.get(), 0);
    assert_eq!(root_registry.last_root_idx.get(), 0);
    assert_eq!(root_registry.bump, root_registry_bump);
    assert_eq!(
        get_array_element(&root_registry.roots_history, 0),
        expected_imt.root
    );
    for index in 1..ROOT_RING_BUFFER_LENGTH {
        assert_eq!(
            get_array_element(&root_registry.roots_history, index),
            EMPTY_TREE_VALUE
        );
    }

    let program_config = read_pod::<ProgramConfig>(&svm, program_config_pda());
    assert_eq!(program_config.owner, payer.pubkey());
    assert!(!program_config.pause.get());
    assert_eq!(
        program_config.min_deposit_lamports.get(),
        DEFAULT_MIN_DEPOSIT_LAMPORTS
    );
}

/// Audit finding 1: every PDA in `initialize` uses plain `init`, so a second call must
/// fail and leave the tree, the root history and the owner exactly as they were.
#[test]
fn second_initialize_call_fails_and_keeps_state() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    // Put one leaf in, so a reset would be visible.
    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();
    send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, DEPOSIT_LAMPORTS),
    );
    let root_registry_address = root_registry_pda().0;
    let registry_before = read_pod::<RootRegistry>(&svm, root_registry_address);
    let vault_lamports_before = account_lamports(&svm, vault_pda());

    // The original owner and a stranger both fail.
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), AIRDROP_LAMPORTS).unwrap();
    for signer in [&payer, &stranger] {
        let failure = send(&mut svm, signer, &[initialize_ix(signer.pubkey())])
            .err()
            .unwrap_or_else(|| panic!("second initialize by {} must fail", signer.pubkey()));
        // The system program refuses to allocate a PDA that already exists.
        assert!(
            failure.meta.logs.iter().any(|log| log.contains("already in use")),
            "expected the system program to reject the existing PDA, logs:\n{}",
            failure.meta.logs.join("\n")
        );
    }

    let registry_after = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(registry_after.imt.next_leaf_idx.get(), 1);
    assert_eq!(registry_after.imt.root, registry_before.imt.root);
    assert_eq!(registry_after.imt.frontiers, registry_before.imt.frontiers);
    assert_eq!(registry_after.last_root_idx.get(), 1);
    assert_eq!(registry_after.roots_history, registry_before.roots_history);
    assert_eq!(account_lamports(&svm, vault_pda()), vault_lamports_before);

    let program_config = read_pod::<ProgramConfig>(&svm, program_config_pda());
    assert_eq!(program_config.owner, payer.pubkey());
}

#[test]
fn deposit_sol_happy_path() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let vault_address = vault_pda();
    let root_registry_address = root_registry_pda().0;
    let payer_lamports_before = account_lamports(&svm, payer.pubkey());
    let vault_lamports_before = account_lamports(&svm, vault_address);

    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();
    let deposit_commitment_hash =
        poseidon_hash::hash2(user_commitment_hash, u64_to_32bytes_le(DEPOSIT_LAMPORTS)).unwrap();

    let mut expected_imt = ImtTree::new().unwrap();
    let expected_root = expected_imt.insert(deposit_commitment_hash).unwrap();

    let deposit_meta = send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, DEPOSIT_LAMPORTS),
    );

    assert_eq!(
        account_lamports(&svm, vault_address),
        vault_lamports_before + DEPOSIT_LAMPORTS
    );
    assert_eq!(
        account_lamports(&svm, payer.pubkey()),
        payer_lamports_before - DEPOSIT_LAMPORTS - deposit_meta.fee
    );

    let root_registry = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(root_registry.imt.next_leaf_idx.get(), 1);
    assert_eq!(root_registry.imt.root, expected_root);
    assert_eq!(root_registry.last_root_idx.get(), 1);
    assert_eq!(
        get_array_element(&root_registry.roots_history, 1),
        expected_root
    );
}

#[test]
fn deposit_zero_lamports_fail_case() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let vault_address = vault_pda();
    let root_registry_address = root_registry_pda().0;
    let payer_lamports_before = account_lamports(&svm, payer.pubkey());
    let vault_lamports_before = account_lamports(&svm, vault_address);
    let root_registry_before = read_pod::<RootRegistry>(&svm, root_registry_address);
    let root_before = root_registry_before.imt.root;

    let result = send(
        &mut svm,
        &payer,
        &[deposit_ix(payer.pubkey(), u64_to_32bytes_le(7), 0)],
    );
    let fee = match &result {
        Err(failure) => failure.meta.fee,
        Ok(_) => 0,
    };
    assert_custom_error(result, DappError::DepositAmountZero);

    assert_eq!(account_lamports(&svm, vault_address), vault_lamports_before);
    let root_registry_after = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(root_registry_after.imt.next_leaf_idx.get(), 0);
    assert_eq!(root_registry_after.imt.root, root_before);
    assert_eq!(root_registry_after.last_root_idx.get(), 0);
    assert_eq!(
        account_lamports(&svm, payer.pubkey()),
        payer_lamports_before - fee
    );
}

#[test]
fn upload_proof_writes_the_slice_into_the_fixed_buffer() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    let proof_mock = vec![1u8, 2, 3, 4];
    let proof_hash = calculate_proof_hash(&proof_mock);
    let (proof_address, proof_bump) = proof_pda(&payer.pubkey(), proof_hash);

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            proof_mock.len() as u16,
            proof_mock.clone(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.bump, proof_bump);
    assert_eq!(stored.proof_current_len.get(), proof_mock.len() as u16);
    assert_eq!(&stored.proof[..proof_mock.len()], proof_mock.as_slice());
    // assert that the rest of the proof is empty (not touched)
    assert!(stored.proof[proof_mock.len()..]
        .iter()
        .all(|byte| *byte == 0));
}

#[test]
fn upload_proof_overwrites_previous_bytes() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    let first_chunk = vec![1u8, 2, 3, 4];
    let second_chunk = vec![9u8, 8];
    // Same account, so both writes use the first chunk's hash.
    let proof_hash = calculate_proof_hash(&first_chunk);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            first_chunk.len() as u16,
            first_chunk,
            proof_hash,
            proof_address,
        ),
    );
    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            second_chunk.len() as u16,
            second_chunk.clone(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_current_len.get(), second_chunk.len() as u16);
    assert_eq!(&stored.proof[..second_chunk.len()], second_chunk.as_slice());
    assert!(stored.proof[second_chunk.len()..]
        .iter()
        .all(|byte| *byte == 0));
}

#[test]
fn upload_proof_rejects_empty_chunk() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    let proof_mock = Vec::<u8>::new();
    let proof_hash = calculate_proof_hash(&proof_mock);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[upload_proof_ix(
                payer.pubkey(),
                0,
                0,
                proof_mock,
                proof_hash,
                proof_address,
            )],
        ),
        DappError::ProofChunkEmpty,
    );
}

#[test]
fn upload_proof_max_proof_length() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    let proof_mock = vec![7u8; 900];
    let proof_hash = calculate_proof_hash(&proof_mock);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            proof_mock.len() as u16,
            proof_mock.clone(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_current_len.get(), proof_mock.len() as u16);
}

#[test]
fn upload_proof_append_second_part_after_first() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    let part_0 = vec![0x11u8; 800];
    let part_1 = vec![0x22u8; 464];
    let mut full_proof = part_0.clone();
    full_proof.extend_from_slice(&part_1);
    let proof_hash = calculate_proof_hash(&full_proof);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            part_0.len() as u16,
            part_0.clone(),
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
            part_1.len() as u16,
            part_1.clone(),
            proof_hash,
            proof_address,
        ),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_current_len.get(), full_proof.len() as u16);
    assert_eq!(&stored.proof[..part_0.len()], part_0.as_slice());
    assert_eq!(
        &stored.proof[part_0.len()..full_proof.len()],
        part_1.as_slice()
    );
    // assert that the rest of the proof account is empty (not touched)
    assert!(stored.proof[full_proof.len()..]
        .iter()
        .all(|byte| *byte == 0));
}

/// The checked-in proof is valid, but nothing was deposited into this pool. Its root was
/// never recorded here, so the withdrawal must be refused before the proof is even checked.
/// The positive path lives in `test_withdraw_with_root_calc.rs`, which makes the deposit first.
#[test]
fn withdraw_without_the_matching_deposit_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let proof_hash = upload_fixture_proof(&mut svm, &payer);

    // Everything else is valid: the fixture proof, its public inputs and its real recipient.
    let public_inputs = public_inputs_from_fixture();
    let result = send(
        &mut svm,
        &payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(
                payer.pubkey(),
                FIXTURE_RECIPIENT,
                public_inputs,
                proof_hash,
            ),
        ],
    );
    assert_custom_error(result, DappError::UnknownRoot);
    // Unknown root fails before any marker is kept. The `init` nullifier is rolled back.
    let (nullifier_address, _) = nullifier_pda(&public_inputs.nullifier);
    assert!(
        svm.get_account(&nullifier_address).is_none(),
        "failed withdraw must not leave a spent marker at {nullifier_address}"
    );
}

#[test]
fn owner_can_pause_and_unpause() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    send_ok(&mut svm, &payer, pause_ix(payer.pubkey()));
    let paused = read_pod::<ProgramConfig>(&svm, program_config_pda());
    assert!(paused.pause.get());
    assert_eq!(paused.owner, payer.pubkey());

    send_ok(&mut svm, &payer, unpause_ix(payer.pubkey()));
    let unpaused = read_pod::<ProgramConfig>(&svm, program_config_pda());
    assert!(!unpaused.pause.get());
}

#[test]
fn non_owner_cannot_pause() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), AIRDROP_LAMPORTS).unwrap();

    assert_custom_error(
        send(&mut svm, &stranger, &[pause_ix(stranger.pubkey())]),
        DappError::Unauthorized,
    );
    assert!(!read_pod::<ProgramConfig>(&svm, program_config_pda()).pause.get());
}

#[test]
fn paused_program_rejects_other_instructions_until_unpause() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    send_ok(&mut svm, &payer, pause_ix(payer.pubkey()));

    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();
    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[deposit_ix(
                payer.pubkey(),
                user_commitment_hash,
                DEPOSIT_LAMPORTS,
            )],
        ),
        DappError::ProgramPaused,
    );

    let proof_mock = vec![1u8, 2, 3, 4];
    let proof_hash = calculate_proof_hash(&proof_mock);
    let proof_address = proof_pda(&payer.pubkey(), proof_hash).0;
    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[upload_proof_ix(
                payer.pubkey(),
                0,
                proof_mock.len() as u16,
                proof_mock.clone(),
                proof_hash,
                proof_address,
            )],
        ),
        DappError::ProgramPaused,
    );

    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[withdraw_ix(
                payer.pubkey(),
                FIXTURE_RECIPIENT,
                public_inputs_from_fixture(),
                [9u8; 32],
            )],
        ),
        DappError::ProgramPaused,
    );

    send_ok(&mut svm, &payer, unpause_ix(payer.pubkey()));
    send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, DEPOSIT_LAMPORTS),
    );
    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(
            payer.pubkey(),
            0,
            proof_mock.len() as u16,
            proof_mock,
            proof_hash,
            proof_address,
        ),
    );
}

#[test]
fn deposit_below_minimum_fails() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let vault_address = vault_pda();
    let root_registry_address = root_registry_pda().0;
    let vault_lamports_before = account_lamports(&svm, vault_address);
    let root_before = read_pod::<RootRegistry>(&svm, root_registry_address).imt.root;

    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();
    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[deposit_ix(
                payer.pubkey(),
                user_commitment_hash,
                DEFAULT_MIN_DEPOSIT_LAMPORTS - 1,
            )],
        ),
        DappError::DepositBelowMinimum,
    );

    // Nothing moved and no leaf was inserted.
    assert_eq!(account_lamports(&svm, vault_address), vault_lamports_before);
    let root_registry_after = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(root_registry_after.imt.next_leaf_idx.get(), 0);
    assert_eq!(root_registry_after.imt.root, root_before);

    // Exactly the minimum is accepted.
    send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, DEFAULT_MIN_DEPOSIT_LAMPORTS),
    );
    assert_eq!(
        account_lamports(&svm, vault_address),
        vault_lamports_before + DEFAULT_MIN_DEPOSIT_LAMPORTS
    );
    assert_eq!(
        read_pod::<RootRegistry>(&svm, root_registry_address).imt.next_leaf_idx.get(),
        1
    );
}

#[test]
fn owner_can_update_min_deposit_and_deposit_follows_it() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let new_minimum = 2 * DEFAULT_MIN_DEPOSIT_LAMPORTS;
    send_ok(&mut svm, &payer, update_min_deposit_ix(payer.pubkey(), new_minimum));

    let program_config = read_pod::<ProgramConfig>(&svm, program_config_pda());
    assert_eq!(program_config.min_deposit_lamports.get(), new_minimum);
    // The rest of the config is untouched.
    assert_eq!(program_config.owner, payer.pubkey());
    assert!(!program_config.pause.get());

    // The old default is now too small, the new minimum passes.
    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();
    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[deposit_ix(
                payer.pubkey(),
                user_commitment_hash,
                DEFAULT_MIN_DEPOSIT_LAMPORTS,
            )],
        ),
        DappError::DepositBelowMinimum,
    );
    send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, new_minimum),
    );

    // Lowering to zero leaves only the zero-amount check.
    send_ok(&mut svm, &payer, update_min_deposit_ix(payer.pubkey(), 0));
    send_ok(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), user_commitment_hash, 1),
    );
    assert_custom_error(
        send(
            &mut svm,
            &payer,
            &[deposit_ix(payer.pubkey(), user_commitment_hash, 0)],
        ),
        DappError::DepositAmountZero,
    );
}

#[test]
fn non_owner_cannot_update_min_deposit() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), AIRDROP_LAMPORTS).unwrap();

    assert_custom_error(
        send(&mut svm, &stranger, &[update_min_deposit_ix(stranger.pubkey(), 1)]),
        DappError::Unauthorized,
    );
    assert_eq!(
        read_pod::<ProgramConfig>(&svm, program_config_pda()).min_deposit_lamports.get(),
        DEFAULT_MIN_DEPOSIT_LAMPORTS
    );
}

#[test]
fn min_deposit_can_be_updated_while_paused() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));
    send_ok(&mut svm, &payer, pause_ix(payer.pubkey()));

    send_ok(&mut svm, &payer, update_min_deposit_ix(payer.pubkey(), 1));
    assert_eq!(
        read_pod::<ProgramConfig>(&svm, program_config_pda()).min_deposit_lamports.get(),
        1
    );
}
