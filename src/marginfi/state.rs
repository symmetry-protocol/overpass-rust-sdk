//! Port of <https://github.com/mrgnlabs/marginfi-v2>.

use solana_program::pubkey::Pubkey;

use crate::common::bytes::{read_pubkey, read_u32, read_u64};
use crate::error::OverpassError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankOperationalState {
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
            _ => Err(OverpassError::InvalidState("marginfi: bad operational_state")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarginfiState {
    pub mint: Pubkey,
    pub mint_decimals: u8,
    pub asset_share_value_bits: u128,
    pub liability_share_value_bits: u128,
    pub liquidity_vault: Pubkey,
    pub total_liability_shares_bits: u128,
    pub total_asset_shares_bits: u128,
    pub last_update: i64,
    pub deposit_limit: u64,
    pub operational_state: BankOperationalState,
    pub asset_tag: u8,
    pub cached_lending_rate: u32,
    pub cached_user_asset_shares_bits: u128,
    pub underlying_token_program: Pubkey,
    pub oracle_setup: u8,
    pub oracle_keys: [Pubkey; 5],
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
        asset_share_value_bits: 0,
        liability_share_value_bits: 0,
        liquidity_vault: Pubkey::default(),
        total_liability_shares_bits: 0,
        total_asset_shares_bits: 0,
        last_update: 0,
        deposit_limit: 0,
        operational_state: BankOperationalState::Paused,
        asset_tag: 0,
        cached_lending_rate: 0,
        cached_user_asset_shares_bits: 0,
        underlying_token_program: SPL_TOKEN_PROGRAM_ID,
        oracle_setup: 0,
        oracle_keys: [Pubkey::default(); 5],
    }
}

/// Number of `remaining_accounts` slots marginfi reads per active balance for
/// health checks, given a bank's `oracle_setup`. The first slot is always the
/// bank pubkey itself; subsequent slots are oracle accounts.
pub fn remaining_accounts_per_bank(oracle_setup: u8) -> usize {
    match oracle_setup {
        1 | 2 | 3 | 4 | 6 => 2, // PythLegacy / SwitchboardV2 / PythPushOracle / SwitchboardPull / KaminoPythPush
        5 => 4,                  // StakedWithPythPush
        _ => 1,                  // None / unknown: bank only
    }
}

pub fn decode(data: &[u8]) -> Result<MarginfiState, OverpassError> {
    if data.len() < BANK_LEN {
        return Err(OverpassError::InvalidAccount("marginfi bank: length"));
    }
    if data[..8] != BANK_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount("marginfi bank: discriminator"));
    }

    let mint = read_pubkey(data, OFF_MINT, "marginfi.mint")?;
    let mint_decimals = data[OFF_MINT_DECIMALS];
    let asset_share_value_bits = read_wrapped_i80f48_as_u128(data, OFF_ASSET_SHARE_VALUE)?;
    let liability_share_value_bits = read_wrapped_i80f48_as_u128(data, OFF_LIABILITY_SHARE_VALUE)?;
    let liquidity_vault = read_pubkey(data, OFF_LIQUIDITY_VAULT, "marginfi.liquidity_vault")?;
    let total_liability_shares_bits =
        read_wrapped_i80f48_as_u128(data, OFF_TOTAL_LIABILITY_SHARES)?;
    let total_asset_shares_bits = read_wrapped_i80f48_as_u128(data, OFF_TOTAL_ASSET_SHARES)?;
    let last_update = read_u64(data, OFF_LAST_UPDATE, "marginfi.last_update")? as i64;
    let deposit_limit = read_u64(data, OFF_CONFIG_DEPOSIT_LIMIT, "marginfi.deposit_limit")?;
    let operational_state = BankOperationalState::from_u8(data[OFF_CONFIG_OPERATIONAL_STATE])?;
    let asset_tag = data[OFF_CONFIG_ASSET_TAG];
    let cached_lending_rate = read_u32(data, OFF_CACHE_LENDING_RATE, "marginfi.cache.lending_rate")?;
    let oracle_setup = data[OFF_CONFIG_ORACLE_SETUP];
    let mut oracle_keys = [Pubkey::default(); 5];
    for i in 0..5 {
        oracle_keys[i] = read_pubkey(
            data,
            OFF_CONFIG_ORACLE_KEYS + i * 32,
            "marginfi.config.oracle_keys",
        )?;
    }

    Ok(MarginfiState {
        mint,
        mint_decimals,
        asset_share_value_bits,
        liability_share_value_bits,
        liquidity_vault,
        total_liability_shares_bits,
        total_asset_shares_bits,
        last_update,
        deposit_limit,
        operational_state,
        asset_tag,
        cached_lending_rate,
        cached_user_asset_shares_bits: 0,
        underlying_token_program: SPL_TOKEN_PROGRAM_ID,
        oracle_setup,
        oracle_keys,
    })
}

fn read_wrapped_i80f48_as_u128(data: &[u8], offset: usize) -> Result<u128, OverpassError> {
    let raw: [u8; 16] = data[offset..offset + 16]
        .try_into()
        .map_err(|_| OverpassError::InvalidAccount("marginfi bank: WrappedI80F48 slice"))?;
    let v = i128::from_le_bytes(raw);
    if v < 0 {
        return Err(OverpassError::InvalidState("marginfi: negative I80F48"));
    }
    Ok(v as u128)
}

pub const MARGINFI_ACCOUNT_DISCRIMINATOR: [u8; 8] = [67, 178, 130, 109, 126, 114, 28, 42];
pub const MARGINFI_ACCOUNT_LEN: usize = 2312;

const OFF_BALANCES_START: usize = 72;
const BALANCE_SIZE: usize = 104;
const BALANCE_COUNT: usize = 16;
const BAL_OFF_ACTIVE: usize = 0;
const BAL_OFF_BANK_PK: usize = 1;
const BAL_OFF_ASSET_SHARES: usize = 40;

pub fn read_asset_shares_for_bank(data: &[u8], bank_pk: &Pubkey) -> Result<u128, OverpassError> {
    if data.len() < OFF_BALANCES_START + BALANCE_COUNT * BALANCE_SIZE {
        return Err(OverpassError::InvalidAccount("marginfi_account: length"));
    }
    if data[..8] != MARGINFI_ACCOUNT_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount("marginfi_account: discriminator"));
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
        let raw: [u8; 16] = data[base + BAL_OFF_ASSET_SHARES..base + BAL_OFF_ASSET_SHARES + 16]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount("marginfi_account: asset_shares"))?;
        let v = i128::from_le_bytes(raw);
        if v <= 0 {
            return Ok(0);
        }
        return Ok(v as u128);
    }
    Ok(0)
}
