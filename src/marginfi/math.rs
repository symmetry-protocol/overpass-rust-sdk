//! Port of <https://github.com/mrgnlabs/marginfi-v2>.

use jupiter_amm_interface::{Quote, QuoteParams, SwapMode};
use rust_decimal::Decimal;

use super::state::{BankOperationalState, MarginfiState};
use crate::common::checked::{mul_bps, u128_to_u64};
use crate::error::OverpassError;
use crate::common::u256::U256;
use crate::accounts::WrapperVault;
use crate::QuoteCtx;


const SECONDS_PER_YEAR: u128 = 31_536_000;
const ONE_I80F48_BITS: u128 = 1u128 << 48;
const ASSET_TAG_DRIFT: u8 = 4;

pub fn quote(
    state: &MarginfiState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    if params.swap_mode != SwapMode::ExactIn {
        return Err(OverpassError::UnsupportedSwapMode);
    }
    if params.amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if state.asset_tag == ASSET_TAG_DRIFT {
        return Err(OverpassError::InvalidState(
            "marginfi: drift asset_tag not supported",
        ));
    }

    if params.input_mint == wv.underlying_mint && params.output_mint == wv.wrapper_mint {
        quote_deposit(state, wv, ctx, params)
    } else if params.input_mint == wv.wrapper_mint && params.output_mint == wv.underlying_mint {
        quote_withdraw(state, wv, ctx, params)
    } else {
        Err(OverpassError::InvalidState("marginfi: mint pair mismatch"))
    }
}

fn quote_deposit(
    state: &MarginfiState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    match state.operational_state {
        BankOperationalState::KilledByBankruptcy => return Err(OverpassError::BankKilled),
        BankOperationalState::Paused => return Err(OverpassError::BankPaused),
        BankOperationalState::ReduceOnly => return Err(OverpassError::BankReduceOnly),
        BankOperationalState::Operational => {}
    }

    let amount = params.amount;
    let protocol_fee = mul_bps(amount, ctx.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let total_fee = protocol_fee
        .checked_add(creator_fee)
        .ok_or(OverpassError::MathOverflow)?;
    let net_amount = amount
        .checked_sub(total_fee)
        .ok_or(OverpassError::MathOverflow)?;
    if net_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let new_share_value_bits = accrued_asset_share_value_bits(state, ctx)?;

    let net_amount_shifted_96 = (net_amount as u128)
        .checked_shl(96)
        .ok_or(OverpassError::MathOverflow)?;
    let shares_delta_bits = net_amount_shifted_96
        .checked_div(new_share_value_bits)
        .ok_or(OverpassError::MathOverflow)?;

    if state.deposit_limit != 0 && state.deposit_limit != u64::MAX {
        let new_total_shares_bits = state
            .total_asset_shares_bits
            .checked_add(shares_delta_bits)
            .ok_or(OverpassError::MathOverflow)?;
        let total_assets_bits = u128_mul_i80f48_floor(new_total_shares_bits, new_share_value_bits)?;
        let limit_bits = (state.deposit_limit as u128)
            .checked_shl(48)
            .ok_or(OverpassError::MathOverflow)?;
        if total_assets_bits >= limit_bits {
            return Err(OverpassError::BankAssetCapacityExceeded);
        }
    }

    let pre_balance_bits = state.cached_user_asset_shares_bits;
    let post_balance_bits = pre_balance_bits
        .checked_add(shares_delta_bits)
        .ok_or(OverpassError::MathOverflow)?;
    let pre_shares_int = pre_balance_bits >> 48;
    let post_shares_int = post_balance_bits >> 48;
    let shares_received_int = post_shares_int
        .checked_sub(pre_shares_int)
        .ok_or(OverpassError::MathOverflow)?;
    if shares_received_int == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let wrapper_to_mint_u128: u128 = if wv.wrapper_supply == 0 {
        net_amount as u128
    } else {
        if wv.intermediate_held == 0 {
            return Err(OverpassError::InvalidState(
                "marginfi: wrapper_supply>0 but intermediate_held=0",
            ));
        }
        shares_received_int
            .checked_mul(wv.wrapper_supply as u128)
            .and_then(|x| x.checked_div(wv.intermediate_held))
            .ok_or(OverpassError::MathOverflow)?
    };
    let wrapper_to_mint = u128_to_u64(wrapper_to_mint_u128)?;
    if wrapper_to_mint == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let fee_pct_bps = ctx.protocol_deposit_fee_bps as i64 + wv.creator_deposit_fee_bps as i64;
    let fee_pct = Decimal::new(fee_pct_bps, 4);

    Ok(Quote {
        in_amount: amount,
        out_amount: wrapper_to_mint,
        fee_amount: total_fee,
        fee_mint: wv.underlying_mint,
        fee_pct,
    })
}

fn quote_withdraw(
    state: &MarginfiState,
    wv: &WrapperVault,
    _ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    match state.operational_state {
        BankOperationalState::KilledByBankruptcy => return Err(OverpassError::BankKilled),
        BankOperationalState::Paused => return Err(OverpassError::BankPaused),
        BankOperationalState::Operational | BankOperationalState::ReduceOnly => {}
    }

    let burn_amount = params.amount;
    if burn_amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }

    let raw_share_value = state.asset_share_value_bits;

    let total_shares = wv.intermediate_held;
    if total_shares == 0 {
        return Err(OverpassError::InvalidState(
            "marginfi: intermediate_held=0 on withdraw",
        ));
    }

    let shares_pro_rata = (burn_amount as u128)
        .checked_mul(total_shares)
        .and_then(|x| x.checked_div(wv.wrapper_supply as u128))
        .ok_or(OverpassError::MathOverflow)?;
    if shares_pro_rata == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let underlying_u128 = shares_pro_rata
        .checked_mul(raw_share_value)
        .ok_or(OverpassError::MathOverflow)?
        >> 48;
    let requested_underlying = u128_to_u64(underlying_u128)?;
    if requested_underlying == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let total_assets_bits =
        u128_mul_i80f48_floor(state.total_asset_shares_bits, raw_share_value)?;
    let total_liabilities_bits = u128_mul_i80f48_floor(
        state.total_liability_shares_bits,
        state.liability_share_value_bits,
    )?;
    let withdrawn_bits = (requested_underlying as u128)
        .checked_shl(48)
        .ok_or(OverpassError::MathOverflow)?;
    let post_assets_bits = total_assets_bits
        .checked_sub(withdrawn_bits)
        .ok_or(OverpassError::InsufficientLiquidity)?;
    if post_assets_bits < total_liabilities_bits {
        return Err(OverpassError::InsufficientLiquidity);
    }

    Ok(Quote {
        in_amount: burn_amount,
        out_amount: requested_underlying,
        fee_amount: 0,
        fee_mint: wv.underlying_mint,
        fee_pct: Decimal::ZERO,
    })
}

