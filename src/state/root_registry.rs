use crate::utils::{
    constants::{FR_ONE, FR_ZERO, MERKLE_TREE_DEPTH, ROOT_RING_BUFFER_LENGTH},
    errors::DappError,
    poseidon_hash::{self},
};
use quasar_lang::prelude::*;
use solana_poseidon::{Endianness, Parameters};

pub const EMPTY_TREE_VALUE: [u8; 32] = FR_ONE;

/// This structure stores the roots history as ring buffer and IMT (Incremental Merkle Tree) representation
/// for inserted deposit commitments.
///
/// # Fields
///
/// - `frontiers` - also called filledSubtrees. It stores exactly one hash per level — the hash of the
///   most recently updated left subtree at that level.
///   One value per level (no need to store it for root level)
///
/// - `zero_values` - Z_k for each level k (precomputed). Contains MERKLE_TREE_DEPTH values (we do not need it on root level)
///
/// - `next_leaf_idx` - Index of the next leaf to be inserted (0.. 2^MERKLE_TREE_DEPTH)
///   First (leaf) level depth == 0 and amount of leafs at this level is 2^MERKLE_TREE_DEPTH
///
/// - `roots_history` - old roots history. Represented as ring buffer.
///
/// - `last_root_idx` - index of the last root in the roots_history
///
/// @dev Note:
/// frontiers, zero_values and roots_history store Fr elements in little endian format
/// Normally we store Fr elements as 32 bytes, so we would have declaration like this:
///   roots_history: [[u8; 32]; ROOT_RING_BUFFER_LENGTH]
/// but because of zeropod limitation which only implements the field trait for byte arrays
/// we need to flatten and manually index the elements so we get this:
///   roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH] // Which gives us ROOT_RING_BUFFER_LENGTH roots of size 32 bytes each
#[account(discriminator = 2, set_inner)]
#[seeds(b"root_registry")]
pub struct RootRegistry {
    // IMT representation
    pub frontiers: [u8; 32 * MERKLE_TREE_DEPTH],
    pub zero_values: [u8; 32 * MERKLE_TREE_DEPTH],
    pub next_leaf_idx: u32,
    // Ring buffer representation
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: u32,
    // Bump which is set by the Quasar
    pub bump: u8,
}

impl RootRegistry {
    /**
     * Example tree indexes for depth = 3 (note: we use level-local indexing - each level index starts from 0) :
     *                     R_0                   - level 3 (root)
     *           A_0                A_1          - level 2
     *     B_0      B_1        B_2         B_3   - level 1
     *  C_0  C_1  C_2  C_3  C_4  C_5  C_6  C_7   - level 0 (leafs)
     */
    pub fn insert(&mut self, leaf: [u8; 32]) -> Result<[u8; 32], ProgramError> {
        if self.next_leaf_idx >= 1 << MERKLE_TREE_DEPTH {
            return Err(DappError::TreeIsFull.into());
        }

        // traverse tree from leaf to root (current leaf index on leaf level == next_leaf_index)
        // each level node indexing starts from 0
        let mut current_idx = self.next_leaf_idx;
        let mut current_node_value = leaf;
        for current_level in 0..MERKLE_TREE_DEPTH {
            let is_left_leaf = current_idx % 2 == 0;

            // go level up add calculate upper node value
            if is_left_leaf {
                Self::set_array_element(&mut self.frontiers, current_level, &current_node_value);
                current_node_value = poseidon_hash::hash2(
                    current_node_value,
                    Self::get_array_element(&self.zero_values, current_level),
                )?
            } else {
                current_node_value = poseidon_hash::hash2(
                    Self::get_array_element(&self.frontiers, current_level),
                    current_node_value,
                )?
            };
            current_idx /= 2;
        }
        self.inc_next_leaf_idx();
        self.inc_last_root_idx();

        let last_root_idx = self.last_root_idx.get() as usize;
        Self::set_array_element(
            &mut self.roots_history,
            last_root_idx,
            &current_node_value,
        );

        Ok(current_node_value)
    }

    fn inc_next_leaf_idx(&mut self) {
        self.next_leaf_idx += 1;
    }

    // root_history is a ring buffer (cyclic array)
    fn inc_last_root_idx(&mut self) {
        if self.last_root_idx.get() as usize == ROOT_RING_BUFFER_LENGTH - 1 {
            self.last_root_idx = PodU32::from(0);
        } else {
            self.last_root_idx += 1;
        }
    }

    // pub fn frontiers(&self, index: usize) -> [u8; 32] {
    //     Self::get_array_element(&self.frontiers, index)
    // }

    // pub fn zero_values(&self, index: usize) -> [u8; 32] {
    //     Self::get_array_element(&self.zero_values, index)
    // }

    // pub fn roots_history(&self, index: usize) -> [u8; 32] {
    //     Self::get_array_element(&self.roots_history, index)
    // }

    pub fn generate_zero_values_for_levels() -> Result<[u8; 32 * MERKLE_TREE_DEPTH], ProgramError> {
        // zero values for each level (except root level)
        // Example for depth 3: 0 (leaf) -> 1 (level 1) -> 2 (level 2) -> 3 (root)
        //                         Z_0   ->    Z_1      ->  Z_2        ->  this we do not need to store in zero_values (used only for root calculation)
        // let mut zero_values = Vec::<Fr>::with_capacity(tree_depth);
        let mut zero_values = [0u8; 32 * MERKLE_TREE_DEPTH];
        let z_0 = FR_ZERO;

        Self::set_array_element(&mut zero_values, 0, &z_0);

        for i in 1..MERKLE_TREE_DEPTH {
            // let z_prev = zero_values[i - 1];
            let z_prev = Self::get_array_element(&zero_values, i - 1);
            // let hash = poseidon_hash::hash2(z_prev, z_prev)?;
            let hash = solana_poseidon::hashv(
                Parameters::Bn254X5,
                Endianness::LittleEndian,
                &[&z_prev, &z_prev],
            )
            .unwrap().to_bytes();
            Self::set_array_element(&mut zero_values, i, &hash);
        }

        Ok(zero_values)
    }

    pub fn get_array_element(array: &[u8], index: usize) -> [u8; 32] {
        let mut element = [0u8; 32];
        let start_element = index * 32;
        element.copy_from_slice(&array[start_element..start_element + 32]);
        element
    }

    pub fn set_array_element(array: &mut [u8], index: usize, element: &[u8; 32]) {
        let start_element = index * 32;
        array[start_element..start_element + 32].copy_from_slice(element);
    }
}
