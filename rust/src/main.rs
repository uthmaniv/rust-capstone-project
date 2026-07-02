use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde_json::Value;
use std::fs::File;
use std::io::Write;

const RPC_URL: &str = "http://127.0.0.1:18443";
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Create wallets (silently ignore if already exists)
    let _ = rpc.call::<Value>("createwallet", &[serde_json::json!("Miner")]);
    let _ = rpc.call::<Value>("createwallet", &[serde_json::json!("Trader")]);

    let miner_url = format!("{}/wallet/Miner", RPC_URL);
    let trader_url = format!("{}/wallet/Trader", RPC_URL);

    let miner = Client::new(
        &miner_url,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;
    let trader = Client::new(
        &trader_url,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Generate a Miner address labeled "Mining Reward"
    let miner_addr = miner
        .get_new_address(Some("Mining Reward"), None)?
        .require_network(Network::Regtest)
        .expect("valid regtest address");
    println!("Miner address: {}", miner_addr);

    // Coinbase rewards stay locked until they mature, so mine until the wallet sees a spendable balance.
    let mut blocks_mined = 0u64;
    while miner.get_balance(None, None)?.to_btc() <= 0.0 {
        miner.generate_to_address(1, &miner_addr)?;
        blocks_mined += 1;
    }
    println!("Mined {} blocks", blocks_mined);

    let balance = miner.get_balance(None, None)?;
    println!("Miner balance: {} BTC", balance.to_btc());

    // Generate a Trader address labeled "Received"
    let trader_addr = trader
        .get_new_address(Some("Received"), None)?
        .require_network(Network::Regtest)
        .expect("valid regtest address");
    println!("Trader address: {}", trader_addr);

    // Send 20 BTC from Miner to Trader
    let txid = miner.send_to_address(
        &trader_addr,
        Amount::from_btc(20.0)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!("Sent 20 BTC, txid: {}", txid);

    // Fetch the unconfirmed transaction from the mempool
    let mempool_entry = miner.get_mempool_entry(&txid)?;
    println!("Mempool entry: {:?}", mempool_entry);

    // Mine 1 block to confirm the transaction
    miner.generate_to_address(1, &miner_addr)?;
    println!("Confirmed transaction");

    // Get transaction details for fee and block info
    let tx_info = miner.get_transaction(&txid, Some(true))?;
    let fee = tx_info.fee.map(|f| f.to_btc()).unwrap_or(0.0);
    let block_height = tx_info.info.blockheight.unwrap_or(0) as u64;
    let block_hash = tx_info
        .info
        .blockhash
        .map(|h| h.to_string())
        .unwrap_or_default();

    // Get the decoded transaction from the hex field
    let decoded = tx_info.transaction()?;

    // The transaction has one input spending a coinbase UTXO from the Miner wallet.
    // Fetch the previous transaction to get the input address and amount.
    let prevout = &decoded.input[0].previous_output;
    let prev_tx_info = miner.get_transaction(&prevout.txid, Some(true))?;
    let prev_decoded = prev_tx_info.transaction()?;
    let prev_txout = &prev_decoded.output[prevout.vout as usize];

    let miner_input_addr = Address::from_script(&prev_txout.script_pubkey, Network::Regtest)
        .expect("valid address from script")
        .to_string();
    let miner_input_amount = prev_txout.value.to_btc();

    // The transaction has two outputs: one for Trader (~20 BTC) and one for Miner (change).
    let (trader_out, change_out) = if (decoded.output[0].value.to_btc() - 20.0).abs() < 0.001 {
        (&decoded.output[0], &decoded.output[1])
    } else {
        (&decoded.output[1], &decoded.output[0])
    };

    let trader_out_addr = Address::from_script(&trader_out.script_pubkey, Network::Regtest)
        .expect("valid address from script")
        .to_string();
    let trader_out_amount = trader_out.value.to_btc();
    let miner_change_addr = Address::from_script(&change_out.script_pubkey, Network::Regtest)
        .expect("valid address from script")
        .to_string();
    let miner_change_amount = change_out.value.to_btc();

    // Write output to ../out.txt in the required format
    let mut out = File::create("../out.txt")?;
    writeln!(out, "{}", txid)?;
    writeln!(out, "{}", miner_input_addr)?;
    writeln!(out, "{:.8}", miner_input_amount)?;
    writeln!(out, "{}", trader_out_addr)?;
    writeln!(out, "{:.8}", trader_out_amount)?;
    writeln!(out, "{}", miner_change_addr)?;
    writeln!(out, "{:.8}", miner_change_amount)?;
    writeln!(out, "{:.8}", fee)?;
    writeln!(out, "{}", block_height)?;
    writeln!(out, "{}", block_hash)?;

    println!("Output written to ../out.txt");
    Ok(())
}
