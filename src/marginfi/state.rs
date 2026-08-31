use fixed::types::I80F48;
use solana_program::pubkey::Pubkey;

use crate::common::bytes::{read_pubkey, read_u32, read_u64, read_u128};
use crate::error::OverpassError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BankOperationalState {
    #[default]
    Paused,
    Operational,
    ReduceOnly,
    KilledByBankruptcy,
}

impl BankOperationalState {
    fn from_u8(b: u8) -> Result<Self, OverpassError> {
        match b {
            0 => Ok(Self::Paused),
            1 => Ok(Self::Operational),
            2 => Ok(Self::ReduceOnly),
            3 => Ok(Self::KilledByBankruptcy),
            _ => Err(OverpassError::InvalidState(
                "marginfi: bad operational_state",
            )),
        }
    }
}

pub const INTEREST_CURVE_LEGACY: u8 = 0;
pub const INTEREST_CURVE_SEVEN_POINT: u8 = 1;

#[derive(Debug, Clone, Copy, Default)]
pub struct RatePoint {
    pub util: u32,
    pub rate: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct InterestRateConfig {
    pub optimal_utilization_rate: I80F48,
    pub plateau_interest_rate: I80F48,
    pub max_interest_rate: I80F48,

    pub insurance_fee_fixed_apr: I80F48,
    pub insurance_ir_fee: I80F48,
    pub protocol_fixed_fee_apr: I80F48,
    pub protocol_ir_fee: I80F48,

    pub zero_util_rate: u32,
    pub hundred_util_rate: u32,
    pub points: [RatePoint; 5],
    pub curve_type: u8,
}

impl Default for InterestRateConfig {
    fn default() -> Self {
        Self {
            optimal_utilization_rate: I80F48::ZERO,
            plateau_interest_rate: I80F48::ZERO,
            max_interest_rate: I80F48::ZERO,
            insurance_fee_fixed_apr: I80F48::ZERO,
            insurance_ir_fee: I80F48::ZERO,
            protocol_fixed_fee_apr: I80F48::ZERO,
            protocol_ir_fee: I80F48::ZERO,
            zero_util_rate: 0,
            hundred_util_rate: 0,
            points: [RatePoint::default(); 5],
            curve_type: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarginfiState {
    pub mint: Pubkey,
    pub mint_decimals: u8,
    pub asset_share_value: I80F48,
    pub liability_share_value: I80F48,
    pub liquidity_vault: Pubkey,
    pub total_liability_shares: I80F48,
    pub total_asset_shares: I80F48,
    pub last_update: i64,
    pub deposit_limit: u64,
    pub operational_state: BankOperationalState,
    pub asset_tag: u8,
    pub cached_lending_rate: u32,
    pub cached_user_asset_shares: I80F48,
    pub underlying_token_program: Pubkey,
    pub oracle_setup: u8,
    pub oracle_keys: [Pubkey; 5],
    pub interest_rate_config: InterestRateConfig,
    pub group_program_fees_enabled: bool,
    pub group_program_fee_fixed: I80F48,
    pub group_program_fee_rate: I80F48,
}

pub const PD_OFF_UNDERLYING_TOKEN_KIND: usize = 98;

pub const BANK_LEN: usize = 1864;
pub const BANK_DISCRIMINATOR: [u8; 8] = [142, 49, 166, 242, 50, 66, 97, 188];

const OFF_MINT: usize = 8;
const OFF_MINT_DECIMALS: usize = 40;
const OFF_ASSET_SHARE_VALUE: usize = 80;
const OFF_LIABILITY_SHARE_VALUE: usize = 96;
const OFF_LIQUIDITY_VAULT: usize = 112;
const OFF_TOTAL_LIABILITY_SHARES: usize = 256;
const OFF_TOTAL_ASSET_SHARES: usize = 272;
const OFF_LAST_UPDATE: usize = 288;
const OFF_CONFIG_DEPOSIT_LIMIT: usize = 360;
const OFF_IR_OPTIMAL_UTIL_RATE: usize = 368;
const OFF_IR_PLATEAU_RATE: usize = 384;
const OFF_IR_MAX_RATE: usize = 400;
const OFF_IR_INSURANCE_FIXED_APR: usize = 416;
const OFF_IR_INSURANCE_IR_FEE: usize = 432;
const OFF_IR_PROTOCOL_FIXED_APR: usize = 448;
const OFF_IR_PROTOCOL_IR_FEE: usize = 464;
const OFF_IR_ZERO_UTIL_RATE: usize = 496;
const OFF_IR_HUNDRED_UTIL_RATE: usize = 500;
const OFF_IR_POINTS: usize = 504;
const OFF_IR_CURVE_TYPE: usize = 544;
const OFF_CONFIG_OPERATIONAL_STATE: usize = 608;
const OFF_CONFIG_ORACLE_SETUP: usize = 609;
const OFF_CONFIG_ORACLE_KEYS: usize = 610;
const OFF_CONFIG_ASSET_TAG: usize = 785;
const OFF_CACHE_LENDING_RATE: usize = 1380;

const SPL_TOKEN_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

pub fn empty() -> MarginfiState {
    MarginfiState {
        mint: Pubkey::default(),
        mint_decimals: 0,
        asset_share_value: I80F48::ZERO,
        liability_share_value: I80F48::ZERO,
        liquidity_vault: Pubkey::default(),
        total_liability_shares: I80F48::ZERO,
        total_asset_shares: I80F48::ZERO,
        last_update: 0,
        deposit_limit: 0,
        operational_state: BankOperationalState::Paused,
        asset_tag: 0,
        cached_lending_rate: 0,
        cached_user_asset_shares: I80F48::ZERO,
        underlying_token_program: SPL_TOKEN_PROGRAM_ID,
        oracle_setup: 0,
        oracle_keys: [Pubkey::default(); 5],
        interest_rate_config: InterestRateConfig::default(),
        group_program_fees_enabled: false,
        group_program_fee_fixed: I80F48::ZERO,
        group_program_fee_rate: I80F48::ZERO,
    }
}

pub fn remaining_accounts_per_bank(oracle_setup: u8) -> usize {
    match oracle_setup {
        1 | 2 | 3 | 4 | 6 => 2,
        5 => 4,
        _ => 1,
    }
}

pub fn decode(data: &[u8]) -> Result<MarginfiState, OverpassError> {
    if data.len() < BANK_LEN {
        return Err(OverpassError::InvalidAccount("marginfi bank: length"));
    }
    if data[..8] != BANK_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "marginfi bank: discriminator",
        ));
    }

