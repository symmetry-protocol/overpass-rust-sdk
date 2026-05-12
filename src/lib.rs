//! Jupiter `Amm` implementation for Overpass wrapper vaults.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use anyhow::Result;
use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, Quote, QuoteParams, Swap,
    SwapAndAccountMetas, SwapParams,
};
use solana_program::{instruction::Instruction, pubkey, pubkey::Pubkey};

pub mod accounts;
pub mod common;
pub mod error;

pub mod klend;
pub mod kvault;
pub mod lulo;
pub mod marginfi;
pub mod save;

use crate::accounts::{
    decode_global_config, decode_wrapper_vault, global_config_pda, GlobalConfig, ProtocolId,
    ProtocolState, WrapperVault,
};

pub const OVERPASS_PROGRAM_ID: Pubkey = pubkey!("WRAPdXmxrH37RKUbH1QMnYrKdNe8w4Kz44t1cXmYeum");

#[derive(Debug, Clone, Copy)]
pub struct QuoteCtx {
    pub current_slot: u64,
    pub unix_timestamp: u64,
    pub protocol_deposit_fee_bps: u16,
}

#[derive(Clone)]
pub struct Overpass {
    wrapper_vault_key: Pubkey,
    wrapper_vault: WrapperVault,
    global_config_key: Pubkey,
    global_config: GlobalConfig,
    clock_ref: ClockRef,
    state: ProtocolState,
}

impl Overpass {
    fn quote_ctx(&self) -> QuoteCtx {
        let unix_timestamp = self.clock_ref.unix_timestamp.load(Ordering::Relaxed);
        QuoteCtx {
            current_slot: self.clock_ref.slot.load(Ordering::Relaxed),
            unix_timestamp: u64::try_from(unix_timestamp).unwrap_or(0),
            protocol_deposit_fee_bps: self.global_config.protocol_deposit_fee_bps,
        }
    }

    pub fn quote_and_swap_instruction(
        &self,
        params: &SwapParams,
    ) -> Result<(Quote, Instruction)> {
        let quote = self.quote(&QuoteParams {
            amount: params.in_amount,
            input_mint: params.source_mint,
            output_mint: params.destination_mint,
            swap_mode: params.swap_mode,
        })?;

        let metas = self.get_swap_and_account_metas(params)?;
        let underlying = self.wrapper_vault.underlying_mint;
        let wrapper = self.wrapper_vault.wrapper_mint;
        let is_deposit = params.source_mint == underlying && params.destination_mint == wrapper;

        let discriminator: [u8; 8] = match (&self.state, is_deposit) {
            (ProtocolState::Kvault(_),   true)  => kvault::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Kvault(_),   false) => kvault::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Klend(_),    true)  => klend::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Klend(_),    false) => klend::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Save(_),     true)  => save::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Save(_),     false) => save::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Marginfi(_), true)  => marginfi::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Marginfi(_), false) => marginfi::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Lulo(_),     true)  => lulo::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Lulo(_),     false) => lulo::WITHDRAW_IX_DISCRIMINATOR,
        };

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&params.in_amount.to_le_bytes());
        data.extend_from_slice(&params.out_amount.to_le_bytes());

        let ix = Instruction {
            program_id: OVERPASS_PROGRAM_ID,
            accounts: metas.account_metas,
            data,
        };
        Ok((quote, ix))
    }
}

impl Amm for Overpass {
    fn from_keyed_account(
        keyed_account: &KeyedAccount,
        amm_context: &AmmContext,
    ) -> Result<Self> {
        let wv = decode_wrapper_vault(&keyed_account.account.data).map_err(anyhow::Error::from)?;
        let pid = ProtocolId::from_byte(wv.protocol).map_err(anyhow::Error::from)?;

        let state = match pid {
            ProtocolId::Kvault => ProtocolState::Kvault(kvault::state::empty()),
            ProtocolId::Klend => ProtocolState::Klend(klend::state::empty()),
            ProtocolId::Save => ProtocolState::Save(save::state::empty()),
            ProtocolId::Lulo => ProtocolState::Lulo(lulo::state::empty()),
            ProtocolId::Marginfi => ProtocolState::Marginfi(marginfi::state::empty()),
        };

        Ok(Self {
            wrapper_vault_key: keyed_account.key,
            wrapper_vault: wv,
            global_config_key: global_config_pda(),
            global_config: GlobalConfig::default(),
            clock_ref: amm_context.clock_ref.clone(),
            state,
        })
    }

    fn label(&self) -> String {
        "Overpass".to_string()
    }

    fn program_id(&self) -> Pubkey {
        OVERPASS_PROGRAM_ID
    }

