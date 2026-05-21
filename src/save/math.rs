use super::state::SaveState;
use super::wad::{self, Wad, WAD};
use crate::accounts::{GlobalConfig, WrapperVault};
use crate::common::checked::{mul_bps, require_no_value_per_share_loss, u128_to_u64};
use crate::error::OverpassError;

const INITIAL_COLLATERAL_RATE: Wad = WAD;

#[derive(Debug, Clone, Copy)]
struct AccruedSave {
    borrowed_amount_wads: Wad,
    accumulated_protocol_fees_wads: Wad,
}

fn accrue_interest(state: &SaveState, current_slot: u64) -> Result<AccruedSave, OverpassError> {
    let slots_elapsed = current_slot.saturating_sub(state.last_update_slot);
    if slots_elapsed == 0 || state.borrowed_amount_wads == 0 {
        return Ok(AccruedSave {
            borrowed_amount_wads: state.borrowed_amount_wads,
            accumulated_protocol_fees_wads: state.accumulated_protocol_fees_wads,
        });
    }

    let borrow_rate = current_borrow_rate(state)?;
    let slot_interest_rate = borrow_rate / wad::SLOTS_PER_YEAR as u128;
    let one_plus_slot_rate = WAD.saturating_add(slot_interest_rate);
    let compounded = wad::wad_pow(one_plus_slot_rate, slots_elapsed)?;

    let new_borrowed = wad::wad_mul(state.borrowed_amount_wads, compounded)?;
    let net_new_debt = new_borrowed.saturating_sub(state.borrowed_amount_wads);

    let take_rate = wad::from_percent(state.protocol_take_rate);
    let protocol_share = wad::wad_mul(net_new_debt, take_rate)?;
    let new_protocol_fees = state
        .accumulated_protocol_fees_wads
        .saturating_add(protocol_share);

    Ok(AccruedSave {
        borrowed_amount_wads: new_borrowed,
        accumulated_protocol_fees_wads: new_protocol_fees,
    })
}

fn total_supply_wad(state: &SaveState, accrued: &AccruedSave) -> Result<Wad, OverpassError> {
    let available_w = wad::from_u64(state.available_amount);
    available_w
        .checked_add(accrued.borrowed_amount_wads)
        .and_then(|x| x.checked_sub(accrued.accumulated_protocol_fees_wads))
        .ok_or(OverpassError::MathOverflow)
}

fn utilization_rate(state: &SaveState, accrued: &AccruedSave) -> Result<Wad, OverpassError> {
    if accrued.borrowed_amount_wads == 0 {
        return Ok(0);
    }
    let available_w = wad::from_u64(state.available_amount);
    let denom = available_w
        .checked_add(accrued.borrowed_amount_wads)
        .ok_or(OverpassError::MathOverflow)?;
    if denom == 0 {
        return Ok(0);
    }
    wad::wad_div(accrued.borrowed_amount_wads, denom)
}

fn current_borrow_rate(state: &SaveState) -> Result<Wad, OverpassError> {
    let pre = AccruedSave {
        borrowed_amount_wads: state.borrowed_amount_wads,
        accumulated_protocol_fees_wads: state.accumulated_protocol_fees_wads,
    };
    let utilization = utilization_rate(state, &pre)?;

    let optimal_util = wad::from_percent(state.optimal_utilization_rate);
    let max_util = wad::from_percent(state.max_utilization_rate);
    let min_rate = wad::from_percent(state.min_borrow_rate);
    let optimal_rate = wad::from_percent(state.optimal_borrow_rate);
    let max_rate = wad::from_percent(state.max_borrow_rate);

    if utilization <= optimal_util {
        if optimal_util == 0 {
            return Ok(min_rate);
        }
        let normalized = wad::wad_div(utilization, optimal_util)?;
        let rate_range = optimal_rate.saturating_sub(min_rate);
        let segment = wad::wad_mul(normalized, rate_range)?;
        Ok(min_rate.saturating_add(segment))
    } else if utilization <= max_util {
        let span = max_util.saturating_sub(optimal_util);
        if span == 0 {
            return Ok(optimal_rate);
        }
        let normalized = wad::wad_div(utilization.saturating_sub(optimal_util), span)?;
        let rate_range = max_rate.saturating_sub(optimal_rate);
        let segment = wad::wad_mul(normalized, rate_range)?;
        Ok(optimal_rate.saturating_add(segment))
    } else {
        let max_util_pct = state.max_utilization_rate as u128;
        let denom_pct = 100u128.saturating_sub(max_util_pct);
        if denom_pct == 0 {
            return Ok(max_rate);
        }
        let denom = wad::from_percent(denom_pct as u8);
        let normalized = wad::wad_div(utilization.saturating_sub(max_util), denom)?;
        let super_max_rate = (state.super_max_borrow_rate as u128).saturating_mul(WAD / 100);
        let rate_range = super_max_rate.saturating_sub(max_rate);
        let segment = wad::wad_mul(normalized, rate_range)?;
        Ok(max_rate.saturating_add(segment))
    }
}

