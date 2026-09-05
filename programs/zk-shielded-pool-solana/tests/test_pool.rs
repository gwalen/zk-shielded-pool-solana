use {
    anchor_lang::{
        bytemuck,
        prelude::{Address, System},
        solana_program::instruction::Instruction,
        Discriminator, Id,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer, VersionedTransaction},
    litesvm::types::{FailedTransactionMetadata, TransactionMetadata},
    solana_message::{v0, VersionedMessage},
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

mod common;
use common::utils::calculate_proof_hash;

/// Default first `#[error_code]` value. Matches Anchor v2's offset.
const ANCHOR_V2_ERROR_CODE_OFFSET: u32 = 6000;

/// 10 SOL covers a 1.25 SOL deposit plus rent for the vault and root registry.
const AIRDROP_LAMPORTS: u64 = 10_000_000_000;

const DEPOSIT_LAMPORTS: u64 = 1_250_000_000;

/// Checked-in GWC proof (`solana-proof-generator/fixtures/proof.bin`).
const CHECKED_IN_PROOF_LEN: usize = 1088;
/// First `upload_proof` chunk. One instruction has about 971 bytes leftover after headers.
const PROOF_UPLOAD_PART0_LEN: usize = 800;
/// Five 32-byte public inputs (`solana-proof-generator/fixtures/public_inputs.bin`).
const CHECKED_IN_PUBLIC_INPUTS_LEN: usize = 160;
const PUBLIC_INPUT_COUNT: usize = 5;
/// Same CU cap the Mollusk verifier harness uses (`SOLANA_TRANSACTION_CU_LIMIT`).
const VERIFY_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;
/// Same heap the Mollusk verifier harness uses. Default 32 KiB overflows in `verify_gwc`.
const VERIFY_HEAP_FRAME_BYTES: u32 = 64 * 1024;
const COMPUTE_BUDGET_PROGRAM_ID: Address =
    anchor_lang::address!("ComputeBudget111111111111111111111111111111");
// This is agave_feature_set::enable_big_mod_exp_syscall::ID. LiteSVM 0.13.1's
// mainnet snapshot does not include it yet.
const ENABLE_BIG_MOD_EXP_SYSCALL_ID: Address =
    anchor_lang::address!("EBq48m8irRKuE7ZnMTLvLg2UuGSqhe8s8oMqnmja1fJw");

fn program_id() -> Address {
    zk_shielded_pool_solana::id()
}

fn setup() -> (LiteSVM, Keypair) {
    let mut feature_set = LiteSVM::mainnet_feature_set();
    feature_set.activate(&ENABLE_BIG_MOD_EXP_SYSCALL_ID, 0);

    // Set the feature before rebuilding the runtime. That puts
    // sol_big_mod_exp in the syscall table used by the loaded program.
    // `svm()` is LiteSVM::new(), plus tracing when `--features profile` is on.
    let mut svm = anchor_v2_testing::svm()
        .with_feature_set(feature_set)
        .with_builtins();
    let zk_shieleded_pool_binary =
        include_bytes!("../../../target/deploy/zk_shielded_pool_solana.so");
    svm.add_program(program_id(), zk_shieleded_pool_binary)
        .unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), AIRDROP_LAMPORTS).unwrap();
    (svm, payer)
}

#[allow(clippy::result_large_err)]
fn send(
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

fn send_ok(svm: &mut LiteSVM, payer: &Keypair, instruction: Instruction) -> TransactionMetadata {
    send_ok_many(svm, payer, &[instruction])
}

fn send_ok_many(
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

fn dapp_error_code(error: DappError) -> u32 {
    error as u32 + ANCHOR_V2_ERROR_CODE_OFFSET
}

fn assert_custom_error(
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

fn account_lamports(svm: &LiteSVM, address: Address) -> u64 {
    svm.get_account(&address)
        .map(|account| account.lamports)
        .unwrap_or(0)
}

fn read_pod<T: Discriminator + bytemuck::Pod>(svm: &LiteSVM, address: Address) -> T {
    let account = svm.get_account(&address).expect("account missing");
    let disc_len = T::DISCRIMINATOR.len();
    // skip discriminator and read the rest of the data
    let payload = &account.data[disc_len..disc_len + core::mem::size_of::<T>()];
    // from_bytes gives &T, so we copy and dereference it to get T (T is Copy)
    *bytemuck::from_bytes(payload)
}

fn vault_pda() -> Address {
    find_pda(&[b"vault"]).0
}

fn root_registry_pda() -> (Address, u8) {
    find_pda(&[b"root_registry"])
}

fn proof_pda(sender: &Address, proof_hash: [u8; 32]) -> (Address, u8) {
    find_pda(&[b"proof_storage", sender.as_ref(), proof_hash.as_ref()])
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

fn upload_proof_ix(
    sender: Address,
    part: u8,
    proof_final_len: u16,
    proof_part: Vec<u8>,
    proof_hash: [u8; 32],
    proof_pda: Address,
) -> Instruction {
    instruction::UploadProof {
        proof_hash,
        part,
        proof_final_len,
        proof: proof_part,
    }
    .to_instruction(accounts::UploadProof {
        sender,
        proof_account: proof_pda,
        system_program: System::id(),
    })
}

fn withdraw_ix(sender: Address, public_inputs: [[u8; 32]; 5], proof_hash: [u8; 32]) -> Instruction {
    instruction::Withdraw {
        proof_hash,
        public_inputs,
    }
    .to_instruction(accounts::Withdraw {
        sender,
        vault: vault_pda(),
        roots_registry: root_registry_pda().0,
        proof_account: proof_pda(&sender, proof_hash).0,
        system_program: System::id(),
    })
}

fn compute_budget_ix(discriminator: u8, value: u32) -> Instruction {
    // Byte 0 selects the compute-budget operation. Bytes 1..5 contain its
    // u32 value in little-endian order.
    let mut data = Vec::with_capacity(5);
    data.push(discriminator);
    data.extend_from_slice(&value.to_le_bytes());
    Instruction {
        program_id: COMPUTE_BUDGET_PROGRAM_ID,
        accounts: vec![],
        data,
    }
}

fn set_compute_unit_limit_ix(units: u32) -> Instruction {
    compute_budget_ix(2, units)
}

fn request_heap_frame_ix(bytes: u32) -> Instruction {
    compute_budget_ix(1, bytes)
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

#[test]
fn withdraw_accepts_checked_in_proof() {
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
    let mut public_inputs = [[0u8; 32]; PUBLIC_INPUT_COUNT];
    for (dst, chunk) in public_inputs
        .iter_mut()
        .zip(public_inputs_bytes.chunks_exact(32))
    {
        dst.copy_from_slice(chunk);
    }

    let meta = send_ok_many(
        &mut svm,
        &payer,
        &[
            set_compute_unit_limit_ix(VERIFY_COMPUTE_UNIT_LIMIT),
            request_heap_frame_ix(VERIFY_HEAP_FRAME_BYTES),
            withdraw_ix(payer.pubkey(), public_inputs, proof_hash),
        ],
    );
    let logs = meta.logs.join("\n");
    println!("withdraw logs: {logs}");
    assert!(
        logs.contains("Proof verified"),
        "expected the program to log Proof verified, got:\n{logs}"
    );
}
