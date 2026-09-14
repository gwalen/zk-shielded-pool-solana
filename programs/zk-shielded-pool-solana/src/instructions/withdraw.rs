use anchor_lang::prelude::*;

use halo2_solana_verifier::{
    curve::{G1, G2},
    kzg::KzgVk,
};

use crate::{
    utils::public_inputs::PublicInputs,
    state::{
        nullifier::Nullifier,
        proof_storage::{ProofStorage, PROOF_BUFFER_LEN},
        root_registry::RootRegistry,
        vault::Vault,
    },
    utils::{common::reverse_byte_order, dest_address_hash::dest_address_hash_le, errors::DappError},
};
use solana_define_syscall::definitions::sol_keccak256;

/// Circuit VK compiled into program data. A caller cannot swap a different
/// circuit. Source: solana-proof-generator/fixtures/vk.bin (749 bytes).
const PINNED_VK: &[u8] =
    include_bytes!("../../../../../solana-proof-generator/fixtures/vk.bin");

/// Trimmed KZG VK compiled into program data: `[1]_1` || `[1]_2` || `[tau]_2`.
/// Source: solana-proof-generator/fixtures/kzg_vk.bin (320 bytes).
const PINNED_KZG_VK_BYTES: &[u8] =
    include_bytes!("../../../../../solana-proof-generator/fixtures/kzg_vk.bin");

const G1_LEN: usize = 64;
const G2_LEN: usize = 128;
const KZG_VK_LEN: usize = G1_LEN + G2_LEN + G2_LEN;

fn pinned_kzg_vk() -> KzgVk {
    let mut g1_one = [0u8; G1_LEN];
    g1_one.copy_from_slice(&PINNED_KZG_VK_BYTES[..G1_LEN]);
    let mut g2_one = [0u8; G2_LEN];
    g2_one.copy_from_slice(&PINNED_KZG_VK_BYTES[G1_LEN..G1_LEN + G2_LEN]);
    let mut g2_tau = [0u8; G2_LEN];
    g2_tau.copy_from_slice(&PINNED_KZG_VK_BYTES[G1_LEN + G2_LEN..]);

    KzgVk {
        g1_one: G1(g1_one),
        g2_one: G2(g2_one),
        g2_tau: G2(g2_tau),
    }
}

#[derive(Accounts)]
#[instruction(proof_hash: [u8; 32], public_inputs: PublicInputs)]
pub struct Withdraw {
    #[account(mut)]
    pub sender: Signer,

    #[account(mut, seeds = [b"vault"], bump = vault.bump)]
    pub vault: Account<Vault>,

    #[account(mut, seeds = [b"root_registry"], bump = roots_registry.bump)]
    pub roots_registry: Account<RootRegistry>,

    #[account(
        init_if_needed,
        payer = sender,
        seeds = [b"proof_storage", sender.address().as_ref(), proof_hash.as_ref()],
        bump,
    )]
    pub proof_account: Account<ProofStorage>,

    /// Receives the payout. Does not sign. Its address is checked against
    /// `public_inputs.dest_address` in the handler, so any other account is rejected.
    #[account(mut)]
    pub recipient: UncheckedAccount,

    /// Nullifier marker for a withdrawal step. Created once, paid by the sender.
    /// `init` (not `init_if_needed`) prevents repeating the same withdrawal step.
    /// Each step represents a withdrawal of a chunk amount to its destination.
    /// Seeds contain only the fixed prefix and big-endian nullifier bytes: no sender, recipient, proof hash, or root.
    /// Once a proof for the given withdrawal step is accepted and the transaction succeeds, this nullifier cannot be used again.
    #[account(
        init,
        payer = sender,
        seeds = [b"nullifier", public_inputs.nullifier.as_ref()],
        bump,
    )]
    pub nullifier_account: Account<Nullifier>,

    pub system_program: Program<System>,
}

pub fn handle(
    ctx: &mut Context<Withdraw>,
    proof_hash: [u8; 32],         // 32 bytes
    public_inputs: &PublicInputs, // 5 * 32 bytes = 160 bytes
) -> Result<()> {
    // public_inputs.root is big-endian - that is what the verifier needs.
    // history stores little-endian, so we create a little-endian version for the check
    let root_le = reverse_byte_order(public_inputs.root);
    require!(
        ctx.accounts.roots_registry.is_known_root(&root_le),
        DappError::UnknownRoot
    );

    // Decoded from the same bytes the verifier checks below, so the amount is the proven one.
    let _chunk_amount_lamports = public_inputs.chunk_amount_u64()?;

    let stored_len = ctx.accounts.proof_account.proof_current_len.get() as usize;
    require!(stored_len <= PROOF_BUFFER_LEN, DappError::ProofBufferFull);

    let proof = ctx
        .accounts
        .proof_account
        .proof
        .get(..stored_len)
        .ok_or(DappError::FailedToReadProofFromStorage)?;
    require!(!proof.is_empty(), DappError::EmptyProof);

    let computed_proof_hash = hash_proof(proof);
    require!(computed_proof_hash == proof_hash, DappError::InvalidProofHash);

    require!(PINNED_KZG_VK_BYTES.len() == KZG_VK_LEN, DappError::ProofVerifierFailed);

    let pinned_vk = PINNED_VK;
    let pinned_kzg_vk = pinned_kzg_vk();

    let accepted = halo2_solana_verifier::verify_gwc(
        pinned_vk,
        proof,
        public_inputs.to_byte_chunks().as_ref(),
        &pinned_kzg_vk,
    )
    .map_err(|_| DappError::ProofVerifierFailed)?;

    require!(accepted, DappError::InvalidProof);

    // The proof commits to a hash of the destination address, not the address itself.
    // Hash the real recipient the same way. The hash comes back little-endian, the public
    // input is big-endian, so flip it before comparing.
    let recipient_hash_le = dest_address_hash_le(&ctx.accounts.recipient.address().to_bytes())?;
    require!(
        reverse_byte_order(recipient_hash_le) == public_inputs.dest_address,
        DappError::DestinationMismatch
    );

    // Anchor created the nullifier account before this handler ran (`init`).
    // If any check above failed, the whole transaction fails and Solana rolls back all changes,
    // so no spent marker is left. Record the bump only after all checks.
    ctx.accounts.nullifier_account.bump = ctx.bumps.nullifier_account;

    msg!("Proof verified");

    Ok(())
}

fn hash_proof(proof: &[u8]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let vals: &[&[u8]] = &[proof];
    // need use unsafe as sol_keccak256 is extern "C" which is considered unsafe by rust
    unsafe {
       sol_keccak256(
        vals.as_ptr() as *const u8, 
        1, // just one chunk to hash (proof array) 
        result.as_mut_ptr())    
    };
    result
}