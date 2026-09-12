use anyhow::{Error, Result};
use halo2_base::halo2_proofs::halo2curves::{bn256::Fr, ff::PrimeField};
use solana_poseidon::{Endianness, Parameters};
use zk_shielded_pool_solana::utils::merkle_proof::MerkleProof;

pub const Z_0: Fr = Fr::zero();
// tree node value that was not updated yet
pub const EMPTY_TREE_VALUE_FR: Fr = Fr::one();

// 1M leafs and total size of full tree 64MB (32 bytes per leaf)
pub const TREE_DEPTH_MAX: usize = 20;

// ********************
// This is a memory-heavy full IMT tree implementation used as a correctness
// reference in tests. It can reconstruct a tree and build a Merkle proof of
// inclusion for a given leaf node (commitment hash), but it is not optimized as
// a production client/indexer.
//
// TODO: A production client/indexer should use an incremental or sparse tree,
// maintain the historical tree versions needed for recent roots, and construct
// a path against a specifically chosen root from the on-chain root history.
//
// It stores hash, Fr::zero and Fr::one are not possible values
// ********************

// Note that we start node indexing from 1 (nodes[0] is unused)
pub struct OffChainImt {
    pub nodes: Vec<Fr>,
    pub zero_values: Vec<Fr>,
    pub first_leaf_idx: usize,
    pub next_free_leaf_idx: usize,
    pub tree_depth: u32,
}

impl OffChainImt {
    pub fn new(tree_depth: u32) -> Self {
        assert!(tree_depth > 0, "Tree depth must be greater than 0");
        assert!(
            tree_depth <= TREE_DEPTH_MAX as u32,
            "Tree depth is too large"
        );

        // For tree_depth = 20 node_code ~= 2M
        let node_count = 2usize.pow(tree_depth + 1);
        let nodes = vec![EMPTY_TREE_VALUE_FR; node_count];

        let zero_values = generate_zero_values_for_levels(tree_depth as usize);
        // We need to skip all the nodes on levels above the zero level where leafs are stored
        // Note: We start node indexing from 1 this makes the math cleaner
        let first_leaf_idx = 2usize.pow(tree_depth as u32);

        let mut off_chain_imt = Self {
            nodes,
            zero_values,
            first_leaf_idx,
            next_free_leaf_idx: first_leaf_idx,
            tree_depth,
        };
        // build the empty tree with zero values
        off_chain_imt.build_tree();
        off_chain_imt
    }

    pub fn root(&self) -> Fr {
        self.nodes[1]
    }

    // Insert leaf on next unused leaf position, do not recalculate the full tree
    pub fn insert_leaf_lazy(&mut self, leaf: Fr) -> Result<()> {
        if leaf == EMPTY_TREE_VALUE_FR || leaf == Z_0 {
            return Err(Error::msg("Leaf value is not valid"));
        }
        if self.next_free_leaf_idx >= self.nodes.len() {
            return Err(Error::msg("Tree is full"));
        }
        self.nodes[self.next_free_leaf_idx] = leaf;
        self.next_free_leaf_idx += 1;
        Ok(())
    }

    pub fn build_tree(&mut self) {
        // first fill the leaf level
        for i in self.first_leaf_idx..self.nodes.len() {
            self.nodes[i] = self.node(i);
        }

        let mut last_level_start_idx = self.first_leaf_idx;
        // fill other levels
        for level in 1..=self.tree_depth {
            let level_start_idx = 2usize.pow(self.tree_depth - level);
            for i in level_start_idx..last_level_start_idx {
                let left_child = self.node(i * 2);
                let right_child = self.node(i * 2 + 1);
                self.nodes[i] = poseidon_hash(left_child, right_child)
            }
            last_level_start_idx = level_start_idx;
        }
    }

