use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::ledger_helpers_local::{self, DefaultDB, DustOutput, Timestamp, WalletSeed};
use super::serde_convert::{dust_generation_info_to_ser, qualified_dust_output_to_ser};
use crate::commands::dust_balance::{DustBalanceJson, GenerationInfoPair};

pub fn dust_balance(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
	seed: WalletSeed,
) -> Result<DustBalanceJson, Box<dyn std::error::Error + Send + Sync>> {
	context.with_wallet_from_seed(seed, |wallet| {
		let dust_state = wallet.dust.dust_local_state.as_ref().unwrap();

		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		let timestamp = Timestamp::from_secs(now);
		let total = dust_state.wallet_balance(timestamp);

		let mut capacity = 0u128;

		let mut generation_infos = Vec::new();
		let mut source = HashMap::new();
		for dust_output in dust_state.utxos() {
			let dust_output_ser = qualified_dust_output_to_ser(dust_output);
			let gen_info = dust_state.generation_info(&dust_output);
			capacity += gen_info
				.as_ref()
				.map(|g| g.value * dust_state.params.night_dust_ratio as u128)
				.unwrap_or(0);
			let gen_info_pair = GenerationInfoPair {
				dust_output: dust_output_ser.clone(),
				generation_info: gen_info.as_ref().map(|g| dust_generation_info_to_ser(*g)),
			};
			generation_infos.push(gen_info_pair);

			if let Some(gen_info) = gen_info {
				let balance = DustOutput::from(dust_output).updated_value(
					&gen_info,
					timestamp,
					&dust_state.params,
				);
				source.insert(dust_output_ser.nonce, balance);
			}
		}
		Ok(DustBalanceJson { generation_infos, source, total, capacity })
	})
}
