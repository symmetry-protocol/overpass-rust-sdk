use jupiter_amm_interface::SwapParams;
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

use super::state::{remaining_accounts_per_bank, MarginfiState};
use super::MARGINFI_PROGRAM_ID;
use crate::common::bytes::read_pubkey;
use crate::error::OverpassError;
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::accounts::{global_config_pda, WrapperVault};

const PD_OFF_GROUP: usize = 0;
const PD_OFF_LIQUIDITY_VAULT: usize = 33;
const PD_OFF_LIQUIDITY_VAULT_AUTHORITY: usize = 65;

use crate::common::direction::{from_swap, Direction};

pub fn build(
    state: &MarginfiState,
    wv: &WrapperVault,
    params: &SwapParams,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let direction = from_swap(wv, params)?;

    let user = params.token_transfer_authority;
    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let marginfi_bank = wv.source_pool;
    let marginfi_account = wv.source_position_pda;
    let marginfi_group = read_pubkey(&wv.protocol_data, PD_OFF_GROUP, "marginfi.protocol_data.group")?;
    let marginfi_liquidity_vault = read_pubkey(&wv.protocol_data, PD_OFF_LIQUIDITY_VAULT, "marginfi.protocol_data.liquidity_vault")?;
    let marginfi_liquidity_vault_authority = read_pubkey(
        &wv.protocol_data,
        PD_OFF_LIQUIDITY_VAULT_AUTHORITY,
        "marginfi.protocol_data.liquidity_vault_authority",
    )?;

    let global_config = global_config_pda();
    let (wrapper_vault_pda, _) =
        Pubkey::find_program_address(&[b"vault", wrapper_mint.as_ref()], &crate::OVERPASS_PROGRAM_ID);
    let (vault_authority, _) =
        Pubkey::find_program_address(&[b"authority", wrapper_mint.as_ref()], &crate::OVERPASS_PROGRAM_ID);

    let (user_underlying_ata, user_wrapper_ata) = match direction {
        Direction::Deposit => (params.source_token_account, params.destination_token_account),
        Direction::Withdraw => (params.destination_token_account, params.source_token_account),
    };

    let underlying_token_program = if state.underlying_token_program == TOKEN_2022_PROGRAM_ID {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };
    let marginfi_token_program = underlying_token_program;
    let wrapper_token_program = SPL_TOKEN_PROGRAM_ID;

    let vault_underlying_ata =
        derive_ata(&vault_authority, &underlying_mint, &underlying_token_program);

    let mut metas = vec![
        AccountMeta::new(user, true),                                   // 0
        AccountMeta::new_readonly(global_config, false),                // 1
        AccountMeta::new(wrapper_vault_pda, false),                     // 2
        AccountMeta::new_readonly(vault_authority, false),              // 3
        AccountMeta::new(wrapper_mint, false),                          // 4
        AccountMeta::new(underlying_mint, false),                       // 5
        AccountMeta::new(user_wrapper_ata, false),                      // 6
        AccountMeta::new(user_underlying_ata, false),                   // 7
        AccountMeta::new(vault_underlying_ata, false),                  // 8
        AccountMeta::new(marginfi_bank, false),                         // 9
        AccountMeta::new_readonly(marginfi_group, false),               // 10
        AccountMeta::new(marginfi_account, false),                      // 11
        AccountMeta::new(marginfi_liquidity_vault, false),              // 12
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


