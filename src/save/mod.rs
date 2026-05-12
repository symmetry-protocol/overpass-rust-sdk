pub mod account_metas;
pub mod math;
pub mod state;
pub mod update;
pub mod wad;

use solana_program::{pubkey, pubkey::Pubkey};
use state::SaveState;

pub const SAVE_PROGRAM_ID: Pubkey = pubkey!("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0xe4, 0x76, 0x53, 0x1d, 0xb1, 0xf3, 0x08, 0x78];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0x3c, 0x22, 0x09, 0x4d, 0x0c, 0x77, 0x42, 0x69];

pub fn program_dependencies() -> Vec<(Pubkey, String)> {
    vec![(SAVE_PROGRAM_ID, "save_lend".to_string())]
}

pub const ACCOUNTS_LEN: usize = 23;

pub fn accounts_len(state: &SaveState) -> usize {
    if state.extra_oracle_pubkey == Pubkey::default() {
        ACCOUNTS_LEN - 1
    } else {
        ACCOUNTS_LEN
    }
}

pub fn is_active(_state: &SaveState) -> bool {
    true
}
