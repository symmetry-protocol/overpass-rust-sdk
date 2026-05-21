use solana_program::pubkey::Pubkey;

use crate::error::OverpassError;
use crate::klend::withdrawal_caps::{self, WithdrawalCaps};

pub const RESERVE_LEN: usize = 8624;
pub const RESERVE_DISCRIMINATOR: [u8; 8] = [43, 242, 204, 202, 26, 247, 59, 127];
pub const BORROW_RATE_CURVE_LEN: usize = 88;

#[derive(Debug, Clone)]
pub struct KlendState {
    pub version: u64,
    pub last_update_slot: u64,
    pub lending_market: Pubkey,
    pub mint_pubkey: Pubkey,
    pub supply_vault: Pubkey,
    pub total_available_amount: u64,
    pub borrowed_amount_sf: u128,
    pub market_price_last_updated_ts: u64,
    pub mint_decimals: u64,
    pub cumulative_borrow_rate_bsf: [u64; 6],
    pub accumulated_protocol_fees_sf: u128,
    pub accumulated_referrer_fees_sf: u128,
    pub pending_referrer_fees_sf: u128,
    pub absolute_referral_rate_sf: u128,
    pub liquidity_token_program: Pubkey,
    pub collateral_mint_pubkey: Pubkey,
    pub collateral_mint_total_supply: u64,
    pub status: u8,
    pub block_ctoken_usage: bool,
    pub emergency_mode: bool,
    pub host_fixed_interest_rate_bps: u16,
    pub protocol_take_rate_pct: u8,
    pub borrow_rate_curve: [u8; BORROW_RATE_CURVE_LEN],
    pub deposit_limit: u64,
    pub max_age_price_seconds: u64,
    pub deposit_withdrawal_cap: WithdrawalCaps,
    pub debt_withdrawal_cap: WithdrawalCaps,
    pub queued_collateral_amount: u64,
    pub scope_price_feed: Pubkey,
    pub switchboard_price_aggregator: Pubkey,
    pub switchboard_twap_aggregator: Pubkey,
    pub pyth_price: Pubkey,
}

pub fn empty() -> KlendState {
    KlendState {
        version: 0,
        last_update_slot: 0,
        lending_market: Pubkey::default(),
        mint_pubkey: Pubkey::default(),
        supply_vault: Pubkey::default(),
        total_available_amount: 0,
        borrowed_amount_sf: 0,
        market_price_last_updated_ts: 0,
        mint_decimals: 0,
        cumulative_borrow_rate_bsf: [0; 6],
        accumulated_protocol_fees_sf: 0,
        accumulated_referrer_fees_sf: 0,
        pending_referrer_fees_sf: 0,
        absolute_referral_rate_sf: 0,
        liquidity_token_program: Pubkey::default(),
        collateral_mint_pubkey: Pubkey::default(),
        collateral_mint_total_supply: 0,
        status: 0,
        block_ctoken_usage: false,
        emergency_mode: false,
        host_fixed_interest_rate_bps: 0,
        protocol_take_rate_pct: 0,
        borrow_rate_curve: [0; BORROW_RATE_CURVE_LEN],
        deposit_limit: 0,
        max_age_price_seconds: 0,
        deposit_withdrawal_cap: WithdrawalCaps::default(),
        debt_withdrawal_cap: WithdrawalCaps::default(),
        queued_collateral_amount: 0,
        scope_price_feed: Pubkey::default(),
        switchboard_price_aggregator: Pubkey::default(),
        switchboard_twap_aggregator: Pubkey::default(),
        pyth_price: Pubkey::default(),
    }
}

use crate::common::bytes::{read_pubkey, read_u128, read_u64};

