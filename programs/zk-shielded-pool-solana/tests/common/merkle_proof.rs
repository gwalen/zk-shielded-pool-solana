use anyhow::{Error, Result};
use zk_shielded_pool_solana::utils::{constants::EMPTY_TREE_VALUE, poseidon_hash};

/**
 * Note: We are starting at index 1 (leaving index 0 unused makes the math clean):
 * Example tree indexes for depth = 3 :
 *            1                - level 3 (root)
 *      2             3        - level 2
 *   4     5      6      7     - level 1
 *  8 9  10 11  12 13  14 15   - level 0 (leafs)
 *
 * Example proof for leaf at node_8:
 * proof.siblings_path = [node_9, node_5, node_3]
 * proof.siblings_side = [1, 1, 1]
 * proof.leaf = node_8
 */
pub struct MerkleProof {
    pub leaf: [u8; 32], // Fr (field element), we prove that this leaf node is part of the tree
    pub siblings_path: Vec<[u8; 32]>, // vector of Fr
    pub siblings_side: Vec<bool>, // side on which node on the path is (0 for left, 1 for right)
}

impl MerkleProof {
    pub fn new(leaf: [u8; 32], siblings_path: Vec<[u8; 32]>, siblings_side: Vec<bool>) -> Self {
        Self {
            leaf,
            siblings_path,
            siblings_side,
        }
    }

    // Taken from zk-shielded-pool-circuit project
    // TODO: move to shared crate with zk-shielded-pool-circuit ?
    pub fn verify_merkle_proof(&self, root: [u8; 32]) -> Result<bool> {
        let mut parent = EMPTY_TREE_VALUE;
        let mut other_sibling = self.leaf;
        for i in 0..self.siblings_path.len() {
            let sibling = self.siblings_path[i];
            let is_left = self.siblings_side[i]; // 1 - left, 0 - right
            if is_left {
                parent = poseidon_hash::hash2(sibling, other_sibling)
                    .map_err(|e| Error::msg(format!("{e}")))?;
            } else {
                parent = poseidon_hash::hash2(other_sibling, sibling)
                    .map_err(|e| Error::msg(format!("{e}")))?;
            }
            // level up
            other_sibling = parent;
        }

        // check if calculated hash equals root
        Ok(root == parent)
    }
}
