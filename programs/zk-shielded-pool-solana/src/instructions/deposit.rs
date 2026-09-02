use anchor_lang::prelude::*;

use crate::{
    state::{root_registry::RootRegistry, vault::Vault},
    utils::{
        common::is_in_fr_range, errors::DappError, events::DepositDone,
        imt_tree::u64_to_32bytes_le, poseidon_hash,
    },
};

#[derive(Accounts)]
pub struct Deposit {
    #[account(mut)]
    pub sender: Signer,

    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<Vault>,

    #[account(mut, seeds = [b"root_registry"], bump = roots_registry.bump)]
    pub roots_registry: Account<RootRegistry>,

    pub system_program: Program<System>,
}

pub fn handle(
    ctx: &mut Context<Deposit>,
    user_commitment_hash: [u8; 32],
    total_amount: u64,
) -> Result<()> {
    if total_amount == 0 {
        return Err(DappError::DepositAmountZero.into());
    }
    if !is_in_fr_range(&user_commitment_hash) {
        return Err(DappError::PoseidonInputLargerThanModulus.into());
    }
    let roots_registry = &mut ctx.accounts.roots_registry;

    let deposit_commitment_hash =
        poseidon_hash::hash2(user_commitment_hash, u64_to_32bytes_le(total_amount))?;
    let new_root = roots_registry.insert(deposit_commitment_hash)?;

    let cpi_accounts = system_program::Transfer {
        from: ctx.accounts.sender.cpi_handle_mut(), // TODO: this is wired, so AI thingy
        to: ctx.accounts.vault.cpi_handle_mut(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.system_program.address(), cpi_accounts);
    system_program::transfer(cpi_ctx, total_amount)?;

    emit!(DepositDone {
        user_commitment_hash: Address::from(user_commitment_hash),
        total_amount,
        deposit_commitment_hash: Address::from(deposit_commitment_hash),
        new_root: Address::from(new_root),
    });

    Ok(())
}
