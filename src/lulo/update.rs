use jupiter_amm_interface::AccountMap;
use solana_program::pubkey::Pubkey;

use super::account_metas;
use super::state::{self, LuloState};
use crate::accounts::WrapperVault;
use crate::error::OverpassError;

pub fn accounts_to_update(_state: &LuloState, wv: &WrapperVault) -> Vec<Pubkey> {
    vec![
        wv.source_pool,
        wv.source_position_pda,
        account_metas::referrer_pool_user(),
    ]
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

    let pool_user_acct = account_map
        .get(&wv.source_position_pda)
        .ok_or(OverpassError::AccountMissing("lulo pool_user"))?;
    state.basis_q60 = state::decode_basis_q60(&pool_user_acct.data)?;
    let (charged, avg) = state::decode_charged_avg_q60(&pool_user_acct.data)?;
    state.charged_q60 = charged;
    state.avg_q60 = avg;

    let referrer = account_metas::referrer_pool_user();
    let referrer_acct = account_map
        .get(&referrer)
        .ok_or(OverpassError::AccountMissing("lulo referrer pool_user"))?;
    state.ref_bps = state::decode_ref_bps(&referrer_acct.data)?;
    Ok(())
}
