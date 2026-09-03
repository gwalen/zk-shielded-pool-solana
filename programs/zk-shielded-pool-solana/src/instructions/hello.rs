use crate::utils::events::BytesEvent;
use anchor_lang::prelude::*;

/// Accounts for the hello instruction.
/// A payer (signer) is required to submit the transaction, but the program
/// simply logs a greeting and the program ID.
#[derive(Accounts)]
pub struct HelloAccountConstraints {
    #[allow(dead_code)]
    pub payer: Signer,
}

#[inline(always)]
pub fn handle_hello(_ctx: &mut Context<HelloAccountConstraints>) -> Result<()> {
    msg!("Hello, Solana!");
    msg!("Our program's Program ID: {}", crate::ID);
    emit!(BytesEvent {
        flag: 0x01,
        val1: 0x1234,
        vector: alloc::vec![0x01, 0x02, 0x03],
        hash: [0xAA; 3],
        amount: 1,
        hash2: [0xAA; 11],
    });
    Ok(())
}