// #![allow(dead_code)]
use quasar_lang::prelude::*;

use crate::{
    state::{root_registry::RootRegistry, vault::Vault},
    utils::{
        constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH}, flatten_array::set_array_element
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

    pub system_program: Program<SystemProgram>,
}

// TODO: add config account that will store the signer as owner (for procol pausing, have pause flag, and is_init flag)
// we don't [inline] this function to keep the handler stack separate from instruction entrypoint function
pub fn handle(ctx: &mut Ctx<Initialize>) -> Result<(), ProgramError> {
    log("Initializing Shielded Pool Program");
    let root_registry = &mut ctx.accounts.root_registry;

    // initialize the empty tree in place with no stack allocation
    root_registry.imt.initialize_empty()?;
    let empty_tree_root = root_registry.imt.root;

    // We need to set the ring buffer to empty values, with the empty-tree root in slot 0.
    // Here we also do in place updates on account data with no stack allocations
    for i in 0..ROOT_RING_BUFFER_LENGTH {
        set_array_element(&mut root_registry.roots_history, i, &EMPTY_TREE_VALUE);
    }
    set_array_element(&mut root_registry.roots_history, 0, &empty_tree_root);

    root_registry.last_root_idx = PodU32::from(0);
    root_registry.bump = ctx.bumps.root_registry;

    Ok(())
}
