use std::{
	io::{BufRead, Write},
	path::PathBuf,
};

use clap::{
	Args, Subcommand,
	builder::{PathBufValueParser, TypedValueParser},
};
use hex::ToHex;
use midnight_node_ledger_helpers::{
	CoinPublicKey, ContractAddress, UnshieldedWallet, WalletSeed, serialize_untagged,
};
use zeroize::{Zeroize, Zeroizing};
pub(crate) mod encoded_zswap_local_state;
pub use encoded_zswap_local_state::{EncodedOutput, EncodedZswapLocalState};

use crate::cli_parsers as cli;

const BUILD_DIST: &str = "dist/bin.js";
const DEFAULT_COMPACTC_VERSION: &str = include_str!("../../../../COMPACTC_VERSION");

#[derive(Args, Debug)]
pub struct ToolkitJs {
	/// location of the toolkit-js.
	#[arg(long = "toolkit-js-path", env = "TOOLKIT_JS_PATH")]
	pub path: String,

	/// version of compactc
	#[arg(
        long = "compactc-version",
        env = "COMPACTC_VERSION",
        default_value = DEFAULT_COMPACTC_VERSION,
        value_parser = cli::semver_decode
    )]
	pub compactc_version: semver::Version,
}

/// Adds some protection against accidentally passing relative types to toolkit-js
#[derive(Clone, Debug)]
pub struct RelativePath(pub PathBuf);
impl RelativePath {
	fn absolute(&self) -> String {
		let input_path = std::path::PathBuf::from(&self.0);
		std::path::absolute(input_path)
			.expect("Failed to create absolute path")
			.to_string_lossy()
			.to_string()
	}
}

impl core::fmt::Display for RelativePath {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.display())
	}
}

impl From<PathBuf> for RelativePath {
	fn from(value: PathBuf) -> Self {
		Self(value)
	}
}

pub enum Command {
	Deploy(DeployArgs),
	Circuit {
		args: CircuitArgs,
		input_zswap_state: Option<RelativePath>,
		ledger_parameters: RelativePath,
	},
	Maintain(MaintainCommand),
}

#[derive(Args, Debug)]
pub struct CircuitArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub config: RelativePath,
	/// Hex-encoded ledger-serialized address of the contract - this should include the network id header
	#[arg(long, short = 'a', value_parser = cli::contract_address_decode)]
	pub contract_address: ContractAddress,
	/// Target network
	#[arg(long, default_value = "undeployed")]
	pub network: String,
	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = cli::coin_public_decode)]
	pub coin_public: CoinPublicKey,
	/// Input file containing the current on-chain circuit state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub input_onchain_state: RelativePath,
	/// Input file containing the private circuit state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub input_private_state: RelativePath,
	/// A file path of where the generated 'ZswapLocalState' is stored.
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub input_zswap_state: Option<RelativePath>,
	/// The output file of the intent
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_intent: RelativePath,
	/// The output file of the on-chain (public) state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_onchain_state: Option<RelativePath>,
	/// The output file of the private state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_private_state: RelativePath,
	/// A file path of where the generated 'ZswapLocalState' data should be written.
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_zswap_state: RelativePath,
	/// A file path of where the invoked circuit result data should be written.
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_result: Option<RelativePath>,
	/// Name of the circuit to invoke
	pub circuit_id: String,
	/// Arguments to pass to the circuit
	pub call_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct DeployArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub config: RelativePath,
	/// Target network
	#[arg(long, default_value = "undeployed")]
	pub network: String,
	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = cli::coin_public_decode)]
	pub coin_public: CoinPublicKey,
	/// Contract maintenance authority seed.
	#[arg(long, value_parser = cli::wallet_seed_decode, conflicts_with = "authority_seed_file")]
	pub authority_seed: Option<WalletSeed>,
	/// Read the contract maintenance authority seed from a file (keeps the secret off argv).
	#[arg(long, value_parser = cli::wallet_seed_from_file)]
	pub authority_seed_file: Option<WalletSeed>,
	/// The output file of the intent
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_intent: RelativePath,
	/// The output file of the private state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_private_state: RelativePath,
	/// A file path of where the generated 'ZswapLocalState' data should be written.
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	pub output_zswap_state: RelativePath,
	/// Arguments to pass to the contract constructor
	pub constructor_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct SharedMaintainArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	config: RelativePath,
	/// Hex-encoded ledger-serialized address of the contract - this should include the network id header
	#[arg(long, short = 'a', value_parser = cli::contract_address_decode)]
	contract_address: ContractAddress,
	/// Target network
	#[arg(long, default_value = "undeployed")]
	network: String,
	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = cli::coin_public_decode)]
	coin_public: CoinPublicKey,
	/// A BIP-340 signing key, hex encoded. Treated as secret: redacted from logs.
	#[arg(long, conflicts_with = "signing_file", value_parser = cli::secret_string_decode)]
	signing: Option<cli::SecretString>,
	/// Read the BIP-340 signing key from a file (keeps the secret off argv).
	#[arg(long, value_parser = cli::secret_string_from_file)]
	signing_file: Option<cli::SecretString>,
	/// Input file containing the current on-chain circuit state
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	input_onchain_state: RelativePath,
	/// The output file of the intent
	#[arg(long, value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p)))]
	output_intent: RelativePath,
}

