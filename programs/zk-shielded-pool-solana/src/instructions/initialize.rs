use anchor_lang::prelude::*;

use crate::state::{root_registry::RootRegistry, vault::Vault};

/// Accounts for the initialize instruction.
/// Creates the vault and root-registry PDAs and fills the empty Merkle tree.
#[derive(Accounts)]
pub struct Initialize {
    #[account(mut)]
    pub signer: Signer,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"vault"],
        bump, // TODO: later add explicit bump
    )]
    pub vault: Account<Vault>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"root_registry"],
        bump, // TODO: later add explicit bump
    )]
    pub root_registry: Account<RootRegistry>,

    pub system_program: Program<System>,
}

// TODO: add config account that will store the signer as owner (for procol pausing, have pause flag, and is_init flag)
// we don't [inline] this function to keep the handler stack separate from instruction entrypoint function
pub fn handle(ctx: &mut Context<Initialize>) -> Result<()> {
    msg!("Initializing Shielded Pool Program");
    ctx.accounts.vault.bump = ctx.bumps.vault;

    let root_registry = &mut ctx.accounts.root_registry;

    // Fills the empty tree and the root ring buffer in place, with no stack allocation.
    root_registry.initialize_empty()?;
    root_registry.bump = ctx.bumps.root_registry;

    Ok(())
}