    pub fn build_merkle_proof(&self, leaf: Fr) -> Result<MerkleProof> {
        let mut proof = MerkleProof {
            leaf: fr_to_le_bytes(leaf),
            siblings_path: Vec::new(),
            siblings_side: Vec::new(),
        };
        let leaf_idx = self
            .find_leaf_index(leaf)
            .ok_or_else(|| Error::msg("Leaf is not in the tree"))?;

        let mut current_idx = leaf_idx;
        while current_idx > 1 {
            let is_left_leaf = current_idx % 2 == 0;
            let sibling = if is_left_leaf {
                self.nodes[current_idx + 1]
            } else {
                self.nodes[current_idx - 1]
            };
            proof.siblings_path.push(fr_to_le_bytes(sibling));
            // The current on-chain verifier hashes (sibling, current) for true.
            // Therefore true means the sibling is on the left, despite its
            // field comment retaining the circuit's old 0-left / 1-right rule.
            proof.siblings_side.push(!is_left_leaf);
            current_idx /= 2; // go level up to parent idx
        }
        Ok(proof)
    }

    fn node(&self, idx: usize) -> Fr {
        if self.nodes[idx] == EMPTY_TREE_VALUE_FR {
            // unset node, fetch zero value based on level
            let node_level = self.calculate_level(idx);
            self.zero_values[node_level]
        } else {
            self.nodes[idx]
        }
    }

    fn find_leaf_index(&self, leaf: Fr) -> Option<usize> {
        for i in self.first_leaf_idx..self.nodes.len() {
            if self.nodes[i] == leaf {
                return Some(i);
            }
        }
        None
    }

    /**
     * Note: We are starting at index 1 (leaving index 0 unused makes the math clean):
     * Example tree indexes for depth = 3 :
     *            1                - level 3 (root)
     *      2             3        - level 2
     *   4     5      6      7     - level 1
     *  8 9  10 11  12 13  14 15   - level 0 (leafs)
     */
    fn calculate_level(&self, node_idx: usize) -> usize {
        // ilog2 - does integer bit logic, not floating-point logarithms. It asks: “what is the position of the highest set bit?”
        let depth_from_root = node_idx.ilog2();
        (self.tree_depth - depth_from_root) as usize
    }
}

pub fn fr_to_le_bytes(value: Fr) -> [u8; 32] {
    value.to_repr()
}

pub fn fr_from_le_bytes(bytes: [u8; 32]) -> Fr {
    Fr::from_repr(bytes).unwrap()
}

pub fn poseidon_hash(left: Fr, right: Fr) -> Fr {
    let hash = solana_poseidon::hashv(
        Parameters::Bn254X5,
        Endianness::LittleEndian,
        &[&fr_to_le_bytes(left), &fr_to_le_bytes(right)],
    )
    .unwrap();
    fr_from_le_bytes(hash.to_bytes())
}

// Helper used in tests to easy get Fr from small integer values
pub fn hash1(seed: u64) -> Fr {
    let hash = solana_poseidon::hash(
        Parameters::Bn254X5,
        Endianness::LittleEndian,
        &fr_to_le_bytes(Fr::from(seed)),
    )
    .unwrap();
    fr_from_le_bytes(hash.to_bytes())
}

pub fn generate_zero_values_for_levels(tree_depth: usize) -> Vec<Fr> {
    // zero values for each level (except root level)
    // Example for depth 3: 0 (leaf) -> 1 (level 1) -> 2 (level 2) -> 3 (root)
    //                         Z_0   ->    Z_1      ->  Z_2        ->  this we do not need to store in zero_values (used only for root calculation)
    let mut zero_values = Vec::<Fr>::with_capacity(tree_depth);

    zero_values.push(Z_0);

    for i in 1..tree_depth {
        let z_prev = zero_values[i - 1];
        zero_values.push(poseidon_hash(z_prev, z_prev));
    }

    zero_values
}

