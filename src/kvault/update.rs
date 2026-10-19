use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, KvaultState, GLOBAL_CONFIG_SEED, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::accounts::WrapperVault;
use crate::common::bytes::{read_u128, read_u64};
use crate::common::programs::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::error::OverpassError;
use crate::klend;

use super::account_metas::{position_accounts, FARMS_PROGRAM_ID};
use super::KVAULT_PROGRAM_ID;

const FARMS_USER_ACTIVE_STAKE_SCALED_OFFSET: usize = 408;
const FARMS_USER_STATE_LEN: usize = 920;
const SPL_TOKEN_AMOUNT_OFFSET: usize = 64;

pub fn accounts_to_update(state: &KvaultState, wv: &WrapperVault) -> Vec<Pubkey> {
    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &KVAULT_PROGRAM_ID);
    let mut accts = vec![wv.source_pool, kvault_global_config];
    for alloc in &state.active_allocations {
        accts.push(alloc.reserve);
    }
    if wv.protocol_data[48] != 0 {
        accts.push(wv.source_position_pda);
        if let Ok(position) = position_accounts(wv) {
            accts.extend(position);
        }
    }
    accts
}

pub fn update(
    state: &mut KvaultState,
    wv: &WrapperVault,
    account_map: &AccountMap,
) -> Result<(), OverpassError> {
    let vault_acct = account_map
        .get(&wv.source_pool)
        .ok_or(OverpassError::AccountMissing("kvault state"))?;
    let mut new_state = state::decode(&vault_acct.data)?;

    new_state.global_withdrawal_penalty_lamports = state.global_withdrawal_penalty_lamports;
    new_state.global_withdrawal_penalty_bps = state.global_withdrawal_penalty_bps;

    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &KVAULT_PROGRAM_ID);
    if let Some(gc_acct) = account_map.get(&kvault_global_config) {
        let (lamports, bps) = state::decode_global_config_penalty(&gc_acct.data)?;
        new_state.global_withdrawal_penalty_lamports = lamports;
        new_state.global_withdrawal_penalty_bps = bps;
    }

    let mut cached_reserves = Vec::with_capacity(new_state.active_allocations.len());
    for alloc in &new_state.active_allocations {
        if let Some(reserve_acct) = account_map.get(&alloc.reserve) {
            let decoded = klend::state::decode(&reserve_acct.data)?;
            cached_reserves.push((alloc.reserve, decoded));
        } else if let Some((_, prev)) = state
            .cached_reserves
            .iter()
            .find(|(k, _)| *k == alloc.reserve)
        {
            cached_reserves.push((alloc.reserve, prev.clone()));
        }
    }
    new_state.cached_reserves = cached_reserves;

    if wv.protocol_data[48] != 0 {
        if let Some(fs_acct) = account_map.get(&wv.source_position_pda) {
            new_state.cached_farm_state = Some(state::decode_farm_state_minimal(&fs_acct.data)?);
        } else {
            new_state.cached_farm_state = state.cached_farm_state;
        }
    }

    let position_keys = position_accounts(wv)?;
    let idle = match position_keys.first().and_then(|k| account_map.get(k)) {
        Some(a) if !a.data.is_empty() => {
            read_u64(&a.data, SPL_TOKEN_AMOUNT_OFFSET, "shares_ata.amount")?
        }
        _ => 0,
    };
    new_state.position_kvault_shares = if wv.protocol_data[48] != 0 {
        let user_stake_scaled = match position_keys.get(1).and_then(|k| account_map.get(k)) {
            Some(a) if a.owner == FARMS_PROGRAM_ID && a.data.len() >= FARMS_USER_STATE_LEN => {
                read_u128(
                    &a.data,
                    FARMS_USER_ACTIVE_STAKE_SCALED_OFFSET,
                    "farms_user.active_stake_scaled",
                )?
            }
            _ => 0,
        };
        let fm = new_state
            .cached_farm_state
            .ok_or(OverpassError::InvalidState(
                "kvault: use_farms but FarmState not cached",
            ))?;
        let farm_kt = state::convert_stake_to_amount(
            user_stake_scaled,
            fm.total_active_stake_scaled,
            fm.total_staked_amount,
        )?;
        (idle as u128)
            .checked_add(farm_kt as u128)
            .ok_or(OverpassError::MathOverflow)?
    } else {
        idle as u128
    };

    new_state.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    *state = new_state;
    Ok(())
}
