use solana_program::pubkey::Pubkey;

use crate::error::OverpassError;

pub fn read_pubkey(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<Pubkey, OverpassError> {
    let end = offset
        .checked_add(32)
        .ok_or(OverpassError::InvalidAccount(label))?;
    if data.len() < end {
        return Err(OverpassError::InvalidAccount(label));
    }
    Pubkey::try_from(&data[offset..end]).map_err(|_| OverpassError::InvalidAccount(label))
}

pub fn read_u8(data: &[u8], offset: usize, label: &'static str) -> Result<u8, OverpassError> {
    data.get(offset)
        .copied()
        .ok_or(OverpassError::InvalidAccount(label))
}

pub fn read_u16(data: &[u8], offset: usize, label: &'static str) -> Result<u16, OverpassError> {
    let end = offset
        .checked_add(2)
        .ok_or(OverpassError::InvalidAccount(label))?;
    if data.len() < end {
        return Err(OverpassError::InvalidAccount(label));
    }
    Ok(u16::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount(label))?,
    ))
}

pub fn read_u32(data: &[u8], offset: usize, label: &'static str) -> Result<u32, OverpassError> {
    let end = offset
        .checked_add(4)
        .ok_or(OverpassError::InvalidAccount(label))?;
    if data.len() < end {
        return Err(OverpassError::InvalidAccount(label));
    }
    Ok(u32::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount(label))?,
    ))
}

pub fn read_u64(data: &[u8], offset: usize, label: &'static str) -> Result<u64, OverpassError> {
    let end = offset
        .checked_add(8)
        .ok_or(OverpassError::InvalidAccount(label))?;
    if data.len() < end {
        return Err(OverpassError::InvalidAccount(label));
    }
    Ok(u64::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount(label))?,
    ))
}

pub fn read_u128(data: &[u8], offset: usize, label: &'static str) -> Result<u128, OverpassError> {
    let end = offset
        .checked_add(16)
        .ok_or(OverpassError::InvalidAccount(label))?;
    if data.len() < end {
        return Err(OverpassError::InvalidAccount(label));
    }
    Ok(u128::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount(label))?,
    ))
}
