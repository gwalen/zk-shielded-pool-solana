// #![allow(dead_code)]
use quasar_lang::prelude::*;

use crate::{
    state::{
        root_registry::{RootRegistry, RootRegistryInner, EMPTY_TREE_VALUE}, 
        vault::Vault}, 
        utils::{constants::{MERKLE_TREE_DEPTH, ROOT_RING_BUFFER_LENGTH}, 
        poseidon_hash::sol_poseidon_hash
    }
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

    // init tree values when tree is empty (no leafs inserted yet) -  but still need to be correct Merkle tree (hashing tree)
    let zero_values: [u8; 32 * MERKLE_TREE_DEPTH] =
        RootRegistry::generate_zero_values_for_levels().unwrap();
    // root is the hash of the top leaf level    
    let root = sol_poseidon_hash(&[
        &RootRegistry::get_array_element(&zero_values, MERKLE_TREE_DEPTH - 1), 
        &RootRegistry::get_array_element(&zero_values, MERKLE_TREE_DEPTH - 1), 
    ]).unwrap();
    let mut roots_history = [0u8; 32 * ROOT_RING_BUFFER_LENGTH];
    // We need to set ring buffer to empty values
    for i in 0..ROOT_RING_BUFFER_LENGTH {
        RootRegistry::set_array_element(&mut roots_history, i, &EMPTY_TREE_VALUE);
    }
    RootRegistry::set_array_element(&mut roots_history, 0, &root);
    // We need to set frontiers to empty values
    let mut frontiers = [0u8; 32 * MERKLE_TREE_DEPTH];
    for i in 0..MERKLE_TREE_DEPTH {
        RootRegistry::set_array_element(&mut frontiers, i, &EMPTY_TREE_VALUE);
    };

    root_registry.set_inner(RootRegistryInner {
        frontiers,
        zero_values,
        roots_history,
        last_root_idx: 0,
        next_leaf_idx: 0,
        bump: ctx.bumps.root_registry,
    });

    Ok(())
}