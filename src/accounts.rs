use solana_program::pubkey::Pubkey;

use crate::common::bytes::{read_pubkey, read_u128, read_u16, read_u64};
use crate::error::OverpassError;
use crate::klend::state::KlendState;
use crate::kvault::state::KvaultState;
use crate::lulo::state::LuloState;
use crate::marginfi::state::MarginfiState;
use crate::save::state::SaveState;

pub const WRAPPER_VAULT_LEN: usize = 558;
pub const WRAPPER_VAULT_VERSION: u8 = 1;
pub const PROTOCOL_DATA_LEN: usize = 200;
pub const WRAPPER_VAULT_DISCRIMINATOR: [u8; 8] = [103, 101, 152, 26, 112, 217, 184, 219];

#[derive(Debug, Clone)]
pub struct WrapperVault {
    pub version: u8,
    pub bump: u8,
    pub vault_authority_bump: u8,
    pub protocol: u8,
    pub creation_timestamp: u64,
    pub creator: Pubkey,
    pub wrapper_mint: Pubkey,
    pub underlying_mint: Pubkey,
    pub source_pool: Pubkey,
    pub source_position_pda: Pubkey,
    pub creator_deposit_fee_bps: u16,
    pub intermediate_held: u128,
    pub free_underlying_held: u64,
    pub accumulated_creator_fees: u64,
    pub accumulated_protocol_fees: u64,
    pub wrapper_supply: u64,
    pub protocol_data: [u8; PROTOCOL_DATA_LEN],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolId {
    Kvault = 0,
    Klend = 1,
    Save = 2,
    Lulo = 3,
    Marginfi = 4,
}

impl ProtocolId {
    pub fn from_byte(b: u8) -> Result<Self, OverpassError> {
        match b {
            0 => Ok(Self::Kvault),
            1 => Ok(Self::Klend),
            2 => Ok(Self::Save),
            3 => Ok(Self::Lulo),
            4 => Ok(Self::Marginfi),
            _ => Err(OverpassError::ProtocolMismatch),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ProtocolState {
    Kvault(KvaultState),
    Klend(KlendState),
    Save(SaveState),
    Lulo(LuloState),
    Marginfi(MarginfiState),
}

pub fn decode_wrapper_vault(data: &[u8]) -> Result<WrapperVault, OverpassError> {
    if data.len() < WRAPPER_VAULT_LEN {
        return Err(OverpassError::InvalidAccount("wrapper_vault: length"));
    }
    if data[..8] != WRAPPER_VAULT_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "wrapper_vault: discriminator",
        ));
    }
    let version = data[8];
    if version != WRAPPER_VAULT_VERSION {
        return Err(OverpassError::InvalidState(
            "unsupported wrapper_vault version",
        ));
    }

    Ok(WrapperVault {
        version,
        bump: data[9],
        vault_authority_bump: data[10],
        protocol: data[11],
        creation_timestamp: read_u64(data, 12, "wrapper_vault.creation_timestamp")?,
        creator: read_pubkey(data, 20, "wrapper_vault.creator")?,
        wrapper_mint: read_pubkey(data, 52, "wrapper_vault.wrapper_mint")?,
        underlying_mint: read_pubkey(data, 84, "wrapper_vault.underlying_mint")?,
        source_pool: read_pubkey(data, 116, "wrapper_vault.source_pool")?,
        source_position_pda: read_pubkey(data, 148, "wrapper_vault.source_position_pda")?,
        creator_deposit_fee_bps: read_u16(data, 180, "wrapper_vault.creator_deposit_fee_bps")?,
        intermediate_held: read_u128(data, 182, "wrapper_vault.intermediate_held")?,
        free_underlying_held: read_u64(data, 198, "wrapper_vault.free_underlying_held")?,
        accumulated_creator_fees: read_u64(data, 206, "wrapper_vault.accumulated_creator_fees")?,
        accumulated_protocol_fees: read_u64(data, 214, "wrapper_vault.accumulated_protocol_fees")?,
        wrapper_supply: read_u64(data, 222, "wrapper_vault.wrapper_supply")?,
        protocol_data: data[230..430]
            .try_into()
            .map_err(|_| OverpassError::InvalidAccount("wrapper_vault.protocol_data"))?,
    })
}

pub const GLOBAL_CONFIG_LEN: usize = 150;
pub const GLOBAL_CONFIG_DISCRIMINATOR: [u8; 8] = [149, 8, 156, 202, 160, 252, 176, 217];

pub const SEED_GLOBAL_CONFIG: &[u8] = b"config";

pub fn global_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[SEED_GLOBAL_CONFIG], &crate::OVERPASS_PROGRAM_ID).0
}

#[derive(Debug, Clone, Default)]
pub struct GlobalConfig {
    pub paused: bool,
    pub protocol_deposit_fee_bps: u16,
}

pub fn decode_global_config(data: &[u8]) -> Result<GlobalConfig, OverpassError> {
    if data.len() < GLOBAL_CONFIG_LEN {
        return Err(OverpassError::InvalidAccount("global_config: length"));
    }
    if data[..8] != GLOBAL_CONFIG_DISCRIMINATOR {
        return Err(OverpassError::InvalidAccount(
            "global_config: discriminator",
        ));
    }
    let paused = data[9] != 0;
    let protocol_deposit_fee_bps = read_u16(data, 82, "global_config.protocol_deposit_fee_bps")?;
    Ok(GlobalConfig {
        paused,
        protocol_deposit_fee_bps,
    })
}
