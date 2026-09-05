/// Same Keccak256 as on-chain `sol_keccak256`. Host tests cannot call that
/// syscall, so this uses the Solana hasher crate with its `sha3` feature.
pub fn calculate_proof_hash(proof: &[u8]) -> [u8; 32] {
    solana_keccak_hasher::hash(proof).to_bytes()
}
