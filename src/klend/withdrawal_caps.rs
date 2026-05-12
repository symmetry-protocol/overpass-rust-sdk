//! Port of <https://github.com/Kamino-Finance/klend>.

use crate::error::OverpassError;

pub const WITHDRAWAL_CAPS_LEN: usize = 32;

#[derive(Debug, Clone, Copy, Default)]
pub struct WithdrawalCaps {
    pub config_capacity: i64,
    pub current_total: i64,
    pub last_interval_start_timestamp: u64,
    pub config_interval_length_seconds: u64,
}

pub fn decode(slice: &[u8]) -> Result<WithdrawalCaps, OverpassError> {
    if slice.len() < WITHDRAWAL_CAPS_LEN {
        return Err(OverpassError::InvalidAccount("klend withdrawal_caps: length"));
    }
    let cap_err = || OverpassError::InvalidAccount("klend withdrawal_caps: field");
    Ok(WithdrawalCaps {
        config_capacity: i64::from_le_bytes(slice[0..8].try_into().map_err(|_| cap_err())?),
        current_total: i64::from_le_bytes(slice[8..16].try_into().map_err(|_| cap_err())?),
        last_interval_start_timestamp: u64::from_le_bytes(
            slice[16..24].try_into().map_err(|_| cap_err())?,
        ),
        config_interval_length_seconds: u64::from_le_bytes(
            slice[24..32].try_into().map_err(|_| cap_err())?,
        ),
    })
}

pub fn remaining_amount(caps: &WithdrawalCaps, timestamp: u64) -> u64 {
    if caps.config_interval_length_seconds == 0 {
        return u64::MAX;
    }
    if caps.config_capacity < 0 {
        return 0;
    }
    let time_spent =
        timestamp.saturating_sub(caps.last_interval_start_timestamp);
    let interval_elapsed = time_spent >= caps.config_interval_length_seconds;
    let current_total = if interval_elapsed { 0 } else { caps.current_total };
    if current_total <= caps.config_capacity {
        let remaining = i128::from(caps.config_capacity) - i128::from(current_total);
        u64::try_from(remaining).unwrap_or(0)
    } else {
        0
    }
}
