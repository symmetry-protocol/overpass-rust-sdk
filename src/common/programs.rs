use jupiter_amm_interface::AccountMap;
use solana_program::{pubkey, pubkey::Pubkey};

use super::bytes::read_u64;
use crate::error::OverpassError;

const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;

pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ATA_PROGRAM_ID: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

pub fn derive_ata(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ATA_PROGRAM_ID,
    )
    .0
}

pub fn read_token_amount_or_zero(
    account_map: &AccountMap,
    key: &Pubkey,
) -> Result<u64, OverpassError> {
    match account_map.get(key) {
        Some(a) if !a.data.is_empty() => {
            read_u64(&a.data, TOKEN_ACCOUNT_AMOUNT_OFFSET, "token_account.amount")
        }
        _ => Ok(0),
    }
}
