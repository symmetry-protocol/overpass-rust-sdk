//! Port of <https://github.com/lulo-labs/lulo-v2-cpi>.

use crate::common::bytes::read_u64;
use crate::error::OverpassError;

const OFF_PROTECTED_TOTAL_SUPPLY: usize = 8 + 48;
const OFF_PROTECTED_AMOUNT: usize = 8 + 120;
const MIN_BYTES_FOR_QUOTE: usize = OFF_PROTECTED_AMOUNT + 8;

#[derive(Debug, Clone, Default)]
pub struct LuloState {
    pub protected_amount: u64,
    pub protected_total_supply: u64,
}

pub fn empty() -> LuloState {
    LuloState::default()
}

pub fn decode(data: &[u8]) -> Result<LuloState, OverpassError> {
    if data.len() < MIN_BYTES_FOR_QUOTE {
        return Err(OverpassError::InvalidAccount("lulo pool: length"));
    }
    let protected_amount = read_u64(data, OFF_PROTECTED_AMOUNT, "lulo.protected_amount")?;
    let protected_total_supply =
        read_u64(data, OFF_PROTECTED_TOTAL_SUPPLY, "lulo.protected_total_supply")?;

    Ok(LuloState {
        protected_amount,
        protected_total_supply,
    })
}
