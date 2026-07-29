use midnight_primitives_cnight_observation::CNightAddresses;
use midnight_primitives_federated_authority_observation::{
	AuthBodyConfig, FederatedAuthorityObservationConfig,
};
use midnight_primitives_mainchain_follower::partner_chains_db_sync_data_sources::DbSyncBlockDataSourceConfig;
use sidechain_domain::{
	MainchainAddress, McBlockHash, McEpochNumber, PolicyId, mainchain_epoch::MainchainEpochConfig,
};
use sp_core::offchain::Duration;
use sp_timestamp::Timestamp;
use std::{env, path::Path, str::FromStr};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct IntegrationTestConfig {
	pub postgres_uri: String,
	pub grpc_endpoint: String,

	pub d_parameter_policy_id: PolicyId,
	pub permissioned_candidates_policy: PolicyId,

	pub committee_candidate_address: MainchainAddress,

	pub cnight_config: CNightAddresses,
	pub epoch_config: MainchainEpochConfig,
	pub authority_config: FederatedAuthorityObservationConfig,
	pub block_source_config: DbSyncBlockDataSourceConfig,
	pub params_config: ParamsConfig,
}

#[derive(Debug, Clone)]
pub struct ParamsConfig {
	pub epoch_number: McEpochNumber,
	pub tx_capacity: usize,
	pub tip: McBlockHash,
	pub timestamp: Timestamp,
}

