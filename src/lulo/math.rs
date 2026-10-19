use super::state::LuloState;
use crate::accounts::{GlobalConfig, WrapperVault};
use crate::common::checked::{
    mul_bps, require_no_value_per_share_loss, u128_to_u64, MIN_DEPOSIT_SHARES_OUT,
};
use crate::common::u256::U256;
use crate::error::OverpassError;

fn math_err(_label: &'static str) -> OverpassError {
    OverpassError::MathOverflow
}

const Q60_SHIFT: usize = 60;

fn cpos(pa: u64, pts: u64, lp: u128, basis_q60: u128) -> Result<u64, OverpassError> {
    if pts == 0 || pa == 0 || lp == 0 {
        return Ok(0);
    }
    let q60: U256 = U256::one() << Q60_SHIFT;
    let pa_q60 = U256::from(pa) * q60;
    let basis_pts = U256::from(basis_q60) * U256::from(pts);
    let gain_q60 = if basis_pts >= pa_q60 {
        U256::zero()
    } else {
        pa_q60 - basis_pts
    };
    let profit = (gain_q60 * U256::from(lp)) / (U256::from(pts) * q60);
    u64::try_from(profit.low_u128()).map_err(|_| math_err("lulo profit overflow"))
}

fn fee_from_profit(profit_usdc: u64, ref_bps: u16) -> Result<u64, OverpassError> {
    let pb = (profit_usdc as u128)
        .checked_mul(ref_bps as u128)
        .ok_or(math_err("lulo fee mul"))?;
    Ok(if pb == 0 {
        0
    } else {
        ((pb - 1) / 10_000) as u64
    })
}

fn position_fee_usdc(
    pa: u64,
    pts: u64,
    lp_a: u128,
    old_lp: u128,
    charged_q60: u128,
    avg_q60: u128,
    ref_bps: u16,
) -> Result<u64, OverpassError> {
    let a = cpos(pa, pts, lp_a, charged_q60)?;
    let b = cpos(pa, pts, old_lp, avg_q60)?;
    fee_from_profit(a.min(b), ref_bps)
}

fn lulo_lp_value(
    pa: u64,
    pts: u64,
    old_lp: u128,
    charged_q60: u128,
    avg_q60: u128,
    ref_bps: u16,
) -> Result<u64, OverpassError> {
    if pts == 0 || pa == 0 || old_lp == 0 {
        return Ok(0);
    }
    let fee_usdc = position_fee_usdc(pa, pts, old_lp, old_lp, charged_q60, avg_q60, ref_bps)?;
    let gross = u64::try_from(
        old_lp
            .checked_mul(pa as u128)
            .and_then(|x| x.checked_div(pts as u128))
            .ok_or(math_err("lulo gross"))?,
    )
    .map_err(|_| math_err("lulo gross overflow"))?;
    Ok(gross.saturating_sub(fee_usdc))
}

