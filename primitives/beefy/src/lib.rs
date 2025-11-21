#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Codec, Decode};
use sp_consensus_beefy::mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet};
use sp_runtime::RuntimeAppPublic;
use sp_std::vec::Vec;

/// The StakeDelegation
pub type Stake = u64;
pub type BeefyAuthoritySetOf<Hash> = BeefyAuthoritySet<Hash>;

/// A List of tuple (Beefy Ids, stake)
pub type BeefyStakes<AuthorityId> = Vec<(AuthorityId, Stake)>;

/// Ids to identify Beefy stakes
pub mod known_payloads {
	use sp_consensus_beefy::BeefyPayloadId;

	pub const CURRENT_BEEFY_STAKES_ID: BeefyPayloadId = *b"cs";
	pub const CURRENT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"cb";
	pub const NEXT_BEEFY_STAKES_ID: BeefyPayloadId = *b"ns";
	pub const NEXT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"nb";
}

// An api to be used and accessed by the Node
sp_api::decl_runtime_apis! {
	pub trait BeefyStakesApi<Hash, AuthorityId>
	where
		BeefyAuthoritySet<Hash>: Decode,
		AuthorityId: Codec + RuntimeAppPublic
	{
		/// Gets the current beefy stakes
		fn current_beefy_stakes() -> BeefyStakes<AuthorityId>;

		/// Gets the next beefy stakes
		fn next_beefy_stakes() -> Option<BeefyStakes<AuthorityId>>;

		/// Returns the authority set based on the current beef stakes
		fn compute_current_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) ->  BeefyAuthoritySet<Hash>;

		/// Returns the authority set based on the next beef stakes
		fn compute_next_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) -> BeefyNextAuthoritySet<Hash> ;
	}
}
