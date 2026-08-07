use quasar_lang::prelude::*;

#[error_code]
pub enum DappError {
    /// Tree is full - no more leaves can be added
    TreeIsFull,
    /// A deposit must be bigger than zero
    DepositAmountZero,

    // --------------------------------------------------------------------------------------------------------
    // ---- Poseidon syscall status codes (solana-poseidon::PoseidonSyscallError) mapped to our app errors ----
    // --------------------------------------------------------------------------------------------------------
    /// Invalid Poseidon parameters (syscall status 1; also the collapsed "any failure" code)
    PoseidonInvalidParameters,
    /// Invalid Poseidon endianness (syscall status 2)
    PoseidonInvalidEndianness,
    /// Invalid number of Poseidon inputs (syscall status 3)
    PoseidonInvalidNumberOfInputs,
    /// Empty Poseidon input (syscall status 4)
    PoseidonEmptyInput,
    /// Poseidon input length is not 32 bytes (syscall status 5)
    PoseidonInvalidInputLength,
    /// Failed to convert bytes into an Fr element (syscall status 6)
    PoseidonBytesToPrimeFieldElement,
    /// Poseidon input >= BN254 Fr modulus (syscall status 7)
    PoseidonInputLargerThanModulus,
    /// Failed to convert Vec to array inside Poseidon (syscall status 8)
    PoseidonVecToArray,
    /// Failed u64→u8 conversion inside Poseidon (syscall status 9)
    PoseidonU64ToU8,
    /// Failed bytes→BigInt conversion inside Poseidon (syscall status 10)
    PoseidonBytesToBigInt,
    /// Invalid Circom Poseidon width (syscall status 11)
    PoseidonInvalidWidthCircom,
    /// Unexpected / unknown Poseidon syscall failure
    PoseidonUnexpected,
}

impl DappError {
    /// Map `sol_poseidon` return status to a program error.
    /// `0` is success and must not be passed here.
    pub fn from_poseidon_status(status: u64) -> Self {
        match status {
            1 => Self::PoseidonInvalidParameters,
            2 => Self::PoseidonInvalidEndianness,
            3 => Self::PoseidonInvalidNumberOfInputs,
            4 => Self::PoseidonEmptyInput,
            5 => Self::PoseidonInvalidInputLength,
            6 => Self::PoseidonBytesToPrimeFieldElement,
            7 => Self::PoseidonInputLargerThanModulus,
            8 => Self::PoseidonVecToArray,
            9 => Self::PoseidonU64ToU8,
            10 => Self::PoseidonBytesToBigInt,
            11 => Self::PoseidonInvalidWidthCircom,
            _ => Self::PoseidonUnexpected,
        }
    }
}