pub fn quote_deposit(
    state: &LuloState,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(math_err("zero amount"));
    }
    if state.halted_flag != 0 {
        return Err(math_err("lulo halted"));
    }
    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let total_fee = protocol_fee
        .checked_add(creator_fee)
        .ok_or(math_err("deposit fee sum"))?;
    let net_amount = amount
        .checked_sub(total_fee)
        .ok_or(math_err("net amount"))?;
    if net_amount == 0 {
        return Err(math_err("net amount zero"));
    }

    let cap_headroom = state.deposit_limit.saturating_sub(state.protected_amount);
    if net_amount > cap_headroom / 2 {
        return Err(math_err("lulo coverage cap"));
    }

    let pa = state.protected_amount;
    let pts = state.protected_total_supply;
    let old_lp = wv.intermediate_held;

    let lp_value_before = lulo_lp_value(
        pa,
        pts,
        old_lp,
        state.charged_q60,
        state.avg_q60,
        state.ref_bps,
    )?;
    let total_value_before = wv
        .free_underlying_held
        .checked_add(lp_value_before)
        .ok_or(math_err("total value"))?;

    let buffer_total = wv
        .free_underlying_held
        .checked_add(net_amount)
        .ok_or(math_err("buffer overflow"))?;
    if pa == 0 {
        return Err(math_err("lulo pa=0"));
    }
    let new_lp = (buffer_total as u128)
        .checked_mul(pts as u128)
        .and_then(|x| x.checked_div(pa as u128))
        .ok_or(math_err("new_lp"))?;
    let lp_a = old_lp.checked_add(new_lp).ok_or(math_err("lp_a"))?;
    let fee_usdc = position_fee_usdc(
        pa,
        pts,
        lp_a,
        old_lp,
        state.charged_q60,
        state.avg_q60,
        state.ref_bps,
    )?;
    let fee_lp_ceil: u128 = if fee_usdc == 0 {
        0
    } else {
        let num = (fee_usdc as u128)
            .checked_mul(pts as u128)
            .ok_or(math_err("fee_lp num"))?;
        num.checked_add((pa as u128) - 1)
            .and_then(|x| x.checked_div(pa as u128))
            .ok_or(math_err("fee_lp_ceil"))?
    };
    let lp_net_pred = new_lp.saturating_sub(fee_lp_ceil);
    let post_lp_pred = old_lp
        .checked_add(lp_net_pred)
        .ok_or(math_err("post_lp_pred"))?;
    let post_value = lulo_lp_value(
        pa,
        pts,
        post_lp_pred,
        state.charged_q60,
        state.avg_q60,
        state.ref_bps,
    )?;
    let post_fee = position_fee_usdc(
        pa,
        pts,
        post_lp_pred,
        post_lp_pred,
        state.charged_q60,
        state.avg_q60,
        state.ref_bps,
    )?;
    let staked_value = post_value
        .checked_add(post_fee)
        .ok_or(math_err("post gross"))?;
    let new_value_pred = staked_value.min(
        total_value_before
            .checked_add(net_amount)
            .ok_or(math_err("dust value"))?,
    );
    let value_added = new_value_pred.saturating_sub(total_value_before);

    let wrapper_to_mint: u64 = if wv.wrapper_supply == 0 {
        if total_value_before != 0 {
            return Err(math_err("supply=0 value>0"));
        }
        new_value_pred
    } else {
        if total_value_before == 0 {
            return Err(math_err("supply>0 value=0"));
        }
        u128_to_u64(
            (value_added as u128)
                .checked_mul(wv.wrapper_supply as u128)
                .and_then(|x| x.checked_div(total_value_before as u128))
                .ok_or(math_err("mint calc"))?,
        )?
    };
    if wrapper_to_mint == 0 {
        return Err(math_err("mint zero"));
    }
    if wv.wrapper_supply != 0 && wrapper_to_mint < MIN_DEPOSIT_SHARES_OUT {
        return Err(OverpassError::DepositSharesTooLow);
    }

    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_add(wrapper_to_mint)
        .ok_or(math_err("new supply"))?;
    require_no_value_per_share_loss(
        total_value_before as u128,
        wv.wrapper_supply,
        new_value_pred as u128,
        new_wrapper_supply,
    )?;

    Ok(wrapper_to_mint)
}

pub fn quote_withdraw(
    state: &LuloState,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(math_err("zero amount"));
    }
    if state.halted_flag != 0 {
        return Err(math_err("lulo halted"));
    }
    if amount > wv.wrapper_supply {
        return Err(math_err("burn exceeds supply"));
    }
    if wv.wrapper_supply == 0 {
        return Err(math_err("supply zero"));
    }

    let pa = state.protected_amount;
    let pts = state.protected_total_supply;
    let old_lp = wv.intermediate_held;
    let lp_value = lulo_lp_value(
        pa,
        pts,
        old_lp,
        state.charged_q60,
        state.avg_q60,
        state.ref_bps,
    )?;
    let total_value_before = wv
        .free_underlying_held
        .checked_add(lp_value)
        .ok_or(math_err("total value"))?;
    if total_value_before == 0 {
        return Err(math_err("total value zero"));
    }

    if amount == wv.wrapper_supply {
        return Err(math_err(
            "lulo full-exit not quoted (drain-zone); withdraw supply-1",
        ));
    }
    let n_owed: u64 = {
        let s = wv.wrapper_supply as u128;
        let b = amount as u128;
        let free_share = b
            .checked_mul(wv.free_underlying_held as u128)
            .and_then(|x| x.checked_div(s))
            .ok_or(math_err("free share"))?;
        let user_lp = b
            .checked_mul(old_lp)
            .and_then(|x| x.checked_div(s))
            .ok_or(math_err("user lp"))?;
        let lp_part = if old_lp == 0 {
            0
        } else {
            (lp_value as u128)
                .checked_mul(user_lp)
                .and_then(|x| x.checked_div(old_lp))
                .ok_or(math_err("lp part"))?
        };
        let ceil_price: u128 = if pts == 0 {
            0
        } else {
            (pa as u128).div_ceil(pts as u128)
        };
        let owed = free_share
            .checked_add(lp_part)
            .ok_or(math_err("owed calc"))?
            .saturating_sub(ceil_price + 1);
        u128_to_u64(owed)?
    };
    if n_owed == 0 {
        return Err(math_err("owed zero"));
    }

    let new_value_pred = (total_value_before as u128)
        .checked_sub(n_owed as u128)
        .ok_or(math_err("new value pred"))?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(math_err("new supply"))?;
    require_no_value_per_share_loss(
        total_value_before as u128,
        wv.wrapper_supply,
        new_value_pred,
        new_wrapper_supply,
    )?;

    Ok(n_owed)
}
