pub const MERKLE_TREE_DEPTH: usize = 20;
pub const ROOT_RING_BUFFER_LENGTH: usize = 100;

/// Default minimum deposit written into `ProgramConfig` by `initialize`: 0.025 SOL.
/// Makes filling the 2^20-leaf tree, or flushing the root ring buffer, cost real SOL
/// instead of only transaction fees. The owner can change it with `update_min_deposit`.
pub const DEFAULT_MIN_DEPOSIT_LAMPORTS: u64 = 25_000_000;

// Fr (scalar field element) zero value
pub const FR_ZERO: [u8; 32] = [0u8; 32];

// Fr (scalar field element) one value
// 1 in little endian format [1, 0, .., 0]
pub const FR_ONE: [u8; 32] = {
    let mut bytes = [0u8; 32];
    bytes[0] = 1;
    bytes
};

// Marker for a slot that holds no real value yet: an unused root-history entry, or a
// frontier level that no insert has passed through on the left.
pub const EMPTY_TREE_VALUE: [u8; 32] = FR_ONE;

/// BN254 r value (scalar field modulus which is special big prime number), little-endian.
pub const BN254_FR_MODULUS_LE: [u8; 32] = [
    1, 0, 0, 240, 147, 245, 225, 67, 145, 112, 185, 121, 72, 232, 51, 40, 93, 88,
    129, 129, 182, 69, 80, 184, 41, 160, 49, 225, 114, 78, 100, 48,
];