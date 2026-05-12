use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, KvaultState, GLOBAL_CONFIG_SEED, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::error::OverpassError;
use crate::klend;
use crate::accounts::WrapperVault;
use crate::common::programs::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};

use super::KVAULT_PROGRAM_ID;

pub fn accounts_to_update(state: &KvaultState, wv: &WrapperVault) -> Vec<Pubkey> {
    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &KVAULT_PROGRAM_ID);
    let mut accts = vec![wv.source_pool, kvault_global_config];
    for alloc in &state.active_allocations {
        accts.push(alloc.reserve);
    }
    if wv.protocol_data[48] != 0 {
        accts.push(wv.source_position_pda);
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

    new_state.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    *state = new_state;
    Ok(())
}
