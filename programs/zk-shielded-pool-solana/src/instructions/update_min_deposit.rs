use anchor_lang::prelude::*;

use crate::{state::program_config::ProgramConfig, utils::errors::DappError};

/// Only the owner stored in `program_config` can change the minimum deposit.
/// Allowed while paused, so the owner can retune the pool before reopening it.
#[derive(Accounts)]
pub struct UpdateMinDeposit {
    #[account(
        address = program_config.owner @ DappError::Unauthorized
    )]
    pub owner: Signer,

    #[account(
        mut,
        seeds = [ProgramConfig::SEED_PREFIX],
        bump,
    )]
    pub program_config: Account<ProgramConfig>,
}

pub fn handle(ctx: &mut Context<UpdateMinDeposit>, min_deposit_lamports: u64) -> Result<()> {
    ctx.accounts.program_config.min_deposit_lamports = PodU64::from(min_deposit_lamports);
    msg!("Minimum deposit set to {} lamports", min_deposit_lamports);
    Ok(())
}
