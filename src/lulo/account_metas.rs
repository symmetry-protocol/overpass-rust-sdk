use jupiter_amm_interface::SwapParams;
use solana_program::instruction::AccountMeta;
use solana_program::pubkey;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;

use super::state::LuloState;
use crate::common::bytes::read_pubkey;
use crate::error::OverpassError;
use crate::accounts::{global_config_pda, WrapperVault};

use super::LULO_PROGRAM_ID;
const LULO_DEFAULT_REFERRER: Pubkey = pubkey!("UserevMsvU5K9u6iW7DT9XJVyVLpmfDCEAfXixBbE7R");
const LULO_SEED_POOL_USER: &[u8] = b"pool_user";

const LULO_POOL_ACCOUNT: Pubkey = pubkey!("A2qdjcuacp6gMVi8TxmEb2vsMZsHqCjM9CGjFeJ2bb2z");
const LULO_POOL_INPUT_TOKEN_ACCOUNT: Pubkey =
    pubkey!("5cLLhzBXWm9pmUqvAnY1GcLEcVRAWc6FHTsjLV5zP69A");
const LULO_POOL_ID: u8 = 0;

const JUP_LENDING_ADMIN: Pubkey = pubkey!("5nmGjA4s7ATzpBQXC5RNceRpaJ7pYw2wKsNBWyuSAZV6");
const JUP_LENDING_USDC: Pubkey = pubkey!("2vVYHYM8VYnvZqQWpTJSj8o8DBf1wM8pVs3bsTgYZiqJ");
const JUP_F_TOKEN_USDC: Pubkey = pubkey!("9BEcn9aPEmhSPbPQeFGjidRiEKki46fVQDyPpSQXPA2D");
const JUP_SUPPLY_LIQUIDITY: Pubkey = pubkey!("94vK29npVbyRHXH63rRcTiSr26SFhrQTzbpNJuhQEDu");
const JUP_LENDING_LIQUIDITY: Pubkey = pubkey!("Hf9gtkM4dpVBahVSzEXSVCAPpKzBsBcns3s8As3z77oF");
const JUP_RATE_MODEL: Pubkey = pubkey!("5pjzT5dFTsXcwixoab1QDLvZQvpYJxJeBphkyfHGn688");
const JUP_VAULT: Pubkey = pubkey!("BmkUoKMFYBxNSzWXyUjyMJjMAaVz4d8ZnxwwmhDCUXFB");
const JUP_CLAIM_ACCOUNT: Pubkey = pubkey!("HN1r4VfkDn53xQQfeGDYrNuDKFdemAhZsHYRwBrFhsW");
const JUP_REWARDS_RATE_MODEL: Pubkey = pubkey!("5xSPBiD3TibamAnwHDhZABdB4z4F9dcj5PnbteroBTTd");
const JUP_LIQUIDITY: Pubkey = pubkey!("7s1da8DduuBFqGra5bJBjpnvL5E9mGzCuMk1Qkh4or2Z");
pub(super) const JUP_LIQUIDITY_PROGRAM: Pubkey =
    pubkey!("jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC");
pub(super) const JUP_PROGRAM: Pubkey = pubkey!("jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9");
const JUP_PROTOCOL_AUTHORITY: Pubkey = pubkey!("DdRpVzxr7F2jzmQKffR25ccNZNpMuHTwVPVPvvnS1g3t");
const JUP_PROTOCOL_AUTHORITY_USDC_ATA: Pubkey =
    pubkey!("CYu5w2BskA8VU4vCUsvztNj9FAaBnM4sC3SMun619rFb");
const JUP_AUTHORITY_F_TOKEN_USDC_ATA: Pubkey =
    pubkey!("Cd3JM8WViAqi4vU2cjxHyPA6AktEWsAZEx9aM1i5ZP87");

use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};

const PD_OFF_PROTECTED_MINT: usize = 0;
const PD_OFF_POOL_ID: usize = 32;

use crate::common::direction::{from_swap, Direction};

