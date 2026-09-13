use crate::utils::constants::BN254_FR_MODULUS_LE;

/// Checks if value is in Fr scalar field values range
/// We compare LE representation of v with r (Fr modulus)
pub fn is_in_fr_range(v: &[u8; 32]) -> bool {
    // LE integer compare: most significant byte is at index 31
    // so we compare one be one backward from most significant byte to lest significant byte
    for i in (0..32).rev() {
        if v[i] != BN254_FR_MODULUS_LE[i] {
            return v[i] < BN254_FR_MODULUS_LE[i];
        }
    }
    false
}
/// Flip a 32-byte field element between big-endian and little-endian order.
///
/// Byte 0 becomes byte 31, byte 1 becomes byte 30, and so on. The two orders are mirror
/// images, so the same function converts either way. Name the result for the order you
/// wanted: `let root_le = reverse_byte_order(root_be);`
///
/// Public inputs arrive big-endian, because that is what the proof verifier reads. The
/// root history stores little-endian. This is the bridge between the two.
pub fn reverse_byte_order(bytes: [u8; 32]) -> [u8; 32] {
    let mut reversed = bytes;
    reversed.reverse();
    reversed
}
