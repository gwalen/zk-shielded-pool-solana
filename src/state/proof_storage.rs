use quasar_lang::prelude::*;

/// Packed Halo2 fixture (`H2PF0001` + proof + public inputs). Checked-in
/// `fixture.bin` is 1264 bytes. The account allocates this many bytes at init.
pub const PROOF_BUFFER_LEN: usize = 1500;

#[account(discriminator = 3)]
#[seeds(b"proof_storage", user_address: Address, proof_hash: u64)]
pub struct ProofStorage {
    pub bump: u8,
    pub proof_len: u16,
    pub proof: [u8; PROOF_BUFFER_LEN],
}
