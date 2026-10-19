use solana_program::instruction::AccountMeta;
use solana_program::pubkey;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;

use super::state::KvaultState;
use super::KVAULT_PROGRAM_ID;
use crate::accounts::{global_config_pda, WrapperVault};
use crate::common::bytes::read_pubkey;
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::error::OverpassError;
use crate::klend::KLEND_PROGRAM_ID;

pub const FARMS_PROGRAM_ID: Pubkey = pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");

const PD_OFF_SHARE_MINT: usize = 0;
const PD_OFF_USE_FARMS: usize = 48;

const KVAULT_SEED_GLOBAL_CONFIG: &[u8] = b"global_config";
const KVAULT_SEED_EVENT_AUTHORITY: &[u8] = b"__event_authority";
const KLEND_SEED_LMA: &[u8] = b"lma";
const FARMS_SEED_USER: &[u8] = b"user";

#[derive(Clone, Copy)]
enum Direction {
    Deposit,
    Withdraw,
}

fn build(
    state: &KvaultState,
    wv: &WrapperVault,
    user: &Pubkey,
    direction: Direction,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let use_farms = wv.protocol_data[PD_OFF_USE_FARMS] != 0;

    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let kvault_state_pk = wv.source_pool;
    let kvault_share_mint = read_pubkey(
        &wv.protocol_data,
        PD_OFF_SHARE_MINT,
        "kvault.protocol_data.share_mint",
    )?;

    let overpass_global_config = global_config_pda();
    let (wrapper_vault_pda, _) = Pubkey::find_program_address(
        &[b"vault", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let (vault_authority, _) = Pubkey::find_program_address(
        &[b"authority", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );

    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[KVAULT_SEED_GLOBAL_CONFIG], &KVAULT_PROGRAM_ID);
    let (kvault_event_authority, _) =
        Pubkey::find_program_address(&[KVAULT_SEED_EVENT_AUTHORITY], &KVAULT_PROGRAM_ID);

    let underlying_token_program = if state.underlying_token_program == TOKEN_2022_PROGRAM_ID {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };
    let shares_token_program = SPL_TOKEN_PROGRAM_ID;
    let wrapper_token_program = SPL_TOKEN_PROGRAM_ID;

    let vault_underlying_ata = derive_ata(
        &vault_authority,
        &underlying_mint,
        &underlying_token_program,
    );
    let vault_shares_ata = derive_ata(&vault_authority, &kvault_share_mint, &shares_token_program);

    let user_underlying_ata = derive_ata(user, &underlying_mint, &underlying_token_program);
    let user_wrapper_ata = derive_ata(user, &wrapper_mint, &SPL_TOKEN_PROGRAM_ID);

    let farm_state_key = if use_farms {
        wv.source_position_pda
    } else {
        FARMS_PROGRAM_ID
    };
    let farms_user_state = if use_farms {
        Pubkey::find_program_address(
            &[
                FARMS_SEED_USER,
                farm_state_key.as_ref(),
                vault_authority.as_ref(),
            ],
            &FARMS_PROGRAM_ID,
        )
        .0
    } else {
        FARMS_PROGRAM_ID
    };
    let (farms_farm_vault, farms_scope_prices, derived_farm_vaults_authority) = if use_farms {
        let fs = state.cached_farm_state.ok_or(OverpassError::InvalidState(
            "kvault: use_farms=true but FarmState not cached (run update first)",
        ))?;
        (
            fs.farm_vault,
            fs.scope_prices.unwrap_or(FARMS_PROGRAM_ID),
            fs.farm_vaults_authority,
        )
    } else {
        (FARMS_PROGRAM_ID, FARMS_PROGRAM_ID, FARMS_PROGRAM_ID)
    };

    let mut metas = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new_readonly(overpass_global_config, false),
        AccountMeta::new(wrapper_vault_pda, false),
        AccountMeta::new(vault_authority, false),
        AccountMeta::new(wrapper_mint, false),
        AccountMeta::new(underlying_mint, false),
        AccountMeta::new(kvault_share_mint, false),
        AccountMeta::new(user_wrapper_ata, false),
        AccountMeta::new(user_underlying_ata, false),
        AccountMeta::new(vault_underlying_ata, false),
        AccountMeta::new(vault_shares_ata, false),
        AccountMeta::new(kvault_state_pk, false),
    ];

    if matches!(direction, Direction::Withdraw) {
        metas.push(AccountMeta::new_readonly(kvault_global_config, false));
    }

    metas.push(AccountMeta::new(state.token_vault, false));
    metas.push(AccountMeta::new_readonly(state.base_vault_authority, false));
    metas.push(AccountMeta::new_readonly(kvault_event_authority, false));

    if matches!(direction, Direction::Withdraw) {
        let primary = state
            .active_allocations
            .iter()
            .max_by_key(|a| a.ctoken_allocation)
            .ok_or(OverpassError::InvalidState(
                "kvault: no active allocations for withdraw",
            ))?;
        let primary_reserve = state
            .cached_reserves
            .iter()
            .find(|(k, _)| *k == primary.reserve)
            .map(|(_, r)| r)
            .ok_or(OverpassError::InvalidState(
                "kvault: primary reserve not in cached_reserves",
            ))?;
        let (primary_lending_market_authority, _) = Pubkey::find_program_address(
            &[KLEND_SEED_LMA, primary_reserve.lending_market.as_ref()],
            &KLEND_PROGRAM_ID,
        );
        metas.push(AccountMeta::new(primary.reserve, false));
        metas.push(AccountMeta::new(primary.ctoken_vault, false));
        metas.push(AccountMeta::new_readonly(
            primary_reserve.lending_market,
            false,
        ));
        metas.push(AccountMeta::new_readonly(
            primary_lending_market_authority,
            false,
        ));
        metas.push(AccountMeta::new(primary_reserve.supply_vault, false));
        metas.push(AccountMeta::new(
            primary_reserve.collateral_mint_pubkey,
            false,
        ));
        metas.push(AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false));
        metas.push(AccountMeta::new_readonly(sysvar::instructions::ID, false));
    }

    metas.push(AccountMeta::new_readonly(KLEND_PROGRAM_ID, false));
    metas.push(AccountMeta::new_readonly(KVAULT_PROGRAM_ID, false));
    metas.push(AccountMeta::new_readonly(FARMS_PROGRAM_ID, false));
    metas.push(AccountMeta::new(farms_user_state, false));
    metas.push(AccountMeta::new(farm_state_key, false));
    metas.push(AccountMeta::new(farms_farm_vault, false));

    if matches!(direction, Direction::Withdraw) {
        metas.push(AccountMeta::new_readonly(
            derived_farm_vaults_authority,
            false,
        ));
    }

    metas.push(AccountMeta::new_readonly(farms_scope_prices, false));
    metas.push(AccountMeta::new_readonly(underlying_token_program, false));
    metas.push(AccountMeta::new_readonly(shares_token_program, false));
    metas.push(AccountMeta::new_readonly(wrapper_token_program, false));
    metas.push(AccountMeta::new_readonly(ATA_PROGRAM_ID, false));
    metas.push(AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false));

    for alloc in &state.active_allocations {
        metas.push(AccountMeta::new(alloc.reserve, false));
    }
    for alloc in &state.active_allocations {
        let cached = state
            .cached_reserves
            .iter()
            .find(|(k, _)| *k == alloc.reserve)
            .map(|(_, r)| r)
            .ok_or(OverpassError::InvalidState(
                "kvault: reserve not cached for ix",
            ))?;
        metas.push(AccountMeta::new_readonly(cached.lending_market, false));
    }

    Ok(metas)
}

pub fn build_deposit(
    state: &KvaultState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build(state, wv, user, Direction::Deposit)
}

pub fn build_withdraw(
    state: &KvaultState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build(state, wv, user, Direction::Withdraw)
}

pub fn position_accounts(wv: &WrapperVault) -> Result<Vec<Pubkey>, OverpassError> {
    let (vault_authority, _) = Pubkey::find_program_address(
        &[b"authority", wv.wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let share_mint = read_pubkey(
        &wv.protocol_data,
        PD_OFF_SHARE_MINT,
        "kvault.protocol_data.share_mint",
    )?;
    let mut accts = vec![derive_ata(&vault_authority, &share_mint, &SPL_TOKEN_PROGRAM_ID)];
    if wv.protocol_data[PD_OFF_USE_FARMS] != 0 {
        let (farms_user_state, _) = Pubkey::find_program_address(
            &[FARMS_SEED_USER, wv.source_position_pda.as_ref(), vault_authority.as_ref()],
            &FARMS_PROGRAM_ID,
        );
        accts.push(farms_user_state);
    }
    Ok(accts)
}