#[derive(Debug, Error)]
pub enum IntegrationTestConfigError {
	#[error("failed to load .env file: {0}")]
	Dotenv(#[from] dotenvy::Error),

	#[error("missing environment variable: {0}")]
	MissingVar(String),

	#[error("malformed environment variable: {0}")]
	Malformed(String),
}

impl IntegrationTestConfig {
	pub fn from_env() -> Result<Self, IntegrationTestConfigError> {
		let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/.env.test");

		dotenvy::from_path(path)?;

		let postgres_uri = get_env("POSTGRES_URI")?;
		let grpc_endpoint = get_env("GRPC_ENDPOINT")?;
		let cnight_policy_id = parse_policy_id("CNIGHT_POLICY_ID")?.0;
		let cnight_asset_name = get_env("CNIGHT_ASSET_NAME")?;
		let mapping_validator_address = get_env("MAPPING_VALIDATOR_ADDRESS")?;
		let auth_token_asset_name = get_env("AUTH_TOKEN_ASSET_NAME")?;
		let d_parameter_policy_id = parse_policy_id("D_PARAMETER_POLICY_ID")?;
		let permissioned_candidates_policy = parse_policy_id("PERMISSIONED_CANDIDATES_POLICY")?;
		let raw_candidate_address = get_env("COMMITTEE_CANDIDATE_ADDRESS")?;
		let committee_candidate_address = MainchainAddress::from_str(&raw_candidate_address)
			.map_err(|e| {
				IntegrationTestConfigError::Malformed(format!("COMMITTEE_CANDIDATE_ADDRESS: {e}"))
			})?;

		let authority_config = FederatedAuthorityObservationConfig {
			council: AuthBodyConfig {
				address: get_env("COUNCIL_ADDRESS")?,
				policy_id: parse_policy_id("COUNCIL_POLICY_ID")?,
				members: Vec::new(),
				members_mainchain: DEFAULT_COUNCIL_MEMBERS_MAINCHAIN.to_vec(),
			},
			technical_committee: AuthBodyConfig {
				address: get_env("TECHNICAL_COMMITTEE_ADDRESS")?,
				policy_id: parse_policy_id("TECHNICAL_COMMITTEE_POLICY_ID")?,
				members: Vec::new(),
				members_mainchain: DEFAULT_TECHNICAL_COMMITTEE_MEMBERS_MAINCHAIN.to_vec(),
			},
		};

		let epoch_config = MainchainEpochConfig {
			epoch_duration_millis: DEFAULT_EPOCH_DURATION_MILLIS,
			slot_duration_millis: DEFAULT_SLOT_DURATION_MILLIS,
			first_epoch_timestamp_millis: DEFAULT_FIRST_EPOCH_TIMESTAMP_MILLIS.into(),
			first_epoch_number: DEFAULT_FIRST_EPOCH_NUMBER,
			first_slot_number: DEFAULT_FIRST_SLOT_NUMBER,
		};

		let block_source_config = DbSyncBlockDataSourceConfig {
			cardano_security_parameter: DEFAULT_SECURITY_PARAMETER,
			cardano_active_slots_coeff: DEFAULT_ACTIVE_SLOTS_COEFF,
			block_stability_margin: DEFAULT_BLOCK_STABILITY_MARGIN,
		};

		let cnight_config = CNightAddresses {
			mapping_validator_address,
			auth_token_asset_name,
			cnight_policy_id,
			cnight_asset_name,
		};

		let params_config = ParamsConfig {
			epoch_number: McEpochNumber(parse_u32("EPOCH_NUMBER")?),
			tx_capacity: parse_u32("TX_CAPACITY")? as usize,
			tip: parse_block_hash("TIP_HASH")?,
			timestamp: parse_timestamp("TIMESTAMP")?,
		};

		Ok(Self {
			postgres_uri,
			grpc_endpoint,
			d_parameter_policy_id,
			permissioned_candidates_policy,
			committee_candidate_address,
			cnight_config,
			authority_config,
			epoch_config,
			block_source_config,
			params_config,
		})
	}
}

fn get_env(var: &str) -> Result<String, IntegrationTestConfigError> {
	env::var(var).map_err(|_| IntegrationTestConfigError::MissingVar(var.into()))
}

fn parse_u32(var: &str) -> Result<u32, IntegrationTestConfigError> {
	let value = get_env(var)?;

	value
		.parse::<u32>()
		.map_err(|e| IntegrationTestConfigError::Malformed(format!("{}={} ({})", var, value, e)))
}

fn parse_u64(var: &str) -> Result<u64, IntegrationTestConfigError> {
	let value = get_env(var)?;

	value
		.parse::<u64>()
		.map_err(|e| IntegrationTestConfigError::Malformed(format!("{}={} ({})", var, value, e)))
}

fn parse_timestamp(var: &str) -> Result<Timestamp, IntegrationTestConfigError> {
	let timestamp = parse_u64(var)?;
	Ok(Timestamp::new(normalize_unix_timestamp(timestamp)))
}

fn normalize_unix_timestamp(timestamp: u64) -> u64 {
	const UNIX_MILLIS_THRESHOLD: u64 = 1_000_000_000_000;

	if timestamp < UNIX_MILLIS_THRESHOLD { timestamp * 1000 } else { timestamp }
}

fn parse_block_hash(var: &str) -> Result<McBlockHash, IntegrationTestConfigError> {
	let value = get_env(var)?;

	let bytes = hex::decode(&value).map_err(|e| {
		IntegrationTestConfigError::Malformed(format!("{var}={value} (invalid hex: {e})"))
	})?;

	let len = bytes.len();

	let arr: [u8; 32] = bytes.try_into().map_err(|_| {
		IntegrationTestConfigError::Malformed(format!(
			"{var}={value} (expected 32 bytes, got {len})"
		))
	})?;

	Ok(McBlockHash(arr))
}

fn parse_policy_id(var: &str) -> Result<PolicyId, IntegrationTestConfigError> {
	let raw = get_env(var)?;

	let bytes = hex::decode(&raw)
		.map_err(|e| IntegrationTestConfigError::Malformed(format!("{var}: {e}")))?;

	let arr: [u8; 28] = bytes
		.try_into()
		.map_err(|_| IntegrationTestConfigError::Malformed(format!("{var}: wrong length")))?;

	Ok(PolicyId(arr))
}

const DEFAULT_SECURITY_PARAMETER: u32 = 432;
const DEFAULT_ACTIVE_SLOTS_COEFF: f64 = 0.05;
const DEFAULT_BLOCK_STABILITY_MARGIN: u32 = 0;

const DEFAULT_FIRST_EPOCH_NUMBER: u32 = 0;
const DEFAULT_FIRST_SLOT_NUMBER: u64 = 0;
const DEFAULT_EPOCH_DURATION_MILLIS: Duration = Duration::from_millis(86400000);
const DEFAULT_FIRST_EPOCH_TIMESTAMP_MILLIS: u64 = 1666656000000;
const DEFAULT_SLOT_DURATION_MILLIS: Duration = Duration::from_millis(1000);

const DEFAULT_COUNCIL_MEMBERS_MAINCHAIN: [PolicyId; 3] = [
	PolicyId([
		0xe3, 0xea, 0xcc, 0x2b, 0xa7, 0xa0, 0xff, 0x8a, 0xe8, 0xd5, 0x28, 0x7a, 0x8e, 0x27, 0x5b,
		0xeb, 0x1b, 0x7d, 0x1e, 0x5f, 0x6f, 0x22, 0x39, 0x4c, 0x44, 0x45, 0xb0, 0x82,
	]),
	PolicyId([
		0xa5, 0xc6, 0xdf, 0x40, 0x8a, 0xbd, 0xbc, 0x52, 0x2a, 0x67, 0xcc, 0x97, 0x6e, 0x17, 0xb4,
		0x4a, 0xa8, 0xe2, 0xef, 0x93, 0x88, 0xc0, 0xe5, 0x88, 0x46, 0xc0, 0xee, 0xa4,
	]),
	PolicyId([
		0x1c, 0xac, 0xdd, 0x48, 0xfb, 0x7e, 0x72, 0x84, 0xca, 0x65, 0x44, 0x65, 0xfa, 0x78, 0xa2,
		0xc2, 0xb2, 0xc1, 0xc0, 0x66, 0x28, 0x5d, 0x51, 0x5e, 0x3a, 0x80, 0x47, 0x2d,
	]),
];
const DEFAULT_TECHNICAL_COMMITTEE_MEMBERS_MAINCHAIN: [PolicyId; 3] = [
	PolicyId([
		0xb9, 0x4a, 0x81, 0x87, 0x1d, 0xa1, 0x64, 0x63, 0x7b, 0x21, 0x30, 0xe0, 0x64, 0x34, 0xc3,
		0x00, 0xba, 0x2d, 0x30, 0x88, 0x26, 0x8f, 0x80, 0x98, 0xa7, 0xdd, 0xf2, 0x46,
	]),
	PolicyId([
		0xde, 0x14, 0xef, 0x01, 0x85, 0x4d, 0x8f, 0x22, 0x04, 0xe0, 0xaf, 0x3c, 0x97, 0xbd, 0x1a,
		0xce, 0x07, 0x92, 0x50, 0x8d, 0x39, 0x7c, 0x95, 0x7b, 0x01, 0x0b, 0x1b, 0x70,
	]),
	PolicyId([
		0x11, 0x91, 0xed, 0xfe, 0x59, 0x02, 0x63, 0xa7, 0x76, 0x1b, 0xe9, 0x2d, 0x7e, 0x3d, 0x3d,
		0x82, 0x48, 0xf2, 0x8a, 0x50, 0xd4, 0x10, 0x34, 0x05, 0xfe, 0xf0, 0x1a, 0xe6,
	]),
];
