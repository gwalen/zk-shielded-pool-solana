use anchor_lang::prelude::*;

#[account]
pub struct ProgramConfig {
    pub owner: Address,
    pub pause: PodBool,
}

impl ProgramConfig {
    pub const SEED_PREFIX: &'static [u8] = b"program_config";

    pub fn new(owner: Address, pause: bool) -> Self {
        Self {
            owner,
            pause: pause.into(),
        }
    }
}