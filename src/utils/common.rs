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