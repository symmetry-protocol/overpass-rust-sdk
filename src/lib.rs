use std::collections::HashSet;
use std::sync::atomic::Ordering;

use anyhow::Result;
use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, Quote, QuoteParams, Swap,
    SwapAndAccountMetas, SwapParams,
};
use rust_decimal::Decimal;
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
use crate::error::OverpassError;

pub const OVERPASS_PROGRAM_ID: Pubkey = pubkey!("WRAPdXmxrH37RKUbH1QMnYrKdNe8w4Kz44t1cXmYeum");

#[derive(Debug, Clone, Copy)]
enum Direction {
    Deposit,
    Withdraw,
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
    fn current_slot(&self) -> u64 {
        self.clock_ref.slot.load(Ordering::Relaxed)
    }

    fn unix_timestamp(&self) -> i64 {
        self.clock_ref.unix_timestamp.load(Ordering::Relaxed)
    }

    fn detect_direction(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
    ) -> Result<Direction, OverpassError> {
        let wv = &self.wrapper_vault;
        if input_mint == wv.underlying_mint && output_mint == wv.wrapper_mint {
            Ok(Direction::Deposit)
        } else if input_mint == wv.wrapper_mint && output_mint == wv.underlying_mint {
            Ok(Direction::Withdraw)
        } else {
            Err(OverpassError::InvalidState("unsupported mint pair"))
        }
    }

    fn run_quote(&self, direction: Direction, amount: u64) -> Result<u64, OverpassError> {
        let wv = &self.wrapper_vault;
        let gc = &self.global_config;
        let slot = self.current_slot();
        let ts = self.unix_timestamp();
        match (&self.state, direction) {
            (ProtocolState::Lulo(s), Direction::Deposit) => {
                lulo::math::quote_deposit(s, wv, gc, amount)
            }
            (ProtocolState::Lulo(s), Direction::Withdraw) => {
                lulo::math::quote_withdraw(s, wv, amount)
            }
            (ProtocolState::Klend(s), Direction::Deposit) => {
                klend::math::quote_deposit(s, slot, ts, wv, gc, amount)
            }
            (ProtocolState::Klend(s), Direction::Withdraw) => {
                klend::math::quote_withdraw(s, slot, ts, wv, amount)
            }
            (ProtocolState::Kvault(s), Direction::Deposit) => {
                kvault::math::quote_deposit(s, slot, ts, wv, gc, amount)
            }
            (ProtocolState::Kvault(s), Direction::Withdraw) => {
                kvault::math::quote_withdraw(s, slot, ts, wv, amount)
            }
            (ProtocolState::Save(s), Direction::Deposit) => {
                save::math::quote_deposit(s, slot, wv, gc, amount)
            }
            (ProtocolState::Save(s), Direction::Withdraw) => {
                save::math::quote_withdraw(s, slot, wv, amount)
            }
            (ProtocolState::Marginfi(s), Direction::Deposit) => {
                marginfi::math::quote_deposit(s, ts, wv, gc, amount)
            }
            (ProtocolState::Marginfi(s), Direction::Withdraw) => {
                marginfi::math::quote_withdraw(s, ts, wv, amount)
            }
        }
    }

