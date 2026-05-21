use super::fraction::{
    approximate_compounded_interest, bps_to_fraction, percent_to_fraction, BigFraction, Fraction,
};
use super::state::{KlendState, BORROW_RATE_CURVE_LEN};
use super::withdrawal_caps;
use crate::accounts::{GlobalConfig, WrapperVault};
use crate::common::checked::{mul_bps, require_no_value_per_share_loss, u128_to_u64, FULL_BPS};
use crate::common::u256::U256;
use crate::error::OverpassError;

pub const POINTS_IN_CURVE: usize = 11;
pub const POINT_SIZE: usize = 8;
pub const SLOTS_PER_YEAR: u64 = 63_072_000;
pub const PROGRAM_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy)]
pub struct AccruedKlend {
    pub borrowed_amount_sf: u128,
    pub accumulated_protocol_fees_sf: u128,
    pub pending_referrer_fees_sf: u128,
    pub accumulated_referrer_fees_sf: u128,
}

fn lookup_borrow_rate(curve: &[u8; BORROW_RATE_CURVE_LEN], utilization: Fraction) -> Fraction {
    let utilization = if utilization > Fraction::ONE {
        Fraction::ONE
    } else {
        utilization
    };
    let util_bps_u32: u32 = (utilization * Fraction::from_num(FULL_BPS))
        .round()
        .to_num();
    let util_bps = util_bps_u32.min(FULL_BPS);

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
            let slope_nom = cur_rate.saturating_sub(prev_rate);
            let slope_denom = cur_util - prev_util;
            let start_util_f = bps_to_fraction(prev_util);
            let coef = utilization - start_util_f;
            let nom = coef * u128::from(slope_nom);
            let base_rate = nom / u128::from(slope_denom);
            let offset = bps_to_fraction(prev_rate);
            return base_rate + offset;
        }
        prev_util = cur_util;
        prev_rate = cur_rate;
    }
    bps_to_fraction(prev_rate)
}

fn total_supply_core(
    total_available_amount: u64,
    borrowed_amount_sf: u128,
    accumulated_protocol_fees_sf: u128,
    accumulated_referrer_fees_sf: u128,
    pending_referrer_fees_sf: u128,
) -> Fraction {
    Fraction::from_num(total_available_amount) + Fraction::from_bits(borrowed_amount_sf)
        - Fraction::from_bits(accumulated_protocol_fees_sf)
        - Fraction::from_bits(accumulated_referrer_fees_sf)
        - Fraction::from_bits(pending_referrer_fees_sf)
}

fn total_supply_fraction_pre(state: &KlendState) -> Fraction {
    total_supply_core(
        state.total_available_amount,
        state.borrowed_amount_sf,
        state.accumulated_protocol_fees_sf,
        state.accumulated_referrer_fees_sf,
        state.pending_referrer_fees_sf,
    )
}

pub fn total_supply_fraction(state: &KlendState, accrued: &AccruedKlend) -> Fraction {
    total_supply_core(
        state.total_available_amount,
        accrued.borrowed_amount_sf,
        accrued.accumulated_protocol_fees_sf,
        accrued.accumulated_referrer_fees_sf,
        accrued.pending_referrer_fees_sf,
    )
}

fn utilization_rate(borrowed_sf: u128, total_supply: Fraction) -> Fraction {
    if total_supply == Fraction::ZERO {
        return Fraction::ZERO;
    }
    Fraction::from_bits(borrowed_sf) / total_supply
}

