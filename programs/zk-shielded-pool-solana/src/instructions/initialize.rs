use anchor_lang::prelude::*;

use crate::{
    state::{program_config::ProgramConfig, root_registry::RootRegistry, vault::Vault},
    utils::constants::DEFAULT_MIN_DEPOSIT_LAMPORTS,
};

/// Accounts for the initialize instruction.
/// Creates the vault and root-registry PDAs and fills the empty Merkle tree.
#[derive(Accounts)]
pub struct Initialize {
    #[account(mut)]
    pub signer: Signer,

    #[account(
        init,
        payer = signer,
        seeds = [ProgramConfig::SEED_PREFIX],
        bump,
    )]
    pub program_config: Account<ProgramConfig>,

    #[account(
        init,
        payer = signer,
        seeds = [Vault::SEED_PREFIX],
        bump,
    )]
    pub vault: Account<Vault>,

    #[account(
        init,
        payer = signer,
        seeds = [RootRegistry::SEED_PREFIX],
        bump,
    )]
    pub root_registry: Account<RootRegistry>,

    pub system_program: Program<System>,
}

// we don't [inline] this function to keep the handler stack separate from instruction entrypoint function
pub fn handle(ctx: &mut Context<Initialize>) -> Result<()> {
    msg!("Initializing Shielded Pool Program");

    let program_config = &mut ctx.accounts.program_config;
    let root_registry = &mut ctx.accounts.root_registry;

    ctx.accounts.vault.bump = ctx.bumps.vault;

    **program_config = ProgramConfig::new(
        *ctx.accounts.signer.address(),
        false,
        DEFAULT_MIN_DEPOSIT_LAMPORTS,
    );

    // Fills the empty tree and the root ring buffer in place, with no stack allocation.
    root_registry.initialize_empty()?;
    root_registry.bump = ctx.bumps.root_registry;

    Ok(())
}
