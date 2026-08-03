// #![allow(dead_code)]
use quasar_lang::prelude::*;

use crate::state::{root_registry::RootRegistry, vault::Vault};

/// Accounts for the hello instruction.
/// A payer (signer) is required to submit the transaction, but the program
/// simply logs a greeting and the program ID.
#[derive(Accounts)]
pub struct Initialize {
    #[account(mut)]
    pub signer: Signer,

    #[account(
        mut, 
        init(idempotent),
        payer = signer,
        address = Vault::seeds(),
    )]
    pub vault: Account<Vault>,

    #[account(
        mut,
        init(idempotent),
        payer = signer,
        address = RootRegistry::seeds(),
    )]
    pub root_registry: Account<RootRegistry>,
}

// this function will only be called from lib.rs and it is safe to enforce inline optimization for CU
#[inline(always)] 
pub fn handle_initialize(_accounts: &mut Initialize) -> Result<(), ProgramError> {
    log("Initializing Shielded Pool Program");

    Ok(())
}