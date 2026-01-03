use crate::config::Settings;
use midnight_node_ledger_helpers::{NIGHT, WalletAddress, WalletSeed};
use midnight_node_toolkit::commands::dust_balance::{self, DustBalanceArgs, DustBalanceResult};
use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs, GenerateTxsError};
use midnight_node_toolkit::t_token;
use midnight_node_toolkit::tx_generator::builder::{Builder, SingleTxArgs};
use midnight_node_toolkit::tx_generator::destination::Destination;
use midnight_node_toolkit::tx_generator::source::{FetchCacheConfig, Source};
use std::str::FromStr;

pub struct ToolkitClient {
    funding_seed: String,
    node_url: String,
}

impl ToolkitClient {
    pub fn new(settings: Settings) -> Self {
        Self {
            funding_seed: settings.constants.payments.midnight_seed,
            node_url: settings.node_client.base_url,
        }
    }

    pub async fn generate_single_tx(&self) -> Result<(), GenerateTxsError> {
        let builder = Builder::SingleTx(SingleTxArgs {
            shielded_amount: Some(0),
            shielded_token_type: t_token(),
            unshielded_amount: Some(1),
            unshielded_token_type: NIGHT,
            source_seed: self.funding_seed.clone(),
            destination_address: vec![
                WalletAddress::from_str(
                    "mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj",
                )
                .unwrap(),
            ],
            rng_seed: None,
        });

        let args = GenerateTxsArgs {
            builder: builder.clone(),
            source: Source {
                src_files: None,
                src_url: Some(self.node_url.clone()),
                fetch_concurrency: 4,
                dust_warp: false,
                fetch_cache: FetchCacheConfig::InMemory,
            },
            destination: Destination {
                dest_file: None,
                dest_urls: vec![self.node_url.clone()],
                rate: 1.0,
                to_bytes: false,
            },
            proof_server: None,
            dry_run: false,
        };

        generate_txs::execute(args).await
    }

    pub async fn dust_balance(
        &self,
        seed: WalletSeed,
    ) -> Result<DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
        let args = DustBalanceArgs {
            source: Source {
                src_files: None,
                src_url: Some(self.node_url.clone()),
                fetch_concurrency: 1,
                dust_warp: false,
                fetch_cache: FetchCacheConfig::InMemory,
            },
            seed,
            dry_run: false,
        };

        dust_balance::execute(args).await
    }
}
