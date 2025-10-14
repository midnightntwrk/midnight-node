use sp_consensus_beefy::{ecdsa_crypto, mmr::BeefyAuthoritySet};
use sp_core::H256;
use sp_mmr_primitives::NodeIndex;
use subxt::ext::{codec, subxt_rpcs};

use crate::types::{Block, BlockHash, RootHash};

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
	ProofVerificationFailed,

	#[error("Block({0}) did not result to a BlockHash")]
	NoBlockHash(Block),

	#[error("No Authority Set returned.")]
	EmptyAuthoritySet,

	#[error("MMR storage query returned none")]
	NoMMRData,

	#[error("Invalid: expected: {expected:?}, actual: {actual:?}")]
	InvalidNextAuthoritySet { expected: BeefyAuthoritySet<H256>, actual: BeefyAuthoritySet<H256> },

	#[error("Failed to convert metadata type")]
	MetadataConversion,

	#[error("No Validator Set to generate")]
	EmptyValidatorSet,

	#[error("Peak Node({node_index}) at Block hash(at_block_hash) not part of proof items")]
	InvalidPeak { node_index: NodeIndex, at_block_hash: BlockHash },

	#[error("Peak Node({node_index}) at Block hash(at_block_hash) not on chain")]
	PeakNotFound { node_index: NodeIndex, at_block_hash: BlockHash },

	// -------- AuthoritiesProof Errors -----------
	#[error("Proof shows {proof_size} authorities; chain shows {validators_size}")]
	MismatchTotalAuthorities { proof_size: u32, validators_size: usize },

	#[error("Wrong proof for {validator} at index({leaf_index}) in root({root})")]
	MismatchAuthority { root: RootHash, leaf_index: u32, validator: ecdsa_crypto::Public },

	#[error("Failed to create proof of authorities list")]
	InvalidAuthoritiesProofCreation,
}
