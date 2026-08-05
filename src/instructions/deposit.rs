use quasar_lang::prelude::*;

use crate::{
    state::{root_registry::RootRegistry, vault::Vault},
    utils::{common::is_in_fr_range, errors::DappError, events::DepositDone, imt_tree::u64_to_32bytes_le, poseidon_hash},
};

#[derive(Accounts)]
pub struct Deposit {
    #[account(mut)]
    pub sender: Signer,

    #[account(mut)]
    pub vault: Vault,

    #[account(mut)]
    pub roots_registry: RootRegistry,

    pub system_program: Program<SystemProgram>,
}

pub fn handle(
    ctx: &mut Ctx<Deposit>,
    user_commitment_hash: [u8; 32],
    total_amount: u64,
) -> Result<(), ProgramError> {
    if !is_in_fr_range(&user_commitment_hash) {
        return Err(DappError::PoseidonInputLargerThanModulus.into());
    }
    let roots_registry = &mut ctx.accounts.roots_registry;

    let deposit_commitment_hash =
        poseidon_hash::hash2(user_commitment_hash, u64_to_32bytes_le(total_amount))?;
    let new_root = roots_registry.insert(deposit_commitment_hash)?;

    ctx.accounts
        .system_program
        .transfer(&ctx.accounts.sender, &ctx.accounts.vault, total_amount)
        .invoke()?;

    emit!(DepositDone {
        user_commitment_hash: Address::from(user_commitment_hash),
        total_amount,
        deposit_commitment_hash: Address::from(deposit_commitment_hash),
        new_root: Address::from(new_root),
    });

    Ok(())
}
