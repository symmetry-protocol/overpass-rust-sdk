use std::str::FromStr;
use std::sync::atomic::Ordering;

use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, QuoteParams, Swap, SwapMode, SwapParams,
};
use overpass_rust_sdk::{accounts, Overpass};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;

const PYUSD_MINT: &str = "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

const KVAULT_WRAPPERS: &[&str] = &[
    "EADiud4DqFw5bbcrZk8ECoKnajw71tiWkq7rY9oLgq3n", // PYUSD (Token-2022, 6 dec)
    "8J2YMb8JWxcqeqjEYHTzreeWaRk1MgepiDQ8bxGnYhsn", // wSOL  (legacy SPL, 9 dec)
    "WRfrzTHSHdrmcLrwgg4aB1STFR6D5iP13YYLao6VqhM",  // PYUSD (Token-2022, 6 dec)
];

fn rpc_client() -> RpcClient {
    let url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    RpcClient::new_with_commitment(url, CommitmentConfig::confirmed())
}

fn vault_pda(wrapper_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", wrapper_mint.as_ref()], &overpass_rust_sdk::OVERPASS_PROGRAM_ID).0
}

fn load_amm(client: &RpcClient, wrapper_mint: Pubkey) -> Overpass {
    let pda = vault_pda(&wrapper_mint);
    let acct = client.get_account(&pda).expect("fetch wrapper_vault");
    let ka = KeyedAccount {
        key: pda,
        account: acct,
        params: None,
    };

    let clock_ref = ClockRef::default();
    let slot = client.get_slot().expect("get_slot");
    clock_ref.slot.store(slot, Ordering::Relaxed);
    if let Ok(bt) = client.get_block_time(slot) {
        clock_ref.unix_timestamp.store(bt, Ordering::Relaxed);
    }
    let amm_ctx = AmmContext { clock_ref };

    let mut amm = Overpass::from_keyed_account(&ka, &amm_ctx).expect("from_keyed_account");

    // 2 passes: first decodes vault_state to discover allocations, second
    // fetches the per-allocation klend reserves.
    fetch_and_update(client, &mut amm);
    fetch_and_update(client, &mut amm);

    amm
}

fn fetch_and_update(client: &RpcClient, amm: &mut Overpass) {
    let to_update = amm.get_accounts_to_update();
    let accounts = client
        .get_multiple_accounts(&to_update)
        .expect("get_multiple_accounts");
    let mut map = AccountMap::default();
    for (key, acct_opt) in to_update.iter().zip(accounts.iter()) {
        if let Some(a) = acct_opt {
            map.insert(*key, a.clone());
        }
    }
    amm.update(&map).expect("update");
}

fn underlying_label(mint: &Pubkey) -> &'static str {
    match mint.to_string().as_str() {
        s if s == PYUSD_MINT => "PYUSD",
        s if s == WSOL_MINT => "wSOL",
        _ => "?",
    }
}

fn mint_decimals(client: &RpcClient, mint: &Pubkey) -> u8 {
    let acct = client.get_account(mint).expect("fetch mint");
    acct.data[44]
}