    fn build_metas(
        &self,
        direction: Direction,
        user: &Pubkey,
    ) -> Result<Vec<solana_program::instruction::AccountMeta>, OverpassError> {
        let wv = &self.wrapper_vault;
        match (&self.state, direction) {
            (ProtocolState::Lulo(s), Direction::Deposit) => {
                lulo::account_metas::build_deposit(s, wv, user)
            }
            (ProtocolState::Lulo(s), Direction::Withdraw) => {
                lulo::account_metas::build_withdraw(s, wv, user)
            }
            (ProtocolState::Klend(s), Direction::Deposit) => {
                klend::account_metas::build_deposit(s, wv, user)
            }
            (ProtocolState::Klend(s), Direction::Withdraw) => {
                klend::account_metas::build_withdraw(s, wv, user)
            }
            (ProtocolState::Kvault(s), Direction::Deposit) => {
                kvault::account_metas::build_deposit(s, wv, user)
            }
            (ProtocolState::Kvault(s), Direction::Withdraw) => {
                kvault::account_metas::build_withdraw(s, wv, user)
            }
            (ProtocolState::Save(s), Direction::Deposit) => {
                save::account_metas::build_deposit(s, wv, user)
            }
            (ProtocolState::Save(s), Direction::Withdraw) => {
                save::account_metas::build_withdraw(s, wv, user)
            }
            (ProtocolState::Marginfi(s), Direction::Deposit) => {
                marginfi::account_metas::build_deposit(s, wv, user)
            }
            (ProtocolState::Marginfi(s), Direction::Withdraw) => {
                marginfi::account_metas::build_withdraw(s, wv, user)
            }
        }
    }

    pub fn quote_and_swap_instruction(&self, params: &SwapParams) -> Result<(Quote, Instruction)> {
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
            (ProtocolState::Kvault(_), true) => kvault::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Kvault(_), false) => kvault::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Klend(_), true) => klend::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Klend(_), false) => klend::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Save(_), true) => save::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Save(_), false) => save::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Marginfi(_), true) => marginfi::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Marginfi(_), false) => marginfi::WITHDRAW_IX_DISCRIMINATOR,
            (ProtocolState::Lulo(_), true) => lulo::DEPOSIT_IX_DISCRIMINATOR,
            (ProtocolState::Lulo(_), false) => lulo::WITHDRAW_IX_DISCRIMINATOR,
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

fn compute_fee(
    amount: u64,
    protocol_fee_bps: u16,
    creator_fee_bps: u16,
) -> Result<(u64, u64, Decimal), OverpassError> {
    let protocol_fee = crate::common::checked::mul_bps(amount, protocol_fee_bps)?;
    let creator_fee = crate::common::checked::mul_bps(amount, creator_fee_bps)?;
    let total_fee = protocol_fee
        .checked_add(creator_fee)
        .ok_or(OverpassError::MathOverflow)?;
    let fee_pct_bps = protocol_fee_bps as i64 + creator_fee_bps as i64;
    let fee_pct = Decimal::new(fee_pct_bps, 4);
    Ok((protocol_fee, total_fee, fee_pct))
}

impl Amm for Overpass {
    fn from_keyed_account(keyed_account: &KeyedAccount, amm_context: &AmmContext) -> Result<Self> {
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
        if params.amount >= u64::MAX / 2 {
            return Err(anyhow::Error::from(OverpassError::InsufficientLiquidity));
        }
        if !matches!(params.swap_mode, jupiter_amm_interface::SwapMode::ExactIn) {
            return Err(anyhow::Error::from(OverpassError::UnsupportedSwapMode));
        }

        let direction = self.detect_direction(params.input_mint, params.output_mint)?;
        let wv = &self.wrapper_vault;
        let out_amount = self.run_quote(direction, params.amount)?;

        let (fee_amount, fee_mint, fee_pct) = match direction {
            Direction::Deposit => {
                let (_protocol_fee, total_fee, pct) = compute_fee(
                    params.amount,
                    self.global_config.protocol_deposit_fee_bps,
                    wv.creator_deposit_fee_bps,
                )?;
                (total_fee, wv.underlying_mint, pct)
            }
            Direction::Withdraw => (0u64, wv.underlying_mint, Decimal::ZERO),
        };

        Ok(Quote {
            in_amount: params.amount,
            out_amount,
            fee_amount,
            fee_mint,
            fee_pct,
        })
    }

    fn get_swap_and_account_metas(&self, params: &SwapParams) -> Result<SwapAndAccountMetas> {
        let direction = self
            .detect_direction(params.source_mint, params.destination_mint)
            .map_err(anyhow::Error::from)?;
        let user = params.token_transfer_authority;
        let account_metas = self
            .build_metas(direction, &user)
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
