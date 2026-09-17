use crate::utils::{
    constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
    flatten_array::{get_array_element, set_array_element},
    imt_tree::ImtTree,
};
use anchor_lang::prelude::*;

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
#[account]
pub struct RootRegistry {
    pub imt: ImtTree,
    // Ring buffer representation
    // TODO: add unit tests for ring buffer functionality
    pub roots_history: [u8; 32 * ROOT_RING_BUFFER_LENGTH],
    pub last_root_idx: PodU32,
    pub bump: u8,
}

impl RootRegistry {
    pub const SEED_PREFIX: &'static [u8] = b"root_registry";

    /// Set up a brand-new registry: an empty tree, and a history that holds only that
    /// empty tree's root in slot 0. Every other slot gets `EMPTY_TREE_VALUE`, the marker
    /// for "nothing recorded here yet".
    ///
    /// All writes go straight into the account bytes, so nothing is copied onto the stack.
    pub fn initialize_empty(&mut self) -> Result<()> {
        self.imt.initialize_empty()?;
        let empty_tree_root = self.imt.root;

        for index in 0..ROOT_RING_BUFFER_LENGTH {
            set_array_element(&mut self.roots_history, index, &EMPTY_TREE_VALUE);
        }
        set_array_element(&mut self.roots_history, 0, &empty_tree_root);
        self.last_root_idx = PodU32::from(0);

        Ok(())
    }

    /// Check if `root` is one of the roots this pool actually recorded, `root` is little-endian,
    /// the same order the history stores.
    pub fn is_known_root(&self, root: &[u8; 32]) -> bool {
        // empty buffer, no deposits yet
        if *root == EMPTY_TREE_VALUE {
            return false;
        }

        for index in 0..ROOT_RING_BUFFER_LENGTH {
            if get_array_element(&self.roots_history, index) == *root {
                return true;
            }
        }
        false
    }

    /// Insert a commitment into the tree and record the resulting root in the ring buffer.
    pub fn insert(&mut self, leaf: [u8; 32]) -> Result<[u8; 32]> {
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
            self.last_root_idx = PodU32::from(self.last_root_idx.get() + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::imt_tree::u64_to_32bytes_le;

    /// A registry in the same state the `initialize` handler leaves it in.
    fn initialized_registry() -> RootRegistry {
        let mut registry: RootRegistry = bytemuck::Zeroable::zeroed();
        registry.initialize_empty().unwrap();
        registry
    }

    #[test]
    fn the_empty_tree_root_is_known_but_the_unused_marker_is_not() {
        let registry = initialized_registry();
        let empty_tree_root = registry.imt.root;

        // Slot 0 holds the empty tree's root. Slots 1..99 still hold the unused marker.
        assert!(registry.is_known_root(&empty_tree_root));
        assert!(!registry.is_known_root(&EMPTY_TREE_VALUE));

        // The two must never collide: the empty tree's root is a Poseidon hash of zero
        // values, the marker is the field value one.
        assert_ne!(empty_tree_root, EMPTY_TREE_VALUE);
        assert_eq!(get_array_element(&registry.roots_history, 1), EMPTY_TREE_VALUE);
    }

    #[test]
    fn a_root_that_was_never_recorded_is_rejected() {
        let registry = initialized_registry();
        assert!(!registry.is_known_root(&u64_to_32bytes_le(42)));
        assert!(!registry.is_known_root(&[0u8; 32]));
    }

    #[test]
    fn every_root_still_in_the_buffer_stays_known() {
        let mut registry = initialized_registry();
        let mut recorded = std::vec![registry.imt.root];

        for leaf in 1..=5u64 {
            recorded.push(registry.insert(u64_to_32bytes_le(leaf)).unwrap());
        }

        assert_eq!(registry.last_root_idx.get(), 5);
        for (position, root) in recorded.iter().enumerate() {
            assert!(
                registry.is_known_root(root),
                "root recorded at slot {position} is no longer accepted"
            );
        }
    }

    #[test]
    fn an_overwritten_history_entry_is_rejected() {
        let mut registry = initialized_registry();
        let empty_tree_root = registry.imt.root;

        // Slot 0 holds the empty tree's root. Inserts fill slots 1..=99, then the index
        // wraps to 0 and insert number 100 writes over the empty tree's root.
        let mut recorded = std::vec![];
        for leaf in 1..=ROOT_RING_BUFFER_LENGTH as u64 {
            recorded.push(registry.insert(u64_to_32bytes_le(leaf)).unwrap());
        }
        assert_eq!(registry.last_root_idx.get(), 0);

        assert!(!registry.is_known_root(&empty_tree_root));
        for root in &recorded {
            assert!(registry.is_known_root(root));
        }
    }
}
