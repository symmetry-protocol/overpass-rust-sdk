use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, KlendState};
use crate::error::OverpassError;
use crate::accounts::WrapperVault;

pub fn accounts_to_update(_state: &KlendState, wv: &WrapperVault) -> Vec<Pubkey> {
    vec![wv.source_pool]
}

pub fn update(
    state: &mut KlendState,
    wv: &WrapperVault,
    account_map: &AccountMap,
) -> Result<(), OverpassError> {
    let reserve_acct = account_map
        .get(&wv.source_pool)
        .ok_or(OverpassError::AccountMissing("klend reserve"))?;
    *state = state::decode(&reserve_acct.data)?;
    Ok(())
}
