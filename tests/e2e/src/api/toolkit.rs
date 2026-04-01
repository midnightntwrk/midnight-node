// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::{Path, PathBuf};

use midnight_node_toolkit::{
    cli_parsers,
    commands::{
        contract_address::{self, ContractAddressArgs},
        contract_state::{self, ContractStateArgs},
        generate_intent::{
            self, CircuitCommandArgs, DeployCommandArgs, GenerateIntentArgs, JsCommand,
        },
        generate_txs::{self, GenerateTxsArgs},
        send_intent::{self, SendIntentArgs},
        show_address::{self, ShowAddress, ShowAddressArgs, SpecificAddressTypeArgs},
        show_transaction::{self, ShowTransactionArgs},
    },
    toolkit_js::{CircuitArgs, DeployArgs, RelativePath, ToolkitJs},
    tx_generator::{
        builder::{Builder, CustomContractArgs},
        destination::Destination,
        source::{FetchCacheConfig, Source},
    },
};
use tempfile::TempDir;

use crate::config::Settings;

const DEFAULT_FETCH_CONCURRENCY: usize = 20;
const DEFAULT_COMPACTC_VERSION: &str = "0.30.0";
const COMPACTC_VERSION_FILE: &str = "../../COMPACTC_VERSION";

pub struct DeployOutput {
    pub intent: PathBuf,
    pub private_state: PathBuf,
    pub zswap_state: PathBuf,
}

pub struct CircuitOutput {
    pub intent: PathBuf,
    pub private_state: PathBuf,
    pub zswap_state: PathBuf,
}

pub struct ToolkitTestHelper {
    settings: Settings,
    pub work_dir: TempDir,
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn default_source() -> Source {
    Source {
        src_url: None,
        fetch_only_cached: false,
        fetch_concurrency: 0,
        fetch_compute_concurrency: None,
        src_files: None,
        dust_warp: true,
        ignore_block_context: false,
        fetch_cache: FetchCacheConfig::InMemory,
        ledger_state_db: String::new(),
    }
}

impl ToolkitTestHelper {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            work_dir: TempDir::new().expect("failed to create temp dir"),
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn compactc_version() -> String {
        std::env::var("COMPACTC_VERSION").unwrap_or_else(|_| {
            let version_file =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(COMPACTC_VERSION_FILE);
            std::fs::read_to_string(&version_file)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| DEFAULT_COMPACTC_VERSION.to_string())
        })
    }

    fn node_url(&self) -> &str {
        &self.settings.node_client.base_url
    }

    fn network(&self) -> String {
        self.settings.network.to_string()
    }

    fn toolkit_js(&self) -> ToolkitJs {
        ToolkitJs {
            path: path_to_string(&self.settings.toolkit.toolkit_js_path),
        }
    }

    fn source_from_url(&self) -> Source {
        Source {
            src_url: Some(self.node_url().to_string()),
            fetch_concurrency: DEFAULT_FETCH_CONCURRENCY,
            ..default_source()
        }
    }

    fn source_from_file(&self, file: &Path) -> Source {
        Source {
            src_files: Some(vec![path_to_string(file)]),
            ..default_source()
        }
    }

    fn dest_to_file(&self, file: &Path) -> Destination {
        Destination {
            dest_urls: vec![],
            rate: 0.0,
            dest_file: Some(path_to_string(file)),
            no_watch_progress: false,
        }
    }

    fn dest_to_url(&self) -> Destination {
        Destination {
            dest_urls: vec![self.node_url().to_string()],
            rate: 1.0,
            dest_file: None,
            no_watch_progress: false,
        }
    }