#[derive(Args, Debug)]
pub struct MaintainContractArgs {
	#[command(flatten)]
	shared: SharedMaintainArgs,
	/// Seed of the new contract maintenance authority (secret: redacted from logs).
	/// Replaces the signing key for the contract.
	#[arg(
		long,
		value_parser = cli::wallet_seed_decode,
		required_unless_present = "new_authority_file",
		conflicts_with = "new_authority_file"
	)]
	new_authority: Option<WalletSeed>,
	/// Read the new contract maintenance authority seed from a file (keeps the secret off argv).
	#[arg(long, value_parser = cli::wallet_seed_from_file)]
	new_authority_file: Option<WalletSeed>,
}

#[derive(Args, Debug)]
pub struct MaintainCircuitArgs {
	#[command(flatten)]
	shared: SharedMaintainArgs,
	/// Name of the circuit to maintain.
	circuit_id: String,
	/// The path to a public BIP-340 verifier key, hex encoded. Replaces the verifier key of the circuit.
	/// If missing, removes the circuit instead.
	#[arg(value_parser = PathBufValueParser::new().map(|p| RelativePath::from(p).absolute()))]
	verifier: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum MaintainCommand {
	Contract(MaintainContractArgs),
	Circuit(MaintainCircuitArgs),
}
impl MaintainCommand {
	fn name(&self) -> &'static str {
		match self {
			Self::Contract(_) => "contract",
			Self::Circuit(_) => "circuit",
		}
	}
	fn shared_args(&self) -> &SharedMaintainArgs {
		match self {
			Self::Contract(args) => &args.shared,
			Self::Circuit(args) => &args.shared,
		}
	}
}

#[derive(thiserror::Error, Debug)]
pub enum ToolkitJsError {
	#[error("failed to execute toolkit-js")]
	ExecutionError(std::io::Error),
	#[error("failed to read toolkit-js output")]
	ToolkitJsOutputReadError(std::io::Error),
	#[error("toolkit-js exited with {status}\nstdout: {stdout}\nstderr: {stderr}")]
	NonZeroExit { status: std::process::ExitStatus, stdout: String, stderr: String },
}

impl ToolkitJs {
	/// `true` if the pinned compactc predates 0.31.0 and therefore needs the
	/// legacy `--network` flag passed to toolkit-js.
	///
	/// The comparison ignores any pre-release suffix on `compactc_version`, otherwise
	/// semver matches doesn't behave as expected (0.30.0-<some-hash> < 0.31.0 is false!)
	fn needs_legacy_network_flag(&self) -> bool {
		let mut version = self.compactc_version.clone();
		version.pre = semver::Prerelease::EMPTY;
		semver::VersionReq::parse("<0.31.0").unwrap().matches(&version)
	}

