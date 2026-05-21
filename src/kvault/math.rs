use super::state::{ActiveAllocation, KvaultState};
use crate::accounts::{GlobalConfig, WrapperVault};
use crate::common::checked::{mul_bps, require_no_value_per_share_loss, u128_to_u64, FULL_BPS};
use crate::common::u256::U256;
use crate::error::OverpassError;
use crate::klend;
use crate::klend::fraction::Fraction;

const SECONDS_PER_YEAR: u128 = 31_556_926;

pub fn quote_deposit(
    state: &KvaultState,
    current_slot: u64,
    unix_timestamp: i64,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
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

    let aum = compute_current_aum(state, current_slot, unix_timestamp)?;
    let kvault_shares_to_mint = get_shares_to_mint(aum, max_user_tokens, state.shares_issued)?;
    if kvault_shares_to_mint == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let wrapper_to_mint = if wv.wrapper_supply == 0 {
        net_amount
    } else {
        if state.position_kvault_shares == 0 {
            return Err(OverpassError::InvalidState(
                "kvault: wrapper_supply>0 but intermediate_held=0",
            ));
        }
        u128_to_u64(
            (kvault_shares_to_mint as u128)
                .checked_mul(wv.wrapper_supply as u128)
                .and_then(|x| x.checked_div(state.position_kvault_shares))
                .ok_or(OverpassError::MathOverflow)?,
        )?
    };
    if wrapper_to_mint == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let new_total_kvault_shares = wv
        .intermediate_held
        .checked_add(kvault_shares_to_mint as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_add(wrapper_to_mint)
        .ok_or(OverpassError::MathOverflow)?;
    require_no_value_per_share_loss(
        state.position_kvault_shares,
        wv.wrapper_supply,
        new_total_kvault_shares,
        new_wrapper_supply,
    )?;

    Ok(wrapper_to_mint)
}

pub fn quote_withdraw(
    state: &KvaultState,
    current_slot: u64,
    unix_timestamp: i64,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }
    if state.position_kvault_shares == 0 {
        return Err(OverpassError::InvalidState(
            "kvault: intermediate_held=0 on withdraw",
        ));
    }

    let shares_to_redeem = u128_to_u64(
        (amount as u128)
            .checked_mul(state.position_kvault_shares)
            .and_then(|x| x.checked_div(wv.wrapper_supply as u128))
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if shares_to_redeem == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let aum = compute_current_aum(state, current_slot, unix_timestamp)?;
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

    let penalty_bps = state.effective_penalty_bps();
    let penalty_lamports = state.effective_penalty_lamports();
    let bps_prod = (total_with_penalty as u128)
        .checked_mul(penalty_bps as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let denom = FULL_BPS as u128;
    let penalty_from_bps_ceil = u128_to_u64(bps_prod.div_ceil(denom))?;
    let penalty = penalty_from_bps_ceil.max(penalty_lamports);
    if penalty >= total_with_penalty {
        return Err(OverpassError::InvalidAmount);
    }
    let total_for_user = total_with_penalty
        .checked_sub(penalty)
        .ok_or(OverpassError::MathOverflow)?;
    if total_for_user == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let available_to_send: u64 = state.token_available.min(total_for_user);
    let final_for_user = if total_for_user <= available_to_send {
        total_for_user
    } else {
        let to_disinvest = total_for_user
            .checked_sub(available_to_send)
            .ok_or(OverpassError::MathOverflow)?;
        let primary = state
            .active_allocations
            .iter()
            .max_by_key(|a| a.ctoken_allocation)
            .ok_or(OverpassError::InvalidState("kvault: no active allocations"))?;
        let reserve = state
            .cached_reserves
            .iter()
            .find(|(k, _)| *k == primary.reserve)
            .map(|(_, r)| r)
            .ok_or(OverpassError::AccountMissing(
                "kvault primary reserve (not in update map)",
            ))?;
        let accrued = klend::math::accrue_interest(reserve, current_slot);
        let total_supply = klend::math::total_supply_fraction(reserve, &accrued);
        let (cs, ts) =
            klend::math::exchange_rate_pair(reserve.collateral_mint_total_supply, total_supply);
        let in_reserve_liq_f =
            klend::math::fraction_collateral_to_liquidity(primary.ctoken_allocation, cs, ts)?;
        let send_f = in_reserve_liq_f.min(Fraction::from_num(to_disinvest));
        if send_f == Fraction::ZERO {
            available_to_send
        } else {
            let send_floor: u64 = send_f.floor().to_num::<u64>();
            let disinvest_ctokens =
                klend::math::fraction_liquidity_to_collateral_ceil(send_floor, cs, ts)?
                    .min(primary.ctoken_allocation);
            let disinvest_liq_f =
                klend::math::fraction_collateral_to_liquidity(disinvest_ctokens, cs, ts)?;
            let rounding_err: u64 = if disinvest_liq_f.frac() > Fraction::ZERO
                && disinvest_liq_f.frac() > send_f.frac()
            {
                1
            } else {
                0
            };
            let actual = send_floor.saturating_sub(rounding_err);
            available_to_send
                .checked_add(actual)
                .ok_or(OverpassError::MathOverflow)?
        }
    };
    if final_for_user == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if final_for_user <= state.min_withdraw_amount {
        return Err(OverpassError::InvalidAmount);
    }

    let new_total_kvault_shares = wv
        .intermediate_held
        .checked_sub(shares_to_redeem as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(OverpassError::MathOverflow)?;
    require_no_value_per_share_loss(
        state.position_kvault_shares,
        wv.wrapper_supply,
        new_total_kvault_shares,
        new_wrapper_supply,
    )?;

    Ok(final_for_user)
}

fn compute_current_aum(
    state: &KvaultState,
    current_slot: u64,
    unix_timestamp: i64,
) -> Result<Fraction, OverpassError> {
    let invested_total = invested_total(state, current_slot)?;
    let available = Fraction::from_num(state.token_available);
    let holdings_total = available + invested_total;
    let pending_fees_after_charge = charged_pending_fees(state, unix_timestamp, invested_total)?;

    if holdings_total < pending_fees_after_charge {
        return Err(OverpassError::InvalidState(
            "kvault: AUM below pending fees",
        ));
    }
    Ok(holdings_total - pending_fees_after_charge)
}

fn invested_total(state: &KvaultState, current_slot: u64) -> Result<Fraction, OverpassError> {
    let mut total = Fraction::ZERO;
    for alloc in &state.active_allocations {
        total += invested_for_allocation(state, current_slot, alloc)?;
    }
    Ok(total)
}

fn invested_for_allocation(
    state: &KvaultState,
    current_slot: u64,
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

    let accrued = klend::math::accrue_interest(reserve, current_slot);
    let total_supply = klend::math::total_supply_fraction(reserve, &accrued);
    let (cs, ts) =
        klend::math::exchange_rate_pair(reserve.collateral_mint_total_supply, total_supply);
    klend::math::fraction_collateral_to_liquidity(alloc.ctoken_allocation, cs, ts)
}

fn charged_pending_fees(
    state: &KvaultState,
    unix_timestamp: i64,
    invested_total: Fraction,
) -> Result<Fraction, OverpassError> {
    if state.last_fee_charge_timestamp == 0 {
        return Ok(state.pending_fees());
    }
    let now = unix_timestamp.max(0) as u64;
    let seconds_passed = now.saturating_sub(state.last_fee_charge_timestamp);

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
