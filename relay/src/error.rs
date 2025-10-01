use sp_consensus_beefy::mmr::BeefyAuthoritySet;
use sp_core::H256;
use subxt::ext::{codec, subxt_rpcs};

use crate::types::Block;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Block({0}): commitment signatures does not match the validator set")]
	NoMatchingSignature(Block),

	#[error("Rpc error: {0}")]
	Rpc(#[from] subxt_rpcs::Error),

	#[error("Scale Decode error: {0}")]
	ScaleDecode(#[from] codec::Error),

	#[error("Failed to read keys from {0}")]
	InvalidKeysFile(String),

	#[error("Failed to parse {0}")]
	SerdeDecode(String),

	#[error("Client Error: {0}")]
	ClientError(#[from] subxt::Error),

	#[error("Justification Subscription Ended")]
	SubscriptionEnd,

	#[error("Verifying proof query returned none")]
	ProofVerifyingFailed,

	#[error("Block({0}) did not result to a BlockHash")]
	NoBlockHash(Block),

	#[error("No Authority Set returned.")]
	EmptyAuthoritySet,

	#[error("Invalid: expected: {expected:?}, actual: {actual:?}")]
	InvalidNextAuthoritySet { expected: BeefyAuthoritySet<H256>, actual: BeefyAuthoritySet<H256> },

	#[error("Failed to convert metadata type")]
	MetadataConversion,

	#[error("No Validator Set to generate")]
	EmptyValidatorSet,
}
