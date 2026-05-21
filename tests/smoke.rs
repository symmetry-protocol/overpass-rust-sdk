use std::str::FromStr;
use std::sync::atomic::Ordering;

use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, SwapMode, SwapParams,
};
use overpass_rust_sdk::{Overpass, OVERPASS_PROGRAM_ID};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;

const DEFAULT_VAULT: &str = "B8jPKEcfjYt4LJKNMZfxmB116MfKmi5HZ5eMQtqcBTkz";
const DEFAULT_INPUT: &str = "So11111111111111111111111111111111111111112";
const DEFAULT_OUTPUT: &str = "5EKV8P6r54Fg74JRd7gPC9QWjfV6ktw72CQTkp3GbVep";
const DEFAULT_AMOUNT: u64 = 10_000_000;

#[test]
fn test_construction() {
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .or_else(|_| std::env::var("RPC_URL"))
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let vault = std::env::var("WRAPPER_VAULT").unwrap_or_else(|_| DEFAULT_VAULT.to_string());
    let input_mint = std::env::var("INPUT_MINT").unwrap_or_else(|_| DEFAULT_INPUT.to_string());
    let output_mint = std::env::var("OUTPUT_MINT").unwrap_or_else(|_| DEFAULT_OUTPUT.to_string());
    let amount: u64 = std::env::var("AMOUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_AMOUNT);

    let vault_pda = Pubkey::from_str(&vault).expect("invalid vault pubkey");
    let input = Pubkey::from_str(&input_mint).expect("invalid input mint");
    let output = Pubkey::from_str(&output_mint).expect("invalid output mint");

    let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let acct = client.get_account(&vault_pda).expect("fetch vault");
    let slot = client.get_slot().expect("get_slot");

    let clock_sysvar = Pubkey::from_str("SysvarC1ock11111111111111111111111111111111").unwrap();
    let unix_ts = client
        .get_account(&clock_sysvar)
        .ok()
        .and_then(|a| {
            a.data
                .get(32..40)
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
        })
        .unwrap_or(0);

    let clock_ref = ClockRef::default();
    clock_ref.slot.store(slot, Ordering::Relaxed);
    clock_ref.unix_timestamp.store(unix_ts, Ordering::Relaxed);
    let amm_ctx = AmmContext { clock_ref };
    let ka = KeyedAccount {
        key: vault_pda,
        account: acct,
        params: None,
    };
    let mut amm = Overpass::from_keyed_account(&ka, &amm_ctx).expect("construct");

    let mut map = AccountMap::default();
    for _ in 0..3 {
        let to_update = amm.get_accounts_to_update();
        let new_keys: Vec<Pubkey> = to_update
            .iter()
            .filter(|k| !map.contains_key(*k))
            .cloned()
            .collect();
        if new_keys.is_empty() {
            break;
        }
        let accounts = client.get_multiple_accounts(&new_keys).expect("gma");
        for (k, a) in new_keys.iter().zip(accounts.iter()) {
            if let Some(a) = a {
                map.insert(*k, a.clone());
            }
        }
        amm.update(&map).expect("update");
    }

    let placeholder_user = Pubkey::from_str("UserevMsvU5K9u6iW7DT9XJVyVLpmfDCEAfXixBbE7R").unwrap();
    let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
    let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    let (in_ata, _) = Pubkey::find_program_address(
        &[
            placeholder_user.as_ref(),
            token_program.as_ref(),
            input.as_ref(),
        ],
        &ata_program,
    );
    let (out_ata, _) = Pubkey::find_program_address(
        &[
            placeholder_user.as_ref(),
            token_program.as_ref(),
            output.as_ref(),
        ],
        &ata_program,
    );
    let jupiter = Pubkey::new_unique();

    let result = amm.quote_and_swap_instruction(&SwapParams {
        swap_mode: SwapMode::ExactIn,
        in_amount: amount,
        out_amount: 0,
        source_mint: input,
        destination_mint: output,
        source_token_account: in_ata,
        destination_token_account: out_ata,
        token_transfer_authority: placeholder_user,
        quote_mint_to_referrer: None,
        jupiter_program_id: &jupiter,
        missing_dynamic_accounts_as_default: false,
    });

    let (out, accts) = match result {
        Ok((quote, ix)) => {
            assert_eq!(ix.program_id, OVERPASS_PROGRAM_ID);
            assert_eq!(ix.data.len(), 24);
            (quote.out_amount.to_string(), ix.accounts.len())
        }
        Err(e) => (
            format!(
                "ERR_{}",
                format!("{:?}", e)
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .take(40)
                    .collect::<String>()
            ),
            0,
        ),
    };
    println!(
        "@QUOTE side=SDK vault={} input={} output={} amount={} slot={} out={} accts={}",
        vault_pda, input, output, amount, slot, out, accts,
    );
}
