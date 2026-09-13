use anchor_lang::prelude::*;

// NOTE: bytes for public inputs are stored as big-endian (BE) after conversion from Fr values
// this is what proof verifier expects
#[derive(AnchorSerialize, AnchorDeserialize, IdlType)]
pub struct PublicInputs {
    pub step: [u8; 32],         // u8 step number as Fr
    pub chunk_amount: [u8; 32], // u64 chunk amount as Fr
    pub dest_address: [u8; 32], // hash of address of the destination account as Fr
    pub nullifier: [u8; 32],    // nullifier as Fr Poseidon(s, step)
    pub root: [u8; 32],         // commitment MT root as time of proof generation as Fr
}

impl PublicInputs {
    pub fn new(
        step: [u8; 32],
        chunk_amount: [u8; 32],
        dest_address: [u8; 32],
        nullifier: [u8; 32],
        root: [u8; 32],
    ) -> Self {
        Self {
            step,
            chunk_amount,
            dest_address,
            nullifier,
            root,
        }
    }

    pub fn to_byte_chunks(&self) -> [[u8; 32]; 5] {
        let mut chunks = [[0u8; 32]; 5];
        chunks[0].copy_from_slice(&self.step);
        chunks[1].copy_from_slice(&self.chunk_amount);
        chunks[2].copy_from_slice(&self.dest_address);
        chunks[3].copy_from_slice(&self.nullifier);
        chunks[4].copy_from_slice(&self.root);
        chunks
    }

    pub fn from_byte_chunks(chunks: &[[u8; 32]; 5]) -> Self {
        Self {
            step: chunks[0],
            chunk_amount: chunks[1],
            dest_address: chunks[2],
            nullifier: chunks[3],
            root: chunks[4],
        }
    }
}