	pub fn execute(&self, cmd: Command) -> Result<(), ToolkitJsError> {
		match cmd {
			Command::Deploy(args) => self.execute_deploy(args),
			Command::Circuit { args, input_zswap_state, ledger_parameters } => {
				self.execute_circuit(args, input_zswap_state, ledger_parameters)
			},
			Command::Maintain(command) => self.execute_maintain(command),
		}
	}

	pub fn execute_deploy(&self, args: DeployArgs) -> Result<(), ToolkitJsError> {
		log::info!("Executing deploy command");
		let config = args.config.absolute();
		let output_intent = args.output_intent.absolute();
		let output_private_state = args.output_private_state.absolute();
		let output_zswap_state = args.output_zswap_state.absolute();
		let coin_public_key: String = args.coin_public.0.0.encode_hex();
		let mut cmd_args = vec![
			"deploy",
			"-c",
			&config,
			"--coin-public",
			&coin_public_key,
			"--output",
			&output_intent,
			"--output-ps",
			&output_private_state,
			"--output-zswap",
			&output_zswap_state,
		];
		if self.needs_legacy_network_flag() {
			cmd_args.extend_from_slice(&["--network", &args.network]);
		}

		// `Zeroizing` clears the hex key even when an early `?` returns.
		let signing_key = args
			.authority_seed
			.or(args.authority_seed_file)
			.map(|s| {
				let mut bytes = serialize_untagged(UnshieldedWallet::default(s).signing_key())
					.map_err(ToolkitJsError::ExecutionError)?;
				let hex = Zeroizing::new(bytes.encode_hex::<String>());
				bytes.zeroize();
				Ok::<Zeroizing<String>, ToolkitJsError>(hex)
			})
			.transpose()?;
		let signing_tmp = signing_key.as_deref().map(|s| secret_temp_file(s)).transpose()?;
		if let Some((_, ref path)) = signing_tmp {
			cmd_args.extend_from_slice(&["--signing-file", path]);
		}
		// Add positional args
		cmd_args.extend(args.constructor_args.iter().map(|s| s.as_str()));
		let secrets: Vec<&str> = signing_key.as_deref().map(|s| s.as_str()).into_iter().collect();
		self.execute_js(&cmd_args, &secrets)?;
		log::info!(
			"written: {}, {}, {}",
			args.output_intent,
			args.output_private_state,
			args.output_zswap_state
		);
		Ok(())
	}

	pub fn execute_circuit(
		&self,
		args: CircuitArgs,
		input_zswap_state: Option<RelativePath>,
		ledger_parameters: RelativePath,
	) -> Result<(), ToolkitJsError> {
		let contract_address_str = hex::encode(args.contract_address.0.0);
		log::info!("Executing circuit command");
		let config = args.config.absolute();
		let input_onchain_state = args.input_onchain_state.absolute();
		let input_private_state = args.input_private_state.absolute();
		let output_intent = args.output_intent.absolute();
		let output_private_state = args.output_private_state.absolute();
		let output_zswap_state = args.output_zswap_state.absolute();
		let coin_public_key = hex::encode(args.coin_public.0.0);
		let input_ledger_parameters = ledger_parameters.absolute();
		let mut cmd_args = vec![
			"circuit",
			"-c",
			&config,
			"--coin-public",
			&coin_public_key,
			"--input",
			&input_onchain_state,
			"--input-ps",
			&input_private_state,
			"--output",
			&output_intent,
			"--output-ps",
			&output_private_state,
			"--output-zswap",
			&output_zswap_state,
			"--input-ledger-params",
			&input_ledger_parameters,
		];
		if self.needs_legacy_network_flag() {
			cmd_args.extend_from_slice(&["--network", &args.network]);
		}
		let input_zswap_state = input_zswap_state.map(|s| s.absolute());
		if let Some(ref input_zswap_state) = input_zswap_state {
			cmd_args.extend_from_slice(&["--input-zswap", &input_zswap_state]);
		}
		let output_onchain_state = args.output_onchain_state.map(|s| s.absolute());
		if let Some(ref output_onchain_state) = output_onchain_state {
			cmd_args.extend_from_slice(&["--output-oc", &output_onchain_state]);
		}
		let output_result = args.output_result.map(|s| s.absolute());
		if let Some(ref output_result) = output_result {
			cmd_args.extend_from_slice(&["--output-result", &output_result]);
		}
		// Add positional args
		cmd_args.extend_from_slice(&[&contract_address_str, &args.circuit_id]);
		cmd_args.extend(args.call_args.iter().map(|s| s.as_str()));
		self.execute_js(&cmd_args, &[])?;
		log::info!(
			"written: {}, {}, {}",
			args.output_intent,
			args.output_private_state,
			args.output_zswap_state
		);
		Ok(())
	}

