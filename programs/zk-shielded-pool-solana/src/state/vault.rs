use anchor_lang::prelude::*;

#[account]
pub struct Vault {
    pub bump: u8,
}

impl Vault {
    pub const SEED_PREFIX: &'static [u8] = b"vault";
}

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultStatus {
    Active = 0,
    Paused = 1,
}
