#![cfg_attr(not(test), no_std)]

extern crate alloc;

use anchor_lang::prelude::*;

pub mod instructions;
pub mod state;
pub mod utils;

use instructions::{
    deposit::{self, *},
    withdraw::{self, *},
    hello::{self, *},
    initialize::{self, *},
    upload_proof::{self, *},
};

declare_id!("FCrymxYUTEnXDJXdDn2E71KPxB9sBjXZCh1ezBwmUhvp");

#[program]
pub mod zk_shielded_pool_solana {
    use super::*;

    pub fn hello(ctx: &mut Context<HelloAccountConstraints>) -> Result<()> {
        hello::handle_hello(ctx)
    }

    pub fn initialize(ctx: &mut Context<Initialize>) -> Result<()> {
        initialize::handle(ctx)
    }

    pub fn deposit(
        ctx: &mut Context<Deposit>,
        user_commitment_hash: [u8; 32],
        total_amount: u64,
    ) -> Result<()> {
        deposit::handle(ctx, user_commitment_hash, total_amount)
    }

    pub fn withdraw(
        ctx: &mut Context<Withdraw>,
        proof_hash: u64,
        public_inputs: [[u8; 32]; 5],
    ) -> Result<()> {
        withdraw::handle(ctx, proof_hash, &public_inputs)
    }

    /**
    On a real cluster, 1088 proof bytes will not fit. Rough leftover for this ix (1 signer, 4 keys):
    - packet budget: 1232
    - overhead (sig, header, 4 pubkeys, blockhash, compiled ix): ~237
    - remaining for ix data: ~995
    - ix data is disc + proof_hash(8) + part(1) + u32 len(4) + proof
     */
    pub fn upload_proof(
        ctx: &mut Context<UploadProof>,
        proof_hash: u64,
        part: u8,
        proof_final_len: u16,
        proof: alloc::vec::Vec<u8>,
    ) -> Result<()> {
        upload_proof::handle(ctx, proof_final_len, part, &proof)
    }
}