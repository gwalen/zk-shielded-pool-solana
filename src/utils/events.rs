use quasar_lang::prelude::*;

#[event(discriminator= 1)]
pub struct DepositDone {
    // TODO: have to log [u8; 32] arrays as Address as Quasar does not accept [u8; N] arrays in events
    pub user_commitment_hash: Address,    //  [u8; 32],
    pub total_amount: u64,
    pub deposit_commitment_hash: Address, //  [u8; 32],
    pub new_root: Address,                //  [u8; 32],
}

// Tets event to check the quasar fix
#[event(discriminator = 8)]
pub struct BytesEvent {
    pub hash: [u8; 64],
    pub amount: u64,
}