    let mint = read_pubkey(data, OFF_MINT, "marginfi.mint")?;
    let mint_decimals = data[OFF_MINT_DECIMALS];
    let asset_share_value = read_wrapped_i80f48(data, OFF_ASSET_SHARE_VALUE)?;
    let liability_share_value = read_wrapped_i80f48(data, OFF_LIABILITY_SHARE_VALUE)?;
    let liquidity_vault = read_pubkey(data, OFF_LIQUIDITY_VAULT, "marginfi.liquidity_vault")?;
    let total_liability_shares = read_wrapped_i80f48(data, OFF_TOTAL_LIABILITY_SHARES)?;
    let total_asset_shares = read_wrapped_i80f48(data, OFF_TOTAL_ASSET_SHARES)?;
    let last_update = read_u64(data, OFF_LAST_UPDATE, "marginfi.last_update")? as i64;
    let deposit_limit = read_u64(data, OFF_CONFIG_DEPOSIT_LIMIT, "marginfi.deposit_limit")?;
    let operational_state = BankOperationalState::from_u8(data[OFF_CONFIG_OPERATIONAL_STATE])?;
    let asset_tag = data[OFF_CONFIG_ASSET_TAG];
    let cached_lending_rate =
        read_u32(data, OFF_CACHE_LENDING_RATE, "marginfi.cache.lending_rate")?;
    let oracle_setup = data[OFF_CONFIG_ORACLE_SETUP];
    let mut oracle_keys = [Pubkey::default(); 5];
    for (i, key) in oracle_keys.iter_mut().enumerate() {
        *key = read_pubkey(
            data,
            OFF_CONFIG_ORACLE_KEYS + i * 32,
            "marginfi.config.oracle_keys",
        )?;
    }

    let interest_rate_config = InterestRateConfig {
        optimal_utilization_rate: read_wrapped_i80f48(data, OFF_IR_OPTIMAL_UTIL_RATE)?,
        plateau_interest_rate: read_wrapped_i80f48(data, OFF_IR_PLATEAU_RATE)?,
        max_interest_rate: read_wrapped_i80f48(data, OFF_IR_MAX_RATE)?,
        insurance_fee_fixed_apr: read_wrapped_i80f48(data, OFF_IR_INSURANCE_FIXED_APR)?,
        insurance_ir_fee: read_wrapped_i80f48(data, OFF_IR_INSURANCE_IR_FEE)?,
        protocol_fixed_fee_apr: read_wrapped_i80f48(data, OFF_IR_PROTOCOL_FIXED_APR)?,
        protocol_ir_fee: read_wrapped_i80f48(data, OFF_IR_PROTOCOL_IR_FEE)?,
        zero_util_rate: read_u32(data, OFF_IR_ZERO_UTIL_RATE, "marginfi.ir.zero_util_rate")?,
        hundred_util_rate: read_u32(
            data,
            OFF_IR_HUNDRED_UTIL_RATE,
            "marginfi.ir.hundred_util_rate",
        )?,
        points: read_rate_points(data, OFF_IR_POINTS)?,
        curve_type: data[OFF_IR_CURVE_TYPE],
    };

