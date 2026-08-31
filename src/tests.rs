use {
    crate::{
        cpi::{
            DepositInstruction, HelloInstruction, InitializeInstruction, UploadProofInstruction,
        },
        state::proof_storage::ProofStorage,
        state::root_registry::RootRegistry,
        state::vault::Vault,
        utils::{
            constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
            errors::DappError,
            flatten_array::get_array_element,
            imt_tree::{u64_to_32bytes_le, ImtTree, ImtTreeZc},
            poseidon_hash,
        },
    },
    quasar_lang::client::DynVec,
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

/**
 * Each quasar test get fresh accounts newly deployed program.
 * State is not persisted between tests.
 * Program deployment happens at the beginning of each test (implicitly).
 */

#[quasar_test]
fn hello_logs_the_greeting(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    println!("program id: {}", test.program_id());

    let outcome = test.send(HelloInstruction { payer: PAYER });
    outcome.succeeds();

    // The program only logs; assert it emitted its greeting, not just that
    // the transaction succeeded.
    let logs = outcome.logs().join("\n");
    println!("my logs: {logs}");
    assert!(
        logs.contains("Hello, Solana!"),
        "expected the program to log its greeting, got:\n{logs}"
    );
    // BytesEvent discriminator 8, 64 0xAA bytes, then 1u64 LE.
    const BYTES_EVENT_LOG: &str = "Program data: CKqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqoBAAAAAAAAAA==";
    assert!(
        logs.contains(BYTES_EVENT_LOG),
        "expected BytesEvent payload in logs, got:\n{logs}"
    );
}

#[quasar_test]
fn initialize_writes_root_registry_in_place(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let (root_registry_address, root_registry_bump) =
        test.derive_pda_with_bump(RootRegistry::seeds());

    test.send(InitializeInstruction { signer: PAYER })
        .succeeds();

    let expected_imt = ImtTree::new().unwrap();
    let root_registry = test.read::<RootRegistry>(root_registry_address);

    assert_eq!(root_registry.imt.root, expected_imt.root);
    assert_eq!(root_registry.imt.frontiers, expected_imt.frontiers);
    assert_eq!(root_registry.imt.zero_values, expected_imt.zero_values);
    assert_eq!(root_registry.imt.next_leaf_idx, 0);
    assert_eq!(root_registry.last_root_idx, 0);
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
}

#[quasar_test]
fn deposit_sol_happy_path(test: &mut Test) {
    const DEPOSIT_LAMPORTS: u64 = 1_250_000_000; // 1.25 SOL

    test.add(Wallet::new().at(PAYER)); // Funds the wallet with 10 SOL by default
    test.send(InitializeInstruction { signer: PAYER })
        .succeeds();

    let vault_address = test.derive_pda(Vault::seeds());
    let root_registry_address = test.derive_pda(RootRegistry::seeds());
    let payer_lamports_before = test.lamports(PAYER);
    // println!("XXX payer_lamports_before: {payer_lamports_before}");
    let vault_lamports_before = test.lamports(vault_address);

    let user_commitment_hash = poseidon_hash::hash2([3u8; 32], [4u8; 32]).unwrap();  // tests values
    let deposit_commitment_hash =
        poseidon_hash::hash2(user_commitment_hash, u64_to_32bytes_le(DEPOSIT_LAMPORTS)).unwrap();

    let mut expected_imt: ImtTreeZc = ImtTree::new().unwrap().into();
    let expected_root = expected_imt.insert(deposit_commitment_hash).unwrap();

    test.send(DepositInstruction {
        sender: PAYER,
        user_commitment_hash,
        total_amount: DEPOSIT_LAMPORTS,
    })
    .succeeds()
    .has_lamports(PAYER, payer_lamports_before - DEPOSIT_LAMPORTS)
    .has_lamports(vault_address, vault_lamports_before + DEPOSIT_LAMPORTS);

    let root_registry = test.read::<RootRegistry>(root_registry_address);
    assert_eq!(root_registry.imt.next_leaf_idx.get(), 1);
    assert_eq!(root_registry.imt.root, expected_root);
    assert_eq!(root_registry.last_root_idx.get(), 1);
    assert_eq!(
        get_array_element(&root_registry.roots_history, 1),
        expected_root
    );
}

#[quasar_test]
fn deposit_zero_lamports_fail_case(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    test.send(InitializeInstruction { signer: PAYER })
        .succeeds();

    let vault_address = test.derive_pda(Vault::seeds());
    let root_registry_address = test.derive_pda(RootRegistry::seeds());
    let payer_lamports_before = test.lamports(PAYER);
    let vault_lamports_before = test.lamports(vault_address);
    let root_registry_before = test.read::<RootRegistry>(root_registry_address);
    let root_before = root_registry_before.imt.root;

    test.send(DepositInstruction {
        sender: PAYER,
        user_commitment_hash: u64_to_32bytes_le(7),
        total_amount: 0,
    })
    .fails_with(DappError::DepositAmountZero);

    assert_eq!(test.lamports(PAYER), payer_lamports_before);
    assert_eq!(test.lamports(vault_address), vault_lamports_before);
    let root_registry_after = test.read::<RootRegistry>(root_registry_address);
    assert_eq!(root_registry_after.imt.next_leaf_idx.get(), 0);
    assert_eq!(root_registry_after.imt.root, root_before);
    assert_eq!(root_registry_after.last_root_idx.get(), 0);
}

const PROOF_HASH: u64 = 7;

#[quasar_test]
fn upload_proof_writes_the_slice_into_the_fixed_buffer(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    let (proof_address, proof_bump) =
        test.derive_pda_with_bump(ProofStorage::seeds(&PAYER, PROOF_HASH));

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(vec![1u8, 2, 3, 4]),
    })
    .succeeds();

    let stored = test.read::<ProofStorage>(proof_address);
    assert_eq!(stored.bump, proof_bump);
    assert_eq!(stored.proof_len.get(), 4);
    assert_eq!(&stored.proof[..4], &[1u8, 2, 3, 4]);
    assert!(stored.proof[4..].iter().all(|byte| *byte == 0));
}