pub fn decode(data: &[u8]) -> Result<KlendState, OverpassError> {
    if data.len() < RESERVE_LEN {
        return Err(OverpassError::InvalidAccount("klend reserve: length"));
    }
    if data[..8] != RESERVE_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "klend reserve: discriminator",
        ));
    }

    let version = read_u64(data, 8, "klend.version")?;
    let last_update_slot = read_u64(data, 16, "klend.last_update.slot")?;
    let lending_market = read_pubkey(data, 32, "klend.lending_market")?;

    let mint_pubkey = read_pubkey(data, 128, "klend.liquidity.mint_pubkey")?;
    let supply_vault = read_pubkey(data, 160, "klend.liquidity.supply_vault")?;
    let total_available_amount = read_u64(data, 224, "klend.total_available_amount")?;
    let borrowed_amount_sf = read_u128(data, 232, "klend.borrowed_amount_sf")?;
    let market_price_last_updated_ts = read_u64(data, 264, "klend.market_price_last_updated_ts")?;
    let mint_decimals = read_u64(data, 272, "klend.mint_decimals")?;

    let mut cumulative_borrow_rate_bsf = [0u64; 6];
    for (i, slot) in cumulative_borrow_rate_bsf.iter_mut().enumerate() {
        *slot = read_u64(data, 296 + i * 8, "klend.cumulative_borrow_rate_bsf")?;
    }

    let accumulated_protocol_fees_sf = read_u128(data, 344, "klend.accumulated_protocol_fees_sf")?;
    let accumulated_referrer_fees_sf = read_u128(data, 360, "klend.accumulated_referrer_fees_sf")?;
    let pending_referrer_fees_sf = read_u128(data, 376, "klend.pending_referrer_fees_sf")?;
    let absolute_referral_rate_sf = read_u128(data, 392, "klend.absolute_referral_rate_sf")?;
    let liquidity_token_program = read_pubkey(data, 408, "klend.liquidity.token_program")?;

    let collateral_mint_pubkey = read_pubkey(data, 2560, "klend.collateral.mint_pubkey")?;
    let collateral_mint_total_supply = read_u64(data, 2592, "klend.collateral.mint_total_supply")?;

    let status = data[4856];
    let host_fixed_interest_rate_bps =
        u16::from_le_bytes(data[4858..4860].try_into().map_err(|_| {
            OverpassError::InvalidAccount("klend.config.host_fixed_interest_rate_bps")
        })?);
    let block_ctoken_usage = data[4862] != 0;
    let emergency_mode = data[4864] != 0;
    let protocol_take_rate_pct = data[4870];

    let borrow_rate_curve: [u8; BORROW_RATE_CURVE_LEN] = data[4920..5008]
        .try_into()
        .map_err(|_| OverpassError::InvalidAccount("klend.config.borrow_rate_curve"))?;

    let deposit_limit = read_u64(data, 5016, "klend.config.deposit_limit")?;

    let max_age_price_seconds = read_u64(data, 5096, "klend.token_info.max_age_price_seconds")?;
    let scope_price_feed = read_pubkey(data, 5112, "klend.token_info.scope_price_feed")?;
    let switchboard_price_aggregator =
        read_pubkey(data, 5160, "klend.token_info.switchboard_price_aggregator")?;
    let switchboard_twap_aggregator =
        read_pubkey(data, 5192, "klend.token_info.switchboard_twap_aggregator")?;
    let pyth_price = read_pubkey(data, 5224, "klend.token_info.pyth_price")?;

    let deposit_withdrawal_cap = withdrawal_caps::decode(&data[5416..5448])?;
    let debt_withdrawal_cap = withdrawal_caps::decode(&data[5448..5480])?;
    let queued_collateral_amount =
        read_u64(data, 6968, "klend.withdraw_queue.queued_collateral_amount")?;

    Ok(KlendState {
        version,
        last_update_slot,
        lending_market,
        mint_pubkey,
        supply_vault,
        total_available_amount,
        borrowed_amount_sf,
        market_price_last_updated_ts,
        mint_decimals,
        cumulative_borrow_rate_bsf,
        accumulated_protocol_fees_sf,
        accumulated_referrer_fees_sf,
        pending_referrer_fees_sf,
        absolute_referral_rate_sf,
        liquidity_token_program,
        collateral_mint_pubkey,
        collateral_mint_total_supply,
        status,
        block_ctoken_usage,
        emergency_mode,
        host_fixed_interest_rate_bps,
        protocol_take_rate_pct,
        borrow_rate_curve,
        deposit_limit,
        max_age_price_seconds,
        deposit_withdrawal_cap,
        debt_withdrawal_cap,
        queued_collateral_amount,
        scope_price_feed,
        switchboard_price_aggregator,
        switchboard_twap_aggregator,
        pyth_price,
    })
}
