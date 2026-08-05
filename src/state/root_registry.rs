use crate::utils::{
    constants::ROOT_RING_BUFFER_LENGTH,
    imt_tree::ImtTree,
    flatten_array::set_array_element,
};
use quasar_lang::prelude::*;

/// This structure stores the roots history as ring buffer and the IMT (Incremental Merkle Tree)
/// representation for inserted deposit commitments.
///
/// # Fields
///
/// - `imt` - the Incremental Merkle Tree itself. See [`ImtTree`] for its own fields.
///
/// - `roots_history` - old roots history. Represented as ring buffer.
///
/// - `last_root_idx` - index of the last root in the roots_history
///
/// @dev Note:
/// roots_history stores 32-byte scalar field elements in little endian format. Normally each
/// would be its own `[u8; 32]`, giving a declaration like:
///   roots_history: [[u8; 32]; ROOT_RING_BUFFER_LENGTH]
/// but zeropod only implements its field traits for byte arrays, so the entries are flattened
/// and indexed by hand:
///   roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH] // ROOT_RING_BUFFER_LENGTH roots of 32 bytes each
#[account(discriminator = 2, set_inner)]
#[seeds(b"root_registry")]
pub struct RootRegistry {
    pub imt: ImtTree,
    // Ring buffer representation
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: u32,
    // Bump which is set by the Quasar
    pub bump: u8,
}

impl RootRegistry {
    /// Insert a commitment into the tree and record the resulting root in the ring buffer.
    pub fn insert(&mut self, leaf: [u8; 32]) -> Result<[u8; 32], ProgramError> {
        let root = self.imt.insert(leaf)?;

        self.inc_last_root_idx();
        let last_root_idx = self.last_root_idx.get() as usize;
        set_array_element(&mut self.roots_history, last_root_idx, &root);

        Ok(root)
    }

    // root_history is a ring buffer (cyclic array)
    fn inc_last_root_idx(&mut self) {
        if self.last_root_idx.get() as usize == ROOT_RING_BUFFER_LENGTH - 1 {
            self.last_root_idx = PodU32::from(0);
        } else {
            self.last_root_idx += 1;
        }
    }
}
