use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, MarginfiState, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::accounts::WrapperVault;
use crate::common::bytes::read_pubkey;
use crate::common::programs::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::error::OverpassError;

const PD_OFF_GROUP: usize = 0;

pub fn accounts_to_update(_state: &MarginfiState, wv: &WrapperVault) -> Vec<Pubkey> {
    let group = read_pubkey(
        &wv.protocol_data,
        PD_OFF_GROUP,
        "marginfi.protocol_data.group",
    )
    .unwrap_or_default();
    vec![wv.source_pool, wv.source_position_pda, group]
}

pub fn update(
    state: &mut MarginfiState,
    wv: &WrapperVault,
    account_map: &AccountMap,
) -> Result<(), OverpassError> {
    let bank_acct = account_map
        .get(&wv.source_pool)
        .ok_or(OverpassError::AccountMissing("marginfi bank"))?;
    let mut bank = state::decode(&bank_acct.data)?;

    if let Some(acct) = account_map.get(&wv.source_position_pda) {
        bank.cached_user_asset_shares =
            state::read_asset_shares_for_bank(&acct.data, &wv.source_pool)?;
    }

    let group_pk = read_pubkey(
        &wv.protocol_data,
        PD_OFF_GROUP,
        "marginfi.protocol_data.group",
    )?;
    let group_acct = account_map
        .get(&group_pk)
        .ok_or(OverpassError::AccountMissing("marginfi group"))?;
    let group = state::decode_group(&group_acct.data)?;
    bank.group_program_fees_enabled = group.program_fees_enabled;
    bank.group_program_fee_fixed = group.program_fee_fixed;
    bank.group_program_fee_rate = group.program_fee_rate;

    bank.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    *state = bank;
    Ok(())
}