#[cfg(test)]
pub mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // Snapshot of the depth-3 reference tree (leafs = commitment(1..=8)).
    // These are the single source of truth for IMT expected values: the
    // on-chain tests cross-check against this off-chain IMT at runtime instead of
    // duplicating constants. Regenerate only if the hash impl legitimately
    // changes (a change here that you didn't intend means something broke).
    // ---------------------------------------------------------------------
    pub(crate) const Z0_HEX: &str =
        "0x0000000000000000000000000000000000000000000000000000000000000000";
    pub(crate) const Z1_HEX: &str =
        "0x2098f5fb9e239eab3ceac3f27b81e481dc3124d55ffed523a839ee8446b64864";
    pub(crate) const Z2_HEX: &str =
        "0x1069673dcdb12263df301a6ff584a7ec261a44cb9dc68df067a4774460b1f1e1";
    pub(crate) const EMPTY_ROOT_HEX: &str =
        "0x18f43331537ee2af2e3d758d50f72106467c6eea50371dd528d57eb2b856d238";
    pub(crate) const SINGLE_ROOT_HEX: &str =
        "0x07eac97f63c362dc3151636e70236b1528a2b9ed70314ccab77a590cd7da7463";
    pub(crate) const PARTIAL3_ROOT_HEX: &str =
        "0x102120bf2b43d7898df1064785e27a9a6f871039b371cf7be59588de6ddb8c52";
    // Full depth-3 tree, all 15 used nodes in array order
    // Includes root, internal nodes, and leaves.
    // Note:
    // It has length 15 because it stores only the used tree nodes, without unused nodes[0], so FULL_DEPTH3_TREE_NODES[0] -> off_chain_imt.nodes[1]
    pub(crate) const FULL_DEPTH3_TREE_NODES: [&str; 15] = [
        "0x1c941927a5dfda40573b22729c1c627c0ae71b7e68dd1bd873d533076e009829",
        "0x0ae35a1d69b5edf22c9c8f3c516e71844d314d2783e6be55ecbb4041dd0f4da8",
        "0x2e2649c7b46d9873f008c516182b540108b41ccf412019564403fa3d9ae3c112",
        "0x163d03f7550746164d022ae803e93c1abe652ec202c073b84e75e737deb4df56",
        "0x23144c1e7794f62515c2ccbaee3076d2e40b673fcba5da8a6457387e054068e0",
        "0x2653538c46d11b9ac939edcd686ec116d66ef466b8542e664f21984837fd99ce",
        "0x06070d18c13133fed911ae4554348a984a42bb12d57314daeea7c45820c4feb5",
        "0x29176100eaa962bdc1fe6c654d6a3c130e96a4d1168b33848b897dc502820133",
        "0x131d73cf6b30079aca0dff6a561cd0ee50b540879abe379a25a06b24bde2bebd",
        "0x0d4e4d24b890fe6799be4cf57ad13078ec0fbaa9fe91423ba8bbd0c2d7043bd4",
        "0x15e36f4ff92e2211fa8ed9f7af707f6c8c0f1442252a85150d2b8d2038890dfc",
        "0x2a267e27e712412e8eefec1e174ce85b1af2f2d9a8014fa4dc723abb4d27ef7d",
        "0x094b8e7acd789372d446e21dcc80162aba6c1923ae3b9a30702f64f0aea70295",
        "0x0f9cebf54307bbb3646866aa15d2cd6e961caea77048b87f4261b7636240254e",
        "0x135ec460f4a519cb3a7eb19a4e3486c6d25bad46c5b7af029af91009534c3be4",
    ];

    fn hex(f: Fr) -> String {
        format!("{:?}", f)
    }

    // test_layout_depth3 — first_leaf_idx==8, nodes.len()==16, next_free_leaf_idx==8 after new(3) | structural
    #[test]
    fn test_layout_depth3() {
        let off_chain_imt = OffChainImt::new(3);
        assert_eq!(off_chain_imt.first_leaf_idx, 8);
        assert_eq!(off_chain_imt.nodes.len(), 16);
        assert_eq!(off_chain_imt.next_free_leaf_idx, 8);
    }

    // test_calculate_level — idx 1→3, 2–3→2, 4–7→1, 8–15→0 (pins the ilog2 math) | structural
    #[test]
    fn test_calculate_level() {
        let off_chain_imt = OffChainImt::new(3);
        assert_eq!(off_chain_imt.calculate_level(1), 3);
        for i in 2..=3 {
            assert_eq!(off_chain_imt.calculate_level(i), 2);
        }
        for i in 4..=7 {
            assert_eq!(off_chain_imt.calculate_level(i), 1);
        }
        for i in 8..=15 {
            assert_eq!(off_chain_imt.calculate_level(i), 0);
        }
    }

    // test_zero_values_snapshot — zero_values == [z0, z1, z2] | snapshot
    #[test]
    fn test_zero_values_snapshot() {
        let zv = generate_zero_values_for_levels(3);
        assert_eq!(hex(zv[0]), Z0_HEX);
        assert_eq!(hex(zv[1]), Z1_HEX);
        assert_eq!(hex(zv[2]), Z2_HEX);
    }

    // test_empty_root_snapshot — empty depth-3 root() matches snapshot, and == poseidon_hash(z2,z2) | snapshot
    #[test]
    fn test_empty_root_snapshot() {
        let off_chain_imt = OffChainImt::new(3);
        assert_eq!(hex(off_chain_imt.root()), EMPTY_ROOT_HEX);
        let zv = generate_zero_values_for_levels(3);
        assert_eq!(off_chain_imt.root(), poseidon_hash(zv[2], zv[2]));
    }

    // test_single_leaf_snapshot — insert one commitment, build_tree, root matches snapshot; path siblings resolve to zero_values | snapshot
    #[test]
    fn test_single_leaf_snapshot() {
        let zv = generate_zero_values_for_levels(3);
        let mut off_chain_imt = OffChainImt::new(3);
        off_chain_imt.insert_leaf_lazy(hash1(1)).unwrap();
        off_chain_imt.build_tree();
        assert_eq!(hex(off_chain_imt.root()), SINGLE_ROOT_HEX);
        // only leaf 0 (node 8) is set; the rest resolve to the leaf-level zero value Z_0
        assert_eq!(off_chain_imt.nodes[8], hash1(1));
        for i in 9..=15 {
            assert_eq!(off_chain_imt.nodes[i], zv[0]);
        }
    }

    // test_full_tree_snapshot — insert 8 commitment leafs, build_tree, assert entire 15-node nodes vector matches snapshot array | snapshot (strong)
    #[test]
    fn test_full_tree_snapshot() {
        let mut off_chain_imt = OffChainImt::new(3);
        for i in 1..=8 {
            off_chain_imt.insert_leaf_lazy(hash1(i)).unwrap();
        }
        off_chain_imt.build_tree();
        for (i, expected) in FULL_DEPTH3_TREE_NODES.iter().enumerate() {
            let node_idx = i + 1;
            assert_eq!(
                hex(off_chain_imt.nodes[node_idx]),
                *expected,
                "node {}",
                node_idx
            );
        }
    }

    // test_partial_tree_zero_substitution — insert 3 leafs, build, root matches snapshot (exercises node() zero-fill) | snapshot
    #[test]
    fn test_partial_tree_zero_substitution() {
        let zv = generate_zero_values_for_levels(3);
        let mut off_chain_imt = OffChainImt::new(3);
        for i in 1..=3 {
            off_chain_imt.insert_leaf_lazy(hash1(i)).unwrap();
        }
        off_chain_imt.build_tree();
        assert_eq!(hex(off_chain_imt.root()), PARTIAL3_ROOT_HEX);
        // leafs 3..7 (nodes 11..15) were never inserted -> zero-filled with Z_0
        for i in 11..=15 {
            assert_eq!(off_chain_imt.nodes[i], zv[0]);
        }
    }

    // test_insert_rejects_zero_and_one — insert_leaf_lazy(Z_0) and (EMPTY_VALUE) return Err | error
    #[test]
    fn test_insert_rejects_zero_and_one() {
        let mut off_chain_imt = OffChainImt::new(3);
        assert!(off_chain_imt.insert_leaf_lazy(Z_0).is_err());
        assert!(off_chain_imt.insert_leaf_lazy(EMPTY_TREE_VALUE_FR).is_err());
    }

    // test_tree_full — 9th insert on depth 3 → Err("Tree is full") | error
    #[test]
    fn test_tree_full() {
        let mut off_chain_imt = OffChainImt::new(3);
        for i in 1..=8 {
            off_chain_imt.insert_leaf_lazy(hash1(i)).unwrap();
        }
        assert!(off_chain_imt.insert_leaf_lazy(hash1(9)).is_err());
    }

    // test_build_tree_idempotent — build_tree() twice → same root | invariant
    #[test]
    fn test_build_tree_idempotent() {
        let mut off_chain_imt = OffChainImt::new(3);
        for i in 1..=5 {
            off_chain_imt.insert_leaf_lazy(hash1(i)).unwrap();
        }
        off_chain_imt.build_tree();
        let r1 = off_chain_imt.root();
        off_chain_imt.build_tree();
        assert_eq!(off_chain_imt.root(), r1);
    }

    #[test]
    fn test_merkle_proof_check() {
        let mut off_chain_imt = OffChainImt::new(3);
        for i in 1..=8 {
            off_chain_imt.insert_leaf_lazy(hash1(i)).unwrap();
        }
        off_chain_imt.build_tree();

        // build and check proof for each leaf
        for i in 1..=8 {
            let proof = off_chain_imt.build_merkle_proof(hash1(i)).unwrap();
            assert_eq!(proof.siblings_path.len(), 3);
            assert_eq!(proof.siblings_side.len(), 3);

            assert_eq!(proof.leaf, fr_to_le_bytes(hash1(i)));
            assert!(
                proof
                    .verify_merkle_proof(fr_to_le_bytes(off_chain_imt.root()))
                    .unwrap(),
                "proof mismatch for leaf {}",
                i
            );
        }
    }

    #[test]
    fn test_partial_tree_proof_bytes_and_sides() {
        let mut tree = OffChainImt::new(3);
        for i in 1..=3 {
            tree.insert_leaf_lazy(hash1(i)).unwrap();
        }
        tree.build_tree();
        let root = fr_to_le_bytes(tree.root());

        for i in 1..=3 {
            let proof = tree.build_merkle_proof(hash1(i)).unwrap();
            assert!(proof.verify_merkle_proof(root).unwrap());
        }

        // Node 9 has siblings 8 (left), 5 (right), and 3 (right).
        let mut proof = tree.build_merkle_proof(hash1(2)).unwrap();
        assert_eq!(proof.siblings_side, vec![true, false, false]);
        assert_eq!(
            proof.siblings_path,
            vec![
                fr_to_le_bytes(tree.nodes[8]),
                fr_to_le_bytes(tree.nodes[5]),
                fr_to_le_bytes(tree.nodes[3]),
            ]
        );
        proof.siblings_side[0] = false;
        assert!(!proof.verify_merkle_proof(root).unwrap());

        let mut proof = tree.build_merkle_proof(hash1(2)).unwrap();
        proof.leaf = fr_to_le_bytes(hash1(4));
        assert!(!proof.verify_merkle_proof(root).unwrap());
        assert!(tree.build_merkle_proof(hash1(4)).is_err());
    }

    #[test]
    fn test_field_bytes_are_little_endian() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x02;
        bytes[1] = 0x01;
        assert_eq!(fr_to_le_bytes(Fr::from(258)), bytes);
        assert_eq!(fr_from_le_bytes(bytes), Fr::from(258));
    }
}

