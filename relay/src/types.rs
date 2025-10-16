use sp_core::H256;

use sp_consensus_beefy::ecdsa_crypto::Signature;

pub type Block = u32;
pub type BlockHash = H256;
pub type RootHash = H256;

pub type Hash = [u8; 32];
pub type Hashes = Vec<Hash>;

pub type ExtraData = Vec<u8>;

pub type BeefySignedCommitment = sp_consensus_beefy::SignedCommitment<Block, Signature>;
pub type BeefyValidatorSet =
	sp_consensus_beefy::ValidatorSet<sp_consensus_beefy::ecdsa_crypto::Public>;
