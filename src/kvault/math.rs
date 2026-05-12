//! Port of <https://github.com/Kamino-Finance/kvault>.

use jupiter_amm_interface::{Quote, QuoteParams, SwapMode};
use rust_decimal::Decimal;

use super::state::{ActiveAllocation, KvaultState};
use crate::common::checked::{mul_bps, mul_div_u64, u128_to_u64, FULL_BPS};
use crate::error::OverpassError;
use crate::klend::fraction::Fraction;
use crate::common::u256::U256;
use crate::klend;
use crate::accounts::WrapperVault;
use crate::QuoteCtx;

const SECONDS_PER_YEAR: u128 = 31_556_926;

pub fn quote(
    state: &KvaultState,
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
        quote_withdraw(state, wv, ctx, params)
    } else {
        Err(OverpassError::InvalidState("kvault: mint pair mismatch"))
    }
}

fn quote_deposit(
    state: &KvaultState,
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

    let num_reserves = state
        .active_allocations
        .iter()
        .filter(|a| a.target_allocation_weight > 0 && a.token_allocation_cap > 0)
        .count() as u64;
    let crank_funds = num_reserves
        .checked_mul(state.crank_fund_fee_per_reserve)
        .ok_or(OverpassError::MathOverflow)?;
    let max_user_tokens = net_amount
        .checked_sub(crank_funds)
        .ok_or(OverpassError::InvalidAmount)?;
    if max_user_tokens < state.min_deposit_amount {
        return Err(OverpassError::InvalidAmount);
    }

    let aum = compute_current_aum(state, ctx)?;
    let kvault_shares_to_mint = get_shares_to_mint(aum, max_user_tokens, state.shares_issued)?;
    if kvault_shares_to_mint == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let wrapper_to_mint = if wv.wrapper_supply == 0 {
        net_amount
    } else {
        if wv.intermediate_held == 0 {
            return Err(OverpassError::InvalidState(
                "kvault: wrapper_supply>0 but intermediate_held=0",
            ));
        }
        u128_to_u64(
            (kvault_shares_to_mint as u128)
                .checked_mul(wv.wrapper_supply as u128)
                .and_then(|x| x.checked_div(wv.intermediate_held))
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
    state: &KvaultState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    let burn_amount = params.amount;
    if burn_amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }
    if wv.intermediate_held == 0 {
        return Err(OverpassError::InvalidState(
            "kvault: intermediate_held=0 on withdraw",
        ));
    }

    let shares_to_redeem = u128_to_u64(
        (burn_amount as u128)
            .checked_mul(wv.intermediate_held)
            .and_then(|x| x.checked_div(wv.wrapper_supply as u128))
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if shares_to_redeem == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let aum = compute_current_aum(state, ctx)?;
    if aum == Fraction::ZERO {
        return Err(OverpassError::InvalidState("kvault: aum=0"));
    }

    let total_with_penalty: u64 = if shares_to_redeem == state.shares_issued {
        u128_to_u64(aum.floor().to_num::<u128>())?
    } else {
        let full = fraction_full_mul_int_ratio(aum, shares_to_redeem, state.shares_issued)?;
        u128_to_u64(full.floor().to_num::<u128>())?
    };
    if total_with_penalty == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if total_with_penalty < state.min_withdraw_amount {
        return Err(OverpassError::InvalidAmount);
    }

    let penalty_bps = state.effective_penalty_bps();
    let penalty_lamports = state.effective_penalty_lamports();
    let penalty_from_bps =
        mul_div_u64(total_with_penalty, penalty_bps, FULL_BPS as u64).unwrap_or(0);
    let penalty = penalty_from_bps.max(penalty_lamports);
    if penalty >= total_with_penalty {
        return Err(OverpassError::InvalidAmount);
    }
    let total_for_user = total_with_penalty
        .checked_sub(penalty)
        .ok_or(OverpassError::MathOverflow)?;
    if total_for_user == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let final_for_user = if total_for_user > state.token_available {
        total_for_user.saturating_sub(1)
    } else {
        total_for_user
    };
    if final_for_user == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    Ok(Quote {
        in_amount: burn_amount,
        out_amount: final_for_user,
        fee_amount: 0,
        fee_mint: wv.underlying_mint,
        fee_pct: Decimal::ZERO,
    })
}

fn compute_current_aum(state: &KvaultState, ctx: &QuoteCtx) -> Result<Fraction, OverpassError> {
    let invested_total = invested_total(state, ctx)?;
    let available = Fraction::from_num(state.token_available);
    let holdings_total = available + invested_total;
    let pending_fees_after_charge = charged_pending_fees(state, ctx, invested_total)?;

    if holdings_total < pending_fees_after_charge {
        return Err(OverpassError::InvalidState("kvault: AUM below pending fees"));
    }
    Ok(holdings_total - pending_fees_after_charge)
}

fn invested_total(state: &KvaultState, ctx: &QuoteCtx) -> Result<Fraction, OverpassError> {
    let mut total = Fraction::ZERO;
    for alloc in &state.active_allocations {
        total += invested_for_allocation(state, ctx, alloc)?;
    }
    Ok(total)
}

fn invested_for_allocation(
    state: &KvaultState,
    ctx: &QuoteCtx,
    alloc: &ActiveAllocation,
) -> Result<Fraction, OverpassError> {
    let reserve = state
        .cached_reserves
        .iter()
        .find(|(k, _)| *k == alloc.reserve)
        .map(|(_, r)| r)
        .ok_or(OverpassError::AccountMissing(
            "kvault allocation reserve (not in update map)",
        ))?;

    let accrued = klend::math::accrue_interest(reserve, ctx);
    let total_supply = klend::math::total_supply_fraction(reserve, &accrued);
    let (cs, ts) = klend::math::exchange_rate_pair(reserve.collateral_mint_total_supply, total_supply);
    klend::math::fraction_collateral_to_liquidity(alloc.ctoken_allocation, cs, ts)
}

fn charged_pending_fees(
    state: &KvaultState,
    ctx: &QuoteCtx,
    invested_total: Fraction,
) -> Result<Fraction, OverpassError> {
    if state.last_fee_charge_timestamp == 0 {
        return Ok(state.pending_fees());
    }
    let seconds_passed = ctx
        .unix_timestamp
        .saturating_sub(state.last_fee_charge_timestamp);

    let new_aum_preview = {
        let avail = Fraction::from_num(state.token_available);
        let holdings = avail + invested_total;
        if holdings < state.pending_fees() {
            Fraction::ZERO
        } else {
            holdings - state.pending_fees()
        }
    };

    let mgmt_charge = if seconds_passed == 0 || state.management_fee_bps == 0 {
        Fraction::ZERO
    } else {
        let mgmt_fee_yearly = bps_to_fraction(state.management_fee_bps);
        let mgmt_fee = mgmt_fee_yearly * Fraction::from_num(seconds_passed)
            / Fraction::from_num(SECONDS_PER_YEAR);
        state.prev_aum() * mgmt_fee
    };

    let earned_interest = if new_aum_preview > state.prev_aum() {
        new_aum_preview - state.prev_aum()
    } else {
        Fraction::ZERO
    };
    let perf_charge = if state.performance_fee_bps == 0 {
        Fraction::ZERO
    } else {
        bps_to_fraction(state.performance_fee_bps) * earned_interest
    };

    let mut new_fees = mgmt_charge + perf_charge;
    if new_fees > new_aum_preview {
        new_fees = new_aum_preview;
    }
    Ok(state.pending_fees() + new_fees)
}

fn get_shares_to_mint(
    aum: Fraction,
    user_amount: u64,
    shares_issued: u64,
) -> Result<u64, OverpassError> {
    if shares_issued == 0 {
        return Ok(user_amount);
    }
    if aum == Fraction::ZERO {
        return Err(OverpassError::InvalidState("kvault: aum=0 with shares>0"));
    }
    let aum_ceil_u64: u64 = u128_to_u64(aum.ceil().to_num::<u128>())?;
    if aum_ceil_u64 == 0 {
        return Err(OverpassError::InvalidState("kvault: aum_ceil=0"));
    }
    let shares =
        fraction_full_mul_int_ratio(Fraction::from_num(shares_issued), user_amount, aum_ceil_u64)?;
    u128_to_u64(shares.floor().to_num::<u128>())
}

fn fraction_full_mul_int_ratio(
    f: Fraction,
    mult: u64,
    div: u64,
) -> Result<Fraction, OverpassError> {
    if div == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let lhs = U256::from(f.to_bits())
        .checked_mul(U256::from(mult))
        .ok_or(OverpassError::MathOverflow)?;
    let quot = lhs / U256::from(div);
    let bits: u128 = quot.try_into().map_err(|_| OverpassError::MathOverflow)?;
    Ok(Fraction::from_bits(bits))
}

fn bps_to_fraction(bps: u64) -> Fraction {
    Fraction::from_num(bps) / Fraction::from_num(FULL_BPS)
}