// Throwaway helper to (re)generate the depth-3 snapshot constants used in the
// tests above. Run with: `cargo test print_snapshots -- --nocapture` and paste
// the printed values into the snapshot consts. Not an assertion test.
#[cfg(test)]
mod capture {
    use super::*;

    #[test]
    fn print_snapshots() {
        let zv = generate_zero_values_for_levels(3);
        println!("Z0 = {:?}", zv[0]);
        println!("Z1 = {:?}", zv[1]);
        println!("Z2 = {:?}", zv[2]);

        let empty = OffChainImt::new(3);
        println!("EMPTY_ROOT = {:?}", empty.root());

        let mut single = OffChainImt::new(3);
        single.insert_leaf_lazy(hash1(1)).unwrap();
        single.build_tree();
        println!("SINGLE_ROOT = {:?}", single.root());

        let mut partial = OffChainImt::new(3);
        for i in 1..=3 {
            partial.insert_leaf_lazy(hash1(i)).unwrap();
        }
        partial.build_tree();
        println!("PARTIAL3_ROOT = {:?}", partial.root());

        let mut full = OffChainImt::new(3);
        for i in 1..=8 {
            full.insert_leaf_lazy(hash1(i)).unwrap();
        }
        full.build_tree();
        for (i, n) in full.nodes.iter().enumerate() {
            println!("FULL_NODE[{}] = {:?}", i, n);
        }
    }
}
