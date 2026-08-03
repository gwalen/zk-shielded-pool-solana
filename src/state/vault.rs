use quasar_lang::prelude::*;

#[account(discriminator = 1, set_inner)]
#[seeds(b"vault")]
pub struct Vault {
    pub bump: u8, // do not need fill manually quasar will do it for us
}