#[quasar_test]
fn upload_proof_overwrites_previous_bytes(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let proof_address = test.derive_pda(ProofStorage::seeds(&PAYER, PROOF_HASH));

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(vec![1u8, 2, 3, 4]),
    })
    .succeeds();

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(vec![9u8, 8]),
    })
    .succeeds();

    let stored = test.read::<ProofStorage>(proof_address);
    assert_eq!(stored.proof_len.get(), 2);
    assert_eq!(&stored.proof[..2], &[9u8, 8]);
    assert!(stored.proof[2..].iter().all(|byte| *byte == 0));
}

#[quasar_test]
fn upload_proof_rejects_empty_chunk(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(vec![]),
    })
    .fails_with(DappError::ProofChunkEmpty);
}

#[quasar_test]
fn upload_proof_max_proof_length(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let proof_address = test.derive_pda(ProofStorage::seeds(&PAYER, PROOF_HASH));

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(vec![7u8; 900]),
    })
    .succeeds();

    let stored = test.read::<ProofStorage>(proof_address);
    assert_eq!(stored.proof_len.get(), 900);
}

#[quasar_test]
fn upload_proof_appends_second_part(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let proof_address = test.derive_pda(ProofStorage::seeds(&PAYER, PROOF_HASH));

    let part_0 = vec![0x11u8; 800];
    let part_1 = vec![0x22u8; 464];

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 0,
        proof: DynVec::<u8, u16>::new(part_0.clone()),
    })
    .succeeds();

    test.send(UploadProofInstruction {
        sender: PAYER,
        proof_hash: PROOF_HASH,
        part: 1,
        proof: DynVec::<u8, u16>::new(part_1.clone()),
    })
    .succeeds();

    let stored = test.read::<ProofStorage>(proof_address);
    assert_eq!(stored.proof_len.get(), 1264);
    assert_eq!(&stored.proof[..800], part_0.as_slice());
    assert_eq!(&stored.proof[800..1264], part_1.as_slice());
    assert!(stored.proof[1264..].iter().all(|byte| *byte == 0));
}