    pub fn load_contract_file(&self, name: &str) -> String {
        let path = self.settings.toolkit.contracts_dir.join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    pub fn load_template(&self, name: &str, vars: &[(&str, &str)]) -> String {
        let mut content = self.load_contract_file(name);
        for (key, value) in vars {
            content = content.replace(&format!("{{{{{key}}}}}"), value);
        }
        content
    }

    pub async fn compile_contract(
        &self,
        compact_source: &str,
        name: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let contract_dir = self.work_dir.path().join(name);
        std::fs::create_dir_all(&contract_dir)?;

        let node_modules_src = self.settings.toolkit.toolkit_js_path.join("node_modules");
        std::os::unix::fs::symlink(&node_modules_src, contract_dir.join("node_modules"))?;

        let source_file = contract_dir.join(format!("{name}.compact"));
        std::fs::write(&source_file, compact_source)?;

        let out_dir = contract_dir.join("out");

        let output = tokio::process::Command::new("npx")
            .arg("run-compactc")
            .arg(&source_file)
            .arg(&out_dir)
            .env("COMPACTC_VERSION", Self::compactc_version())
            .current_dir(&self.settings.toolkit.toolkit_js_path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(format!(
                "Contract compilation failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
            .into());
        }

        assert!(
            out_dir.join("contract").exists(),
            "Compilation succeeded but output directory not found"
        );

        Ok(out_dir)
    }

    pub fn write_config(&self, content: &str, name: &str) -> PathBuf {
        let path = self.work_dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create config parent dir");
        }
        std::fs::write(&path, content).expect("failed to write config file");
        path
    }

    pub fn show_address_coin_public(&self, seed: &str) -> String {
        let args = ShowAddressArgs {
            network: self.network(),
            seed: cli_parsers::wallet_seed_decode(seed).expect("invalid wallet seed"),
            specific_address: SpecificAddressTypeArgs {
                coin_public: true,
                ..Default::default()
            },
        };
        match show_address::execute(args) {
            ShowAddress::SingleAddress(addr) => addr,
            ShowAddress::Addresses(_) => panic!("expected single address"),
        }
    }

    pub async fn generate_intent_deploy(
        &self,
        config_file: &Path,
        coin_public: &str,
    ) -> Result<DeployOutput, Box<dyn std::error::Error + Send + Sync>> {
        let intent = self.work_dir.path().join("deploy_intent.bin");
        let private_state = self.work_dir.path().join("deploy_private_state.json");
        let zswap_state = self.work_dir.path().join("deploy_zswap_state.json");

        let args = GenerateIntentArgs {
            js_command: JsCommand::Deploy(DeployCommandArgs {
                toolkit_js: self.toolkit_js(),
                deploy: DeployArgs {
                    config: RelativePath(config_file.to_path_buf()),
                    network: self.network(),
                    coin_public: cli_parsers::coin_public_decode(coin_public)
                        .expect("invalid coin public key"),
                    authority_seed: None,
                    output_intent: RelativePath(intent.clone()),
                    output_private_state: RelativePath(private_state.clone()),
                    output_zswap_state: RelativePath(zswap_state.clone()),
                    constructor_args: vec![],
                },
                dry_run: false,
            }),
        };
        generate_intent::execute(args).await?;

        Ok(DeployOutput {
            intent,
            private_state,
            zswap_state,
        })
    }

    pub async fn generate_intent_circuit(
        &self,
        config_file: &Path,
        coin_public: &str,
        onchain_state: &Path,
        private_state: &Path,
        contract_address: &str,
        circuit_id: &str,
        call_args: &[&str],
    ) -> Result<CircuitOutput, Box<dyn std::error::Error + Send + Sync>> {
        let out_intent = self.work_dir.path().join(format!("{circuit_id}_intent.bin"));
        let out_private_state = self
            .work_dir
            .path()
            .join(format!("{circuit_id}_private_state.json"));
        let out_zswap_state = self
            .work_dir
            .path()
            .join(format!("{circuit_id}_zswap_state.json"));

        let args = GenerateIntentArgs {
            js_command: JsCommand::Circuit(CircuitCommandArgs {
                source: self.source_from_url(),
                wallet_seed: None,
                toolkit_js: self.toolkit_js(),
                circuit_call: CircuitArgs {
                    config: RelativePath(config_file.to_path_buf()),
                    contract_address: cli_parsers::contract_address_decode(contract_address)
                        .expect("invalid contract address"),
                    network: self.network(),
                    coin_public: cli_parsers::coin_public_decode(coin_public)
                        .expect("invalid coin public key"),
                    input_onchain_state: RelativePath(onchain_state.to_path_buf()),
                    input_private_state: RelativePath(private_state.to_path_buf()),
                    input_zswap_state: None,
                    output_intent: RelativePath(out_intent.clone()),
                    output_onchain_state: None,
                    output_private_state: RelativePath(out_private_state.clone()),
                    output_zswap_state: RelativePath(out_zswap_state.clone()),
                    output_result: None,
                    circuit_id: circuit_id.to_string(),
                    call_args: call_args.iter().map(|s| s.to_string()).collect(),
                },
                custom_ledger_parameters: None,
                dry_run: false,
            }),
        };
        generate_intent::execute(args).await?;

        Ok(CircuitOutput {
            intent: out_intent,
            private_state: out_private_state,
            zswap_state: out_zswap_state,
        })
    }

    pub async fn send_intent(
        &self,
        intent_file: &Path,
        compiled_dir: &Path,
        funding_seed: &str,
        zswap_state_file: Option<&Path>,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let output = self.work_dir.path().join(
            intent_file
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replace("_intent", "_tx.mn"),
        );

        let args = SendIntentArgs {
            source: self.source_from_url(),
            destination: self.dest_to_file(&output),
            proof_server: None,
            contract_args: CustomContractArgs {
                funding_seed: funding_seed.to_string(),
                rng_seed: None,
                compiled_contract_dirs: vec![path_to_string(compiled_dir)],
                intent_files: vec![path_to_string(intent_file)],
                utxo_inputs: vec![],
                zswap_state_file: zswap_state_file.map(path_to_string),
                shielded_destinations: vec![],
            },
            dry_run: false,
        };
        send_intent::execute(args).await?;

        Ok(output)
    }

    pub async fn submit_tx(
        &self,
        src_file: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args = GenerateTxsArgs {
            builder: Builder::Send,
            source: self.source_from_file(src_file),
            destination: self.dest_to_url(),
            proof_server: None,
            dry_run: false,
        };
        generate_txs::execute(args).await?;
        Ok(())
    }

    pub fn contract_address(
        &self,
        src_file: &Path,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args = ContractAddressArgs {
            tagged: false,
            untagged: false,
            src_file: path_to_string(src_file),
        };
        Ok(contract_address::execute(args)?)
    }

    pub async fn contract_state(
        &self,
        address: &str,
        dest_file: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args = ContractStateArgs {
            source: self.source_from_url(),
            contract_address: cli_parsers::contract_address_decode(address)
                .expect("invalid contract address"),
            dest_file: Some(path_to_string(dest_file)),
            dry_run: false,
        };
        contract_state::execute(args).await
    }

    pub fn show_transaction(
        &self,
        src_file: &Path,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let args = ShowTransactionArgs {
            src_file: path_to_string(src_file),
        };
        let result = show_transaction::execute(args)?;
        Ok(format!("{result}"))
    }
}
