use solana_program::instruction::AccountMeta;
use solana_program::pubkey;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;

use super::state::KlendState;
use super::KLEND_PROGRAM_ID;
use crate::accounts::{global_config_pda, WrapperVault};
use crate::common::bytes::read_pubkey;
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::error::OverpassError;

const KLEND_NULL_PUBKEY: Pubkey = pubkey!("nu11111111111111111111111111111111111111111");

fn build_base(
    state: &KlendState,
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
    let (lending_market_authority, _) =
        Pubkey::find_program_address(&[b"lma", state.lending_market.as_ref()], &KLEND_PROGRAM_ID);

    let underlying_token_program = resolve_token_program(state.liquidity_token_program);

    let user_underlying_ata = derive_ata(user, &underlying_mint, &underlying_token_program);
    let user_wrapper_ata = derive_ata(user, &wrapper_mint, &SPL_TOKEN_PROGRAM_ID);
    let vault_underlying_ata = derive_ata(
        &vault_authority,
        &underlying_mint,
        &underlying_token_program,
    );
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
        AccountMeta::new_readonly(state.lending_market, false),
        AccountMeta::new_readonly(lending_market_authority, false),
        AccountMeta::new(state.supply_vault, false),
        AccountMeta::new_readonly(sysvar::instructions::ID, false),
        AccountMeta::new_readonly(KLEND_PROGRAM_ID, false),
        AccountMeta::new_readonly(underlying_token_program, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];

    metas.push(oracle_or_placeholder(state.pyth_price));
    metas.push(oracle_or_placeholder(state.switchboard_price_aggregator));
    metas.push(oracle_or_placeholder(state.switchboard_twap_aggregator));
    metas.push(oracle_or_placeholder(state.scope_price_feed));

    Ok(metas)
}

fn oracle_or_placeholder(key: Pubkey) -> AccountMeta {
    if key == Pubkey::default() || key == KLEND_NULL_PUBKEY {
        AccountMeta::new_readonly(KLEND_PROGRAM_ID, false)
    } else {
        AccountMeta::new_readonly(key, false)
    }
}

fn resolve_token_program(stored: Pubkey) -> Pubkey {
    if stored == TOKEN_2022_PROGRAM_ID {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    }
}

pub fn build_deposit(
    state: &KlendState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build_base(state, wv, user)
}

pub fn build_withdraw(
    state: &KlendState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build_base(state, wv, user)
}

const PD_OFF_COLLATERAL_MINT: usize = 32;

pub fn position_account(wv: &WrapperVault) -> Result<Pubkey, OverpassError> {
    let (vault_authority, _) = Pubkey::find_program_address(
        &[b"authority", wv.wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let collateral_mint = read_pubkey(
        &wv.protocol_data,
        PD_OFF_COLLATERAL_MINT,
        "klend.protocol_data.collateral_mint",
    )?;
    Ok(derive_ata(&vault_authority, &collateral_mint, &SPL_TOKEN_PROGRAM_ID))
}
