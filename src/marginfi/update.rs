use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, MarginfiState, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::error::OverpassError;
use crate::accounts::WrapperVault;
use crate::common::programs::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};

pub fn accounts_to_update(_state: &MarginfiState, wv: &WrapperVault) -> Vec<Pubkey> {
    vec![wv.source_pool, wv.source_position_pda]
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
        bank.cached_user_asset_shares_bits =
            state::read_asset_shares_for_bank(&acct.data, &wv.source_pool)?;
    }

    bank.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    *state = bank;
    Ok(())
}
