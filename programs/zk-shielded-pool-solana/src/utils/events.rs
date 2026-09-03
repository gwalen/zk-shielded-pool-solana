use anchor_lang::prelude::*;
extern crate alloc;

#[event]
pub struct DepositDone {
    // TODO: have to log [u8; 32] arrays as Address as Quasar does not accept [u8; N] arrays in events
    pub user_commitment_hash: Address,    //  [u8; 32],
    pub total_amount: u64,
    pub deposit_commitment_hash: Address, //  [u8; 32],
    pub new_root: Address,                //  [u8; 32],
}

// Tets event to check the quasar fix
#[event]
pub struct BytesEvent {
    pub flag: u8,
    pub hash: [u8; 3],
    pub val1: u16,
    pub amount: u64,
    pub vector: alloc::vec::Vec<u8>,
    pub hash2: [u8; 11],
}