#[test]
#[ignore]
fn test_sweep_kvault_wrappers() {
    let client = rpc_client();
    let mut deposit_ok = 0usize;
    let mut withdraw_ok = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for wm_str in KVAULT_WRAPPERS {
        let wm = Pubkey::from_str(wm_str).unwrap();
        let amm = load_amm(&client, wm);
        let mints = amm.get_reserve_mints();
        let underlying = mints[0];
        let label = underlying_label(&underlying);

        assert_eq!(amm.program_id(), overpass_rust_sdk::OVERPASS_PROGRAM_ID, "{}", wm);
        assert_eq!(amm.label(), "Overpass");
        assert_eq!(amm.key(), vault_pda(&wm));
        assert!(!amm.supports_exact_out());
        assert!(amm.has_dynamic_accounts(), "kvault should report dynamic accounts");
        assert_eq!(mints.len(), 2);
        assert_eq!(mints[1], wm, "wrapper mint should be index 1");

        let wv_acct = client.get_account(&vault_pda(&wm)).unwrap();
        let wv = accounts::decode_wrapper_vault(&wv_acct.data).unwrap();
        let decimals = mint_decimals(&client, &underlying);
        let amount: u64 = 10u64.pow(decimals as u32);

        let dep_line = match amm.quote(&QuoteParams {
            amount,
            input_mint: underlying,
            output_mint: wm,
            swap_mode: SwapMode::ExactIn,
        }) {
            Ok(d) => {
                assert_eq!(d.in_amount, amount, "deposit in_amount {}", wm);
                assert!(d.out_amount > 0, "deposit out_amount > 0 {}", wm);
                assert_eq!(d.fee_mint, underlying, "deposit fee_mint {}", wm);
                deposit_ok += 1;
                format!(
                    "  deposit  {:>13} {:<5} -> {:>15} wrap (fee {:>6} {})",
                    d.in_amount, label, d.out_amount, d.fee_amount, label,
                )
            }
            Err(e) => {
                failures.push(format!("[{}] {:<5} deposit: {}", wm, label, e));
                format!("  deposit  FAIL: {}", e)
            }
        };

        let withdraw_amount: u64 = amount;
        let wd_line = match amm.quote(&QuoteParams {
            amount: withdraw_amount,
            input_mint: wm,
            output_mint: underlying,
            swap_mode: SwapMode::ExactIn,
        }) {
            Ok(w) => {
                withdraw_ok += 1;
                format!(
                    "  withdraw {:>13} wrap -> {:>15} {:<5} (fee {:>6} {})",
                    w.in_amount, w.out_amount, label, w.fee_amount, label,
                )
            }
            Err(e) => {
                failures.push(format!("[{}] {:<5} withdraw: {}", wm, label, e));
                format!("  withdraw FAIL: {}", e)
            }
        };

        let placeholder_user = Pubkey::from_str("UserevMsvU5K9u6iW7DT9XJVyVLpmfDCEAfXixBbE7R").unwrap();
        let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        let (placeholder_in_ata, _) = Pubkey::find_program_address(
            &[placeholder_user.as_ref(), token_program.as_ref(), underlying.as_ref()],
            &ata_program,
        );
        let (placeholder_out_ata, _) = Pubkey::find_program_address(
            &[placeholder_user.as_ref(), token_program.as_ref(), wm.as_ref()],
            &ata_program,
        );
        let placeholder_jupiter = Pubkey::new_unique();

        let dep_metas = amm
            .get_swap_and_account_metas(&SwapParams {
                swap_mode: SwapMode::ExactIn,
                in_amount: amount,
                out_amount: 0,
                source_mint: underlying,
                destination_mint: wm,
                source_token_account: placeholder_in_ata,
                destination_token_account: placeholder_out_ata,
                token_transfer_authority: placeholder_user,
                quote_mint_to_referrer: None,
                jupiter_program_id: &placeholder_jupiter,
                missing_dynamic_accounts_as_default: false,
            })
            .unwrap_or_else(|e| panic!("deposit account_metas {}: {}", wm, e));
        assert!(matches!(dep_metas.swap, Swap::RaydiumCP));
        let kv_state_acct = client.get_account(&wv.source_pool).unwrap();
        let kv_state = overpass_rust_sdk::kvault::state::decode(&kv_state_acct.data).unwrap();
        let n = kv_state.active_allocations.len();
        assert_eq!(
            dep_metas.account_metas.len(),
            27 + 2 * n,
            "deposit metas count {}",
            wm,
        );
        assert_eq!(dep_metas.account_metas[0].pubkey, placeholder_user, "0=user");
        assert_eq!(dep_metas.account_metas[2].pubkey, vault_pda(&wm), "2=wrapper_vault");
        assert_eq!(dep_metas.account_metas[11].pubkey, wv.source_pool, "11=kvault_state");

        let wd_metas = amm
            .get_swap_and_account_metas(&SwapParams {
                swap_mode: SwapMode::ExactIn,
                in_amount: withdraw_amount,
                out_amount: 0,
                source_mint: wm,
                destination_mint: underlying,
                source_token_account: placeholder_out_ata,
                destination_token_account: placeholder_in_ata,
                token_transfer_authority: placeholder_user,
                quote_mint_to_referrer: None,
                jupiter_program_id: &placeholder_jupiter,
                missing_dynamic_accounts_as_default: false,
            })
            .unwrap_or_else(|e| panic!("withdraw account_metas {}: {}", wm, e));
        assert_eq!(
            wd_metas.account_metas.len(),
            37 + 2 * n,
            "withdraw metas count {}",
            wm,
        );
        assert_eq!(wd_metas.account_metas[11].pubkey, wv.source_pool, "11=kvault_state");

        let dep_n = dep_metas.account_metas.len();
        let wd_n = wd_metas.account_metas.len();
        println!("\n{}", wm);
        println!("  {} ({}) <> {}", label, wv.underlying_mint, wm);
        println!("{}", dep_line);
        println!("{}", wd_line);
        println!("  deposit accounts: {}", dep_n);
        println!("  withdraw accounts: {}", wd_n);
    }

    println!(
        "\nsweep: {}/{} deposits ok, {}/{} withdraws ok",
        deposit_ok,
        KVAULT_WRAPPERS.len(),
        withdraw_ok,
        KVAULT_WRAPPERS.len(),
    );

    if !failures.is_empty() {
        println!("\nfailures (collect-and-continue):");
        for f in &failures {
            println!("  {}", f);
        }
    }

    assert_eq!(
        deposit_ok,
        KVAULT_WRAPPERS.len(),
        "deposit failures (see above)",
    );
}
