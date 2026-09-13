use anchor_lang::prelude::*;

use crate::utils::errors::DappError;

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

    /// Read the proven chunk amount as lamports
    ///
    /// `chunk_amount` is big-endian 32 byte array, so the number sits in the last bytes.
    /// A u64 needs 8 bytes: bytes 24..32. Bytes 0..24 must all be zero.
    /// If any of them is not zero, the value is bigger than u64::MAX and we reject it.
    /// We never drop those upper bytes silently.
    pub fn chunk_amount_u64(&self) -> Result<u64> {
        be_bytes_to_u64(&self.chunk_amount)
    }
}

fn be_bytes_to_u64(bytes: &[u8; 32]) -> Result<u64> {
    let (upper, lower) = bytes.split_at(24);
    require!(upper.iter().all(|b| *b == 0), DappError::ChunkAmountTooLarge);

    let mut value = [0u8; 8];
    value.copy_from_slice(lower);
    Ok(u64::from_be_bytes(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be_bytes(value: u64) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&value.to_be_bytes());
        bytes
    }

    #[test]
    fn representative_amounts_decode_exactly() {
        for value in [0, 1, 2, 2_000_000_000, 3_000_000_000, 4_000_000_000, u64::MAX - 1, u64::MAX] {
            assert_eq!(be_bytes_to_u64(&be_bytes(value)).unwrap(), value);
        }
    }

    #[test]
    fn two_sol_has_the_expected_byte_layout() {
        // 2_000_000_000 = 0x77_35_94_00. Big-endian, so it fills the last four bytes.
        let mut bytes = [0u8; 32];
        bytes[28..].copy_from_slice(&[0x77, 0x35, 0x94, 0x00]);
        assert_eq!(be_bytes_to_u64(&bytes).unwrap(), 2_000_000_000);
    }

    #[test]
    fn every_nonzero_upper_byte_is_rejected() {
        for position in 0..24 {
            let mut bytes = be_bytes(5);
            bytes[position] = 1;
            assert!(
                be_bytes_to_u64(&bytes).is_err(),
                "a nonzero byte at position {position} was accepted"
            );
        }
    }

    #[test]
    fn u64_max_plus_one_is_rejected() {
        // u64::MAX + 1 = 2^64: byte 23 is 1, bytes 24..32 are zero.
        let mut bytes = [0u8; 32];
        bytes[23] = 1;
        assert!(be_bytes_to_u64(&bytes).is_err());
    }
}
