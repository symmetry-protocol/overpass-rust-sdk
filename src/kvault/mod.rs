pub mod account_metas;
pub mod math;
pub mod state;
pub mod update;

use solana_program::{pubkey, pubkey::Pubkey};
use state::KvaultState;

pub const KVAULT_PROGRAM_ID: Pubkey = pubkey!("KvauGMspG5k6rtzrqqn7WNn3oZdyKqLKwK2XWQ8FLjd");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0x9e, 0xa8, 0x14, 0xa4, 0x47, 0x9b, 0x1c, 0xa3];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0xe0, 0x69, 0xdf, 0xf4, 0xc5, 0xfa, 0x41, 0x3e];

pub fn program_dependencies() -> Vec<(Pubkey, String)> {
    vec![
        (KVAULT_PROGRAM_ID, "kamino_vault".to_string()),
        (crate::klend::KLEND_PROGRAM_ID, "kamino_lend".to_string()),
        (account_metas::FARMS_PROGRAM_ID, "kamino_farms".to_string()),
    ]
}

pub const ACCOUNTS_LEN: usize = 37;

pub fn accounts_len(state: &KvaultState, _wv: &crate::accounts::WrapperVault) -> usize {
    ACCOUNTS_LEN + 2 * state.active_allocations.len()
}

pub fn is_active(_state: &KvaultState) -> bool {
    false
}