	pub fn execute_maintain(&self, command: MaintainCommand) -> Result<(), ToolkitJsError> {
		let args = command.shared_args();
		let contract_address_str = hex::encode(args.contract_address.0.0);
		log::info!("Executing maintain command");
		let config = args.config.absolute();
		let input_onchain_state = args.input_onchain_state.absolute();
		let output_intent = args.output_intent.absolute();
		let coin_public_key = hex::encode(args.coin_public.0.0);
		let mut cmd_args = vec![
			"maintain",
			command.name(),
			"-c",
			&config,
			"--coin-public",
			&coin_public_key,
			"--input",
			&input_onchain_state,
			"--output",
			&output_intent,
		];
		if self.needs_legacy_network_flag() {
			cmd_args.extend_from_slice(&["--network", &args.network]);
		}

		let signing = args.signing.as_deref().or(args.signing_file.as_deref());
		let signing_tmp = signing.map(secret_temp_file).transpose()?;
		if let Some((_, ref path)) = signing_tmp {
			cmd_args.extend_from_slice(&["--signing-file", path]);
		}
		// Add positional args
		cmd_args.push(&contract_address_str);
		// `Zeroizing` clears the hex seed even when an early `?` returns.
		let new_authority = match &command {
			MaintainCommand::Contract(MaintainContractArgs {
				new_authority,
				new_authority_file,
				..
			}) => {
				let seed = new_authority.as_ref().or(new_authority_file.as_ref()).expect(
					"clap enforces one of --new-authority/--new-authority-file being present",
				);
				Some(Zeroizing::new(seed.as_bytes().encode_hex::<String>()))
			},
			_ => None,
		};
		// bin.ts expands this back into the positional new-authority value, so
		// it must sit exactly where the positional would (after the address).
		let new_authority_tmp =
			new_authority.as_deref().map(|s| secret_temp_file(s)).transpose()?;
		if let Some((_, ref path)) = new_authority_tmp {
			cmd_args.extend_from_slice(&["--new-authority-file", path]);
		}
		if let MaintainCommand::Circuit(args) = &command {
			cmd_args.push(&args.circuit_id);
			if let Some(vk_path) = &args.verifier {
				cmd_args.push(&vk_path);
			}
		}
		let secrets: Vec<&str> = signing
			.into_iter()
			.chain(new_authority.as_deref().map(|s| s.as_str()))
			.collect();
		self.execute_js(&cmd_args, &secrets)?;
		log::info!("written: {}", args.output_intent);
		Ok(())
	}

	/// `secrets` lists argument values that must never reach the logs. In normal
	/// operation secrets travel via temp files (see [`secret_temp_file`]) and
	/// never appear in `args` at all; the value-based redaction here is a
	/// backstop against future code reintroducing inline secrets.
	fn execute_js(&self, args: &[&str], secrets: &[&str]) -> Result<(), ToolkitJsError> {
		let cmd = PathBuf::from(&self.path).join(BUILD_DIST).to_string_lossy().to_string();
		log::info!("Executing {cmd}...");
		if log::log_enabled!(log::Level::Debug) {
			let redacted_args = redact_args(args, secrets);
			log::debug!("Executing {cmd} with arguments: {redacted_args:?}...");
		}

		let output = std::process::Command::new(cmd)
			.env("COMPACTC_VERSION", self.compactc_version.to_string())
			.current_dir(&self.path)
			.args(args)
			.output()
			.map_err(ToolkitJsError::ExecutionError)?;

		for line in output.stdout.lines() {
			let line = line.map_err(|e| ToolkitJsError::ToolkitJsOutputReadError(e))?;
			let line = line.trim_end();
			if line.is_empty() {
				println!("toolkit-js>");
			} else {
				println!("toolkit-js> {line}");
			}
		}

		for line in output.stderr.lines() {
			let line = line.map_err(|e| ToolkitJsError::ToolkitJsOutputReadError(e))?;
			let line = line.trim_end();
			if line.is_empty() {
				eprintln!("toolkit-js>");
			} else {
				eprintln!("toolkit-js> {line}");
			}
		}

		if !output.status.success() {
			return Err(ToolkitJsError::NonZeroExit {
				status: output.status,
				stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
				stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
			});
		}
		Ok(())
	}
}

