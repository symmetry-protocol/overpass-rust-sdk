use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

use super::state::SaveState;
use super::SAVE_PROGRAM_ID;
use crate::accounts::{global_config_pda, WrapperVault};
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID,
};
use crate::error::OverpassError;

fn build_base(
    state: &SaveState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let collateral_mint = state.collateral_mint_pubkey;

    let global_config = global_config_pda();
    let (wrapper_vault_pda, _) = Pubkey::find_program_address(
        &[b"vault", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let (vault_authority, _) = Pubkey::find_program_address(
        &[b"authority", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let (save_lending_market_authority, _) =
        Pubkey::find_program_address(&[state.lending_market.as_ref()], &SAVE_PROGRAM_ID);

    let user_underlying_ata = derive_ata(user, &underlying_mint, &SPL_TOKEN_PROGRAM_ID);
    let user_wrapper_ata = derive_ata(user, &wrapper_mint, &SPL_TOKEN_PROGRAM_ID);
    let vault_underlying_ata =
        derive_ata(&vault_authority, &underlying_mint, &SPL_TOKEN_PROGRAM_ID);
    let vault_collateral_ata =
        derive_ata(&vault_authority, &collateral_mint, &SPL_TOKEN_PROGRAM_ID);

    let mut metas = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new_readonly(global_config, false),
        AccountMeta::new(wrapper_vault_pda, false),
        AccountMeta::new_readonly(vault_authority, false),
        AccountMeta::new(wrapper_mint, false),
        AccountMeta::new(underlying_mint, false),
        AccountMeta::new(collateral_mint, false),
        AccountMeta::new(user_wrapper_ata, false),
        AccountMeta::new(user_underlying_ata, false),
        AccountMeta::new(vault_underlying_ata, false),
        AccountMeta::new(vault_collateral_ata, false),
        AccountMeta::new(wv.source_pool, false),
        AccountMeta::new(state.lending_market, false),
        AccountMeta::new_readonly(save_lending_market_authority, false),
        AccountMeta::new(state.supply_vault, false),
        AccountMeta::new_readonly(state.pyth_oracle, false),
        AccountMeta::new_readonly(state.switchboard_oracle, false),
        AccountMeta::new_readonly(SAVE_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];

    if state.extra_oracle_pubkey != Pubkey::default() {
        metas.push(AccountMeta::new_readonly(state.extra_oracle_pubkey, false));
    }

    Ok(metas)
}

pub fn build_deposit(
    state: &SaveState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build_base(state, wv, user)
}

pub fn build_withdraw(
    state: &SaveState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build_base(state, wv, user)
}
