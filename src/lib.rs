#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;

use instructions::hello::{self, *};

#[cfg(test)]
mod tests;

declare_id!("Emgth1ockby4pw4oBte5trB9QoMprwfWCWq5qwwsnUs9");

#[program]
mod quasar_hello_solana {

    use super::*;

    #[instruction(discriminator = 0)]
    pub fn hello(ctx: Ctx<HelloAccountConstraints>) -> Result<(), ProgramError> {
        hello::handle_hello(&mut ctx.accounts)
    }
}