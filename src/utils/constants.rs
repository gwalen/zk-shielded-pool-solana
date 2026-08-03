pub const MERKLE_TREE_DEPTH: usize = 20;
pub const ROOT_RING_BUFFER_LENGTH: usize = 100;

// Fr (scalar field element) zero value
pub const FR_ZERO: [u8; 32] = [0u8; 32];

// Fr (scalar field element) one value
// 1 in little endian format [1, 0, .., 0]
pub const FR_ONE: [u8; 32] = {
    let mut bytes = [0u8; 32];
    bytes[0] = 1;
    bytes
};