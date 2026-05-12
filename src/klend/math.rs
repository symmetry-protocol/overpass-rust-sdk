//! Port of <https://github.com/Kamino-Finance/klend> @ `3f7bd69`.

use jupiter_amm_interface::{Quote, QuoteParams, SwapMode};
use rust_decimal::Decimal;

use super::state::{KlendState, BORROW_RATE_CURVE_LEN};
use super::withdrawal_caps;
use crate::common::checked::{mul_bps, u128_to_u64, FULL_BPS};
use crate::common::direction::Direction;
use crate::common::u256::U256;
use crate::error::OverpassError;
use super::fraction::{
    approximate_compounded_interest, bps_to_fraction, percent_to_fraction, BigFraction, Fraction,
};
use crate::accounts::WrapperVault;
use crate::QuoteCtx;

pub const POINTS_IN_CURVE: usize = 11;
pub const POINT_SIZE: usize = 8;
pub const SLOTS_PER_YEAR: u64 = 63_072_000;
pub const PROGRAM_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AccruedKlend {
    pub(crate) borrowed_amount_sf: u128,
    pub(crate) accumulated_protocol_fees_sf: u128,
    pub(crate) pending_referrer_fees_sf: u128,
    pub(crate) accumulated_referrer_fees_sf: u128,
}

pub fn quote(
    state: &KlendState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
) -> Result<Quote, OverpassError> {
    if params.amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if !matches!(params.swap_mode, SwapMode::ExactIn) {
        return Err(OverpassError::UnsupportedSwapMode);
    }

    let direction = crate::common::direction::from_quote(wv, params)?;

    if state.emergency_mode {
        return Err(OverpassError::GlobalEmergencyMode);
    }
    if state.version != PROGRAM_VERSION {
        return Err(OverpassError::ReserveDeprecated);
    }
    if matches!(direction, Direction::Deposit) {
        if state.status != 0 {
            return Err(OverpassError::ReserveObsolete);
        }
        if state.block_ctoken_usage {
            return Err(OverpassError::CTokenUsageBlocked);
        }
    }

    let accrued = accrue_interest(state, ctx);
    let total_supply_f = total_supply_fraction(state, &accrued);
    let collateral_supply = state.collateral_mint_total_supply;

    match direction {
        Direction::Deposit => quote_deposit(state, wv, ctx, params, total_supply_f, collateral_supply),
        Direction::Withdraw => quote_withdraw(state, wv, ctx, params, total_supply_f, collateral_supply),
    }
}


fn lookup_borrow_rate(curve: &[u8; BORROW_RATE_CURVE_LEN], utilization: Fraction) -> Fraction {
    let utilization = if utilization > Fraction::ONE {
        Fraction::ONE
    } else {
        utilization
    };
    let util_bps: u32 = (utilization * Fraction::from_num(FULL_BPS))
        .round()
        .to_num();
    let util_bps = util_bps.min(FULL_BPS);

    let point = |i: usize| -> (u32, u32) {
        let off = i * POINT_SIZE;
        let util = u32::from_le_bytes([curve[off], curve[off + 1], curve[off + 2], curve[off + 3]]);
        let rate = u32::from_le_bytes([
            curve[off + 4],
            curve[off + 5],
            curve[off + 6],
            curve[off + 7],
        ]);
        (util, rate)
    };

    let (mut prev_util, mut prev_rate) = point(0);
    if util_bps == prev_util {
        return bps_to_fraction(prev_rate);
    }
    for i in 1..POINTS_IN_CURVE {
        let (cur_util, cur_rate) = point(i);
        if util_bps == cur_util {
            return bps_to_fraction(cur_rate);
        }
        if util_bps > prev_util && util_bps < cur_util {
            let span = (cur_util - prev_util) as u128;
            let frac = ((util_bps - prev_util) as u128) * (1u128 << 60) / span;
            let delta_rate_bps = (cur_rate as i128) - (prev_rate as i128);
            let prev_rate_f = bps_to_fraction(prev_rate);
            if delta_rate_bps == 0 {
                return prev_rate_f;
            }
            let frac_f = Fraction::from_bits(frac);
            let abs_delta: u32 = delta_rate_bps.unsigned_abs().try_into().unwrap_or(u32::MAX);
            let delta_f = bps_to_fraction(abs_delta);
            return if delta_rate_bps >= 0 {
                prev_rate_f + frac_f * delta_f
            } else {
                prev_rate_f - frac_f * delta_f
            };
        }
        prev_util = cur_util;
        prev_rate = cur_rate;
    }
    bps_to_fraction(prev_rate)
}