    Ok(MarginfiState {
        mint,
        mint_decimals,
        asset_share_value,
        liability_share_value,
        liquidity_vault,
        total_liability_shares,
        total_asset_shares,
        last_update,
        deposit_limit,
        operational_state,
        asset_tag,
        cached_lending_rate,
        cached_user_asset_shares: I80F48::ZERO,
        underlying_token_program: SPL_TOKEN_PROGRAM_ID,
        oracle_setup,
        oracle_keys,
        interest_rate_config,
        group_program_fees_enabled: false,
        group_program_fee_fixed: I80F48::ZERO,
        group_program_fee_rate: I80F48::ZERO,
    })
}

pub fn read_wrapped_i80f48(data: &[u8], offset: usize) -> Result<I80F48, OverpassError> {
    Ok(I80F48::from_bits(
        read_u128(data, offset, "marginfi bank: WrappedI80F48 slice")? as i128,
    ))
}

fn read_rate_points(data: &[u8], offset: usize) -> Result<[RatePoint; 5], OverpassError> {
    let mut points = [RatePoint::default(); 5];
    for (i, point) in points.iter_mut().enumerate() {
        let base = offset + i * 8;
        *point = RatePoint {
            util: read_u32(data, base, "marginfi.ir.points.util")?,
            rate: read_u32(data, base + 4, "marginfi.ir.points.rate")?,
        };
    }
    Ok(points)
}

pub const MARGINFI_ACCOUNT_DISCRIMINATOR: [u8; 8] = [67, 178, 130, 109, 126, 114, 28, 42];
pub const MARGINFI_ACCOUNT_LEN: usize = 2312;

const OFF_BALANCES_START: usize = 72;
const BALANCE_SIZE: usize = 104;
const BALANCE_COUNT: usize = 16;
const BAL_OFF_ACTIVE: usize = 0;
const BAL_OFF_BANK_PK: usize = 1;
const BAL_OFF_ASSET_SHARES: usize = 40;

pub fn read_asset_shares_for_bank(data: &[u8], bank_pk: &Pubkey) -> Result<I80F48, OverpassError> {
    if data.len() < OFF_BALANCES_START + BALANCE_COUNT * BALANCE_SIZE {
        return Err(OverpassError::InvalidAccount("marginfi_account: length"));
    }
    if data[..8] != MARGINFI_ACCOUNT_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "marginfi_account: discriminator",
        ));
    }

    let bank_bytes = bank_pk.as_ref();
    for i in 0..BALANCE_COUNT {
        let base = OFF_BALANCES_START + i * BALANCE_SIZE;
        if data[base + BAL_OFF_ACTIVE] == 0 {
            continue;
        }
        if &data[base + BAL_OFF_BANK_PK..base + BAL_OFF_BANK_PK + 32] != bank_bytes {
            continue;
        }
        return read_wrapped_i80f48(data, base + BAL_OFF_ASSET_SHARES);
    }
    Ok(I80F48::ZERO)
}

pub const MARGINFI_GROUP_LEN: usize = 1064;
pub const MARGINFI_GROUP_DISCRIMINATOR: [u8; 8] = [182, 23, 173, 240, 151, 206, 182, 67];
const PROGRAM_FEES_ENABLED: u64 = 1;

const OFF_GROUP_FLAGS: usize = 40;
const OFF_GROUP_PROGRAM_FEE_FIXED: usize = 80;
const OFF_GROUP_PROGRAM_FEE_RATE: usize = 96;

#[derive(Debug, Clone, Default)]
pub struct MarginfiGroupState {
    pub program_fees_enabled: bool,
    pub program_fee_fixed: I80F48,
    pub program_fee_rate: I80F48,
}

pub fn decode_group(data: &[u8]) -> Result<MarginfiGroupState, OverpassError> {
    if data.len() < MARGINFI_GROUP_LEN {
        return Err(OverpassError::InvalidAccount("marginfi group: length"));
    }
    if data[..8] != MARGINFI_GROUP_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "marginfi group: discriminator",
        ));
    }
    let group_flags = read_u64(data, OFF_GROUP_FLAGS, "marginfi.group.group_flags")?;
    let program_fees_enabled = (group_flags & PROGRAM_FEES_ENABLED) != 0;
    let program_fee_fixed = read_wrapped_i80f48(data, OFF_GROUP_PROGRAM_FEE_FIXED)?;
    let program_fee_rate = read_wrapped_i80f48(data, OFF_GROUP_PROGRAM_FEE_RATE)?;
    Ok(MarginfiGroupState {
        program_fees_enabled,
        program_fee_fixed,
        program_fee_rate,
    })
}
