use jupiter_amm_interface::{QuoteParams, SwapParams};

use crate::error::OverpassError;
use crate::accounts::WrapperVault;

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Deposit,
    Withdraw,
}

pub fn from_quote(wv: &WrapperVault, params: &QuoteParams) -> Result<Direction, OverpassError> {
    detect(wv, params.input_mint, params.output_mint)
}

pub fn from_swap(wv: &WrapperVault, params: &SwapParams) -> Result<Direction, OverpassError> {
    detect(wv, params.source_mint, params.destination_mint)
}

fn detect(wv: &WrapperVault, src: solana_program::pubkey::Pubkey, dst: solana_program::pubkey::Pubkey) -> Result<Direction, OverpassError> {
    if src == wv.underlying_mint && dst == wv.wrapper_mint {
        Ok(Direction::Deposit)
    } else if src == wv.wrapper_mint && dst == wv.underlying_mint {
        Ok(Direction::Withdraw)
    } else {
        Err(OverpassError::InvalidState("unsupported mint pair"))
    }
}
