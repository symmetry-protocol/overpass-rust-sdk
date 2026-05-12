//! Port of <https://github.com/Kamino-Finance/klend>.

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
    pub fn from_num(num: u128) -> Self {
        let value = U256::from(num);
        Self(panicking_shl(value, FRACTION_BITS_SHIFT))
    }

    pub fn try_into_fraction(self) -> Option<Fraction> {
        let bits: u128 = self.0.try_into().ok()?;
        Some(Fraction::from_bits(bits))
    }
}

impl core::ops::Mul for BigFraction {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        let extra_scaled = self.0 * rhs.0;
        Self(extra_scaled >> FRACTION_BITS_SHIFT)
    }
}

impl core::ops::Div for BigFraction {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        let extra_scaled = panicking_shl(self.0, FRACTION_BITS_SHIFT);
        Self(extra_scaled / rhs.0)
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

fn panicking_shl(number: U256, bit_count: u32) -> U256 {
    let result = number << bit_count;
    let recovered = result >> bit_count;
    assert!(recovered == number, "shl overflow");
    result
}

pub fn approximate_compounded_interest(rate: Fraction, elapsed_slots: u64) -> Fraction {
    let slots_per_year: u128 = crate::klend::math::SLOTS_PER_YEAR.into();
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

pub fn floor_sf_to_u64(sf: u128) -> u64 {
    (sf >> FRACTION_BITS_SHIFT) as u64
}

pub fn u64_to_sf(amount: u64) -> u128 {
    (amount as u128) << FRACTION_BITS_SHIFT
}

pub fn bps_to_fraction(bps: u32) -> Fraction {
    Fraction::from_num(bps) / 10_000u128
}

pub fn percent_to_fraction(pct: u8) -> Fraction {
    Fraction::from_num(pct) / 100u128
}
