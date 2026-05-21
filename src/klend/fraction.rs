pub use fixed::types::U68F60 as Fraction;

use crate::common::u256::U256;

pub const FRACTION_BITS_SHIFT: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct BigFraction(pub U256);

impl From<Fraction> for BigFraction {
    fn from(fraction: Fraction) -> Self {
        Self(U256::from(fraction.to_bits()))
    }
}

impl BigFraction {
    pub fn from_num(num: u128) -> Option<Self> {
        Some(Self(checked_shl(U256::from(num), FRACTION_BITS_SHIFT)?))
    }

    pub fn try_into_fraction(self) -> Option<Fraction> {
        let bits: u128 = self.0.try_into().ok()?;
        Some(Fraction::from_bits(bits))
    }

    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        if rhs.0.is_zero() {
            return None;
        }
        let extra_scaled = checked_shl(self.0, FRACTION_BITS_SHIFT)?;
        Some(Self(extra_scaled / rhs.0))
    }
}

impl core::ops::Mul for BigFraction {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        let extra_scaled = self.0 * rhs.0;
        Self(extra_scaled >> FRACTION_BITS_SHIFT)
    }
}

impl core::ops::Add for BigFraction {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Sub for BigFraction {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

fn checked_shl(number: U256, bit_count: u32) -> Option<U256> {
    let result = number << bit_count;
    if result >> bit_count == number {
        Some(result)
    } else {
        None
    }
}

pub fn approximate_compounded_interest(rate: Fraction, elapsed_slots: u64) -> Fraction {
    let slots_per_year: u128 = super::math::SLOTS_PER_YEAR.into();
    let base = rate / slots_per_year;

    match elapsed_slots {
        0 => return Fraction::ONE,
        1 => return Fraction::ONE + base,
        2 => return (Fraction::ONE + base) * (Fraction::ONE + base),
        3 => return (Fraction::ONE + base) * (Fraction::ONE + base) * (Fraction::ONE + base),
        4 => {
            let pow_two = (Fraction::ONE + base) * (Fraction::ONE + base);
            return pow_two * pow_two;
        }
        _ => (),
    }

    let exp: u128 = elapsed_slots.into();
    let exp_minus_one = exp.wrapping_sub(1);
    let exp_minus_two = exp.wrapping_sub(2);

    let first_term = base * exp;
    let second_term = (first_term * base * exp_minus_one) / 2;
    let third_term = (second_term * base * exp_minus_two) / 3;

    Fraction::ONE + first_term + second_term + third_term
}

pub fn bps_to_fraction(bps: u32) -> Fraction {
    Fraction::from_num(bps) / 10_000u128
}

pub fn percent_to_fraction(pct: u8) -> Fraction {
    Fraction::from_num(pct) / 100u128
}
