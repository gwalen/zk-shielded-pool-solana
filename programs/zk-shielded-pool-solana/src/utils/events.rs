use anchor_lang::prelude::*;

#[event]
pub struct DepositDone {
    pub user_commitment_hash: Address,    //  [u8; 32],
    pub total_amount: u64,
    pub deposit_commitment_hash: Address, //  [u8; 32],
    pub new_root: Address,                //  [u8; 32],
}