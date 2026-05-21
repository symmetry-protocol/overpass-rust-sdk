use crate::common::bytes::{read_u128, read_u16, read_u64, read_u8};
use crate::error::OverpassError;

const OFF_HALTED_FLAG: usize = 8 + 5;
const OFF_BOOSTED_ONLY_HALTED_FLAG: usize = 8 + 6;
const OFF_COVERAGE_FLOAT_BPS: usize = 8 + 14;
const OFF_PROTECTED_TOTAL_SUPPLY: usize = 8 + 48;
const OFF_REGULAR_TOTAL_SUPPLY: usize = 8 + 56;
const OFF_REGULAR_AMOUNT: usize = 8 + 112;
const OFF_PROTECTED_AMOUNT: usize = 8 + 120;
const OFF_DEPOSIT_LIMIT: usize = 8 + 128;
const OFF_TOTAL_LIQUIDITY: usize = 8 + 200;
const MIN_BYTES_FOR_QUOTE: usize = OFF_TOTAL_LIQUIDITY + 8;

#[derive(Debug, Clone, Default)]
pub struct LuloState {
    pub halted_flag: u8,
    pub boosted_only_halted_flag: u8,
    pub coverage_float_bps: u16,
    pub protected_total_supply: u64,
    pub regular_total_supply: u64,
    pub regular_amount: u64,
    pub protected_amount: u64,
    pub deposit_limit: u64,
    pub total_liquidity: u64,
    pub basis_q60: u128,
    pub charged_q60: u128,
    pub avg_q60: u128,
    pub ref_bps: u16,
}

pub fn empty() -> LuloState {
    LuloState::default()
}

pub fn decode(data: &[u8]) -> Result<LuloState, OverpassError> {
    if data.len() < MIN_BYTES_FOR_QUOTE {
        return Err(OverpassError::InvalidAccount("lulo pool: length"));
    }
    let halted_flag = read_u8(data, OFF_HALTED_FLAG, "lulo.halted_flag")?;
    let boosted_only_halted_flag = read_u8(
        data,
        OFF_BOOSTED_ONLY_HALTED_FLAG,
        "lulo.boosted_only_halted_flag",
    )?;
    let coverage_float_bps = read_u16(data, OFF_COVERAGE_FLOAT_BPS, "lulo.coverage_float_bps")?;
    let protected_amount = read_u64(data, OFF_PROTECTED_AMOUNT, "lulo.protected_amount")?;
    let protected_total_supply = read_u64(
        data,
        OFF_PROTECTED_TOTAL_SUPPLY,
        "lulo.protected_total_supply",
    )?;
    let regular_total_supply =
        read_u64(data, OFF_REGULAR_TOTAL_SUPPLY, "lulo.regular_total_supply")?;
    let regular_amount = read_u64(data, OFF_REGULAR_AMOUNT, "lulo.regular_amount")?;
    let deposit_limit = read_u64(data, OFF_DEPOSIT_LIMIT, "lulo.deposit_limit")?;
    let total_liquidity = read_u64(data, OFF_TOTAL_LIQUIDITY, "lulo.total_liquidity")?;
    Ok(LuloState {
        halted_flag,
        boosted_only_halted_flag,
        coverage_float_bps,
        protected_amount,
        protected_total_supply,
        regular_total_supply,
        regular_amount,
        deposit_limit,
        total_liquidity,
        basis_q60: 0,
        charged_q60: 0,
        avg_q60: 0,
        ref_bps: 0,
    })
}

pub fn decode_charged_avg_q60(data: &[u8]) -> Result<(u128, u128), OverpassError> {
    let charged = read_u128(
        data,
        POOL_USER_PROTECTED_CHARGED_PRICE,
        "lulo.pool_user.charged_price",
    )?;
    let avg = read_u128(
        data,
        POOL_USER_PROTECTED_AVG_PRICE,
        "lulo.pool_user.avg_price",
    )?;
    Ok((charged, avg))
}

const POOL_USER_PROTECTED_AVG_PRICE: usize = 96;
const POOL_USER_PROTECTED_CHARGED_PRICE: usize = 112;
const POOL_USER_REFERRAL_FEE_BPS: usize = 160;

pub fn decode_basis_q60(data: &[u8]) -> Result<u128, OverpassError> {
    let charged = read_u128(
        data,
        POOL_USER_PROTECTED_CHARGED_PRICE,
        "lulo.pool_user.charged_price",
    )?;
    if charged != 0 {
        return Ok(charged);
    }
    read_u128(
        data,
        POOL_USER_PROTECTED_AVG_PRICE,
        "lulo.pool_user.avg_price",
    )
}

pub fn decode_ref_bps(data: &[u8]) -> Result<u16, OverpassError> {
    read_u16(data, POOL_USER_REFERRAL_FEE_BPS, "lulo.referrer.fee_bps")
}