fn exchange_rate(collateral_supply: u64, total_supply_w: Wad) -> Result<Wad, OverpassError> {
    if total_supply_w == 0 || collateral_supply == 0 {
        return Ok(INITIAL_COLLATERAL_RATE);
    }
    let coll_w = wad::from_u64(collateral_supply);
    wad::wad_div(coll_w, total_supply_w)
}

fn liquidity_to_collateral(liquidity: u64, rate_w: Wad) -> Result<u64, OverpassError> {
    let liq_w = wad::from_u64(liquidity);
    let product = wad::wad_mul(liq_w, rate_w)?;
    Ok(wad::to_floor_u64(product))
}

fn collateral_to_liquidity(collateral: u64, rate_w: Wad) -> Result<u64, OverpassError> {
    if rate_w == 0 {
        return Err(OverpassError::MathOverflow);
    }
    let coll_w = wad::from_u64(collateral);
    let quot = wad::wad_div(coll_w, rate_w)?;
    Ok(wad::to_floor_u64(quot))
}

pub fn quote_deposit(
    state: &SaveState,
    current_slot: u64,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let accrued = accrue_interest(state, current_slot)?;
    let total_supply_w = total_supply_wad(state, &accrued)?;
    let exchange_rate_w = exchange_rate(state.collateral_mint_total_supply, total_supply_w)?;

    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let net_amount = amount
        .checked_sub(protocol_fee)
        .and_then(|x| x.checked_sub(creator_fee))
        .ok_or(OverpassError::MathOverflow)?;
    if net_amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let post_supply_w = wad::from_u64(net_amount)
        .checked_add(total_supply_w)
        .ok_or(OverpassError::MathOverflow)?;
    if wad::to_floor_u64(post_supply_w) > state.deposit_limit {
        return Err(OverpassError::DepositLimitExceeded);
    }

    let collateral_received = liquidity_to_collateral(net_amount, exchange_rate_w)?;
    if collateral_received == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let wrapper_supply = wv.wrapper_supply;
    let prev_intermediate = wv.intermediate_held;
    let wrapper_to_mint = if wrapper_supply == 0 {
        net_amount
    } else {
        if prev_intermediate == 0 {
            return Err(OverpassError::InvalidState("save: ctokens out of sync"));
        }
        u128_to_u64(
            (collateral_received as u128)
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
        .checked_add(collateral_received as u128)
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
    state: &SaveState,
    current_slot: u64,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }
    if wv.intermediate_held == 0 {
        return Err(OverpassError::InvalidState("save: no cTokens"));
    }

    let accrued = accrue_interest(state, current_slot)?;
    let total_supply_w = total_supply_wad(state, &accrued)?;
    let exchange_rate_w = exchange_rate(state.collateral_mint_total_supply, total_supply_w)?;

    let ctokens_to_redeem = u128_to_u64(
        (amount as u128)
            .checked_mul(wv.intermediate_held)
            .ok_or(OverpassError::MathOverflow)?
            .checked_div(wv.wrapper_supply as u128)
            .ok_or(OverpassError::MathOverflow)?,
    )?;
    if ctokens_to_redeem == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    let underlying_received = collateral_to_liquidity(ctokens_to_redeem, exchange_rate_w)?;
    if underlying_received == 0 {
        return Err(OverpassError::InvalidAmount);
    }

    if underlying_received > state.available_amount {
        return Err(OverpassError::InsufficientLiquidity);
    }

    let new_intermediate = wv
        .intermediate_held
        .checked_sub(ctokens_to_redeem as u128)
        .ok_or(OverpassError::MathOverflow)?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(OverpassError::MathOverflow)?;
    require_no_value_per_share_loss(
        wv.intermediate_held,
        wv.wrapper_supply,
        new_intermediate,
        new_wrapper_supply,
    )?;

    Ok(underlying_received)
}
