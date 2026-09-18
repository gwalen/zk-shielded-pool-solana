use anchor_lang::prelude::*;

use crate::state::proof_storage::ProofStorage;

/// Closes a proof account and returns its rent to the sender who created it.
/// The PDA is seeded with the sender address, so only the uploader can close it.
/// Never touches nullifiers: spent markers stay, so a closed proof cannot be replayed.
/// Allowed while paused, because it only returns the sender's own rent.
#[derive(Accounts)]
#[instruction(_proof_hash: [u8; 32])]
pub struct CloseProof {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        mut,
        seeds = [ProofStorage::SEED_PREFIX, sender.address().as_ref(), _proof_hash.as_ref()],
        bump = proof_account.bump,
        close = sender,
    )]
    pub proof_account: Account<ProofStorage>,
}

pub fn handle(_ctx: &mut Context<CloseProof>) -> Result<()> {
    msg!("Proof account closed");
    Ok(())
}
