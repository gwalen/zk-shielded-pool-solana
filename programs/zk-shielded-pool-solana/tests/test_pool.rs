use {
    anchor_lang::{
        bytemuck,
        prelude::{Address, System},
        solana_program::instruction::Instruction,
        Discriminator, Id,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Message, Signer, VersionedMessage, VersionedTransaction},
    zk_shielded_pool_solana::{
        accounts, instruction,
        state::{proof_storage::ProofStorage, root_registry::RootRegistry},
        utils::{
            constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
            errors::DappError,
            flatten_array::get_array_element,
            imt_tree::{u64_to_32bytes_le, ImtTree},
            poseidon_hash,
        },
    },
};

/// Default first `#[error_code]` value. Matches Anchor v2's offset.
const ERROR_CODE_OFFSET: u32 = 6000;

/// 10 SOL covers a 1.25 SOL deposit plus rent for the vault and root registry.
const AIRDROP_LAMPORTS: u64 = 10_000_000_000;

const DEPOSIT_LAMPORTS: u64 = 1_250_000_000;
const PROOF_HASH: u64 = 7;

fn program_id() -> Address {
    zk_shielded_pool_solana::id()
}

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = anchor_v2_testing::svm();
    let bytes = include_bytes!("../../../target/deploy/zk_shielded_pool_solana.so");
    svm.add_program(program_id(), bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), AIRDROP_LAMPORTS).unwrap();
    (svm, payer)
}

fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
) -> Result<
    litesvm::types::TransactionMetadata,
    litesvm::types::FailedTransactionMetadata,
> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
}

fn send_ok(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
) -> litesvm::types::TransactionMetadata {
    send(svm, payer, instruction).unwrap_or_else(|failure| {
        panic!(
            "transaction failed: {:?}\nlogs:\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        )
    })
}

fn dapp_error_code(error: DappError) -> u32 {
    error as u32 + ERROR_CODE_OFFSET
}

fn assert_custom_error(
    result: Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
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

fn lamports(svm: &LiteSVM, address: Address) -> u64 {
    svm.get_account(&address)
        .map(|account| account.lamports)
        .unwrap_or(0)
}

fn read_pod<T: Discriminator + bytemuck::Pod>(svm: &LiteSVM, address: Address) -> T {
    let account = svm.get_account(&address).expect("account missing");
    let disc_len = T::DISCRIMINATOR.len();
    let payload = &account.data[disc_len..disc_len + core::mem::size_of::<T>()];
    *bytemuck::from_bytes(payload)
}

fn vault_pda() -> Address {
    find_pda(&[b"vault"]).0
}

fn root_registry_pda() -> (Address, u8) {
    find_pda(&[b"root_registry"])
}

fn proof_pda(sender: &Address) -> (Address, u8) {
    let proof_hash_bytes = PROOF_HASH.to_le_bytes();
    find_pda(&[b"proof_storage", sender.as_ref(), &proof_hash_bytes])
}

fn find_pda(seeds: &[&[u8]]) -> (Address, u8) {
    Address::find_program_address(seeds, &program_id())
}

fn hello_ix(payer: Address) -> Instruction {
    instruction::Hello {}.to_instruction(accounts::HelloAccountConstraints { payer })
}

fn initialize_ix(signer: Address) -> Instruction {
    instruction::Initialize {}.to_instruction(accounts::Initialize {
        signer,
        vault: vault_pda(),
        root_registry: root_registry_pda().0,
        system_program: System::id(),
    })
}

fn deposit_ix(sender: Address, user_commitment_hash: [u8; 32], total_amount: u64) -> Instruction {
    instruction::Deposit {
        user_commitment_hash,
        total_amount,
    }
    .to_instruction(accounts::Deposit {
        sender,
        vault: vault_pda(),
        roots_registry: root_registry_pda().0,
        system_program: System::id(),
    })
}

fn upload_proof_ix(sender: Address, part: u8, proof: Vec<u8>) -> Instruction {
    instruction::UploadProof {
        proof_hash: PROOF_HASH,
        part,
        proof,
    }
    .to_instruction(accounts::UploadProof {
        sender,
        proof_account: proof_pda(&sender).0,
        system_program: System::id(),
    })
}

#[test]
fn hello_logs_the_greeting() {
    let (mut svm, payer) = setup();
    println!("program id: {}", program_id());

    let meta = send_ok(&mut svm, &payer, hello_ix(payer.pubkey()));
    let logs = meta.logs.join("\n");
    println!("my logs: {logs}");
    assert!(
        logs.contains("Hello, Solana!"),
        "expected the program to log its greeting, got:\n{logs}"
    );
    assert!(
        logs.contains(&program_id().to_string()),
        "expected the program to log its program ID, got:\n{logs}"
    );
}

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
}

#[test]
fn deposit_sol_happy_path() {
    let (mut svm, payer) = setup();
    send_ok(&mut svm, &payer, initialize_ix(payer.pubkey()));

    let vault_address = vault_pda();
    let root_registry_address = root_registry_pda().0;
    let payer_lamports_before = lamports(&svm, payer.pubkey());
    let vault_lamports_before = lamports(&svm, vault_address);

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
        lamports(&svm, vault_address),
        vault_lamports_before + DEPOSIT_LAMPORTS
    );
    assert_eq!(
        lamports(&svm, payer.pubkey()),
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
    let payer_lamports_before = lamports(&svm, payer.pubkey());
    let vault_lamports_before = lamports(&svm, vault_address);
    let root_registry_before = read_pod::<RootRegistry>(&svm, root_registry_address);
    let root_before = root_registry_before.imt.root;

    let result = send(
        &mut svm,
        &payer,
        deposit_ix(payer.pubkey(), u64_to_32bytes_le(7), 0),
    );
    let fee = match &result {
        Err(failure) => failure.meta.fee,
        Ok(_) => 0,
    };
    assert_custom_error(result, DappError::DepositAmountZero);

    assert_eq!(lamports(&svm, vault_address), vault_lamports_before);
    let root_registry_after = read_pod::<RootRegistry>(&svm, root_registry_address);
    assert_eq!(root_registry_after.imt.next_leaf_idx.get(), 0);
    assert_eq!(root_registry_after.imt.root, root_before);
    assert_eq!(root_registry_after.last_root_idx.get(), 0);
    assert_eq!(
        lamports(&svm, payer.pubkey()),
        payer_lamports_before - fee
    );
}

#[test]
fn upload_proof_writes_the_slice_into_the_fixed_buffer() {
    let (mut svm, payer) = setup();
    let (proof_address, proof_bump) = proof_pda(&payer.pubkey());

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 0, vec![1u8, 2, 3, 4]),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.bump, proof_bump);
    assert_eq!(stored.proof_len.get(), 4);
    assert_eq!(&stored.proof[..4], &[1u8, 2, 3, 4]);
    assert!(stored.proof[4..].iter().all(|byte| *byte == 0));
}

