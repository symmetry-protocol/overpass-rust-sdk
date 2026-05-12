use crate::error::OverpassError;

pub const FULL_BPS: u32 = 10_000;

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
