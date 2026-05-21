use fixed::types::I80F48;

use super::state::{
    BankOperationalState, MarginfiState, RatePoint, INTEREST_CURVE_LEGACY,
    INTEREST_CURVE_SEVEN_POINT,
};
use crate::accounts::{GlobalConfig, WrapperVault};
use crate::common::checked::{mul_bps, require_no_value_per_share_loss};
use crate::error::OverpassError;

const ASSET_TAG_DRIFT: u8 = 4;

fn seconds_per_year() -> I80F48 {
    I80F48::from_num(31_536_000)
}

fn math_err(_label: &'static str) -> OverpassError {
    OverpassError::MathOverflow
}

fn get_asset_amount(state: &MarginfiState, shares: I80F48) -> Result<I80F48, OverpassError> {
    shares
        .checked_mul(state.asset_share_value)
        .ok_or(math_err("get_asset_amount"))
}

fn get_liability_amount(state: &MarginfiState, shares: I80F48) -> Result<I80F48, OverpassError> {
    shares
        .checked_mul(state.liability_share_value)
        .ok_or(math_err("get_liability_amount"))
}

pub fn quote_deposit(
    state: &MarginfiState,
    unix_timestamp: i64,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if state.asset_tag == ASSET_TAG_DRIFT {
        return Err(OverpassError::InvalidState(
            "marginfi: drift asset_tag not supported",
        ));
    }
    match state.operational_state {
        BankOperationalState::KilledByBankruptcy => return Err(OverpassError::BankKilled),
        BankOperationalState::Paused => return Err(OverpassError::BankPaused),
        BankOperationalState::ReduceOnly => return Err(OverpassError::BankReduceOnly),
        BankOperationalState::Operational => {}
    }

    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let total_fee = protocol_fee
        .checked_add(creator_fee)
        .ok_or(math_err("fee sum"))?;
    let net_amount = amount
        .checked_sub(total_fee)
        .ok_or(math_err("net amount"))?;
    if net_amount == 0 {
        return Err(math_err("net zero"));
    }

    let accrued = accrue_share_values(state, unix_timestamp)?;
    if accrued.asset_share_value == I80F48::ZERO {
        return Err(math_err("share value zero"));
    }

    let value = I80F48::from_num(net_amount);
    let shares_delta = value
        .checked_div(accrued.asset_share_value)
        .ok_or(math_err("get_asset_shares"))?;

    if state.deposit_limit != 0 && state.deposit_limit != u64::MAX {
        let new_total_shares = state
            .total_asset_shares
            .checked_add(shares_delta)
            .ok_or(math_err("total shares"))?;
        let new_total_assets = new_total_shares
            .checked_mul(accrued.asset_share_value)
            .ok_or(math_err("get_asset_amount"))?;
        if new_total_assets >= I80F48::from_num(state.deposit_limit) {
            return Err(OverpassError::BankAssetCapacityExceeded);
        }
    }

    let pre_balance = state.cached_user_asset_shares;
    if shares_delta <= I80F48::ZERO {
        return Err(math_err("shares zero"));
    }

    let wrapper_to_mint: u64 = if wv.wrapper_supply == 0 {
        net_amount
    } else {
        if pre_balance <= I80F48::ZERO {
            return Err(math_err("wrapper>0 pre-balance zero"));
        }
        shares_delta
            .checked_mul(I80F48::from_num(wv.wrapper_supply))
            .and_then(|x| x.checked_div(pre_balance))
            .ok_or(math_err("wrap calc"))?
            .floor()
            .checked_to_num::<u64>()
            .ok_or(math_err("mint overflow"))?
    };
    if wrapper_to_mint == 0 {
        return Err(math_err("mint zero"));
    }

    let pre_asset_shares_raw = pre_balance.to_bits().max(0) as u128;
    let expected_asset_shares_raw = shares_delta.to_bits().max(0) as u128;
    let new_asset_shares_raw = pre_asset_shares_raw
        .checked_add(expected_asset_shares_raw)
        .ok_or(math_err("new shares total"))?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_add(wrapper_to_mint)
        .ok_or(math_err("new supply"))?;
    require_no_value_per_share_loss(
        pre_asset_shares_raw,
        wv.wrapper_supply,
        new_asset_shares_raw,
        new_wrapper_supply,
    )?;

    Ok(wrapper_to_mint)
}