fn total_supply_fraction_pre(state: &KlendState) -> Fraction {
    Fraction::from_num(state.total_available_amount) + Fraction::from_bits(state.borrowed_amount_sf)
        - Fraction::from_bits(state.accumulated_protocol_fees_sf)
        - Fraction::from_bits(state.accumulated_referrer_fees_sf)
        - Fraction::from_bits(state.pending_referrer_fees_sf)
}

pub(crate) fn total_supply_fraction(state: &KlendState, accrued: &AccruedKlend) -> Fraction {
    Fraction::from_num(state.total_available_amount)
        + Fraction::from_bits(accrued.borrowed_amount_sf)
        - Fraction::from_bits(accrued.accumulated_protocol_fees_sf)
        - Fraction::from_bits(accrued.accumulated_referrer_fees_sf)
        - Fraction::from_bits(accrued.pending_referrer_fees_sf)
}

fn utilization_rate(borrowed_sf: u128, total_supply: Fraction) -> Fraction {
    if total_supply == Fraction::ZERO {
        return Fraction::ZERO;
    }
    Fraction::from_bits(borrowed_sf) / total_supply
}

pub(crate) fn accrue_interest(state: &KlendState, ctx: &QuoteCtx) -> AccruedKlend {
    let slots_elapsed = ctx.current_slot.saturating_sub(state.last_update_slot);
    if slots_elapsed == 0 {
        return AccruedKlend {
            borrowed_amount_sf: state.borrowed_amount_sf,
            accumulated_protocol_fees_sf: state.accumulated_protocol_fees_sf,
            pending_referrer_fees_sf: state.pending_referrer_fees_sf,
            accumulated_referrer_fees_sf: state.accumulated_referrer_fees_sf,
        };
    }

    let pre_total_supply = total_supply_fraction_pre(state);
    let utilization = utilization_rate(state.borrowed_amount_sf, pre_total_supply);
    let current_borrow_rate = lookup_borrow_rate(&state.borrow_rate_curve, utilization);
    let host_fixed_interest_rate = bps_to_fraction(state.host_fixed_interest_rate_bps.into());
    let protocol_take_rate = percent_to_fraction(state.protocol_take_rate_pct);
    let referral_rate = Fraction::ZERO;

    let compounded_interest_rate =
        approximate_compounded_interest(current_borrow_rate + host_fixed_interest_rate, slots_elapsed);
    let compounded_fixed_rate =
        approximate_compounded_interest(host_fixed_interest_rate, slots_elapsed);

    let previous_debt_f = Fraction::from_bits(state.borrowed_amount_sf);
    let acc_protocol_fees_f = Fraction::from_bits(state.accumulated_protocol_fees_sf);

    let new_debt_f = previous_debt_f * compounded_interest_rate;
    let fixed_host_fee = (previous_debt_f * compounded_fixed_rate) - previous_debt_f;
    let net_new_variable_debt_f = new_debt_f - previous_debt_f - fixed_host_fee;
    let variable_protocol_fee_f = net_new_variable_debt_f * protocol_take_rate;
    let absolute_referral_rate = protocol_take_rate * referral_rate;
    let max_referrers_fees_f = net_new_variable_debt_f * absolute_referral_rate;

    let new_acc_protocol_fees_f =
        acc_protocol_fees_f + fixed_host_fee + variable_protocol_fee_f - max_referrers_fees_f;

    AccruedKlend {
        borrowed_amount_sf: new_debt_f.to_bits(),
        accumulated_protocol_fees_sf: new_acc_protocol_fees_f.to_bits(),
        pending_referrer_fees_sf: state.pending_referrer_fees_sf + max_referrers_fees_f.to_bits(),
        accumulated_referrer_fees_sf: state.accumulated_referrer_fees_sf,
    }
}

pub(crate) fn exchange_rate_pair(collateral_supply: u64, total_supply: Fraction) -> (u128, Fraction) {
    if collateral_supply == 0 || total_supply == Fraction::ZERO {
        (1, Fraction::ONE)
    } else {
        (collateral_supply as u128, total_supply)
    }
}

