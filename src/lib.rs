#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;
mod state;
mod utils;

use instructions::{
    deposit::{self, *},
    hello::{self, *},
    initialize::{self, *},
    upload_proof::{self, *},
};

#[cfg(test)]
mod tests;

declare_id!("Emgth1ockby4pw4oBte5trB9QoMprwfWCWq5qwwsnUs9");

#[program]
mod quasar_hello_solana {
    use super::*;

    // TODO: remove later
    #[instruction(discriminator = 0)]
    pub fn hello(ctx: Ctx<HelloAccountConstraints>) -> Result<(), ProgramError> {
        hello::handle_hello(&mut ctx.accounts)
    }

    #[instruction(discriminator = 1)]
    pub fn initialize(ctx: Ctx<Initialize>) -> Result<(), ProgramError> {
        // initialize::handle_initialize(&mut ctx.accounts)
        initialize::handle(&mut ctx)
    }

    #[instruction(discriminator = 2)]
    pub fn deposit(
        ctx: Ctx<Deposit>,
        user_commitment_hash: [u8; 32],
        total_amount: u64,
    ) -> Result<(), ProgramError> {
        deposit::handle(&mut ctx, user_commitment_hash, total_amount)
    }

    /**
    On a real cluster, 1088 proof bytes will not fit. Rough leftover for this ix (1 signer, 4 keys):
    - packet budget: 1232
    - overhead (sig, header, 4 pubkeys, blockhash, compiled ix): ~237
    - remaining for ix data: ~995
    - ix data is disc(1) + proof_hash(8) + u16 len(2) + proof
    - so proof max is about ~980 bytes, not 1088
     */
    #[instruction(discriminator = 3)]
    pub fn upload_proof(
        ctx: Ctx<UploadProof>,
        proof_hash: u64,
        // 800 bytes leaves room in a 1232-byte packet for the signature,
        // header, account keys, and proof_hash.
        // #[max(800)] 
        // proof: Vec<u8, 800>,
        part: u8,
        proof: Vec<u8, 900>,
    ) -> Result<(), ProgramError> {
        upload_proof::handle(&mut ctx, part, &proof)
    }
}