use quasar_lang::prelude::*;

use crate::utils::errors::DappError;

#[cfg(not(target_os = "solana"))]
use solana_poseidon::{Endianness, Parameters, PoseidonSyscallError};

const POSEIDON_BN254_X5: u64 = 0;
// const POSEIDON_BIG_ENDIAN: u64 = 0;
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
            vals.as_ptr() as *const u8,
            vals.len() as u64,
            hash_result.as_mut_ptr(),
        )
    };
    match status {
        0 => Ok(hash_result),
        status => Err(DappError::from_poseidon_status(status).into()),
    }
}

// Host (off-chain) stub: quasar build compiles this crate for the host OS to
// generate the IDL. Uses solana-poseidon / light_poseidon instead of the syscall.
#[cfg(not(target_os = "solana"))]
pub fn sol_poseidon_hash(vals: &[&[u8]]) -> Result<[u8; 32], ProgramError> {
    let hash = solana_poseidon::hashv(Parameters::Bn254X5, Endianness::LittleEndian, vals)
        .map_err(poseidon_syscall_error_to_dapp)?
        .to_bytes();
    Ok(hash)
}

#[cfg(not(target_os = "solana"))]
fn poseidon_syscall_error_to_dapp(err: PoseidonSyscallError) -> ProgramError {
    // Mirror the u64 status mapping used on-chain.
    DappError::from_poseidon_status(u64::from(err)).into()
}
