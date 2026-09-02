use anchor_lang::prelude::*;

#[account]
pub struct Vault {
    pub bump: u8,
}

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultStatus {
    Active = 0,
    Paused = 1,
}