    fn key(&self) -> Pubkey {
        self.wrapper_vault_key
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![
            self.wrapper_vault.underlying_mint,
            self.wrapper_vault.wrapper_mint,
        ]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        let mut accts = vec![self.wrapper_vault_key, self.global_config_key];
        let wv = &self.wrapper_vault;
        accts.extend(match &self.state {
            ProtocolState::Klend(s) => klend::update::accounts_to_update(s, wv),
            ProtocolState::Kvault(s) => kvault::update::accounts_to_update(s, wv),
            ProtocolState::Save(s) => save::update::accounts_to_update(s, wv),
            ProtocolState::Marginfi(s) => marginfi::update::accounts_to_update(s, wv),
            ProtocolState::Lulo(s) => lulo::update::accounts_to_update(s, wv),
        });
        accts
    }

    fn has_dynamic_accounts(&self) -> bool {
        matches!(self.state, ProtocolState::Kvault(_))
    }

    fn update(&mut self, account_map: &AccountMap) -> Result<()> {
        if let Some(account) = account_map.get(&self.wrapper_vault_key) {
            self.wrapper_vault =
                decode_wrapper_vault(&account.data).map_err(anyhow::Error::from)?;
        }
        if let Some(account) = account_map.get(&self.global_config_key) {
            self.global_config =
                decode_global_config(&account.data).map_err(anyhow::Error::from)?;
        }
        let wv = &self.wrapper_vault;
        match &mut self.state {
            ProtocolState::Klend(s) => klend::update::update(s, wv, account_map),
            ProtocolState::Kvault(s) => kvault::update::update(s, wv, account_map),
            ProtocolState::Save(s) => save::update::update(s, wv, account_map),
            ProtocolState::Marginfi(s) => marginfi::update::update(s, wv, account_map),
            ProtocolState::Lulo(s) => lulo::update::update(s, wv, account_map),
        }
        .map_err(anyhow::Error::from)?;
        Ok(())
    }

    fn quote(&self, params: &QuoteParams) -> Result<Quote> {
        let ctx = self.quote_ctx();
        let wv = &self.wrapper_vault;
        match &self.state {
            ProtocolState::Klend(s) => klend::math::quote(s, wv, &ctx, params),
            ProtocolState::Kvault(s) => kvault::math::quote(s, wv, &ctx, params),
            ProtocolState::Save(s) => save::math::quote(s, wv, &ctx, params),
            ProtocolState::Marginfi(s) => marginfi::math::quote(s, wv, &ctx, params),
            ProtocolState::Lulo(s) => lulo::math::quote(s, wv, &ctx, params),
        }
        .map_err(anyhow::Error::from)
    }

    fn get_swap_and_account_metas(&self, params: &SwapParams) -> Result<SwapAndAccountMetas> {
        let wv = &self.wrapper_vault;
        let account_metas = match &self.state {
            ProtocolState::Klend(s) => klend::account_metas::build(s, wv, params),
            ProtocolState::Kvault(s) => kvault::account_metas::build(s, wv, params),
            ProtocolState::Save(s) => save::account_metas::build(s, wv, params),
            ProtocolState::Marginfi(s) => marginfi::account_metas::build(s, wv, params),
            ProtocolState::Lulo(s) => lulo::account_metas::build(s, wv, params),
        }
        .map_err(anyhow::Error::from)?;
        Ok(SwapAndAccountMetas {
            swap: Swap::RaydiumCP,
            account_metas,
        })
    }

    fn clone_amm(&self) -> Box<dyn Amm + Send + Sync> {
        Box::new(self.clone())
    }

    fn supports_exact_out(&self) -> bool {
        false
    }

    fn get_accounts_len(&self) -> usize {
        match &self.state {
            ProtocolState::Klend(_) => klend::ACCOUNTS_LEN,
            ProtocolState::Kvault(s) => kvault::accounts_len(s, &self.wrapper_vault),
            ProtocolState::Save(s) => save::accounts_len(s),
            ProtocolState::Marginfi(s) => marginfi::accounts_len(s),
            ProtocolState::Lulo(_) => lulo::ACCOUNTS_LEN,
        }
    }

    fn is_active(&self) -> bool {
        if self.global_config.paused {
            return false;
        }
        match &self.state {
            ProtocolState::Klend(s) => klend::is_active(s),
            ProtocolState::Kvault(s) => kvault::is_active(s),
            ProtocolState::Save(s) => save::is_active(s),
            ProtocolState::Marginfi(s) => marginfi::is_active(s),
            ProtocolState::Lulo(s) => lulo::is_active(s),
        }
    }

    fn program_dependencies(&self) -> Vec<(Pubkey, String)> {
        let mut deps = vec![(OVERPASS_PROGRAM_ID, "overpass".to_string())];
        deps.extend(match &self.state {
            ProtocolState::Klend(_) => klend::program_dependencies(),
            ProtocolState::Kvault(_) => kvault::program_dependencies(),
            ProtocolState::Save(_) => save::program_dependencies(),
            ProtocolState::Marginfi(_) => marginfi::program_dependencies(),
            ProtocolState::Lulo(_) => lulo::program_dependencies(),
        });
        deps
    }

    fn underlying_liquidities(&self) -> Option<HashSet<Pubkey>> {
        let mut set = HashSet::new();
        set.insert(self.wrapper_vault.source_pool);
        Some(set)
    }
}