/// Hand a secret to toolkit-js via an owner-only temp file instead of argv
/// (0600 on Unix — the `tempfile` crate's default; on other platforms it is
/// whatever `NamedTempFile` provides): a child's argv is world-readable on
/// Linux via `/proc/<pid>/cmdline`
/// for its whole (ZK-slow) lifetime. `bin.ts` expands the `*-file` flag back
/// into the in-memory argv, which the kernel's cmdline snapshot never sees.
/// The file is deleted when the returned handle drops, so keep it alive until
/// the child has exited.
fn secret_temp_file(secret: &str) -> Result<(tempfile::NamedTempFile, String), ToolkitJsError> {
	let mut file = tempfile::NamedTempFile::new().map_err(ToolkitJsError::ExecutionError)?;
	file.write_all(secret.as_bytes()).map_err(ToolkitJsError::ExecutionError)?;
	file.flush().map_err(ToolkitJsError::ExecutionError)?;
	let path = file.path().to_string_lossy().into_owned();
	Ok((file, path))
}

/// Replace every argument that exactly matches a secret with `[REDACTED]`.
fn redact_args<'a>(args: &[&'a str], secrets: &[&str]) -> Vec<&'a str> {
	args.iter()
		.map(|&arg| if secrets.contains(&arg) { "[REDACTED]" } else { arg })
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	const SECRET: &str = "9f2d3a4b5c6d7e8f9f2d3a4b5c6d7e8f9f2d3a4b5c6d7e8f9f2d3a4b5c6d7e8f";

	/// Regression test for the April key-disclosure report. Production code no
	/// longer puts secrets in the child argv at all (temp-file handoff), so
	/// this guards the backstop: if inline secrets are ever reintroduced,
	/// redaction must catch them both as flag values (`--signing <key>`) and
	/// positionally (maintain-contract's new authority).
	#[test]
	fn redacts_flagged_and_positional_secrets() {
		let args = ["maintain", "contract", "--signing", SECRET, "0011aabb", SECRET, "my_circuit"];
		let redacted = redact_args(&args, &[SECRET]);
		let rendered = format!("{redacted:?}");
		assert!(!rendered.contains(SECRET), "secret leaked into log output: {rendered}");
		assert_eq!(redacted.iter().filter(|a| **a == "[REDACTED]").count(), 2);
	}

	/// Secrets travel to toolkit-js via owner-only temp files, never argv.
	#[test]
	fn secret_temp_file_is_owner_only_and_deleted_on_drop() {
		let (file, path) = secret_temp_file(SECRET).unwrap();
		assert_eq!(std::fs::read_to_string(&path).unwrap(), SECRET);
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let mode = std::fs::metadata(&path).unwrap().permissions().mode();
			assert_eq!(mode & 0o777, 0o600, "secret temp file must be owner-only");
		}
		drop(file);
		assert!(!std::path::Path::new(&path).exists(), "temp file must vanish on drop");
	}

	#[test]
	fn non_secrets_pass_through_unchanged() {
		let args = ["deploy", "-c", "config.ts", "--coin-public", "aabbccdd"];
		assert_eq!(redact_args(&args, &[]), args);
		assert_eq!(redact_args(&args, &[SECRET]), args);
	}
}
