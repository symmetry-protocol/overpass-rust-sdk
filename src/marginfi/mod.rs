pub mod account_metas;
pub mod math;
pub mod state;
pub mod update;

use solana_program::{pubkey, pubkey::Pubkey};
use state::{BankOperationalState, MarginfiState};

pub const MARGINFI_PROGRAM_ID: Pubkey = pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0x67, 0x07, 0xd7, 0x7b, 0xb2, 0xf4, 0xb9, 0xa3];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0xd5, 0x85, 0x8d, 0x5d, 0xf2, 0x3a, 0xf0, 0x85];

pub fn program_dependencies() -> Vec<(Pubkey, String)> {
    vec![(MARGINFI_PROGRAM_ID, "marginfi".to_string())]
}

pub const ACCOUNTS_LEN: usize = 21;

pub fn accounts_len(state: &MarginfiState) -> usize {
    ACCOUNTS_LEN + 1 + state::remaining_accounts_per_bank(state.oracle_setup)
}

pub fn is_active(state: &MarginfiState) -> bool {
    matches!(state.operational_state, BankOperationalState::Operational)
}
