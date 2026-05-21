use crate::common::u256::U256;
use crate::error::OverpassError;

pub const FULL_BPS: u32 = 10_000;

pub fn require_no_value_per_share_loss(
    old_value: u128,
    old_supply: u64,
    new_value: u128,
    new_supply: u64,
) -> Result<(), OverpassError> {
    if old_supply == 0 || new_supply == 0 {
        return Ok(());
    }
    let lhs = U256::from(new_value) * U256::from(old_supply);
    let rhs = U256::from(old_value) * U256::from(new_supply);
    if lhs < rhs {
        return Err(OverpassError::ValuePerShareDecreased);
    }
    Ok(())
}

pub fn mul_div_u64(a: u64, b: u64, denom: u64) -> Result<u64, OverpassError> {
    let prod = (a as u128)
        .checked_mul(b as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let quot = prod
        .checked_div(denom as u128)
        .ok_or(OverpassError::MathOverflow)?;
    u64::try_from(quot).map_err(|_| OverpassError::MathOverflow)
}

pub fn u128_to_u64(v: u128) -> Result<u64, OverpassError> {
    u64::try_from(v).map_err(|_| OverpassError::MathOverflow)
}

pub fn mul_bps(amount: u64, bps: u16) -> Result<u64, OverpassError> {
    mul_div_u64(amount, bps as u64, FULL_BPS as u64)
}
