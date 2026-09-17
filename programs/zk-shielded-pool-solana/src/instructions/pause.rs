use anchor_lang::prelude::*;

use crate::{state::program_config::ProgramConfig, utils::errors::DappError};

/// Shared accounts for pause and unpause. Only the owner stored in
/// `program_config` can call either instruction.
#[derive(Accounts)]
pub struct PauseUnpause {
    #[account(address = program_config.owner @ DappError::Unauthorized)]
    pub owner: Signer,

    #[account(
        mut,
        seeds = [ProgramConfig::SEED_PREFIX],
        bump,
    )]
    pub program_config: Account<ProgramConfig>,
}

pub fn handle_pause(ctx: &mut Context<PauseUnpause>) -> Result<()> {
    ctx.accounts.program_config.pause = true.into();
    msg!("Program paused");
    Ok(())
}

pub fn handle_unpause(ctx: &mut Context<PauseUnpause>) -> Result<()> {
    ctx.accounts.program_config.pause = false.into();
    msg!("Program unpaused");
    Ok(())
}
