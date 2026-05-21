use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

use super::state::{remaining_accounts_per_bank, MarginfiState};
use super::MARGINFI_PROGRAM_ID;
use crate::accounts::{global_config_pda, WrapperVault};
use crate::common::bytes::read_pubkey;
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::error::OverpassError;

const PD_OFF_GROUP: usize = 0;
const PD_OFF_LIQUIDITY_VAULT: usize = 33;
const PD_OFF_LIQUIDITY_VAULT_AUTHORITY: usize = 65;

#[derive(Clone, Copy)]
enum Direction {
    Deposit,
    Withdraw,
}

fn build(
    state: &MarginfiState,
    wv: &WrapperVault,
    user: &Pubkey,
    direction: Direction,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let marginfi_bank = wv.source_pool;
    let marginfi_account = wv.source_position_pda;
    let marginfi_group = read_pubkey(
        &wv.protocol_data,
        PD_OFF_GROUP,
        "marginfi.protocol_data.group",
    )?;
    let marginfi_liquidity_vault = read_pubkey(
        &wv.protocol_data,
        PD_OFF_LIQUIDITY_VAULT,
        "marginfi.protocol_data.liquidity_vault",
    )?;
    let marginfi_liquidity_vault_authority = read_pubkey(
        &wv.protocol_data,
        PD_OFF_LIQUIDITY_VAULT_AUTHORITY,
        "marginfi.protocol_data.liquidity_vault_authority",
    )?;

    let global_config = global_config_pda();
    let (wrapper_vault_pda, _) = Pubkey::find_program_address(
        &[b"vault", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );
    let (vault_authority, _) = Pubkey::find_program_address(
        &[b"authority", wrapper_mint.as_ref()],
        &crate::OVERPASS_PROGRAM_ID,
    );

    let underlying_token_program = if state.underlying_token_program == TOKEN_2022_PROGRAM_ID {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };
    let marginfi_token_program = underlying_token_program;
    let wrapper_token_program = SPL_TOKEN_PROGRAM_ID;

    let user_underlying_ata = derive_ata(user, &underlying_mint, &underlying_token_program);
    let user_wrapper_ata = derive_ata(user, &wrapper_mint, &SPL_TOKEN_PROGRAM_ID);
    let vault_underlying_ata = derive_ata(
        &vault_authority,
        &underlying_mint,
        &underlying_token_program,
    );

    let mut metas = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new_readonly(global_config, false),
        AccountMeta::new(wrapper_vault_pda, false),
        AccountMeta::new_readonly(vault_authority, false),
        AccountMeta::new(wrapper_mint, false),
        AccountMeta::new(underlying_mint, false),
        AccountMeta::new(user_wrapper_ata, false),
        AccountMeta::new(user_underlying_ata, false),
        AccountMeta::new(vault_underlying_ata, false),
        AccountMeta::new(marginfi_bank, false),
        AccountMeta::new_readonly(marginfi_group, false),
        AccountMeta::new(marginfi_account, false),
        AccountMeta::new(marginfi_liquidity_vault, false),
    ];

    if matches!(direction, Direction::Withdraw) {
        metas.push(AccountMeta::new_readonly(
            marginfi_liquidity_vault_authority,
            false,
        ));
    }

    metas.extend_from_slice(&[
        AccountMeta::new_readonly(MARGINFI_PROGRAM_ID, false),
        AccountMeta::new_readonly(marginfi_token_program, false),
        AccountMeta::new_readonly(underlying_token_program, false),
        AccountMeta::new_readonly(wrapper_token_program, false),
        AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ]);

    if underlying_token_program == TOKEN_2022_PROGRAM_ID {
        metas.push(AccountMeta::new_readonly(underlying_mint, false));
    }

    if matches!(direction, Direction::Withdraw) {
        let total_slots = remaining_accounts_per_bank(state.oracle_setup);
        let oracle_count = total_slots.saturating_sub(1);
        metas.push(AccountMeta::new_readonly(marginfi_bank, false));
        for i in 0..oracle_count.min(5) {
            let key = state.oracle_keys[i];
            if key != Pubkey::default() {
                metas.push(AccountMeta::new_readonly(key, false));
            }
        }
    }

    Ok(metas)
}

pub fn build_deposit(
    state: &MarginfiState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build(state, wv, user, Direction::Deposit)
}

pub fn build_withdraw(
    state: &MarginfiState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    build(state, wv, user, Direction::Withdraw)
}
