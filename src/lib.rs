#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;
mod state;
mod utils;

use instructions::{hello::{self, *}, initialize::{self, *}};

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
        initialize::handle_initialize(&mut ctx)
    }
}