use subxt::ext::subxt_rpcs;

use crate::BlockNumber;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Failed to read Beefy keys from {0}")]
	InvalidKeysFile(String),

	#[error("Failed to parse {0}")]
	JsonDecodeError(String),

	#[error("Subxt Error: {0}")]
	Subxt(#[from] subxt::Error),

	#[error("Rpc Error: {0}")]
	Rpc(#[from] subxt_rpcs::Error),

	#[error("Codec Error: {0}")]
	Codec(#[from] parity_scale_codec::Error),

	#[error("Block({0}): commitment signature(1) does not match the validator set")]
	NoMatchingSignature(BlockNumber, u32),

	#[error("Failed to create proof of authorities list")]
	InvalidAuthoritiesProofCreation,

	#[error("Missing Leaf data in MMR Proof")]
	NoLeafFound,

	#[error("No Validator Set to generate")]
	EmptyValidatorSet,
}
