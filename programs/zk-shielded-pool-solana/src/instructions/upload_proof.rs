use anchor_lang::prelude::*;

use crate::{
    state::proof_storage::{ProofStorage, PROOF_BUFFER_LEN},
    utils::errors::DappError,
};

#[derive(Accounts)]
#[instruction(_proof_hash: [u8; 32])]
pub struct UploadProof {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        init_if_needed,
        payer = sender,
        seeds = [b"proof_storage", sender.address().as_ref(), _proof_hash.as_ref()],
        bump,
    )]
    pub proof_account: Account<ProofStorage>,

    pub system_program: Program<System>,
}

pub fn handle(
    ctx: &mut Context<UploadProof>,
    proof_final_len: u16,
    part: u8,
    proof: &[u8],
) -> Result<()> {
    if proof.is_empty() {
        return Err(DappError::ProofChunkEmpty.into());
    }

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
    proof_account.proof_current_len = PodU16::from(buffer_end as u16);
    Ok(())
}
