//! Port of <https://github.com/lulo-labs/lulo-v2-cpi>.

use jupiter_amm_interface::{Quote, QuoteParams, SwapMode};
use rust_decimal::Decimal;

use super::state::LuloState;
use crate::common::checked::{mul_bps, u128_to_u64};
use crate::error::OverpassError;
use crate::accounts::WrapperVault;
use crate::QuoteCtx;


pub fn quote(
    state: &LuloState,
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
    if params.input_mint == wv.underlying_mint && params.output_mint == wv.wrapper_mint {
        quote_deposit(state, wv, ctx, params)
    } else if params.input_mint == wv.wrapper_mint && params.output_mint == wv.underlying_mint {
        quote_withdraw(state, wv, params)
    } else {
        Err(OverpassError::InvalidState("lulo: mint pair mismatch"))
    }
}

fn quote_deposit(
    state: &LuloState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
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

    let pre_lp = wv.intermediate_held;
    let lp_value_before: u64 = if state.protected_total_supply == 0 {
        0
    } else {
        let v = pre_lp
            .checked_mul(state.protected_amount as u128)
            .and_then(|x| x.checked_div(state.protected_total_supply as u128))
            .ok_or(OverpassError::MathOverflow)?;
        u128_to_u64(v)?
    };
    let total_value_before = wv
        .free_underlying_held
        .checked_add(lp_value_before)
        .ok_or(OverpassError::MathOverflow)?;

    let wrapper_to_mint: u64 = if wv.wrapper_supply == 0 {
        if total_value_before != 0 {
            return Err(OverpassError::InvalidState(
                "lulo: wrapper_supply=0 but total_value>0",
            ));
        }
        net_amount
    } else {
        if total_value_before == 0 {
            return Err(OverpassError::InvalidState(
                "lulo: wrapper_supply>0 but total_value=0",
            ));
        }
        u128_to_u64(
            (net_amount as u128)
                .checked_mul(wv.wrapper_supply as u128)
                .and_then(|x| x.checked_div(total_value_before as u128))
                .ok_or(OverpassError::MathOverflow)?,
        )?
    };
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
    state: &LuloState,
    wv: &WrapperVault,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    let burn_amount = params.amount;
    if burn_amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }
    if wv.wrapper_supply == 0 {
        return Err(OverpassError::InvalidState("lulo: wrapper_supply=0"));
    }

    let pre_lp = wv.intermediate_held;
    let lp_value: u64 = if state.protected_total_supply == 0 {
        0
    } else {
        let v = pre_lp
            .checked_mul(state.protected_amount as u128)
            .and_then(|x| x.checked_div(state.protected_total_supply as u128))
            .ok_or(OverpassError::MathOverflow)?;
        u128_to_u64(v)?
    };
    let total_value_before = wv
        .free_underlying_held
        .checked_add(lp_value)
        .ok_or(OverpassError::MathOverflow)?;
    if total_value_before == 0 {
        return Err(OverpassError::InvalidState("lulo: total_value=0"));
    }

    let n_owed = u128_to_u64(
        (burn_amount as u128)
            .checked_mul(total_value_before as u128)
            .and_then(|x| x.checked_div(wv.wrapper_supply as u128))
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if n_owed == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    Ok(Quote {
        in_amount: burn_amount,
        out_amount: n_owed,
        fee_amount: 0,
        fee_mint: wv.underlying_mint,
        fee_pct: Decimal::ZERO,
    })
}
