use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

use super::state::LuloState;
use super::LULO_PROGRAM_ID;
use crate::accounts::{global_config_pda, WrapperVault};
use crate::common::bytes::read_pubkey;
use crate::common::programs::{
    derive_ata, ATA_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::error::OverpassError;
use crate::OVERPASS_PROGRAM_ID;

const LULO_DEFAULT_REFERRER: Pubkey =
    Pubkey::from_str_const("UserevMsvU5K9u6iW7DT9XJVyVLpmfDCEAfXixBbE7R");
const LULO_SEED_POOL_USER: &[u8] = b"pool_user";

const LULO_POOL_ACCOUNT: Pubkey =
    Pubkey::from_str_const("A2qdjcuacp6gMVi8TxmEb2vsMZsHqCjM9CGjFeJ2bb2z");
const LULO_POOL_INPUT_TOKEN_ACCOUNT: Pubkey =
    Pubkey::from_str_const("5cLLhzBXWm9pmUqvAnY1GcLEcVRAWc6FHTsjLV5zP69A");
const LULO_POOL_ID: u8 = 0;

const JUP_LENDING_ADMIN: Pubkey =
    Pubkey::from_str_const("5nmGjA4s7ATzpBQXC5RNceRpaJ7pYw2wKsNBWyuSAZV6");
const JUP_LENDING_USDC: Pubkey =
    Pubkey::from_str_const("2vVYHYM8VYnvZqQWpTJSj8o8DBf1wM8pVs3bsTgYZiqJ");
const JUP_F_TOKEN_USDC: Pubkey =
    Pubkey::from_str_const("9BEcn9aPEmhSPbPQeFGjidRiEKki46fVQDyPpSQXPA2D");
const JUP_SUPPLY_LIQUIDITY: Pubkey =
    Pubkey::from_str_const("94vK29npVbyRHXH63rRcTiSr26SFhrQTzbpNJuhQEDu");
const JUP_LENDING_LIQUIDITY: Pubkey =
    Pubkey::from_str_const("Hf9gtkM4dpVBahVSzEXSVCAPpKzBsBcns3s8As3z77oF");
const JUP_RATE_MODEL: Pubkey =
    Pubkey::from_str_const("5pjzT5dFTsXcwixoab1QDLvZQvpYJxJeBphkyfHGn688");
const JUP_VAULT: Pubkey = Pubkey::from_str_const("BmkUoKMFYBxNSzWXyUjyMJjMAaVz4d8ZnxwwmhDCUXFB");
const JUP_CLAIM_ACCOUNT: Pubkey =
    Pubkey::from_str_const("HN1r4VfkDn53xQQfeGDYrNuDKFdemAhZsHYRwBrFhsW");
const JUP_REWARDS_RATE_MODEL: Pubkey =
    Pubkey::from_str_const("5xSPBiD3TibamAnwHDhZABdB4z4F9dcj5PnbteroBTTd");
const JUP_LIQUIDITY: Pubkey =
    Pubkey::from_str_const("7s1da8DduuBFqGra5bJBjpnvL5E9mGzCuMk1Qkh4or2Z");
pub(super) const JUP_LIQUIDITY_PROGRAM: Pubkey =
    Pubkey::from_str_const("jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC");
pub(super) const JUP_PROGRAM: Pubkey =
    Pubkey::from_str_const("jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9");
const JUP_PROTOCOL_AUTHORITY: Pubkey =
    Pubkey::from_str_const("DdRpVzxr7F2jzmQKffR25ccNZNpMuHTwVPVPvvnS1g3t");
const JUP_PROTOCOL_AUTHORITY_USDC_ATA: Pubkey =
    Pubkey::from_str_const("CYu5w2BskA8VU4vCUsvztNj9FAaBnM4sC3SMun619rFb");
const JUP_AUTHORITY_F_TOKEN_USDC_ATA: Pubkey =
    Pubkey::from_str_const("Cd3JM8WViAqi4vU2cjxHyPA6AktEWsAZEx9aM1i5ZP87");

const SYSVAR_RENT_ID: Pubkey =
    Pubkey::from_str_const("SysvarRent111111111111111111111111111111111");
const SYSVAR_INSTRUCTIONS_ID: Pubkey =
    Pubkey::from_str_const("Sysvar1nstructions1111111111111111111111111");

const LULO_POOL_JLUSDC_ATA: Pubkey =
    Pubkey::from_str_const("GhmCHQhFhFX9vbxAskH7c1ymNzxyN5kaza4Zqqms6zWL");
const LULO_TRAILING_ACCOUNT: Pubkey =
    Pubkey::from_str_const("43kuLXFrEEExVP6n3fDprjbKvzMRqhLdLk24f4TQaiGv");

const KAMINO_PROTOCOL_AUTHORITY: Pubkey =
    Pubkey::from_str_const("4dg3naKuGezNCzNY2qrTFCzsNZG8hcdkzzNT5PFLZFLR");
const KAMINO_PROTOCOL_TOKEN_ACCOUNT: Pubkey =
    Pubkey::from_str_const("mZovrHTXASkL3jUE5Usqtgtau7iKF5dXStAwbAu1wyx");
const KAMINO_MARKET: Pubkey =
    Pubkey::from_str_const("7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF");
const KAMINO_OBLIGATION: Pubkey =
    Pubkey::from_str_const("H3LwbNZyccycRro2V1xjS4ASfKADa6KKuE8AwUomSZeU");
const KAMINO_RESERVE: Pubkey =
    Pubkey::from_str_const("D6q6wuQSrifJKZYpR1M8R4YawnLDtDsMmWM1NbBmgJ59");
const KAMINO_RESERVE_LIQUIDITY_SUPPLY: Pubkey =
    Pubkey::from_str_const("Bgq7trRgVMeq33yt235zM2onQ4bRDBsY5EWiTetF4qw6");
const KAMINO_LENDING_MARKET_AUTHORITY: Pubkey =
    Pubkey::from_str_const("9DrvZvyWh1HuAoZxvYWMvkf2XCzryCpGgHqrMjyDWpmo");
const KAMINO_COLLATERAL_MINT: Pubkey =
    Pubkey::from_str_const("B8V6WVjPxW1UGwVDfxH2d2r8SyT4cqn7dQRK6XneVa7D");
const KAMINO_COLLATERAL_TOKEN_ACCOUNT: Pubkey =
    Pubkey::from_str_const("BBaPKj94sodQurHUYRyBb19xxhXXJt1hbjimactKRR91");
const KAMINO_RESERVE_COLLATERAL_SUPPLY: Pubkey =
    Pubkey::from_str_const("3DzjXRfxRm6iejfyyMynR4tScddaanrePJ1NJU2XnPPL");
const KAMINO_PROGRAM: Pubkey =
    Pubkey::from_str_const("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const KAMINO_FARMS_PROGRAM: Pubkey =
    Pubkey::from_str_const("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
const KAMINO_RESERVE_FARM_STATE: Pubkey =
    Pubkey::from_str_const("JAvnB9AKtgPsTEoKmn24Bq64UMoYcrtWtq42HHBdsPkh");
const KAMINO_OBLIGATION_FARM_USER_STATE: Pubkey =
    Pubkey::from_str_const("FtKHAgqLSeXv3nngZR4PuXQct4yPf5KXCkHJ8wbzR7SR");
const KAMINO_SCOPE_PRICES: Pubkey =
    Pubkey::from_str_const("3t4JZcueEzTbVP6kLxXrL3VpWx45jDer4eqysweBchNH");

const PD_OFF_PROTECTED_MINT: usize = 0;
const PD_OFF_POOL_ID: usize = 32;

pub(super) fn referrer_pool_user() -> Pubkey {
    Pubkey::find_program_address(
        &[
            LULO_SEED_POOL_USER,
            &[LULO_POOL_ID],
            LULO_DEFAULT_REFERRER.as_ref(),
        ],
        &LULO_PROGRAM_ID,
    )
    .0
}

fn build_base(
    _state: &LuloState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
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

    let wrapper_mint = wv.wrapper_mint;
    let underlying_mint = wv.underlying_mint;
    let protected_mint = read_pubkey(
        &wv.protocol_data,
        PD_OFF_PROTECTED_MINT,
        "lulo.protocol_data.protected_mint",
    )?;
    let lulo_pool = wv.source_pool;
    let lulo_pool_user = wv.source_position_pda;

    let global_config = global_config_pda();
    let (wrapper_vault_pda, _) =
        Pubkey::find_program_address(&[b"vault", wrapper_mint.as_ref()], &OVERPASS_PROGRAM_ID);
    let (vault_authority, _) =
        Pubkey::find_program_address(&[b"authority", wrapper_mint.as_ref()], &OVERPASS_PROGRAM_ID);

    let referrer_pool_user = referrer_pool_user();

    let underlying_token_program = SPL_TOKEN_PROGRAM_ID;
    let lp_token_program = TOKEN_2022_PROGRAM_ID;

    let vault_underlying_ata = derive_ata(
        &vault_authority,
        &underlying_mint,
        &underlying_token_program,
    );
    let lulo_pool_user_lp_token_account =
        derive_ata(&lulo_pool_user, &protected_mint, &lp_token_program);

    let user_underlying_ata = derive_ata(user, &underlying_mint, &SPL_TOKEN_PROGRAM_ID);
    let user_wrapper_ata = derive_ata(user, &wrapper_mint, &SPL_TOKEN_PROGRAM_ID);

    Ok(vec![
        AccountMeta::new(*user, true),
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
    ])
}

pub fn build_deposit(
    state: &LuloState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let mut metas = build_base(state, wv, user)?;
    metas.extend_from_slice(&[
        AccountMeta::new_readonly(LULO_POOL_JLUSDC_ATA, false),
        AccountMeta::new_readonly(LULO_TRAILING_ACCOUNT, false),
        AccountMeta::new_readonly(LULO_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSVAR_RENT_ID, false),
    ]);
    Ok(metas)
}

#[allow(dead_code)]
fn jupiter_withdraw_backend() -> Vec<AccountMeta> {
    vec![
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
    ]
}

fn kamino_withdraw_backend() -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(KAMINO_PROTOCOL_AUTHORITY, false),
        AccountMeta::new(KAMINO_PROTOCOL_TOKEN_ACCOUNT, false),
        AccountMeta::new(KAMINO_MARKET, false),
        AccountMeta::new(KAMINO_OBLIGATION, false),
        AccountMeta::new(KAMINO_RESERVE, false),
        AccountMeta::new(KAMINO_RESERVE_LIQUIDITY_SUPPLY, false),
        AccountMeta::new_readonly(KAMINO_LENDING_MARKET_AUTHORITY, false),
        AccountMeta::new(KAMINO_COLLATERAL_MINT, false),
        AccountMeta::new(KAMINO_COLLATERAL_TOKEN_ACCOUNT, false),
        AccountMeta::new(KAMINO_RESERVE_COLLATERAL_SUPPLY, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSVAR_INSTRUCTIONS_ID, false),
        AccountMeta::new_readonly(KAMINO_PROGRAM, false),
        AccountMeta::new_readonly(KAMINO_FARMS_PROGRAM, false),
        AccountMeta::new(KAMINO_RESERVE_FARM_STATE, false),
        AccountMeta::new(KAMINO_OBLIGATION_FARM_USER_STATE, false),
        AccountMeta::new_readonly(KAMINO_SCOPE_PRICES, false),
    ]
}

fn withdraw_trailing() -> [AccountMeta; 3] {
    [
        AccountMeta::new(LULO_POOL_INPUT_TOKEN_ACCOUNT, false),
        AccountMeta::new_readonly(LULO_POOL_JLUSDC_ATA, false),
        AccountMeta::new_readonly(LULO_TRAILING_ACCOUNT, false),
    ]
}

pub fn build_withdraw(
    state: &LuloState,
    wv: &WrapperVault,
    user: &Pubkey,
) -> Result<Vec<AccountMeta>, OverpassError> {
    let mut metas = build_base(state, wv, user)?;
    metas.extend_from_slice(&[
        AccountMeta::new_readonly(LULO_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(TOKEN_2022_PROGRAM_ID, false),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ]);
    metas.extend(kamino_withdraw_backend());
    metas.extend_from_slice(&withdraw_trailing());
    Ok(metas)
}
