use std::str::FromStr;
use std::sync::atomic::Ordering;

use jupiter_amm_interface::{
    AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, SwapMode, SwapParams,
};
use overpass_rust_sdk::Overpass;
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;

fn rpc_client() -> RpcClient {
    let url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    RpcClient::new_with_commitment(url, CommitmentConfig::confirmed())
}

fn vault_pda(wrapper_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"vault", wrapper_mint.as_ref()],
        &overpass_rust_sdk::OVERPASS_PROGRAM_ID,
    )
    .0
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
    let amm_ctx = AmmContext { clock_ref };

    let mut amm = Overpass::from_keyed_account(&ka, &amm_ctx).expect("from_keyed_account");
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
    amm
}

fn run_swap_smoke(wrapper_mint: &str, in_mint: &str, out_mint: &str, in_amount: u64) {
    let wm = Pubkey::from_str(wrapper_mint).unwrap();
    let in_m = Pubkey::from_str(in_mint).unwrap();
    let out_m = Pubkey::from_str(out_mint).unwrap();

    let client = rpc_client();
    let amm = load_amm(&client, wm);

    let direction = if in_m == wm {
        "WITHDRAW"
    } else if out_m == wm {
        "DEPOSIT"
    } else {
        "SWAP"
    };

    println!("\n========================================================================");
    println!("  {}  ({} -> {})", direction, in_m, out_m);
    println!("========================================================================");
    println!("  wrapper_vault: {}", vault_pda(&wm));

    let placeholder_user =
        Pubkey::from_str("UserevMsvU5K9u6iW7DT9XJVyVLpmfDCEAfXixBbE7R").unwrap();
    let ata_program =
        Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
    let token_program =
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    let (in_ata, _) = Pubkey::find_program_address(
        &[placeholder_user.as_ref(), token_program.as_ref(), in_m.as_ref()],
        &ata_program,
    );
    let (out_ata, _) = Pubkey::find_program_address(
        &[placeholder_user.as_ref(), token_program.as_ref(), out_m.as_ref()],
        &ata_program,
    );
    let placeholder_jupiter = Pubkey::new_unique();
    let (quote, ix) = amm
        .quote_and_swap_instruction(&SwapParams {
            swap_mode: SwapMode::ExactIn,
            in_amount,
            out_amount: 0,
            source_mint: in_m,
            destination_mint: out_m,
            source_token_account: in_ata,
            destination_token_account: out_ata,
            token_transfer_authority: placeholder_user,
            quote_mint_to_referrer: None,
            jupiter_program_id: &placeholder_jupiter,
            missing_dynamic_accounts_as_default: false,
        })
        .expect("quote_and_swap_instruction");

    println!(
        "  quote:  {} in  ->  {} out   (fee {} on {}, {})",
        quote.in_amount, quote.out_amount, quote.fee_amount, quote.fee_mint, quote.fee_pct,
    );

    let amount_le = u64::from_le_bytes(ix.data[8..16].try_into().unwrap());
    let min_out_le = u64::from_le_bytes(ix.data[16..24].try_into().unwrap());
    println!(
        "  ix:     {} bytes total | program {}",
        ix.data.len(),
        ix.program_id,
    );
    println!("    discriminator: {:02x?}", &ix.data[0..8]);
    println!("    amount:        {}", amount_le);
    println!("    min_out:       {}", min_out_le);

    let n = ix.accounts.len();
    let signers = ix.accounts.iter().filter(|m| m.is_signer).count();
    let writables = ix.accounts.iter().filter(|m| m.is_writable).count();
    println!(
        "  metas:  {} entries | {} signer(s) | {} writable",
        n, signers, writables,
    );
}

#[test]
fn smoke_swap() {
    run_swap_smoke(
        "9gizcsm4Y8HvXJHLErq4tQe9GPDViT32HctXU7FGm1Fd",
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "9gizcsm4Y8HvXJHLErq4tQe9GPDViT32HctXU7FGm1Fd",
        100_000_000,
    );

    run_swap_smoke(
        "9gizcsm4Y8HvXJHLErq4tQe9GPDViT32HctXU7FGm1Fd",
        "9gizcsm4Y8HvXJHLErq4tQe9GPDViT32HctXU7FGm1Fd",
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        100_000_000,
    );
}
