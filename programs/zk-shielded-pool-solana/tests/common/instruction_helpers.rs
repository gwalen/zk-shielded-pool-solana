use {
    anchor_lang::{
        prelude::{Address, System},
        solana_program::instruction::Instruction,
        Id,
    },
    zk_shielded_pool_solana::{
        accounts, instruction,
        state::{
            nullifier::Nullifier, program_config::ProgramConfig, proof_storage::ProofStorage,
            root_registry::RootRegistry, vault::Vault,
        },
        utils::public_inputs::PublicInputs,
    },
};
use super::constants::*;


pub fn program_config_pda() -> Address {
    find_pda(&[ProgramConfig::SEED_PREFIX]).0
}

pub fn vault_pda() -> Address {
    find_pda(&[Vault::SEED_PREFIX]).0
}

pub fn root_registry_pda() -> (Address, u8) {
    find_pda(&[RootRegistry::SEED_PREFIX])
}

pub fn proof_pda(sender: &Address, proof_hash: [u8; 32]) -> (Address, u8) {
    find_pda(&[ProofStorage::SEED_PREFIX, sender.as_ref(), proof_hash.as_ref()])
}

pub fn nullifier_pda(nullifier_be: &[u8; 32]) -> (Address, u8) {
    find_pda(&[Nullifier::SEED_PREFIX, nullifier_be.as_ref()])
}

pub fn find_pda(seeds: &[&[u8]]) -> (Address, u8) {
    Address::find_program_address(seeds, &zk_shielded_pool_solana::id())
}

pub fn pause_ix(owner: Address) -> Instruction {
    instruction::Pause {}.to_instruction(accounts::PauseUnpause {
        owner,
        program_config: program_config_pda(),
    })
}

pub fn unpause_ix(owner: Address) -> Instruction {
    instruction::Unpause {}.to_instruction(accounts::PauseUnpause {
        owner,
        program_config: program_config_pda(),
    })
}

pub fn initialize_ix(signer: Address) -> Instruction {
    instruction::Initialize {}.to_instruction(accounts::Initialize {
        signer,
        program_config: program_config_pda(),
        vault: vault_pda(),
        root_registry: root_registry_pda().0,
        system_program: System::id(),
    })
}

pub fn deposit_ix(sender: Address, user_commitment_hash: [u8; 32], total_amount: u64) -> Instruction {
    instruction::Deposit {
        user_commitment_hash,
        total_amount,
    }
    .to_instruction(accounts::Deposit {
        sender,
        program_config: program_config_pda(),
        vault: vault_pda(),
        roots_registry: root_registry_pda().0,
        system_program: System::id(),
    })
}

pub fn upload_proof_ix(
    sender: Address,
    part: u8,
    proof_final_len: u16,
    proof_part: Vec<u8>,
    proof_hash: [u8; 32],
    proof_pda: Address,
) -> Instruction {
    instruction::UploadProof {
        _proof_hash: proof_hash,
        part,
        proof_final_len,
        proof: proof_part,
    }
    .to_instruction(accounts::UploadProof {
        sender,
        program_config: program_config_pda(),
        proof_account: proof_pda,
        system_program: System::id(),
    })
}

pub fn withdraw_ix(
    sender: Address,
    recipient: Address,
    public_inputs: PublicInputs,
    proof_hash: [u8; 32],
) -> Instruction {
    // Nullifier PDA uses the same big-endian bytes the program seeds with.
    // Copy them first so the struct can still be moved into the instruction.
    let nullifier_bytes = public_inputs.nullifier;
    let nullifier_address = nullifier_pda(&nullifier_bytes).0;
    instruction::Withdraw {
        proof_hash,
        public_inputs,
    }
    .to_instruction(accounts::Withdraw {
        sender,
        program_config: program_config_pda(),
        vault: vault_pda(),
        roots_registry: root_registry_pda().0,
        proof_account: proof_pda(&sender, proof_hash).0,
        recipient,
        nullifier_account: nullifier_address,
        system_program: System::id(),
    })
}

pub fn compute_budget_ix(discriminator: u8, value: u32) -> Instruction {
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

pub fn set_compute_unit_limit_ix(units: u32) -> Instruction {
    compute_budget_ix(2, units)
}

pub fn request_heap_frame_ix(bytes: u32) -> Instruction {
    compute_budget_ix(1, bytes)
}