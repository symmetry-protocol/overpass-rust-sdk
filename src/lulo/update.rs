use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::state::{self, LuloState};
use crate::error::OverpassError;
use crate::accounts::WrapperVault;

pub fn accounts_to_update(_state: &LuloState, wv: &WrapperVault) -> Vec<Pubkey> {
    vec![wv.source_pool]
}

pub fn update(
    state: &mut LuloState,
    wv: &WrapperVault,
    account_map: &AccountMap,
) -> Result<(), OverpassError> {
    let pool_acct = account_map
        .get(&wv.source_pool)
        .ok_or(OverpassError::AccountMissing("lulo pool"))?;
    *state = state::decode(&pool_acct.data)?;
    Ok(())
}