pub fn accrue_interest(state: &KlendState, current_slot: u64) -> AccruedKlend {
    let slots_elapsed = current_slot.saturating_sub(state.last_update_slot);
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

    let compounded_interest_rate = approximate_compounded_interest(
        current_borrow_rate + host_fixed_interest_rate,
        slots_elapsed,
    );
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

pub fn exchange_rate_pair(collateral_supply: u64, total_supply: Fraction) -> (u128, Fraction) {
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
    let big = BigFraction::from_num(lhs).ok_or(OverpassError::MathOverflow)?;
    let rhs = BigFraction::from(total_supply);
    let result = big.checked_div(rhs).ok_or(OverpassError::MathOverflow)?;
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

    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(OverpassError::MathOverflow)?;
    let numerator = product
        .checked_add(cs - U256::one())
        .ok_or(OverpassError::MathOverflow)?;
    let quotient = numerator / cs;

    let bits: u128 = quotient
        .try_into()
        .map_err(|_| OverpassError::MathOverflow)?;
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

    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(OverpassError::MathOverflow)?;
    let quotient = product / cs;

    let bits: u128 = quotient
        .try_into()
        .map_err(|_| OverpassError::MathOverflow)?;
    let fraction = Fraction::from_bits(bits);
    let floored: u128 = fraction.floor().to_num();
    u128_to_u64(floored)
}

pub fn fraction_collateral_to_liquidity(
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
    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(OverpassError::MathOverflow)?;
    let quotient = product / cs;
    let bits: u128 = quotient
        .try_into()
        .map_err(|_| OverpassError::MathOverflow)?;
    Ok(Fraction::from_bits(bits))
}

pub fn fraction_liquidity_to_collateral_ceil(
    liquidity_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, OverpassError> {
    if collateral_supply == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let den = U256::from(total_supply.to_bits());
    if den.is_zero() {
        return Err(OverpassError::MathOverflow);
    }
    let num = U256::from(liquidity_amount)
        .checked_mul(U256::from(collateral_supply))
        .ok_or(OverpassError::MathOverflow)?
        .checked_mul(U256::from(1u128 << 60))
        .ok_or(OverpassError::MathOverflow)?;
    let quotient = (num + den - U256::one()) / den;
    let v: u128 = quotient
        .try_into()
        .map_err(|_| OverpassError::MathOverflow)?;
    u128_to_u64(v)
}

pub fn quote_deposit(
    state: &KlendState,
    current_slot: u64,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if state.emergency_mode {
        return Err(OverpassError::GlobalEmergencyMode);
    }
    if state.version != PROGRAM_VERSION {
        return Err(OverpassError::ReserveDeprecated);
    }
    if state.status != 0 {
        return Err(OverpassError::ReserveObsolete);
    }
    if state.block_ctoken_usage {
        return Err(OverpassError::CTokenUsageBlocked);
    }

    let accrued = accrue_interest(state, current_slot);
    let total_supply_f = total_supply_fraction(state, &accrued);
    let collateral_supply = state.collateral_mint_total_supply;

    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let net_amount = amount
        .checked_sub(protocol_fee)
        .and_then(|x| x.checked_sub(creator_fee))
        .ok_or(OverpassError::MathOverflow)?;
    if net_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let (cs, ts) = exchange_rate_pair(collateral_supply, total_supply_f);

    let collateral_amount = liquidity_to_collateral(net_amount, cs, ts)?;
    if collateral_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    let liquidity_amount_to_deposit = collateral_to_liquidity_ceil(collateral_amount, cs, ts)?;
    if liquidity_amount_to_deposit > net_amount {
        return Err(OverpassError::MathOverflow);
    }

    let new_supply_f = Fraction::from_num(liquidity_amount_to_deposit) + total_supply_f;
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

    let new_intermediate = prev_intermediate
        .checked_add(collateral_amount as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let new_wrapper_supply = wrapper_supply
        .checked_add(wrapper_to_mint)
        .ok_or(OverpassError::MathOverflow)?;
    require_no_value_per_share_loss(
        prev_intermediate,
        wrapper_supply,
        new_intermediate,
        new_wrapper_supply,
    )?;

    Ok(wrapper_to_mint)
}

pub fn quote_withdraw(
    state: &KlendState,
    current_slot: u64,
    unix_timestamp: i64,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if state.emergency_mode {
        return Err(OverpassError::GlobalEmergencyMode);
    }
    if state.version != PROGRAM_VERSION {
        return Err(OverpassError::ReserveDeprecated);
    }
    if amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }

    let total_ctokens = wv.intermediate_held;
    if total_ctokens == 0 {
        return Err(OverpassError::InvalidState("klend: no cTokens"));
    }
    let ctokens_to_redeem = u128_to_u64(
        (amount as u128)
            .checked_mul(total_ctokens)
            .ok_or(OverpassError::MathOverflow)?
            .checked_div(wv.wrapper_supply as u128)
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if ctokens_to_redeem == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let accrued = accrue_interest(state, current_slot);
    let total_supply_f = total_supply_fraction(state, &accrued);
    let (cs, ts) = exchange_rate_pair(state.collateral_mint_total_supply, total_supply_f);

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

    let remaining = withdrawal_caps::remaining_amount(
        &state.deposit_withdrawal_cap,
        unix_timestamp.max(0) as u64,
    );
    if underlying_received > remaining {
        return Err(OverpassError::WithdrawalCapReached);
    }

    let new_intermediate = total_ctokens
        .checked_sub(ctokens_to_redeem as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(OverpassError::MathOverflow)?;
    require_no_value_per_share_loss(
        total_ctokens,
        wv.wrapper_supply,
        new_intermediate,
        new_wrapper_supply,
    )?;

    Ok(underlying_received)
}
