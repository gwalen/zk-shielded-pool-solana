use anchor_lang::prelude::*;

/// Packed Halo2 fixture (`H2PF0001` + proof + public inputs). Checked-in
/// `fixture.bin` is 1264 bytes. The account allocates this many bytes at init.
pub const PROOF_BUFFER_LEN: usize = 1500;

#[account]
pub struct ProofStorage {
    pub bump: u8,
    pub proof_len: PodU16,
    pub proof: [u8; PROOF_BUFFER_LEN],
}
