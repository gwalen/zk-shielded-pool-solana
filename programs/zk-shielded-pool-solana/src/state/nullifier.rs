use anchor_lang::prelude::*;

/// Spent-nullifier marker. One PDA per withdrawn nullifier.
///
/// Seeds: `[b"nullifier", nullifier_be]` where `nullifier_be` is the
/// big-endian `public_inputs.nullifier` (Poseidon(s, step)).
#[account]
pub struct Nullifier {
    pub bump: u8,
}
