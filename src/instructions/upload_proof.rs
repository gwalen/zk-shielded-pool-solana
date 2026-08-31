use quasar_lang::prelude::*;

use crate::{
    state::proof_storage::{ProofStorage, PROOF_BUFFER_LEN},
    utils::errors::DappError,
};

#[derive(Accounts)]
#[instruction(proof_hash: u64)]
pub struct UploadProof {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        mut,
        init(idempotent),
        payer = sender,
        address = ProofStorage::seeds(sender.address(), proof_hash),
    )]
    pub proof_account: Account<ProofStorage>,

    pub system_program: Program<SystemProgram>,
}

pub fn handle(ctx: &mut Ctx<UploadProof>, part: u8, proof: &[u8]) -> Result<(), ProgramError> {
    if proof.is_empty() {
        return Err(DappError::ProofChunkEmpty.into());
    }

    let proof_account = &mut ctx.accounts.proof_account;

    let buffer_start = if part == 0 { 0 } else { proof_account.proof_len.get() as usize };
    let buffer_end = buffer_start + proof.len();

    if buffer_end > PROOF_BUFFER_LEN {
        return Err(DappError::ProofBufferFull.into());
    }

    proof_account.bump = ctx.bumps.proof_account;
    proof_account.proof[buffer_start..buffer_end].copy_from_slice(proof);
    proof_account.proof[buffer_end..].fill(0);
    proof_account.proof_len = PodU16::from(buffer_end as u16);
    Ok(())
}