fn liquidity_to_collateral(
    liquidity_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, OverpassError> {
    let lhs = collateral_supply
        .checked_mul(liquidity_amount as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let big = BigFraction::from_num(lhs);
    let rhs = BigFraction::from(total_supply);
    let result = big / rhs;
    let fraction = result
        .try_into_fraction()
        .ok_or(OverpassError::MathOverflow)?;
    let floored: u128 = fraction.floor().to_num();
    u128_to_u64(floored)
}

fn collateral_to_liquidity_ceil(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, OverpassError> {
    if collateral_supply == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);

    let product = coll.checked_mul(liquidity_sbf).ok_or(OverpassError::MathOverflow)?;
    let numerator = product
        .checked_add(cs - U256::one())
        .ok_or(OverpassError::MathOverflow)?;
    let quotient = numerator / cs;

    let bits: u128 = quotient.try_into().map_err(|_| OverpassError::MathOverflow)?;
    let fraction = Fraction::from_bits(bits);
    let ceiled: u128 = fraction.ceil().to_num();
    u128_to_u64(ceiled)
}

fn collateral_to_liquidity(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, OverpassError> {
    if collateral_supply == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);

    let product = coll.checked_mul(liquidity_sbf).ok_or(OverpassError::MathOverflow)?;
    let quotient = product / cs;

    let bits: u128 = quotient.try_into().map_err(|_| OverpassError::MathOverflow)?;
    let fraction = Fraction::from_bits(bits);
    let floored: u128 = fraction.floor().to_num();
    u128_to_u64(floored)
}

pub(crate) fn fraction_collateral_to_liquidity(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<Fraction, OverpassError> {
    if collateral_supply == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);
    let product = coll.checked_mul(liquidity_sbf).ok_or(OverpassError::MathOverflow)?;
    let quotient = product / cs;
    let bits: u128 = quotient.try_into().map_err(|_| OverpassError::MathOverflow)?;
    Ok(Fraction::from_bits(bits))
}

fn quote_deposit(
    state: &KlendState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
    total_supply: Fraction,
    collateral_supply: u64,
) -> Result<Quote, OverpassError> {
    let amount = params.amount;

    let protocol_fee = mul_bps(amount, ctx.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let net_amount = amount
        .checked_sub(protocol_fee)
        .and_then(|x| x.checked_sub(creator_fee))
        .ok_or(OverpassError::MathOverflow)?;
    if net_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let (cs, ts) = exchange_rate_pair(collateral_supply, total_supply);

    let collateral_amount = liquidity_to_collateral(net_amount, cs, ts)?;
    if collateral_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    let liquidity_amount_to_deposit = collateral_to_liquidity_ceil(collateral_amount, cs, ts)?;
    if liquidity_amount_to_deposit > net_amount {
        return Err(OverpassError::MathOverflow);
    }

    let new_supply_f =
        Fraction::from_num(liquidity_amount_to_deposit) + total_supply;
    if new_supply_f > Fraction::from_num(state.deposit_limit) {
        return Err(OverpassError::DepositLimitExceeded);
    }

    let wrapper_supply = wv.wrapper_supply;
    let prev_intermediate = wv.intermediate_held;
    let wrapper_to_mint = if wrapper_supply == 0 {
        net_amount
    } else {
        if prev_intermediate == 0 {
            return Err(OverpassError::InvalidState("klend: ctokens out of sync"));
        }
        u128_to_u64(
            (collateral_amount as u128)
                .checked_mul(wrapper_supply as u128)
                .ok_or(OverpassError::MathOverflow)?
                .checked_div(prev_intermediate)
                .ok_or(OverpassError::MathOverflow)?,
        )?
    };
    if wrapper_to_mint == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let total_fee = protocol_fee
        .checked_add(creator_fee)
        .ok_or(OverpassError::MathOverflow)?;
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
    state: &KlendState,
    wv: &WrapperVault,
    ctx: &QuoteCtx,
    params: &QuoteParams,
    total_supply: Fraction,
    collateral_supply: u64,
) -> Result<Quote, OverpassError> {
    let burn_amount = params.amount;
    if burn_amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }

    let total_ctokens = wv.intermediate_held;
    if total_ctokens == 0 {
        return Err(OverpassError::InvalidState("klend: no cTokens"));
    }
    let ctokens_to_redeem = u128_to_u64(
        (burn_amount as u128)
            .checked_mul(total_ctokens)
            .ok_or(OverpassError::MathOverflow)?
            .checked_div(wv.wrapper_supply as u128)
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if ctokens_to_redeem == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let (cs, ts) = exchange_rate_pair(collateral_supply, total_supply);

    let underlying_received = collateral_to_liquidity(ctokens_to_redeem, cs, ts)?;
    if underlying_received == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let queued_liquidity = if state.queued_collateral_amount == 0 {
        0
    } else {
        collateral_to_liquidity(state.queued_collateral_amount, cs, ts)?
    };
    let freely_available = state
        .total_available_amount
        .saturating_sub(queued_liquidity);
    if underlying_received > freely_available {
        return Err(OverpassError::InsufficientLiquidity);
    }


    let remaining =
        withdrawal_caps::remaining_amount(&state.deposit_withdrawal_cap, ctx.unix_timestamp);
    if underlying_received > remaining {
        return Err(OverpassError::WithdrawalCapReached);
    }

    Ok(Quote {
        in_amount: burn_amount,
        out_amount: underlying_received,
        fee_amount: 0,
        fee_mint: wv.underlying_mint,
        fee_pct: Decimal::ZERO,
    })
}
