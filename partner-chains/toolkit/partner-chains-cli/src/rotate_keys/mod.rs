use crate::CmdRun;
use crate::config::KEYS_FILE_PATH;
use crate::config::config_fields;
use crate::io::{IOContext, prompt_can_write};
use crate::keystore::keystore_path;
use crate::permissioned_candidates::PermissionedCandidateKeys;
use crate::register::register1::{
	GeneratedKeysFileContent, load_chain_config_field, read_generated_keys,
};
use crate::register::register2::get_stake_pool_cold_skey;
use crate::register::register3::submit_candidate_registration;
use crate::register::{
	CandidateKeyParam, RegisterValidatorMessage, get_ecdsa_pair_from_file,
	select_registration_utxo, sign_registration_message_with_sidechain_key,
};
use crate::substrate_rpc::{SubstrateRpcRequest, SubstrateRpcResponse};
use anyhow::anyhow;
use sidechain_domain::byte_string::ByteString;
use sidechain_domain::crypto::cardano_spo_public_key_and_signature;
use sidechain_domain::{CROSS_CHAIN_KEY_TYPE_ID, SidechainPublicKey, SidechainSignature};
use sp_core::Pair;
use sp_core::crypto::KeyTypeId;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, clap::Parser)]
pub struct RotateKeysCmd {
	#[clap(flatten)]
	common_arguments: crate::CommonArguments,
	/// URL of the RPC endpoint of the running partner chain node. If not provided, the value
	/// from the resources configuration is used (prompting for it if missing).
	#[arg(long)]
	node_url: Option<String>,
	/// Path to the SPO cold signing key file. If provided, the wizard completes the full
	/// re-registration without prompting. If the cold key is not available on this machine,
	/// the wizard prints the `register2` command to be run on the cold machine instead.
	#[arg(long)]
	mainchain_signing_key_file: Option<String>,
	/// Path to the Cardano payment signing key file used to sign and pay for the registration
	/// transaction (only used when the re-registration is submitted by this wizard). If not
	/// provided, the value from the resources configuration is used.
	#[arg(long)]
	payment_signing_key_file: Option<String>,
}

impl CmdRun for RotateKeysCmd {
	fn run<C: IOContext>(&self, context: &C) -> anyhow::Result<()> {
		context.print("⚙️ Rotating session keys of a registered committee candidate");
		context.print(
			"This wizard generates fresh session keys on a RUNNING partner chain node and re-registers the candidate with them. The cross-chain identity key is never rotated.",
		);
		context.print(
			"The node must accept the 'author_rotateKeys' RPC call. It is allowed by default for localhost connections; otherwise the node must be run with '--rpc-methods=unsafe'.",
		);
		context.enewline();

		let genesis_utxo = load_chain_config_field(context, &config_fields::GENESIS_UTXO)?;
		let node_data_base_path =
			config_fields::SUBSTRATE_NODE_DATA_BASE_PATH.load_or_prompt_and_save(context);

		let GeneratedKeysFileContent { partner_chains_key, .. } = read_generated_keys(context)
			.map_err(|e| {
				context.eprint(&format!(
					"⚠️ The keys file `{KEYS_FILE_PATH}` is missing or invalid. Please run the `generate-keys` command first"
				));
				anyhow!(e)
			})?;

		let ecdsa_pair = get_ecdsa_pair_from_file(
			context,
			&keystore_path(&node_data_base_path),
			&partner_chains_key.to_hex_string(),
		)
		.map_err(|e| {
			context.eprint(&format!("⚠️ Failed to read partner chain key from the keystore: {e}"));
			anyhow!(e)
		})?;
		if AsRef::<[u8]>::as_ref(&ecdsa_pair.public()) != partner_chains_key.0.as_slice() {
			return Err(anyhow!(
				"the partner chain key in `{KEYS_FILE_PATH}` does not match the key present in the keystore"
			));
		}

		let node_url = match &self.node_url {
			Some(url) => url.clone(),
			None => config_fields::SUBSTRATE_NODE_RPC_URL
				.prompt_with_default_from_file_and_save(context),
		};

		context.eprint(&format!("⚙️ Rotating session keys via {node_url}"));
		let SubstrateRpcResponse::RotatedKeys(rotated_keys_blob) =
			context.substrate_rpc(&node_url, SubstrateRpcRequest::AuthorRotateKeys)?
		else {
			return Err(anyhow!("unexpected node response to 'author_rotateKeys'"));
		};
		context.eprint(&format!(
			"🔑 New session keys (opaque): 0x{}",
			hex::encode(&rotated_keys_blob)
		));

		let SubstrateRpcResponse::DecodedKeys(decoded) = context.substrate_rpc(
			&node_url,
			SubstrateRpcRequest::DecodeSessionKeys { encoded: rotated_keys_blob },
		)?
		else {
			return Err(anyhow!("unexpected node response to 'state_call'"));
		};
		let decoded = decoded
			.ok_or_else(|| anyhow!("the node runtime could not decode the rotated session keys"))?;
		let keys = candidate_key_params_from_decoded(decoded)?;

		write_keys_file(context, &partner_chains_key, &keys)?;

		let registration_utxo = select_registration_utxo(context)?;

		let sidechain_pub_key = SidechainPublicKey(partner_chains_key.0.clone());
		let registration_message = RegisterValidatorMessage {
			genesis_utxo,
			sidechain_pub_key: sidechain_pub_key.clone(),
			registration_utxo,
		};
		let pc_signature =
			sign_registration_message_with_sidechain_key(registration_message.clone(), ecdsa_pair)?;

		let cold_key_path: Option<String> = match &self.mainchain_signing_key_file {
			Some(path) => Some(path.clone()),
			None => context
				.prompt_yes_no("Is your SPO cold signing key available on this machine?", false)
				.then(|| context.prompt("Path to Stake Pool signing key file", Some("cold.skey"))),
		};

		match cold_key_path {
			None => {
				let partner_chains_key_str = partner_chains_key.to_hex_string();
				let executable = context.current_executable()?;
				context.print("Run the following command to generate signatures on the next step. It has to be executed on the machine with your SPO cold signing key.");
				context.print("");
				context.print(&format!(
					"{executable} wizards register2 \\
--genesis-utxo {genesis_utxo} \\
--registration-utxo {registration_utxo} \\
--partner-chain-pub-key {partner_chains_key_str} \\
--partner-chain-signature {pc_signature}{}",
					keys.iter()
						.map(CandidateKeyParam::to_string)
						.map(|arg| format!(" \\\n--keys {arg}"))
						.collect::<Vec<_>>()
						.join("")
				));
			},
			Some(path) => {
				let mainchain_signing_key =
					get_stake_pool_cold_skey(context, &path).inspect_err(|_| {
						context.eprint("Unable to read Stake Pool signing key file")
					})?;

				let (spo_public_key, spo_signature) = cardano_spo_public_key_and_signature(
					mainchain_signing_key.0,
					registration_message,
				);

				let partner_chain_signature = SidechainSignature(
					hex::decode(&pc_signature)
						.map_err(|e| anyhow!("internal error: signature is not valid hex: {e}"))?,
				);

				submit_candidate_registration(
					context,
					&self.common_arguments,
					genesis_utxo,
					registration_utxo,
					&sidechain_pub_key,
					&partner_chain_signature,
					&spo_public_key,
					&spo_signature,
					&keys,
					self.payment_signing_key_file.as_deref(),
				)?;
			},
		}

		context.enewline();
		context.eprint(
			"⚠️ The new session keys take effect only after the updated registration is observed on Cardano and a committee using it is selected: registrations included in mainchain epoch N become effective in epoch N+2.",
		);
		context.eprint(
			"⚠️ 'author_rotateKeys' adds new keys to the node keystore without deleting the previous ones. Keep the old keys in the keystore until the last committee selected with them has finished.",
		);
		Ok(())
	}
}

