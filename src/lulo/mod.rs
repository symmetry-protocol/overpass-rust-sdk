pub mod account_metas;
pub mod math;
pub mod state;
pub mod update;

use solana_program::{pubkey, pubkey::Pubkey};
use state::LuloState;

pub const LULO_PROGRAM_ID: Pubkey = pubkey!("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0xe7, 0x17, 0x29, 0x7c, 0xe2, 0x0c, 0x4a, 0xef];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0xf4, 0xa6, 0xeb, 0x1b, 0x92, 0xf2, 0x79, 0x23];

pub fn program_dependencies() -> Vec<(Pubkey, String)> {
    vec![
        (LULO_PROGRAM_ID, "lulo".to_string()),
        (
            account_metas::JUP_LIQUIDITY_PROGRAM,
            "jupiter_liquidity".to_string(),
        ),
        (account_metas::JUP_PROGRAM, "jupiter_lend".to_string()),
    ]
}

pub const ACCOUNTS_LEN: usize = 36;

pub fn is_active(_state: &LuloState) -> bool {
    true
}
