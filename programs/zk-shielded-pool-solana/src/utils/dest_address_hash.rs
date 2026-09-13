use anchor_lang::prelude::*;

use crate::utils::{imt_tree::u64_to_32bytes_le, poseidon_hash::sol_poseidon_hash};

/// Hash a 32-byte public key into one field value. The result is little-endian.
///
/// Same mapping as `convert_pubkey_32bytes_to_fr` in the circuit
/// (solana-proof-generator/circuits/shielded-pool/src/circuit/utils.rs):
///
/// 1. Split the key into four groups of 8 bytes: bytes 0..8, 8..16, 16..24, 24..32.
/// 2. Read each group as a big-endian u64. Byte 0 of the group is the most significant byte.
/// 3. Put each u64 into a 32-byte little-endian array. Bytes 0..8 hold the number,
///    bytes 8..32 are zero. This is the field value of that u64.
/// 4. Poseidon (BN254 X5, little-endian) over the four arrays, in group order.
///
/// A whole public key cannot go into Poseidon as one value. 32 bytes can be bigger than
/// the field modulus. One u64 is always smaller, so each group is a valid field value.
pub fn dest_address_hash_le(address: &[u8; 32]) -> Result<[u8; 32]> {
    let mut limbs = [[0u8; 32]; 4];
    for (i, limb) in limbs.iter_mut().enumerate() {
        let start = i * 8;
        let mut group = [0u8; 8];
        group.copy_from_slice(&address[start..start + 8]);
        *limb = u64_to_32bytes_le(u64::from_be_bytes(group));
    }
    sol_poseidon_hash(&[&limbs[0], &limbs[1], &limbs[2], &limbs[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::common::reverse_byte_order;

    fn hex32(s: &str) -> [u8; 32] {
        assert_eq!(s.len(), 64);
        core::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
    }

    /// Expected values come from running the circuit's own `convert_pubkey_32bytes_to_fr`
    /// on these keys, then writing the Fr out little-endian.
    /// (keys as hex, hash as little-endian hex)
    const REFERENCE: [(&str, &str, &str); 4] = [
        (
            // dstH17g8RBGdUo3YeYhSFHDdFzHrWkAzNCKSveAchyD
            "dst",
            "097271a50fa501a5658a19ee58e6fa6d2bdc786a62d39bb5e4bf243f4561d144",
            "c2332be1ab10bbdb95a6794fd868716df5c6a979b237b4f7d395f5e568f7fe2e",
        ),
        (
            // every byte different and nonzero: 01 02 03 .. 20
            "counting_1_to_32",
            "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
            "4f911d86995e40d8f4457c07caf876f181be2c2a214d4e35e00d59318826c72d",
        ),
        (
            // each group is u64::MAX
            "all_ff",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "3979c0a464c141a68d7347adb6ce18d8601d5135158bbcc813f5b450c27e732b",
        ),
        (
            "zero",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "46993eb76d20c1880406798b1b9237092515c2d9949620510ec7196e43fd3205",
        ),
    ];

    #[test]
    fn matches_the_off_chain_mapping_for_fixed_keys() {
        for (name, key, expected_le) in REFERENCE {
            assert_eq!(
                dest_address_hash_le(&hex32(key)).unwrap(),
                hex32(expected_le),
                "hash mismatch for key {name}"
            );
        }
    }

    #[test]
    fn a_matching_recipient_equals_the_big_endian_public_input() {
        // What the handler does: the public input is big-endian, so it flips our result.
        let (_, key, expected_le) = REFERENCE[0];
        let dest_address_public_input_be = reverse_byte_order(hex32(expected_le));
        let recipient_hash_le = dest_address_hash_le(&hex32(key)).unwrap();
        assert_eq!(reverse_byte_order(recipient_hash_le), dest_address_public_input_be);

        // Without the flip it does not match. Catches a missing byte-order conversion.
        assert_ne!(recipient_hash_le, dest_address_public_input_be);
    }

    #[test]
    fn a_different_recipient_does_not_match() {
        let (_, _, dst_hash_le) = REFERENCE[0];
        let mut other = hex32(REFERENCE[0].1);
        other[31] ^= 1; // flip one bit in the last byte
        assert_ne!(dest_address_hash_le(&other).unwrap(), hex32(dst_hash_le));
    }

    #[test]
    fn group_order_and_byte_order_change_the_hash() {
        let key = hex32(REFERENCE[1].1);
        let expected = dest_address_hash_le(&key).unwrap();

        // Reversed key: every group reversed and in reverse order.
        let mut reversed = key;
        reversed.reverse();
        assert_ne!(dest_address_hash_le(&reversed).unwrap(), expected);

        // Swap group 0 (bytes 0..8) with group 1 (bytes 8..16).
        let mut swapped = key;
        swapped[..16].rotate_left(8);
        assert_ne!(dest_address_hash_le(&swapped).unwrap(), expected);

        // Reverse bytes inside group 0 only: 01..08 becomes 08..01.
        let mut inner = key;
        inner[..8].reverse();
        assert_ne!(dest_address_hash_le(&inner).unwrap(), expected);
    }
}
