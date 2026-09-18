use anchor_lang::prelude::*;

#[event]
pub struct DepositDone {
    pub user_commitment_hash: Address,    //  [u8; 32],
    pub total_amount: u64,
    pub deposit_commitment_hash: Address, //  [u8; 32],
    pub new_root: Address,                //  [u8; 32],
}

#[event]
#[derive(Debug, PartialEq, AnchorDeserialize)]
pub struct FullProofUploaded {
    pub sender: Address,
    pub proof_hash: Address, // [u8; 32]
    pub proof_len: u16,
}

#[event]
#[derive(Debug, PartialEq, AnchorDeserialize)]
pub struct PartialProofUploaded {
    pub sender: Address,
    pub proof_hash: Address, // [u8; 32]
    pub proof_len: u16,
}