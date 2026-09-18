use anchor_lang::prelude::*;

#[account]
pub struct ProgramConfig {
    pub owner: Address,
    pub pause: PodBool,
    /// Smallest `total_amount` a `deposit` accepts, in lamports.
    pub min_deposit_lamports: PodU64,
}

impl ProgramConfig {
    pub const SEED_PREFIX: &'static [u8] = b"program_config";

    pub fn new(owner: Address, pause: bool, min_deposit_lamports: u64) -> Self {
        Self {
            owner,
            pause: pause.into(),
            min_deposit_lamports: PodU64::from(min_deposit_lamports),
        }
    }
}