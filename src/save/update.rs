use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, SaveState};
use crate::accounts::WrapperVault;
use crate::error::OverpassError;

pub fn accounts_to_update(_state: &SaveState, wv: &WrapperVault) -> Vec<Pubkey> {
    vec![wv.source_pool]
}

pub fn update(
    state: &mut SaveState,
    wv: &WrapperVault,
    account_map: &AccountMap,
) -> Result<(), OverpassError> {
    let reserve_acct = account_map
        .get(&wv.source_pool)
        .ok_or(OverpassError::AccountMissing("save reserve"))?;
    *state = state::decode(&reserve_acct.data)?;
    Ok(())
}
