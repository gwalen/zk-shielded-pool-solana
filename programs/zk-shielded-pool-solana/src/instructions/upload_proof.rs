use anchor_lang::prelude::*;

use crate::{
    state::{
        program_config::ProgramConfig,
        proof_storage::{ProofStorage, PROOF_BUFFER_LEN},
    },
    utils::{
        errors::DappError,
        events::{FullProofUploaded, PartialProofUploaded},
    },
};

#[derive(Accounts)]
#[instruction(_proof_hash: [u8; 32])]
pub struct UploadProof {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        seeds = [ProgramConfig::SEED_PREFIX],
        bump,
        constraint = !program_config.pause.get() @ DappError::ProgramPaused,
    )]
    pub program_config: Account<ProgramConfig>,

    #[account(
        init_if_needed,
        payer = sender,
        seeds = [ProofStorage::SEED_PREFIX, sender.address().as_ref(), _proof_hash.as_ref()],
        bump,
    )]
    pub proof_account: Account<ProofStorage>,

    pub system_program: Program<System>,
}

pub fn handle(
    ctx: &mut Context<UploadProof>,
    proof_hash: [u8; 32],
    proof_final_len: u16,
    part: u8,
    proof: &[u8],
) -> Result<()> {
    if proof.is_empty() {
        return Err(DappError::ProofChunkEmpty.into());
    }

    let sender = *ctx.accounts.sender.address();
    let proof_account = &mut ctx.accounts.proof_account;
    // we overidde the final length of the proof each time we upload a chunk
    // this is not perfect but will work for MVP
    proof_account.proof_final_len = PodU16::from(proof_final_len);

    let buffer_start = if part == 0 {
        0
    } else {
        proof_account.proof_current_len.get() as usize
    };
    let buffer_end = buffer_start + proof.len();

    if buffer_end > PROOF_BUFFER_LEN {
        return Err(DappError::ProofBufferFull.into());
    }

    proof_account.bump = ctx.bumps.proof_account;
    proof_account.proof[buffer_start..buffer_end].copy_from_slice(proof);
    proof_account.proof[buffer_end..].fill(0);
    let curr_len = buffer_end as u16;
    proof_account.proof_current_len = PodU16::from(curr_len);

    if proof_account.proof_final_len.get() == curr_len {
        emit!(FullProofUploaded {
            sender,
            proof_hash: Address::from(proof_hash),
            proof_len: curr_len,
        });
    } else {
        emit!(PartialProofUploaded {
            sender,
            proof_hash: Address::from(proof_hash),
            proof_len: curr_len,
        });
    }

    Ok(())
}