pub fn quote_withdraw(
    state: &MarginfiState,
    unix_timestamp: i64,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, OverpassError> {
    if amount == 0 {
        return Err(OverpassError::InvalidAmount);
    }
    if state.asset_tag == ASSET_TAG_DRIFT {
        return Err(OverpassError::InvalidState(
            "marginfi: drift asset_tag not supported",
        ));
    }
    match state.operational_state {
        BankOperationalState::KilledByBankruptcy => return Err(OverpassError::BankKilled),
        BankOperationalState::Paused => return Err(OverpassError::BankPaused),
        BankOperationalState::Operational | BankOperationalState::ReduceOnly => {}
    }
    if amount > wv.wrapper_supply {
        return Err(OverpassError::BurnExceedsWrapperSupply);
    }

    let total_shares_raw: u128 = {
        let bits = state.cached_user_asset_shares.to_bits();
        if bits <= 0 {
            return Err(math_err("cached shares zero"));
        }
        bits as u128
    };

    let shares_pro_rata_u128 = (amount as u128)
        .checked_mul(total_shares_raw)
        .and_then(|x| x.checked_div(wv.wrapper_supply as u128))
        .ok_or(math_err("pro rata"))?;
    if shares_pro_rata_u128 == 0 {
        return Err(math_err("pro rata zero"));
    }

    let accrued = accrue_share_values(state, unix_timestamp)?;

    let shares_pro_rata = I80F48::from_bits(shares_pro_rata_u128 as i128);
    let underlying = shares_pro_rata
        .checked_mul(accrued.asset_share_value)
        .ok_or(math_err("get_asset_amount"))?;
    let requested_underlying: u64 = underlying
        .floor()
        .checked_to_num()
        .ok_or(math_err("underlying overflow"))?;
    if requested_underlying == 0 {
        return Err(math_err("underlying zero"));
    }

    let total_assets = state
        .total_asset_shares
        .checked_mul(accrued.asset_share_value)
        .ok_or(math_err("total assets"))?;
    let total_liabilities = state
        .total_liability_shares
        .checked_mul(accrued.liability_share_value)
        .ok_or(math_err("total liabilities"))?;
    let withdrawn = I80F48::from_num(requested_underlying);
    let post_assets = total_assets
        .checked_sub(withdrawn)
        .ok_or(OverpassError::InsufficientLiquidity)?;
    if post_assets < total_liabilities {
        return Err(OverpassError::InsufficientLiquidity);
    }

    let new_asset_shares_raw = total_shares_raw
        .checked_sub(shares_pro_rata_u128)
        .ok_or(math_err("new shares total"))?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(math_err("new supply"))?;
    require_no_value_per_share_loss(
        total_shares_raw,
        wv.wrapper_supply,
        new_asset_shares_raw,
        new_wrapper_supply,
    )?;

    Ok(requested_underlying)
}

struct AccruedShareValues {
    asset_share_value: I80F48,
    #[allow(dead_code)]
    liability_share_value: I80F48,
}

fn accrue_share_values(
    state: &MarginfiState,
    unix_timestamp: i64,
) -> Result<AccruedShareValues, OverpassError> {
    if state.last_update <= 0 || unix_timestamp <= state.last_update {
        return Ok(AccruedShareValues {
            asset_share_value: state.asset_share_value,
            liability_share_value: state.liability_share_value,
        });
    }
    let time_delta: u64 = (unix_timestamp - state.last_update) as u64;
    if time_delta == 0 {
        return Ok(AccruedShareValues {
            asset_share_value: state.asset_share_value,
            liability_share_value: state.liability_share_value,
        });
    }

    let total_assets = get_asset_amount(state, state.total_asset_shares)?;
    let total_liabilities = get_liability_amount(state, state.total_liability_shares)?;

    if total_assets == I80F48::ZERO || total_liabilities == I80F48::ZERO {
        return Ok(AccruedShareValues {
            asset_share_value: state.asset_share_value,
            liability_share_value: state.liability_share_value,
        });
    }

    let ir_calc = create_interest_rate_calculator(state);
    let changes = calc_interest_rate_accrual_state_changes(
        time_delta,
        total_assets,
        total_liabilities,
        &ir_calc,
        state.asset_share_value,
        state.liability_share_value,
    )?;

    Ok(AccruedShareValues {
        asset_share_value: changes.new_asset_share_value,
        liability_share_value: changes.new_liability_share_value,
    })
}

fn create_interest_rate_calculator(state: &MarginfiState) -> InterestRateCalc {
    let cfg = &state.interest_rate_config;
    InterestRateCalc {
        optimal_utilization_rate: cfg.optimal_utilization_rate,
        plateau_interest_rate: cfg.plateau_interest_rate,
        max_interest_rate: cfg.max_interest_rate,
        insurance_fixed_fee: cfg.insurance_fee_fixed_apr,
        insurance_rate_fee: cfg.insurance_ir_fee,
        protocol_fixed_fee: cfg.protocol_fixed_fee_apr,
        protocol_rate_fee: cfg.protocol_ir_fee,
        add_program_fees: state.group_program_fees_enabled,
        program_fee_fixed: state.group_program_fee_fixed,
        program_fee_rate: state.group_program_fee_rate,
        zero_util_rate: cfg.zero_util_rate,
        hundred_util_rate: cfg.hundred_util_rate,
        points: cfg.points,
        curve_type: cfg.curve_type,
    }
}

#[derive(Debug, Clone)]
struct InterestRateCalc {
    optimal_utilization_rate: I80F48,
    plateau_interest_rate: I80F48,
    max_interest_rate: I80F48,

    insurance_fixed_fee: I80F48,
    insurance_rate_fee: I80F48,
    protocol_fixed_fee: I80F48,
    protocol_rate_fee: I80F48,

    program_fee_fixed: I80F48,
    program_fee_rate: I80F48,

    add_program_fees: bool,
    zero_util_rate: u32,
    hundred_util_rate: u32,
    points: [RatePoint; 5],
    curve_type: u8,
}

struct Fees {
    insurance_fee_rate: I80F48,
    insurance_fee_fixed: I80F48,
    group_fee_rate: I80F48,
    group_fee_fixed: I80F48,
    protocol_fee_rate: I80F48,
    protocol_fee_fixed: I80F48,
}

#[allow(dead_code)]
struct ComputedInterestRates {
    base_rate_apr: I80F48,
    lending_rate_apr: I80F48,
    borrowing_rate_apr: I80F48,
    group_fee_apr: I80F48,
    insurance_fee_apr: I80F48,
    protocol_fee_apr: I80F48,
}

impl InterestRateCalc {
    fn calc_interest_rate(
        &self,
        utilization_ratio: I80F48,
    ) -> Result<ComputedInterestRates, OverpassError> {
        let Fees {
            insurance_fee_rate,
            insurance_fee_fixed,
            group_fee_rate,
            group_fee_fixed,
            protocol_fee_rate,
            protocol_fee_fixed,
        } = self.get_fees();

        let fee_ir: I80F48 = insurance_fee_rate
            .checked_add(group_fee_rate)
            .and_then(|x| x.checked_add(protocol_fee_rate))
            .ok_or(math_err("fee_ir"))?;
        let fee_fixed: I80F48 = insurance_fee_fixed
            .checked_add(group_fee_fixed)
            .and_then(|x| x.checked_add(protocol_fee_fixed))
            .ok_or(math_err("fee_fixed"))?;

        let base_rate_apr: I80F48 = match self.curve_type {
            INTEREST_CURVE_LEGACY => self.interest_rate_curve(utilization_ratio)?,
            INTEREST_CURVE_SEVEN_POINT => self.interest_rate_multipoint_curve(utilization_ratio)?,
            _ => return Err(math_err("unsupported curve type")),
        };

        let lending_rate_apr: I80F48 = base_rate_apr
            .checked_mul(utilization_ratio)
            .ok_or(math_err("lending_rate_apr"))?;

        let borrowing_rate_apr: I80F48 = base_rate_apr
            .checked_mul(
                I80F48::ONE
                    .checked_add(fee_ir)
                    .ok_or(math_err("1+fee_ir"))?,
            )
            .ok_or(math_err("borrow*1+fee_ir"))?
            .checked_add(fee_fixed)
            .ok_or(math_err("borrow+fee_fixed"))?;

        let group_fee_apr: I80F48 = calc_fee_rate(base_rate_apr, group_fee_rate, group_fee_fixed)?;
        let insurance_fee_apr: I80F48 =
            calc_fee_rate(base_rate_apr, insurance_fee_rate, insurance_fee_fixed)?;
        let protocol_fee_apr: I80F48 =
            calc_fee_rate(base_rate_apr, protocol_fee_rate, protocol_fee_fixed)?;

        Ok(ComputedInterestRates {
            base_rate_apr,
            lending_rate_apr,
            borrowing_rate_apr,
            group_fee_apr,
            insurance_fee_apr,
            protocol_fee_apr,
        })
    }

    fn interest_rate_curve(&self, ur: I80F48) -> Result<I80F48, OverpassError> {
        let optimal_ur: I80F48 = self.optimal_utilization_rate;
        let plateau_ir: I80F48 = self.plateau_interest_rate;
        let max_ir: I80F48 = self.max_interest_rate;

        if ur <= optimal_ur {
            ur.checked_div(optimal_ur)
                .and_then(|x| x.checked_mul(plateau_ir))
                .ok_or(math_err("legacy curve <= optimal"))
        } else {
            (ur - optimal_ur)
                .checked_div(I80F48::ONE - optimal_ur)
                .and_then(|x| x.checked_mul(max_ir - plateau_ir))
                .and_then(|x| x.checked_add(plateau_ir))
                .ok_or(math_err("legacy curve > optimal"))
        }
    }

    fn interest_rate_multipoint_curve(&self, ur: I80F48) -> Result<I80F48, OverpassError> {
        let zero_rate: I80F48 = Self::rate_from_u32(self.zero_util_rate);
        let hundred_rate: I80F48 = Self::rate_from_u32(self.hundred_util_rate);

        let mut prev_util: I80F48 = I80F48::ZERO;
        let mut prev_rate: I80F48 = zero_rate;
        let ur: I80F48 = ur.max(I80F48::ZERO).min(I80F48::ONE);

        for point in self.points.iter().filter(|p| p.util != 0) {
            let point_util: I80F48 = Self::util_from_u32(point.util);
            let point_rate: I80F48 = Self::rate_from_u32(point.rate);

            if ur <= point_util {
                return Self::lerp(prev_util, prev_rate, point_util, point_rate, ur);
            }

            prev_util = point_util;
            prev_rate = point_rate;
        }

        Self::lerp(prev_util, prev_rate, I80F48::ONE, hundred_rate, ur)
    }

    fn lerp(
        start_x: I80F48,
        start_y: I80F48,
        end_x: I80F48,
        end_y: I80F48,
        target_x: I80F48,
    ) -> Result<I80F48, OverpassError> {
        if end_x <= start_x {
            return Ok(start_y);
        }
        if target_x < start_x {
            return Err(math_err("lerp target<start"));
        }
        if target_x > end_x {
            return Err(math_err("lerp target>end"));
        }
        if end_y < start_y {
            return Err(math_err("lerp end_y<start_y"));
        }

        let delta_x: I80F48 = end_x - start_x;
        if delta_x.is_zero() {
            return Ok(start_y);
        }

        let offset: I80F48 = target_x - start_x;
        let proportion: I80F48 = offset / delta_x;
        let delta_y: I80F48 = end_y - start_y;
        let scaled_delta: I80F48 = delta_y
            .checked_mul(proportion)
            .ok_or(math_err("lerp mul"))?;
        Ok(start_y + scaled_delta)
    }

    fn rate_from_u32(rate: u32) -> I80F48 {
        let ratio: I80F48 = I80F48::from_num(rate) / I80F48::from_num(u32::MAX);
        ratio * I80F48::from_num(10)
    }

    fn util_from_u32(util: u32) -> I80F48 {
        I80F48::from_num(util) / I80F48::from_num(u32::MAX)
    }

    fn get_fees(&self) -> Fees {
        let (protocol_fee_rate, protocol_fee_fixed) = if self.add_program_fees {
            (self.program_fee_rate, self.program_fee_fixed)
        } else {
            (I80F48::ZERO, I80F48::ZERO)
        };

        Fees {
            insurance_fee_rate: self.insurance_rate_fee,
            insurance_fee_fixed: self.insurance_fixed_fee,
            group_fee_rate: self.protocol_rate_fee,
            group_fee_fixed: self.protocol_fixed_fee,
            protocol_fee_rate,
            protocol_fee_fixed,
        }
    }
}

fn calc_fee_rate(
    base_rate: I80F48,
    rate_fees: I80F48,
    fixed_fees: I80F48,
) -> Result<I80F48, OverpassError> {
    if rate_fees.is_zero() {
        return Ok(fixed_fees);
    }
    base_rate
        .checked_mul(rate_fees)
        .and_then(|x| x.checked_add(fixed_fees))
        .ok_or(math_err("calc_fee_rate"))
}

fn calc_accrued_interest_payment_per_period(
    apr: I80F48,
    time_delta: u64,
    value: I80F48,
) -> Result<I80F48, OverpassError> {
    let ir_per_period: I80F48 = apr
        .checked_mul(I80F48::from_num(time_delta))
        .and_then(|x| x.checked_div(seconds_per_year()))
        .ok_or(math_err("ir_per_period"))?;

    value
        .checked_mul(
            I80F48::ONE
                .checked_add(ir_per_period)
                .ok_or(math_err("1+ir"))?,
        )
        .ok_or(math_err("accrued_interest mul"))
}

struct InterestRateStateChanges {
    new_asset_share_value: I80F48,
    new_liability_share_value: I80F48,
}

fn calc_interest_rate_accrual_state_changes(
    time_delta: u64,
    total_assets_amount: I80F48,
    total_liabilities_amount: I80F48,
    interest_rate_calc: &InterestRateCalc,
    asset_share_value: I80F48,
    liability_share_value: I80F48,
) -> Result<InterestRateStateChanges, OverpassError> {
    let utilization_rate: I80F48 = total_liabilities_amount
        .checked_div(total_assets_amount)
        .ok_or(math_err("utilization_rate"))?;
    let interest_rates = interest_rate_calc.calc_interest_rate(utilization_rate)?;

    let new_asset_share_value = calc_accrued_interest_payment_per_period(
        interest_rates.lending_rate_apr,
        time_delta,
        asset_share_value,
    )?;
    let new_liability_share_value = calc_accrued_interest_payment_per_period(
        interest_rates.borrowing_rate_apr,
        time_delta,
        liability_share_value,
    )?;

    Ok(InterestRateStateChanges {
        new_asset_share_value,
        new_liability_share_value,
    })
}
