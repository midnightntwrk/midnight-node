use std::io::Read as _;

use parity_scale_codec::Decode;
use serde::{Deserialize, Serialize};
use subxt::{
    OnlineClient, SubstrateConfig,
    backend::{legacy::LegacyRpcMethods, rpc::RpcClient},
};

/// SCALE-decoded mirror of pallet_midnight::UnshieldedTokensDetails
#[derive(Decode)]
struct UnshieldedTokensEvent {
    spent: Vec<UtxoInfoRaw>,
    created: Vec<UtxoInfoRaw>,
}

/// SCALE-decoded mirror of midnight_node_ledger::UtxoInfo
#[derive(Decode)]
struct UtxoInfoRaw {
    _address: [u8; 32],
    _token_type: [u8; 32],
    intent_hash: [u8; 32],
    _value: u128,
    output_no: u32,
}

/// SCALE-decoded mirror of pallet_midnight::TxAppliedDetails
#[derive(Decode)]
struct TxAppliedEvent {
    tx_hash: [u8; 32],
}

#[derive(Serialize)]
struct OverrideEntry {
    tx_hash: String,
    block_height: u64,
    created: Vec<UtxoRef>,
    spent: Vec<UtxoRef>,
}

#[derive(Serialize)]
struct UtxoRef {
    intent_hash: String,
    output_index: u32,
}

#[derive(Deserialize)]
struct ExistingEntry {
    block_height: u64,
}

fn to_refs(utxos: &[UtxoInfoRaw]) -> Vec<UtxoRef> {
    utxos
        .iter()
        .map(|u| UtxoRef {
            intent_hash: hex::encode(u.intent_hash),
            output_index: u.output_no,
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <ws-url> [existing-override.json]", args[0]);
        eprintln!();
        eprintln!("Queries a synced midnight node for UnshieldedTokens events");
        eprintln!("to generate correct UTXO ordering override data.");
        eprintln!();
        eprintln!("If existing-override.json is given, queries only those block heights.");
        eprintln!("Otherwise reads JSON array of block heights from stdin.");
        std::process::exit(1);
    }

    let url = &args[1];

    let block_heights: Vec<u64> = if let Some(file) = args.get(2) {
        let content = std::fs::read_to_string(file)?;
        let entries: Vec<ExistingEntry> = serde_json::from_str(&content)?;
        let mut heights: Vec<u64> = entries.iter().map(|e| e.block_height).collect();
        heights.sort();
        heights.dedup();
        heights
    } else {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        serde_json::from_str(&input)?
    };

    eprintln!("Connecting to {url}...");
    let rpc_client = RpcClient::from_insecure_url(url).await?;
    let api = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client.clone()).await?;
    let rpc = LegacyRpcMethods::<SubstrateConfig>::new(rpc_client);
    eprintln!("Connected. Querying {} block(s)...", block_heights.len());

    let mut results = Vec::new();

    for &height in &block_heights {
        let block_hash = rpc
            .chain_get_block_hash(Some(height.into()))
            .await?
            .ok_or_else(|| format!("no block hash for height {height}"))?;

        let events = api.events().at(block_hash).await?;

        let mut pending: Option<(Vec<UtxoRef>, Vec<UtxoRef>)> = None;

        for event in events.iter() {
            let event = event?;
            if event.pallet_name() != "Midnight" {
                continue;
            }

            match event.variant_name() {
                "UnshieldedTokens" => {
                    let mut bytes = event.field_bytes();
                    let details = UnshieldedTokensEvent::decode(&mut bytes)?;
                    pending = Some((to_refs(&details.spent), to_refs(&details.created)));
                }
                "TxApplied" | "TxPartialSuccess" => {
                    if let Some((spent, created)) = pending.take() {
                        let mut bytes = event.field_bytes();
                        let details = TxAppliedEvent::decode(&mut bytes)?;
                        let tx_hash = hex::encode(details.tx_hash);

                        if spent.len() > 1 || created.len() > 1 {
                            eprintln!(
                                "  block {height}: tx {:.8}… ({} created, {} spent)",
                                tx_hash,
                                created.len(),
                                spent.len()
                            );
                            results.push(OverrideEntry {
                                tx_hash,
                                block_height: height,
                                created,
                                spent,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    results.sort_by_key(|e| e.block_height);
    println!("{}", serde_json::to_string_pretty(&results)?);
    eprintln!("Done. {} override entries.", results.len());

    Ok(())
}