pub fn build(
    _state: &LuloState,
    wv: &WrapperVault,
    params: &SwapParams,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let direction = from_swap(wv, params)?;

    if wv.source_pool != LULO_POOL_ACCOUNT {
        return Err(OverpassError::InvalidState(
            "lulo: only pool_id=0 (USDC protected) supported",
        ));
    }
    if wv.protocol_data[PD_OFF_POOL_ID] != LULO_POOL_ID {
        return Err(OverpassError::InvalidState(
            "lulo: protocol_data pool_id != 0",
        ));
    }

    let user = params.token_transfer_authority;
    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let protected_mint = read_pubkey(&wv.protocol_data, PD_OFF_PROTECTED_MINT, "lulo.protocol_data.protected_mint")?;
    let lulo_pool = wv.source_pool;
    let lulo_pool_user = wv.source_position_pda;

    let global_config = global_config_pda();
    let (wrapper_vault_pda, _) =
        Pubkey::find_program_address(&[b"vault", wrapper_mint.as_ref()], &crate::OVERPASS_PROGRAM_ID);
    let (vault_authority, _) =
        Pubkey::find_program_address(&[b"authority", wrapper_mint.as_ref()], &crate::OVERPASS_PROGRAM_ID);

    let (referrer_pool_user, _) = Pubkey::find_program_address(
        &[LULO_SEED_POOL_USER, &[LULO_POOL_ID], LULO_DEFAULT_REFERRER.as_ref()],
        &LULO_PROGRAM_ID,
    );

    let underlying_token_program = SPL_TOKEN_PROGRAM_ID;
    let lp_token_program = TOKEN_2022_PROGRAM_ID;
    let wrapper_token_program = SPL_TOKEN_PROGRAM_ID;

    let vault_underlying_ata =
        derive_ata(&vault_authority, &underlying_mint, &underlying_token_program);
    let lulo_pool_user_lp_token_account =
        derive_ata(&lulo_pool_user, &protected_mint, &lp_token_program);

    let (user_underlying_ata, user_wrapper_ata) = match direction {
        Direction::Deposit => (params.source_token_account, params.destination_token_account),
        Direction::Withdraw => (params.destination_token_account, params.source_token_account),
    };

    let mut metas = vec![
        AccountMeta::new(user, true),
        AccountMeta::new_readonly(global_config, false),
        AccountMeta::new(wrapper_vault_pda, false),
        AccountMeta::new(vault_authority, false),
        AccountMeta::new(wrapper_mint, false),
        AccountMeta::new(underlying_mint, false),
        AccountMeta::new(protected_mint, false),
        AccountMeta::new(user_wrapper_ata, false),
        AccountMeta::new(user_underlying_ata, false),
        AccountMeta::new(vault_underlying_ata, false),
        AccountMeta::new(lulo_pool, false),
        AccountMeta::new(LULO_POOL_INPUT_TOKEN_ACCOUNT, false),
        AccountMeta::new(lulo_pool_user, false),
        AccountMeta::new(lulo_pool_user_lp_token_account, false),
        AccountMeta::new(referrer_pool_user, false),
    ];

    match direction {
        Direction::Deposit => {
            metas.extend_from_slice(&[
                AccountMeta::new_readonly(LULO_PROGRAM_ID, false),
                AccountMeta::new_readonly(underlying_token_program, false),
                AccountMeta::new_readonly(lp_token_program, false),
                AccountMeta::new_readonly(wrapper_token_program, false),
                AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(sysvar::rent::ID, false),
            ]);
        }
        Direction::Withdraw => {
            metas.extend_from_slice(&[
                AccountMeta::new(JUP_PROTOCOL_AUTHORITY, false),
                AccountMeta::new(JUP_PROTOCOL_AUTHORITY_USDC_ATA, false),
                AccountMeta::new(JUP_AUTHORITY_F_TOKEN_USDC_ATA, false),
                AccountMeta::new_readonly(JUP_LENDING_ADMIN, false),
                AccountMeta::new(JUP_LENDING_USDC, false),
                AccountMeta::new(JUP_F_TOKEN_USDC, false),
                AccountMeta::new(JUP_SUPPLY_LIQUIDITY, false),
                AccountMeta::new(JUP_LENDING_LIQUIDITY, false),
                AccountMeta::new_readonly(JUP_RATE_MODEL, false),
                AccountMeta::new(JUP_VAULT, false),
                AccountMeta::new(JUP_CLAIM_ACCOUNT, false),
                AccountMeta::new(JUP_LIQUIDITY, false),
                AccountMeta::new_readonly(JUP_REWARDS_RATE_MODEL, false),
                AccountMeta::new(JUP_LIQUIDITY_PROGRAM, false),
                AccountMeta::new_readonly(JUP_PROGRAM, false),
                AccountMeta::new_readonly(LULO_PROGRAM_ID, false),
                AccountMeta::new_readonly(underlying_token_program, false),
                AccountMeta::new_readonly(lp_token_program, false),
                AccountMeta::new_readonly(wrapper_token_program, false),
                AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ]);
        }
    }

    Ok(metas)
}
