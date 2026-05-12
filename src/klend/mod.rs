pub mod account_metas;
pub mod fraction;
pub mod math;
pub mod state;
pub mod update;
pub mod withdrawal_caps;

use solana_program::{pubkey, pubkey::Pubkey};
use state::KlendState;

pub const KLEND_PROGRAM_ID: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0x63, 0xd5, 0x64, 0x24, 0xeb, 0xa4, 0x87, 0x42];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0x9a, 0xa6, 0x06, 0x52, 0xdd, 0xc0, 0xc5, 0x9e];

pub fn program_dependencies() -> Vec<(Pubkey, String)> {
    vec![(KLEND_PROGRAM_ID, "kamino_lend".to_string())]
}

pub const ACCOUNTS_LEN: usize = 26;

pub fn is_active(state: &KlendState) -> bool {
    state.status == 0
        && !state.emergency_mode
        && !state.block_ctoken_usage
        && state.version == math::PROGRAM_VERSION
}