/// Converts the (key type id, public key bytes) pairs decoded by the runtime into candidate
/// key parameters, excluding the cross-chain identity key, which is not a rotatable
/// consensus key.
fn candidate_key_params_from_decoded(
	decoded: Vec<(KeyTypeId, Vec<u8>)>,
) -> anyhow::Result<Vec<CandidateKeyParam>> {
	let mut seen = BTreeSet::new();
	let mut keys = vec![];
	for (id, bytes) in decoded {
		if id == CROSS_CHAIN_KEY_TYPE_ID {
			continue;
		}
		let id_str = String::from_utf8(id.0.to_vec()).map_err(|_| {
			anyhow!("Key type id 0x{} returned by the node is not valid UTF-8", hex::encode(id.0))
		})?;
		if !seen.insert(id_str.clone()) {
			return Err(anyhow!("Duplicate key type id '{id_str}' returned by the node"));
		}
		keys.push(CandidateKeyParam::new(id.0, bytes));
	}
	keys.sort_by_key(|key| key.0.id);
	Ok(keys)
}

/// Overwrites the public keys file with the rotated session keys, keeping the cross-chain
/// key unchanged. Fails if the user refuses to overwrite the existing file, before any
/// on-chain action is taken.
fn write_keys_file<C: IOContext>(
	context: &C,
	partner_chains_key: &ByteString,
	keys: &[CandidateKeyParam],
) -> anyhow::Result<()> {
	if !prompt_can_write("keys file", KEYS_FILE_PATH, context) {
		context.eprint("Refusing to overwrite keys file - aborting. Please note that the rotated keys have already been added to the node keystore.");
		return Err(anyhow!("Rotation aborted by user"));
	}
	let keys_map: BTreeMap<String, ByteString> = keys
		.iter()
		.map(|key| {
			(
				String::from_utf8(key.0.id.to_vec()).expect("key type ids were validated as utf-8"),
				ByteString(key.0.bytes.clone()),
			)
		})
		.collect();
	let public_keys_json = serde_json::to_string_pretty(&PermissionedCandidateKeys {
		partner_chains_key: partner_chains_key.clone(),
		keys: keys_map,
	})?;
	context.write_file(KEYS_FILE_PATH, &public_keys_json);
	context.eprint(&format!(
		"🔑 The following public keys were generated and saved to the {KEYS_FILE_PATH} file:"
	));
	context.print(&public_keys_json);
	context.enewline();
	Ok(())
}
