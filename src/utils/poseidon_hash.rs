use quasar_lang::prelude::*;

const POSEIDON_BN254_X5: u64 = 0;
const POSEIDON_BIG_ENDIAN: u64 = 0;
const POSEIDON_LITTLE_ENDIAN: u64 = 1;

pub fn hash2(a: [u8; 32], b: [u8; 32]) -> Result<[u8; 32], ProgramError> {
    sol_poseidon_hash(&[&a, &b])
}

/// Poseidon over 32-byte field elements. On-chain only.
#[cfg(any(target_os = "solana"))]
pub fn sol_poseidon_hash(vals: &[&[u8]]) -> Result<[u8; 32], ProgramError> {
    use solana_define_syscall::definitions::sol_poseidon;
    let mut hash_result = [0u8; 32];

    // Syscall side (C ABI, no slices):
    // fn sol_poseidon(..., vals: *const u8, val_len: u64, ...) -> u64
    let status = unsafe {
        sol_poseidon(
            POSEIDON_BN254_X5,
            POSEIDON_LITTLE_ENDIAN,
            vals.as_ptr() as *const u8, // need to covert to: *const u8
            vals.len() as u64,
            hash_result.as_mut_ptr(),
        )
    };
    match status {
        0 => Ok(hash_result),
        _ => Err(ProgramError::InvalidArgument), // or a named PoseidonFailed
    }
}

// This is a host (off-chain) stub is there because quasar build compiles crate for the host OS to generate the IDL.
// Without it that build breaks. Since we don't want an off-chain implementation, a stub is just a placeholder.
#[cfg(not(target_os = "solana"))]
pub fn sol_poseidon_hash(_vals: &[&[u8]]) -> Result<[u8; 32], ProgramError> {
    unimplemented!("poseidon is a syscall, available only in the program build")
}