#[test]
fn upload_proof_overwrites_previous_bytes() {
    let (mut svm, payer) = setup();
    let proof_address = proof_pda(&payer.pubkey()).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 0, vec![1u8, 2, 3, 4]),
    );
    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 0, vec![9u8, 8]),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_len.get(), 2);
    assert_eq!(&stored.proof[..2], &[9u8, 8]);
    assert!(stored.proof[2..].iter().all(|byte| *byte == 0));
}

#[test]
fn upload_proof_rejects_empty_chunk() {
    let (mut svm, payer) = setup();

    assert_custom_error(
        send(
            &mut svm,
            &payer,
            upload_proof_ix(payer.pubkey(), 0, vec![]),
        ),
        DappError::ProofChunkEmpty,
    );
}

#[test]
fn upload_proof_max_proof_length() {
    let (mut svm, payer) = setup();
    let proof_address = proof_pda(&payer.pubkey()).0;

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 0, vec![7u8; 900]),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_len.get(), 900);
}

#[test]
fn upload_proof_appends_second_part() {
    let (mut svm, payer) = setup();
    let proof_address = proof_pda(&payer.pubkey()).0;

    let part_0 = vec![0x11u8; 800];
    let part_1 = vec![0x22u8; 464];

    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 0, part_0.clone()),
    );
    send_ok(
        &mut svm,
        &payer,
        upload_proof_ix(payer.pubkey(), 1, part_1.clone()),
    );

    let stored = read_pod::<ProofStorage>(&svm, proof_address);
    assert_eq!(stored.proof_len.get(), 1264);
    assert_eq!(&stored.proof[..800], part_0.as_slice());
    assert_eq!(&stored.proof[800..1264], part_1.as_slice());
    assert!(stored.proof[1264..].iter().all(|byte| *byte == 0));
}
