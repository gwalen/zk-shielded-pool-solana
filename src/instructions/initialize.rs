// #![allow(dead_code)]
use quasar_lang::prelude::*;

use crate::{
    state::{
        root_registry::{RootRegistry, RootRegistryInner},
        vault::Vault,
    },
    utils::{
        constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
        imt_tree::{set_array_element, ImtTree},
    },
};

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
pub fn handle_initialize(ctx: &mut Ctx<Initialize>) -> Result<(), ProgramError> {
    log("Initializing Shielded Pool Program");
    let root_registry = &mut ctx.accounts.root_registry;

    let imt = ImtTree::new()?;

    // We need to set the ring buffer to empty values, with the empty-tree root in slot 0.
    let mut roots_history = [0u8; 32 * ROOT_RING_BUFFER_LENGTH];
    for i in 0..ROOT_RING_BUFFER_LENGTH {
        set_array_element(&mut roots_history, i, &EMPTY_TREE_VALUE);
    }
    set_array_element(&mut roots_history, 0, &imt.root);

    root_registry.set_inner(RootRegistryInner {
        imt,
        roots_history,
        last_root_idx: 0,
        bump: ctx.bumps.root_registry,
    });

    Ok(())
}
