use anchor_lang::prelude::*;

/// GWC proof bytes. The account allocates this many bytes at init. The
/// checked-in `proof.bin` is 1088 bytes.
pub const PROOF_BUFFER_LEN: usize = 1500;

#[account]
pub struct ProofStorage {
    pub bump: u8,
    pub proof_final_len: PodU16,
    pub proof_current_len: PodU16,
    pub proof: [u8; PROOF_BUFFER_LEN],
}