fn accrued_asset_share_value_bits(
    state: &MarginfiState,
    ctx: &QuoteCtx,
) -> Result<u128, OverpassError> {
    if state.last_update <= 0 {
        return Ok(state.asset_share_value_bits);
    }
    let last = state.last_update as u128;
    if ctx.unix_timestamp as u128 <= last {
        return Ok(state.asset_share_value_bits);
    }
    let time_delta = (ctx.unix_timestamp as u128) - last;
    if time_delta == 0 || state.cached_lending_rate == 0 {
        return Ok(state.asset_share_value_bits);
    }

    let rate_bits = (state.cached_lending_rate as u128)
        .checked_mul(10)
        .and_then(|x| x.checked_shl(16))
        .ok_or(OverpassError::MathOverflow)?;

    let accrued_growth_bits = rate_bits
        .checked_mul(time_delta)
        .and_then(|x| x.checked_div(SECONDS_PER_YEAR))
        .ok_or(OverpassError::MathOverflow)?;

    let multiplier_bits = ONE_I80F48_BITS
        .checked_add(accrued_growth_bits)
        .ok_or(OverpassError::MathOverflow)?;

    u128_mul_i80f48_floor(state.asset_share_value_bits, multiplier_bits)
}

fn u128_mul_i80f48_floor(a_bits: u128, b_bits: u128) -> Result<u128, OverpassError> {
    let prod = U256::from(a_bits)
        .checked_mul(U256::from(b_bits))
        .ok_or(OverpassError::MathOverflow)?;
    let shifted = prod >> 48;
    if shifted.bits() > 128 {
        return Err(OverpassError::MathOverflow);
    }
    Ok(shifted.low_u128())
}

